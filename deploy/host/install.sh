#!/usr/bin/env bash
# Invoked by publish.ps1. The platform runs natively; Docker is a managed resource.
set -Eeuo pipefail
umask 027
die() { printf '%s\n' "$*" >&2; exit 1; }
[[ $# == 11 ]] || die 'Use deploy/publish.ps1 to supply the installation arguments.'
root=$1 release_id=$2 web_port=$3 api_port=$4 web_origin=$5 license_api=$6
default_origin=$7 default_license_api=$8 initialize=$9 stage=${10}
legacy_container=${11}
host_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
source_dir=$(cd -- "$host_dir/../.." && pwd)
[[ $(id -u) == 0 ]] || die 'Installation requires root (or sudo -n).'
[[ $(uname -s) == Linux ]] || die 'Native deployment requires Linux with systemd.'
[[ $release_id =~ ^[A-Za-z0-9][A-Za-z0-9._-]+$ ]] || die 'Invalid release ID.'
[[ $stage == "/tmp/yaya-operation-center-$release_id" ]] || die 'Unexpected upload directory.'
for tool in python3 cargo rustc cc node pnpm docker curl systemctl runuser flock getent useradd usermod groupadd install tar ss stat readlink; do
  command -v "$tool" >/dev/null || die "Missing prerequisite: $tool (see deploy/README.md)."
done
python3 "$host_dir/configure.py" validate-root "$root" >/dev/null
systemctl show-environment >/dev/null || die 'A running systemd instance is required.'
[[ $(node -p 'Number(process.versions.node.split(".")[0])') -ge 22 ]] || die 'Node.js 22+ is required.'
docker_bin=$(readlink -f "$(command -v docker)")
node_bin=$(readlink -f "$(command -v node)")
python_bin=$(readlink -f "$(command -v python3)")
[[ -S /var/run/docker.sock ]] || die 'Local /var/run/docker.sock is unavailable.'
[[ $("$docker_bin" --host unix:///var/run/docker.sock info --format '{{.OSType}}') == linux ]] || die 'Linux Docker Engine is required.'

mkdir -p "$root"
chmod 0755 "$root"
exec 9>/run/lock/yaya-operation-center-publish.lock
flock -n 9 || die 'Another platform installation or restore is in progress.'
units=(yaya-operation-center.target yaya-host-monitor.service yaya-operation-center-api.service yaya-operation-center-web.service yaya-file-agent.service)
for unit in "${units[@]}"; do
  unit_path="/etc/systemd/system/$unit"
  unit_fragment=$(systemctl show --property FragmentPath --value "$unit" 2>/dev/null || true)
  [[ -z $unit_fragment || $unit_fragment == "$unit_path" ]] || die "Refusing to shadow an existing system service: $unit"
  if [[ -e $unit_path || -L $unit_path ]]; then
    [[ -f $unit_path && ! -L $unit_path ]] && grep -Fxq "# YAYA_INSTALL_ROOT=$root" "$unit_path" || die "Refusing to replace a service from another installation: $unit"
  fi
done
previous_release=''
if [[ -e $root/current || -L $root/current ]]; then
  [[ -L $root/current ]] || die 'current must be a managed release symlink.'
  previous_release=$(readlink -f "$root/current")
  [[ $previous_release == "$root/releases/"* && -d $previous_release ]] || die 'Invalid previous release.'
elif [[ -z $legacy_container && ( -f $root/deploy/compose.yaml || -f $root/deploy/.env ) ]]; then
  die 'Legacy container deployment detected. Migrate its data and secrets before native installation; no container or volume has been changed.'
fi
if [[ -n $legacy_container ]]; then
  [[ -z $previous_release && $initialize != true && $legacy_container =~ ^[A-Za-z0-9][A-Za-z0-9_.-]*$ ]] || die 'Container migration is only valid for the first native installation, without -Initialize.'
  [[ $(docker inspect -f '{{ index .Config.Labels "com.docker.compose.service" }}' "$legacy_container") == operation-center ]] || die 'Migration target is not the operation-center Compose service.'
fi
if [[ $initialize == true && -n $previous_release ]]; then
  die '-Initialize is only for a first installation; upgrades retain the installed signing keys.'
fi
if [[ -n $previous_release ]]; then
  [[ -f $root/shared/operation-center.env && -d $root/shared/data ]] || die 'Previous native configuration or data directory is missing.'
  for unit in "${units[@]}"; do
    # File Agent is introduced by this upgrade, so older installations lack it.
    [[ $unit != yaya-file-agent.service ]] || continue
    [[ -f /etc/systemd/system/$unit ]] || die "Previous native service definition is missing: $unit"
  done
fi
legacy_containers=$("$docker_bin" --host unix:///var/run/docker.sock ps -a --format '{{.Names}} {{.Image}}' | grep -E '(^|[/ ])(yaya-operation-center|license-center)([: /]|$)' || true)
if [[ -z $legacy_container ]]; then
  [[ -z $legacy_containers ]] || die 'A legacy operation/license-center container exists. Use -MigrateContainer for explicit data migration.'
else
  [[ $(printf '%s\n' "$legacy_containers" | wc -l) == 1 && $legacy_containers == "$legacy_container "* ]] || die 'Unexpected or multiple legacy operation containers; inspect before migrating.'
fi

# Do not silently add the service account to the root group for socket access.
docker_gid=$(stat -c '%g' /var/run/docker.sock)
docker_group=$(getent group "$docker_gid" | cut -d: -f1)
[[ -n $docker_group && $docker_gid != 0 ]] || die 'Docker socket needs a dedicated non-root group before installing the service account.'
getent group yaya-operation-center >/dev/null || groupadd --system yaya-operation-center
if ! id yaya-operation-center >/dev/null 2>&1; then
  useradd --system --no-create-home --gid yaya-operation-center --home-dir "$root/shared/data" --shell /usr/sbin/nologin yaya-operation-center
fi
service_uid=$(id -u yaya-operation-center)
[[ $service_uid != 0 ]] || die 'Refusing to run the platform account as root.'
usermod -a -G "$docker_group" yaya-operation-center
runuser -u yaya-operation-center -- "$node_bin" --version >/dev/null || die 'Node must be installed at a path accessible by the service account (not a private root home).'
runuser -u yaya-operation-center -- "$docker_bin" --host unix:///var/run/docker.sock info >/dev/null || die 'The platform account cannot access the local Docker Engine.'

release="$root/releases/$release_id"
[[ ! -e $release ]] || die 'Release already exists; refusing to overwrite it.'
install -d -m 0755 "$root/releases" "$release" "$release/bin" "$release/web" "$release/deploy"
install -d -m 0750 -o root -g yaya-operation-center "$root/shared" "$root/shared/secrets"
install -d -m 0750 -o yaya-operation-center -g yaya-operation-center "$root/shared/data" "$root/shared/monitor"
install -d -m 0700 "$root/backups" "$root/build-cache"
for secret in private.pem public.pem admin-password.txt; do
  secret_source="$root/shared/secrets/$secret"
  if [[ -n $legacy_container ]]; then
    docker cp "$legacy_container:/run/secrets/$secret" "$stage/secrets/$secret"
    secret_source="$stage/secrets/$secret"
  fi
  [[ $initialize != true ]] || secret_source="$stage/secrets/$secret"
  [[ -s $secret_source && ! -L $secret_source ]] || die "Missing or invalid $secret; first deployment requires -Initialize."
  if [[ $initialize != true && -z $legacy_container ]]; then
    runuser -u yaya-operation-center -- test -r "$secret_source" || die "The platform account cannot read installed $secret."
  fi
done

# Compile before stopping the installed platform. No operational container is built.
export CARGO_HTTP_TIMEOUT=45 CARGO_NET_RETRY=2 CARGO_TERM_PROGRESS_WHEN=never
cargo build --release --locked --manifest-path "$source_dir/api/Cargo.toml" --target-dir "$root/build-cache/api"
cargo build --release --locked --manifest-path "$source_dir/host-monitor/Cargo.toml" --target-dir "$root/build-cache/host-monitor"
(
  cd "$source_dir"
  pnpm install --frozen-lockfile --prod=false
  # Rewrites are compiled into the standalone server, so use the selected API port.
  YAYA_OPERATION_API_UPSTREAM="http://127.0.0.1:$api_port" NEXT_PUBLIC_OPERATION_API_BASE_URL=/operation-api pnpm --dir web build
)
install -m 0755 "$root/build-cache/api/release/yaya-operation-center-api" "$release/bin/"
install -m 0755 "$root/build-cache/host-monitor/release/yaya-host-monitor" "$release/bin/"
install -m 0755 "$host_dir/run-api.sh" "$release/bin/run-api.sh"
if [[ -f $stage/yaya-desktop.exe ]]; then
  install -m 0644 "$stage/yaya-desktop.exe" "$release/bin/yaya-desktop.exe"
  rm -f -- "$stage/yaya-desktop.exe"
elif [[ -n $previous_release && -f $previous_release/bin/yaya-desktop.exe ]]; then
  install -m 0644 "$previous_release/bin/yaya-desktop.exe" "$release/bin/yaya-desktop.exe"
fi
if [[ -f $release/bin/yaya-desktop.exe ]]; then
  configured_nsis=$(python3 "$host_dir/configure.py" get "$root/shared/operation-center.env" YAYA_DESKTOP_MAKENSIS)
  bash "$host_dir/prepare-nsis.sh" "$release" "$previous_release" "$docker_bin" "$stage" "$configured_nsis"
fi
cp -a "$source_dir/web/.next/standalone/." "$release/web/"
[[ -f $release/web/web/server.js ]] || die 'Unexpected Next.js standalone layout; expected web/server.js.'
mkdir -p "$release/web/web/.next"
cp -a "$source_dir/web/.next/static" "$release/web/web/.next/static"
if [[ -d $source_dir/web/public ]]; then cp -a "$source_dir/web/public" "$release/web/web/public"; fi
cp -a "$host_dir" "$release/deploy/host"
chmod -R a+rX "$release"
install -d -m 0750 -o yaya-operation-center -g yaya-operation-center "$root/shared/data/web-cache"
if [[ -d $release/web/web/.next/cache ]]; then
  mv "$release/web/web/.next/cache" "$release/web/build-cache"
fi
ln -s "$root/shared/data/web-cache" "$release/web/web/.next/cache"

# Customer data-volume backup still uses a small helper container. This is not
# deployment of the operations platform itself, and runtime tasks never pull it.
helper_image=$(python3 "$host_dir/configure.py" get "$root/shared/operation-center.env" YAYA_BACKUP_HELPER_IMAGE)
if [[ -z $helper_image ]]; then
  if "$docker_bin" --host unix:///var/run/docker.sock image inspect docker:27-cli >/dev/null 2>&1; then
    helper_image=docker:27-cli
  else
    helper_image=alpine:3.21
    "$docker_bin" --host unix:///var/run/docker.sock image inspect "$helper_image" >/dev/null 2>&1 || "$docker_bin" --host unix:///var/run/docker.sock pull "$helper_image"
  fi
fi
helper_id=$("$docker_bin" --host unix:///var/run/docker.sock image inspect --format '{{.Id}}' "$helper_image")
python3 "$host_dir/configure.py" render --root "$root" --output "$release/config" \
  --web-port "$web_port" --api-port "$api_port" --uid "$service_uid" \
  --web-origin "$web_origin" --license-api "$license_api" \
  --default-origin "$default_origin" --default-license-api "$default_license_api" \
  --node "$node_bin" --docker "$docker_bin" --docker-group "$docker_group" --helper-image "$helper_id" --legacy-container "$legacy_container"
chmod 0700 "$release/config"

stopped=false
legacy_stopped=false
activated=false
backup=''
failure() {
  result=$?
  trap - EXIT
  if (( result != 0 )); then
    if [[ $activated == true ]]; then
      systemctl stop "${units[@]}" || true
      printf 'New release failed. Services are stopped; data and diagnostics are retained.\n' >&2
      if [[ -n $previous_release && -n $backup && -f $backup/complete ]]; then
        printf 'Restore code, configuration and pre-upgrade data together with:\n  bash %q %q %q\n' "$release/deploy/host/restore.sh" "$root" "$backup" >&2
      fi
    elif [[ $legacy_stopped == true ]]; then
      docker start "$legacy_container" >/dev/null || true
    elif [[ $stopped == true && -n $previous_release ]]; then
      systemctl start yaya-operation-center.target || true
    fi
  fi
  exit "$result"
}
trap failure EXIT
stopped=true
for unit in "${units[@]}"; do
  # New units do not exist on the first upgrade introducing them.
  if [[ -f /etc/systemd/system/$unit ]]; then
    systemctl stop "$unit" || die "Unable to stop the previous service: $unit"
  fi
done
for unit in "${units[@]:1}"; do
  if systemctl is-active --quiet "$unit"; then die "Service still running: $unit"; fi
done
if [[ -n $legacy_container ]]; then
  legacy_stopped=true
  docker stop --time 45 "$legacy_container" >/dev/null
  legacy_backup="$root/backups/legacy-$release_id"
  install -d -m 0700 "$legacy_backup" "$legacy_backup/data" "$legacy_backup/secrets"
  docker cp "$legacy_container:/var/lib/yaya-operation-center/." "$legacy_backup/data/"
  for secret in private.pem public.pem admin-password.txt; do
    install -m 0600 "$stage/secrets/$secret" "$legacy_backup/secrets/$secret"
  done
  docker inspect "$legacy_container" > "$legacy_backup/container-inspect.json"
  chmod 0600 "$legacy_backup/container-inspect.json"
  # Some historical releases stored imported Skills in their working directory.
  for skill_path in /app/runtime/skills /app/api/runtime/skills; do
    if docker cp "$legacy_container:$skill_path" "$legacy_backup/legacy-skills" 2>/dev/null; then break; fi
  done
  [[ -f $legacy_backup/data/operation-center.sqlite3 || -f $legacy_backup/data/license-center.sqlite3 ]] || die 'Legacy volume has no known SQLite database; migration stopped.'
  [[ ! -f $root/shared/data/operation-center.sqlite3 ]] || die 'Native database already exists; refusing to overwrite it with container data.'
  cp -a "$legacy_backup/data/." "$root/shared/data/"
  if [[ ! -f $root/shared/data/operation-center.sqlite3 ]]; then
    for suffix in '' -wal -shm; do
      [[ ! -f $root/shared/data/license-center.sqlite3$suffix ]] || cp -a "$root/shared/data/license-center.sqlite3$suffix" "$root/shared/data/operation-center.sqlite3$suffix"
    done
  fi
  if [[ -d $legacy_backup/legacy-skills ]]; then
    mkdir -p "$root/shared/data/runtime"
    [[ -d $root/shared/data/runtime/skills ]] || cp -a "$legacy_backup/legacy-skills" "$root/shared/data/runtime/skills"
  fi
  chown -R yaya-operation-center:yaya-operation-center "$root/shared/data"
fi
for port in "$web_port" "$api_port"; do
  [[ -z $(ss -H -ltn "sport = :$port") ]] || die "Host port $port is in use; no process was killed."
done

# With all platform writers stopped, copying SQLite + WAL and the other files is
# consistent. Shared data includes encryption keys, uploads and monitor identity.
backup="$root/backups/$release_id"
install -d -m 0700 "$backup" "$backup/units"
tar -czpf "$backup/shared.tar.gz" -C "$root" shared
printf '%s\n' "$previous_release" > "$backup/previous-release"
printf '%s\n' "$root" > "$backup/install-root"
for unit in "${units[@]}"; do
  [[ ! -f /etc/systemd/system/$unit ]] || cp -p "/etc/systemd/system/$unit" "$backup/units/$unit"
done
touch "$backup/complete"

# From this point a failed upgrade must restore the data snapshot too: startup
# may have applied schema migrations. Never just restart an old binary on it.
activated=true
if [[ $initialize == true || -n $legacy_container ]]; then
  for secret in private.pem public.pem admin-password.txt; do
    install -m 0640 -o root -g yaya-operation-center "$stage/secrets/$secret" "$root/shared/secrets/$secret"
  done
fi
install -m 0600 "$release/config/operation-center.env" "$root/shared/operation-center.env"
install -m 0600 "$release/config/file-agent.env" "$root/shared/file-agent.env"
for unit in "${units[@]}"; do
  install -m 0644 "$release/config/$unit" "/etc/systemd/system/$unit"
done
ln -s "$release" "$root/.current-$release_id"
mv -Tf "$root/.current-$release_id" "$root/current"
systemctl daemon-reload
systemctl enable yaya-operation-center.target
systemctl reset-failed "${units[@]}" || true
systemctl start yaya-operation-center.target
ready=false
for ((attempt=0; attempt<30; attempt++)); do
  if runuser -u yaya-operation-center -- env YAYA_FILE_AGENT_SOCKET=/run/yaya-file-agent/files.sock "$python_bin" "$release/deploy/host/health.py" --api-port "$api_port" --web-port "$web_port" >/dev/null 2>&1; then
    ready=true
    break
  fi
  sleep 2
done
[[ $ready == true ]] || die 'Native API, web proxy or host-monitor health check failed; inspect journalctl for the platform units.'
runuser -u yaya-operation-center -- "$docker_bin" --host unix:///var/run/docker.sock info >/dev/null
printf 'Installed native release: %s\nBackup: %s\n' "$release" "$backup"
printf 'Management UI: %s\n' "$(python3 "$host_dir/configure.py" get "$root/shared/operation-center.env" YAYA_OPERATION_CENTER_WEB_ORIGIN)"
systemctl --no-pager --plain is-active "${units[@]}"
if [[ -n $legacy_container ]]; then
  # Remove the old platform container only after native health checks succeed.
  # Its Docker volume, image, source directory and cold migration backup remain.
  docker rm "$legacy_container" >/dev/null
  printf 'Legacy platform container removed; original volume and migration backup retained: %s\n' "$legacy_backup"
fi
for secret in private.pem public.pem admin-password.txt; do
  [[ ! -f $stage/secrets/$secret ]] || rm -f -- "$stage/secrets/$secret"
done
rm -f -- "$stage/source.tar.gz"
