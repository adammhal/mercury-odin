#!/usr/bin/env bash
# Build an installable Decky zip: out/Mercury.zip with the plugin, mercuryd and unrar (aarch64).
# Runs in Docker (fedora:44, arm64) so it works from a Mac or any Linux machine with Docker.
# Install on the device: Decky, Settings, Developer, Install plugin from zip.
set -euo pipefail
cd "$(dirname "$0")/.."
UNRAR_VERSION=7.3.1
mkdir -p out
docker run --rm --platform linux/arm64 -v "$PWD":/src -v mercury-cargo:/root/.cargo -v mercury-target:/src/mercuryd/target \
  -v mercury-node:/src/plugin/node_modules -e npm_config_store_dir=/root/.pnpm-store -w /src fedora:44 bash -euc "
  dnf install -y -q gcc gcc-c++ make cargo nodejs zip curl >/dev/null 2>&1
  (cd mercuryd && cargo build --release --quiet)
  (cd plugin && npx -y pnpm@9 install --silent >/dev/null && npx -y pnpm@9 run build >/dev/null 2>&1)
  rm -rf /tmp/unrar && mkdir /tmp/unrar && cd /tmp/unrar
  curl -fsSL https://www.rarlab.com/rar/unrarsrc-$UNRAR_VERSION.tar.gz | tar xz
  make -s -C unrar -j\$(nproc) >/dev/null 2>&1
  rm -rf /tmp/pkg && mkdir -p /tmp/pkg/Mercury/bin /tmp/pkg/Mercury/dist
  cd /src/plugin
  cp plugin.json package.json main.py ../LICENSE /tmp/pkg/Mercury/
  cp dist/index.js /tmp/pkg/Mercury/dist/
  cp /src/mercuryd/target/release/mercuryd /tmp/unrar/unrar/unrar /tmp/pkg/Mercury/bin/
  cp /tmp/unrar/unrar/license.txt /tmp/pkg/Mercury/bin/unrar-license.txt
  strip /tmp/pkg/Mercury/bin/mercuryd /tmp/pkg/Mercury/bin/unrar
  # The released plugin has no hot reload.
  sed -i 's/\"flags\": \[\"debug\"\]/\"flags\": []/' /tmp/pkg/Mercury/plugin.json
  cd /tmp/pkg && rm -f /src/out/Mercury.zip && zip -qr /src/out/Mercury.zip Mercury
"
ls -lh out/Mercury.zip
unzip -l out/Mercury.zip
