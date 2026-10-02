#!/bin/sh
# Send a Real-Debrid API token from the Mac's clipboard to Mercury on the Windows PC.
# The token travels only over SSH stdin: never in argv, shell history, or a file on the Mac.
# Usage: copy the token from real-debrid.com/apitoken, then run: scripts/set-rd-key-pc.sh [user@host]
set -u
HOST=${1:-adamm@100.71.14.107}
token=$(pbpaste | tr -d '[:space:]')
printf '' | pbcopy
case "$token" in
  *[!A-Za-z0-9]*|"") echo "Clipboard did not hold a Real-Debrid token. Copy it from real-debrid.com/apitoken and try again."; exit 1 ;;
esac
if [ ${#token} -lt 30 ]; then echo "Clipboard text was only ${#token} characters; a Real-Debrid token is longer."; exit 1; fi
COPYFILE_DISABLE=1 scp -q "$(dirname "$0")/../pc/set_rd_key.py" "$HOST:AppData/Local/Temp/mercury_set_rd_key.py" || exit 1
printf '%s' "$token" | ssh -o BatchMode=yes "$HOST" 'python "%TEMP%\mercury_set_rd_key.py"'
status=$?
unset token
echo "Clipboard cleared."
exit $status
