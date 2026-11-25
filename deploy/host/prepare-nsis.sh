#!/usr/bin/env bash
set -Eeuo pipefail

release=$1
previous_release=$2
docker_bin=$3
stage=$4
configured_compiler=$5
compiler="$release/bin/makensis"

nsis_version() {
  local version
  version=$(runuser -u yaya-operation-center -- "$1" -VERSION 2>/dev/null) || return 1
  [[ $version =~ ^v?(3\.[0-9]+(\.[0-9]+)?)(-[0-9][[:alnum:].+~:-]*)?$ ]] || return 1
  printf '%s\n' "${BASH_REMATCH[1]}"
}

version_at_least() {
  printf '%s\n%s\n' "$2" "$1" | LC_ALL=C sort -sCV
}

install_latest_nsis() {
  local installed_version
  # APT verifies the Debian archive. Keep its compiler and loader together so
  # the API can run them on the host without Docker, including on CentOS 7.
  "$docker_bin" --host unix:///var/run/docker.sock run --rm -i debian:sid-slim sh -se > "$stage/nsis.tar" <<'EXPORT' || return 1
export DEBIAN_FRONTEND=noninteractive
apt-get update >&2
apt-get install -y --no-install-recommends nsis >&2
mkdir -p /export/lib
cp /usr/bin/makensis /export/
cp -a /usr/share/nsis /export/share
cp -a /usr/lib/x86_64-linux-gnu/gconv /export/gconv
dependencies=$(ldd /usr/bin/makensis)
if printf '%s\n' "$dependencies" | grep -q 'not found'; then
  printf 'NSIS has unresolved shared library dependencies.\n' >&2
  exit 1
fi
for library in $(printf '%s\n' "$dependencies" | awk '$1 ~ /^\// {print $1} $3 ~ /^\// {print $3}'); do
  cp -L "$library" /export/lib/
done
tar -C /export -cf - .
EXPORT

  mkdir -p "$release/bin/nsis" || return 1
  tar --no-same-owner -xf "$stage/nsis.tar" -C "$release/bin/nsis" || return 1
  cat > "$compiler" <<'WRAPPER' || return 1
#!/bin/sh
set -eu
nsis_dir=$(dirname -- "$0")/nsis
export NSISDIR="$nsis_dir/share"
export GCONV_PATH="$nsis_dir/gconv"
for loader in "$nsis_dir"/lib/ld-linux*.so.*; do
  if [ -x "$loader" ]; then
    exec "$loader" --library-path "$nsis_dir/lib" "$nsis_dir/makensis" "$@"
  fi
done
printf 'NSIS dynamic loader is missing.\n' >&2
exit 1
WRAPPER
  chmod -R a+rX "$release/bin/nsis" || return 1
  chmod 0755 "$compiler" || return 1
  installed_version=$(nsis_version "$compiler") || return 1
  if ! version_at_least "$installed_version" "$latest_version"; then
    printf 'Debian provides NSIS %s, but upstream has released %s.\n' "$installed_version" "$latest_version" >&2
    return 1
  fi
  printf 'Installed NSIS %s.\n' "$installed_version"
}

previous_compiler=''
if [[ -n $previous_release ]]; then
  previous_compiler="$previous_release/bin/makensis"
fi
best_compiler=''
best_version=''
for candidate in "$configured_compiler" "$(command -v makensis || true)" "$previous_compiler"; do
  [[ -n $candidate ]] || continue
  if version=$(nsis_version "$candidate") &&
      { [[ -z $best_version ]] || ! version_at_least "$best_version" "$version"; }; then
    best_compiler=$candidate
    best_version=$version
  fi
done

if upstream=$(curl -fsSL --connect-timeout 10 --max-time 25 https://nsis.sourceforge.io/Download) &&
    [[ $upstream =~ nsis-(3\.[0-9]+(\.[0-9]+)?)-src\.tar\.bz2 ]]; then
  latest_version=${BASH_REMATCH[1]}
  printf 'Latest stable NSIS version: %s.\n' "$latest_version"
  if [[ -z $best_version ]] || ! version_at_least "$best_version" "$latest_version"; then
    printf 'Preparing NSIS %s for this release (currently %s).\n' "$latest_version" "${best_version:-missing}"
    if install_latest_nsis; then
      rm -f -- "$stage/nsis.tar"
      exit 0
    fi
    rm -f -- "$stage/nsis.tar" "$compiler"
    rm -rf -- "$release/bin/nsis"
    printf 'Could not install the latest NSIS release.\n' >&2
  fi
else
  printf 'Could not check the latest stable NSIS version.\n' >&2
fi

if [[ -z $best_compiler ]]; then
  printf 'No usable NSIS 3 compiler is available.\n' >&2
  exit 1
fi
if [[ $best_compiler == "$previous_compiler" && -d $previous_release/bin/nsis ]]; then
  cp -a "$previous_release/bin/nsis" "$previous_compiler" "$release/bin/"
else
  # Resolve links in earlier releases so that later release cleanup is safe.
  ln -s "$(readlink -f -- "$best_compiler")" "$compiler"
fi
printf 'Reusing NSIS %s.\n' "$best_version"
