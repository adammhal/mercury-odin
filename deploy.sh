#!/bin/sh
# Sync the repo to the Odin, build there in the lsfg-vk-build distrobox, and install the plugin.
# Usage: ./deploy.sh [build|plugin|all]   (default: all)
set -eu
HOST=armada@192.168.8.240
WHAT=${1:-all}
rsync -a --delete --exclude target --exclude node_modules --exclude dist --exclude .git ./ "$HOST:mercury/src/"
if [ "$WHAT" = build ] || [ "$WHAT" = all ]; then
  ssh "$HOST" 'distrobox enter lsfg-vk-build -- bash -lc "cd ~/mercury/src/mercuryd && cargo build --release 2>&1 | grep -E \"^(error|warning: unused)|Finished|-->\" | head -40"'
fi
if [ "$WHAT" = plugin ] || [ "$WHAT" = all ]; then
  ssh "$HOST" 'set -e; cd ~/mercury/src/plugin && distrobox enter lsfg-vk-build -- bash -lc "cd ~/mercury/src/plugin && npx -y pnpm@9 install --silent >/dev/null && npx -y pnpm@9 run build 2>&1 | grep -E \"error|TS[0-9]+|created\" | head -20"
    T=~/homebrew/plugins/mercury
    # $T is owned by armada (created once with sudo), so no root is needed here.
    # Decky re-owns the plugin folder and plugin.json to root on load, but leaves dist/, bin/ and
    # main.py with armada. First install: sudo install -d -o armada -g armada $T, then copy everything.
    [ -w $T/dist ] && [ -w $T/bin ] || { echo "$T/dist or bin not writable; see the first-install note in deploy.sh"; exit 1; }
    cmp -s plugin.json $T/plugin.json || echo "NOTE: plugin.json changed; copy it with sudo"
    cp main.py $T/main.py
    cp dist/index.js $T/dist/
    cp ~/mercury/src/mercuryd/target/release/mercuryd $T/bin/mercuryd.new
    mv -f $T/bin/mercuryd.new $T/bin/mercuryd
    echo installed to $T'
fi
