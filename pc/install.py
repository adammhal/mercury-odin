"""Install Mercury for Windows, add it to Steam, and make Steam start in Big Picture with its debug port open. Run on the PC after `npx tauri build --no-bundle`:
    python install.py
Copies Mercury.exe and mercuryd.exe to %LOCALAPPDATA%\\Programs\\Mercury, adds a Steam shortcut for it, and puts a
Big Picture shortcut in the Startup folder. Pass --warmup to also refresh the warmUP tile."""
import base64, os, shutil, sqlite3, subprocess, sys, time

HERE = os.path.dirname(os.path.abspath(__file__))
DEST = os.path.join(os.environ["LOCALAPPDATA"], "Programs", "Mercury")
WARMUP = os.path.join(os.environ["APPDATA"], "dev.warmup.console", "warmup.db")

def js_hash(s):
    h = 0
    for unit in s.encode("utf-16-le").hex(" ", 2).split():
        h = ((h << 5) - h + int.from_bytes(bytes.fromhex(unit), "little")) & 0xFFFFFFFF
    return abs(h - 2**32 if h >= 2**31 else h)

app = os.path.join(HERE, "src-tauri", "target", "release", "mercury.exe")
engine = os.path.join(HERE, "..", "mercuryd", "target", "release", "mercuryd.exe")
for p in (app, engine):
    if not os.path.isfile(p): sys.exit(f"missing build output: {p}")

# A running engine holds mercuryd.exe open; stop it so the new one can be copied in.
subprocess.run(["taskkill", "/IM", "mercuryd.exe", "/F"], capture_output=True)
subprocess.run(["taskkill", "/IM", "Mercury.exe", "/F"], capture_output=True)
time.sleep(1)
os.makedirs(DEST, exist_ok=True)
shutil.copy2(app, os.path.join(DEST, "Mercury.exe"))
shutil.copy2(engine, os.path.join(DEST, "mercuryd.exe"))
print("installed to", DEST)
shutil.copytree(os.path.join(HERE, "art"), os.path.join(DEST, "art"), dirs_exist_ok=True)

# Steam (Big Picture) is the launcher: start it at login in Big Picture, with the debug-port flag file Mercury needs.
def steam_dir():
    for p in (os.path.join(os.environ.get("ProgramFiles(x86)", ""), "Steam"), r"C:\Program Files (x86)\Steam"):
        if os.path.isfile(os.path.join(p, "steam.exe")): return p
steam = steam_dir()
if steam:
    open(os.path.join(steam, ".cef-enable-remote-debugging"), "a").close()
    lnk = os.path.join(os.environ["APPDATA"], "Microsoft", "Windows", "Start Menu", "Programs", "Startup", "Steam Big Picture.lnk")
    subprocess.run(["powershell", "-NoProfile", "-Command",
        f"$s=(New-Object -ComObject WScript.Shell).CreateShortcut('{lnk}'); $s.TargetPath='{steam}\\steam.exe'; $s.Arguments='-bigpicture'; $s.WorkingDirectory='{steam}'; $s.Save()"], check=True)
    print("startup shortcut:", lnk)
    # Start the engine in the logged-in user's desktop session. A process started from SSH lands in a hidden session, and
    # the installers and prompts it opens would be invisible.
    ps = ("$a=New-ScheduledTaskAction -Execute '%s' -WorkingDirectory '%s'; "
          "$p=New-ScheduledTaskPrincipal -UserId \"$env:COMPUTERNAME\\$env:USERNAME\" -LogonType Interactive -RunLevel Limited; "
          "Register-ScheduledTask -TaskName MercuryEngineStart -Action $a -Principal $p -Force | Out-Null; "
          "Start-ScheduledTask -TaskName MercuryEngineStart; Start-Sleep 3; Unregister-ScheduledTask -TaskName MercuryEngineStart -Confirm:$false") % (os.path.join(DEST, "mercuryd.exe"), DEST)
    subprocess.run(["powershell", "-NoProfile", "-Command", ps], check=False)
    import json, urllib.request
    for _ in range(20):
        try: urllib.request.urlopen("http://127.0.0.1:47800/status", timeout=2); break
        except Exception: time.sleep(1)
    try: print("Steam setup:", json.load(urllib.request.urlopen(urllib.request.Request("http://127.0.0.1:47800/launcher/setup", method="POST", data=b"{}", headers={"Content-Type": "application/json"}), timeout=150)))
    except Exception as e: print("Steam setup failed:", e)
else:
    print("Steam not found; skipped the Big Picture setup")
if "--warmup" not in sys.argv: sys.exit(0)

if not os.path.isfile(WARMUP):
    sys.exit("warmUP not found; Mercury is installed but has no warmUP tile")
exe = os.path.join(DEST, "Mercury.exe").replace("\\", "/")
gid = f"manual-{js_hash(exe)}"
uri = lambda f: "data:image/jpeg;base64," + base64.b64encode(open(os.path.join(HERE, "art", f), "rb").read()).decode()
con = sqlite3.connect(WARMUP, timeout=30)
bdir = os.path.join(os.environ["APPDATA"], "Mercury", "warmup-backups"); os.makedirs(bdir, exist_ok=True)
b = sqlite3.connect(os.path.join(bdir, f"warmup-{int(time.time()*1000)}.db")); con.backup(b); b.close()
if con.execute("select 1 from games where id=?", (gid,)).fetchone():
    with con: con.execute("update games set cover_image_url=?, hero_image_url=?, is_installed=1, install_state='installed' where id=?", (uri("cover.jpg"), uri("hero.jpg"), gid))
    print("warmUP tile refreshed:", gid)
else:
    ref = con.execute("select cover_gradient, hero_gradient, max(last_scanned_at) from games where platform='manual'").fetchone()
    with con:
        con.execute("""insert into games (id, title, genre, platform, cover_image_url, hero_image_url, cover_gradient, hero_gradient, accent_color, playtime,
            is_installed, executable_path, description, install_path, install_state, source_kind, last_scanned_at)
            values (?, 'Mercury', 'Utility', 'manual', ?, ?, ?, ?, '#6366f1', 0, 1, ?, ?, ?, 'installed', 'native', ?)""",
            (gid, uri("cover.jpg"), uri("hero.jpg"), ref[0], ref[1], exe,
             "Find PC games, download them through Real-Debrid, and install them straight into warmUP.", DEST.replace("\\", "/"), ref[2]))
    print("warmUP tile added:", gid, "- press library sync in warmUP")
con.close()
