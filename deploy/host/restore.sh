#!/usr/bin/env bash
# Explicit operator action: restore the matching native release AND shared data.
set -Eeuo pipefail
if [[ -f /opt/yaya-native-runtime/environment.sh ]]; then source /opt/yaya-native-runtime/environment.sh; fi
umask 077
die() { printf '%s\n' "$*" >&2; exit 1; }
[[ $# == 2 && $(id -u) == 0 ]] || die 'Usage (root): bash restore.sh INSTALL_ROOT BACKUP_DIRECTORY'
root=$1 backup=$2
host_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
python3 "$host_dir/configure.py" validate-root "$root" >/dev/null
[[ $(readlink -f "$backup") == "$backup" && $backup == "$root/backups/"* && -f $backup/complete ]] || die 'Not a complete backup within this installation.'
[[ $(cat "$backup/install-root") == "$root" ]] || die 'Backup belongs to another installation.'
previous_release=$(cat "$backup/previous-release")
[[ -n $previous_release && $previous_release == "$root/releases/"* && -d $previous_release && $(readlink -f "$previous_release") == "$previous_release" ]] || die 'Backup has no available previous native release; first-install recovery requires manual inspection.'
exec 9>/run/lock/yaya-operation-center-publish.lock
flock -n 9 || die 'Another installation or restore is in progress.'
units=(yaya-operation-center.target yaya-host-monitor.service yaya-operation-center-api.service yaya-operation-center-web.service yaya-file-agent.service)
for unit in "${units[@]}"; do
  [[ -f $backup/units/$unit || $unit == yaya-file-agent.service ]] || die "Backup is missing the previous service definition: $unit"
  grep -Fxq "# YAYA_INSTALL_ROOT=$root" "/etc/systemd/system/$unit" || die 'Current services belong to another installation.'
done
# Extract only the archive produced by install.sh; reject traversal and links
# that could make a root extraction write outside the staging directory.
python3 - "$backup/shared.tar.gz" <<'PY'
import sys
import tarfile
from pathlib import PurePosixPath
with tarfile.open(sys.argv[1], 'r:gz') as archive:
    for member in archive:
        path = PurePosixPath(member.name)
        if path.is_absolute() or '..' in path.parts or not path.parts or path.parts[0] != 'shared':
            raise SystemExit('Unsafe backup path')
        if not (member.isfile() or member.isdir()):
            raise SystemExit('Backup links/devices require manual recovery')
PY
stamp="$(date -u +%Y%m%d%H%M%S)-$$"
restore_dir="$root/.restore-$stamp"
mkdir -m 0700 "$restore_dir"
tar -xzpf "$backup/shared.tar.gz" -C "$restore_dir"
[[ -f $restore_dir/shared/operation-center.env && -d $restore_dir/shared/data ]] || die 'Backup lacks configuration or data.'
systemctl stop "${units[@]}"
for unit in "${units[@]:1}"; do
  if systemctl is-active --quiet "$unit"; then die "Service is still running: $unit"; fi
done
# Retain the failed/newer data in full. Restore is explicit because it reverts
# platform records to the snapshot; external customer containers are untouched.
mv "$root/shared" "$root/failed-shared-$stamp"
mv "$restore_dir/shared" "$root/shared"
for unit in "${units[@]}"; do
  if [[ -f $backup/units/$unit ]]; then
    install -m 0644 "$backup/units/$unit" "/etc/systemd/system/$unit"
  elif [[ $unit == yaya-file-agent.service ]]; then
    # A backup from before file-agent was introduced has no corresponding unit.
    rm -f -- /etc/systemd/system/yaya-file-agent.service
  fi
done
ln -s "$previous_release" "$root/.current-restore-$stamp"
mv -Tf "$root/.current-restore-$stamp" "$root/current"
systemctl daemon-reload
systemctl reset-failed "${units[@]}" || true
systemctl start yaya-operation-center.target
api_port=$(python3 "$host_dir/configure.py" get "$root/shared/operation-center.env" YAYA_OPERATION_CENTER_PORT)
web_port=$(python3 "$host_dir/configure.py" get "$root/shared/operation-center.env" PORT)
ready=false
for ((attempt=0; attempt<30; attempt++)); do
  file_socket=$(python3 "$host_dir/configure.py" get "$root/shared/operation-center.env" YAYA_FILE_AGENT_SOCKET)
  if runuser -u yaya-operation-center -- env YAYA_FILE_AGENT_SOCKET="$file_socket" python3 "$previous_release/deploy/host/health.py" --api-port "$api_port" --web-port "$web_port" >/dev/null 2>&1; then
    ready=true
    break
  fi
  sleep 2
done
[[ $ready == true ]] || die 'Restore was applied but service health checks failed; inspect systemd journals.'
printf 'Restored %s\nNewer data retained at %s\n' "$previous_release" "$root/failed-shared-$stamp"
