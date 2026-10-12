#!/usr/bin/env bash
# Build out/Mercury-SteamDeck.zip: the Decky plugin for x86_64 SteamOS (Steam Deck), with mercuryd, unrar and
# 7-Zip's standalone 7zz (SteamOS ships no 7-Zip). Binaries are built against Debian 12's glibc (2.36), older than
# SteamOS's, so they run there. Cross-compiled from an arm64 Docker host (a Mac) with Debian's x86-64 toolchain.
# Install on the Deck: Decky, Settings, Developer mode on, Developer, Install Plugin from URL (or from zip).
set -euo pipefail
cd "$(dirname "$0")/.."
UNRAR_VERSION=7.3.1
SEVENZIP=7z2501-linux-x64.tar.xz
mkdir -p out
# The plugin's JavaScript is the same for every machine.
docker run --rm -v "$PWD":/src -v mercury-node:/src/plugin/node_modules -e npm_config_store_dir=/root/.pnpm-store -w /src/plugin \
  fedora:44 bash -euc "dnf install -y -q nodejs >/dev/null 2>&1; npx -y pnpm@9 run build >/dev/null 2>&1"
docker run --rm -v "$PWD":/src -v mercury-cargo-deck:/usr/local/cargo/registry -v mercury-target-deck:/src/mercuryd/target-deck \
  -w /src rust:1-bookworm bash -euc "
  dpkg --add-architecture amd64 >/dev/null
  apt-get update -qq >/dev/null && apt-get install -y -qq gcc-x86-64-linux-gnu g++-x86-64-linux-gnu libc6-dev-amd64-cross zip curl xz-utils make >/dev/null
  rustup target add x86_64-unknown-linux-gnu >/dev/null 2>&1
  export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=x86_64-linux-gnu-gcc CC_x86_64_unknown_linux_gnu=x86_64-linux-gnu-gcc
  (cd mercuryd && cargo build --release --quiet --target x86_64-unknown-linux-gnu --target-dir target-deck)
  rm -rf /tmp/unrar && mkdir /tmp/unrar && cd /tmp/unrar
  curl -fsSL https://www.rarlab.com/rar/unrarsrc-$UNRAR_VERSION.tar.gz | tar xz
  make -s -C unrar -j\$(nproc) CXX=x86_64-linux-gnu-g++ STRIP=x86_64-linux-gnu-strip >/dev/null 2>&1
  mkdir -p /tmp/7z && curl -fsSL https://www.7-zip.org/a/$SEVENZIP | tar xJ -C /tmp/7z
  rm -rf /tmp/pkg && mkdir -p /tmp/pkg/Mercury/bin /tmp/pkg/Mercury/dist
  cd /src/plugin
  cp plugin.json package.json main.py ../LICENSE /tmp/pkg/Mercury/
  cp dist/index.js /tmp/pkg/Mercury/dist/
  cp /src/mercuryd/target-deck/x86_64-unknown-linux-gnu/release/mercuryd /tmp/unrar/unrar/unrar /tmp/7z/7zz /tmp/pkg/Mercury/bin/
  cp /tmp/unrar/unrar/license.txt /tmp/pkg/Mercury/bin/unrar-license.txt
  cp /tmp/7z/License.txt /tmp/pkg/Mercury/bin/7zip-license.txt
  x86_64-linux-gnu-strip /tmp/pkg/Mercury/bin/mercuryd /tmp/pkg/Mercury/bin/unrar
  sed -i 's/\"flags\": \[\"debug\"\]/\"flags\": []/' /tmp/pkg/Mercury/plugin.json
  cd /tmp/pkg && rm -f /src/out/Mercury-SteamDeck.zip && zip -qr /src/out/Mercury-SteamDeck.zip Mercury
"
ls -lh out/Mercury-SteamDeck.zip
unzip -l out/Mercury-SteamDeck.zip
