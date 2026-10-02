#!/bin/sh
# Send a Real-Debrid API token from the Mac's clipboard to Mercury on the Odin.
# The token travels only over SSH stdin: never in argv, shell history, or a file on the Mac.
# It is checked with Real-Debrid first and saved only if it works. The clipboard is cleared either way.
# Usage: copy the token from real-debrid.com/apitoken, then run: scripts/set-rd-key.sh [user@host]
set -u
HOST=${1:-armada@192.168.8.240}

token=$(pbpaste | tr -d '[:space:]')
printf '' | pbcopy
case "$token" in
  *[!A-Za-z0-9]*|"") echo "Clipboard did not hold a Real-Debrid token. Copy it from real-debrid.com/apitoken and try again."; exit 1 ;;
esac
if [ ${#token} -lt 30 ]; then echo "Clipboard text was only ${#token} characters; a Real-Debrid token is longer."; exit 1; fi

printf '%s' "$token" | ssh -o BatchMode=yes "$HOST" 'python3 -c "
import json, sys, urllib.request, urllib.error
key = sys.stdin.read().strip()
try:
    req = urllib.request.Request(\"https://api.real-debrid.com/rest/1.0/user\", headers={\"Authorization\": \"Bearer \" + key})
    u = json.load(urllib.request.urlopen(req, timeout=20))
except urllib.error.HTTPError as e:
    sys.exit(f\"Real-Debrid rejected the token (HTTP {e.code}). Nothing was saved.\")
req = urllib.request.Request(\"http://127.0.0.1:47800/config\", method=\"PUT\", data=json.dumps({\"rd_key\": key}).encode(),
    headers={\"Content-Type\": \"application/json\"})
try:
    saved = json.load(urllib.request.urlopen(req, timeout=10))
except Exception as e:
    sys.exit(f\"Token is valid, but Mercury did not save it: {e}. Is the Mercury plugin running?\")
if not saved.get(\"rd_key_set\"):
    sys.exit(\"Token is valid, but Mercury did not save it.\")
print(\"Saved. Real-Debrid account:\", u.get(\"username\"), \"|\", u.get(\"type\"), \"until\", str(u.get(\"expiration\"))[:10])
"'
status=$?
unset token
echo "Clipboard cleared."
exit $status
