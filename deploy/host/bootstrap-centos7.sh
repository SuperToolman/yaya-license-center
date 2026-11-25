#!/usr/bin/env bash
# Optional, explicit bootstrap for the existing x86_64 CentOS 7 host.
# Leaves /usr/bin/python, system glibc and the host's other services unchanged.
set -Eeuo pipefail
[[ $(id -u) == 0 && $(uname -m) == x86_64 ]] || exit 1
runtime=/opt/yaya-native-runtime
install -d -m 0755 "$runtime" "$runtime/downloads" "$runtime/bin"
cd "$runtime/downloads"
node_archive=node-v22.22.0-linux-x64-glibc-217.tar.xz
if [[ ! -x $runtime/node/bin/node ]]; then
  if ! printf '%s  %s\n' db4a1d582e6fffcf7fb348149ca4ac8fa685699c5bc46cd7e22bbf9a7e673454 "$node_archive" | sha256sum -c - >/dev/null 2>&1; then
    curl -fL --retry 3 --connect-timeout 20 "https://registry.npmmirror.com/-/binary/node-unofficial-builds/v22.22.0/$node_archive" -o "$node_archive"
  fi
  printf '%s  %s\n' db4a1d582e6fffcf7fb348149ca4ac8fa685699c5bc46cd7e22bbf9a7e673454 "$node_archive" | sha256sum -c -
  tar -xJf "$node_archive" -C "$runtime"
  mv "$runtime/node-v22.22.0-linux-x64-glibc-217" "$runtime/node"
  chown -R root:root "$runtime/node"
fi
"$runtime/node/bin/node" --version
for tool in node npm npx corepack; do
  [[ ! -e $runtime/node/bin/$tool ]] || ln -sfn "$runtime/node/bin/$tool" "$runtime/bin/$tool"
done
export PATH="$runtime/bin:$runtime/node/bin:$PATH"
if [[ ! -x $runtime/python/bin/python3 ]]; then
  [[ -s Python-3.11.11.tgz ]] || curl -fL --retry 3 --connect-timeout 20 https://mirrors.huaweicloud.com/python/3.11.11/Python-3.11.11.tgz -o Python-3.11.11.tgz
  printf '%s  %s\n' 883bddee3c92fcb91cf9c09c5343196953cbb9ced826213545849693970868ed Python-3.11.11.tgz | sha256sum -c -
  tar -xzf Python-3.11.11.tgz
  (
    cd Python-3.11.11
    ./configure --prefix="$runtime/python" --without-ensurepip
    make -j4
    make install
  )
fi
ln -sfn "$runtime/python/bin/python3" "$runtime/bin/python3"
"$runtime/bin/python3" --version
if [[ ! -x $runtime/bin/pnpm ]]; then
  npm install --global --prefix "$runtime/pnpm" pnpm@11.1.2 --registry=https://registry.npmmirror.com
  ln -sfn "$runtime/pnpm/bin/pnpm" "$runtime/bin/pnpm"
fi
export RUSTUP_HOME="$runtime/rustup" CARGO_HOME="$runtime/cargo"
if [[ ! -x $CARGO_HOME/bin/cargo ]]; then
  export RUSTUP_DIST_SERVER=https://rsproxy.cn
  export RUSTUP_UPDATE_ROOT=https://rsproxy.cn/rustup
  curl -fL --retry 3 --connect-timeout 20 https://rsproxy.cn/rustup-init.sh -o rustup-init.sh
  sh rustup-init.sh -y --profile minimal --default-toolchain stable --no-modify-path
fi
for tool in cargo rustc rustup; do ln -sfn "$CARGO_HOME/bin/$tool" "$runtime/bin/$tool"; done
cat > "$runtime/environment.sh" <<'ENV'
export PATH=/opt/yaya-native-runtime/bin:/opt/yaya-native-runtime/node/bin:$PATH
export RUSTUP_HOME=/opt/yaya-native-runtime/rustup
export CARGO_HOME=/opt/yaya-native-runtime/cargo
export RUSTUP_DIST_SERVER=https://rsproxy.cn
export RUSTUP_UPDATE_ROOT=https://rsproxy.cn/rustup
ENV
printf 'Native runtime ready; source %s/environment.sh before native installation.\n' "$runtime"
