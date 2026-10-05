"""Save a Real-Debrid API token (read from stdin) into Mercury for Windows. Checks it with Real-Debrid first.
Run by scripts/set-rd-key-pc.sh from the Mac; the token never appears on a command line."""
import json, os, sys, urllib.error, urllib.request
key = sys.stdin.read().strip()
try:
    req = urllib.request.Request("https://api.real-debrid.com/rest/1.0/user", headers={"Authorization": "Bearer " + key})
    u = json.load(urllib.request.urlopen(req, timeout=20))
except urllib.error.HTTPError as e:
    sys.exit(f"Real-Debrid rejected the token (HTTP {e.code}). Nothing was saved.")
try:
    # Engine running: save through it so its in-memory settings change too.
    r = urllib.request.Request("http://127.0.0.1:47800/config", method="PUT", data=json.dumps({"rd_key": key}).encode(), headers={"Content-Type": "application/json"})
    saved = json.load(urllib.request.urlopen(r, timeout=10)).get("rd_key_set")
    how = "through the running engine"
except OSError:
    # Engine not running: write the settings file it reads on start.
    p = os.path.join(os.environ["APPDATA"], "Mercury", "config.json")
    cfg = json.load(open(p, encoding="utf-8")) if os.path.exists(p) else {}
    cfg["rd_key"] = key
    os.makedirs(os.path.dirname(p), exist_ok=True)
    tmp = p + ".tmp"; json.dump(cfg, open(tmp, "w", encoding="utf-8"), indent=2); os.replace(tmp, p)
    saved, how = True, "into %APPDATA%\\Mercury\\config.json"
if not saved: sys.exit("Token is valid, but Mercury did not save it.")
print(f"Saved {how}. Real-Debrid account: {u.get('username')} | {u.get('type')} until {str(u.get('expiration'))[:10]}")
