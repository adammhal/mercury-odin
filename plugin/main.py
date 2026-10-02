import asyncio
import os
import socket
import subprocess

import decky

PORT = 47800
BIN = os.path.join(decky.DECKY_PLUGIN_DIR, "bin", "mercuryd")
LOG = os.path.join(decky.DECKY_USER_HOME, ".local/share/mercury/mercuryd.log")


def _engine_pids() -> list:
    """Every running mercuryd. Only this plugin starts them, so any it does not own is left over."""
    out = []
    for d in os.listdir("/proc"):
        if not d.isdigit():
            continue
        try:
            with open(f"/proc/{d}/comm") as f:
                if f.read().strip() == "mercuryd":
                    out.append(int(d))
        except OSError:
            pass
    return out


def _stop_pids(pids, timeout=5.0):
    import signal
    import time
    for p in pids:
        try:
            os.kill(p, signal.SIGTERM)
        except OSError:
            pass
    end = time.time() + timeout
    while time.time() < end and any(os.path.exists(f"/proc/{p}") for p in pids):
        time.sleep(0.1)
    for p in pids:
        if os.path.exists(f"/proc/{p}"):
            try:
                os.kill(p, signal.SIGKILL)
            except OSError:
                pass


def _listening() -> bool:
    with socket.socket() as s:
        s.settimeout(0.3)
        return s.connect_ex(("127.0.0.1", PORT)) == 0


class Plugin:
    """Starts the native mercuryd engine, restarts it if it exits, and stops it with the plugin."""

    proc = None
    stopping = False

    def _start(self):
        os.makedirs(os.path.dirname(LOG), exist_ok=True)
        env = {**os.environ, "HOME": decky.DECKY_USER_HOME, "RUST_LOG": "mercuryd=info"}
        self.proc = subprocess.Popen([BIN], stdout=open(LOG, "ab"), stderr=subprocess.STDOUT, env=env, start_new_session=True)
        decky.logger.info(f"started mercuryd pid {self.proc.pid}")

    async def _main(self):
        # An engine left over from a previous plugin instance (for example after a reload) would keep the port
        # and keep running an old binary. Replace it with ours.
        stray = _engine_pids()
        if stray:
            decky.logger.info(f"stopping leftover mercuryd {stray}")
            await asyncio.to_thread(_stop_pids, stray)
        delay = 2
        while not self.stopping:
            if self.proc is None or self.proc.poll() is not None:
                if self.proc is not None:
                    decky.logger.warning(f"mercuryd exited with {self.proc.returncode}; restarting in {delay}s")
                    await asyncio.sleep(delay)
                    delay = min(delay * 2, 60)
                if _listening():
                    stray = [p for p in _engine_pids() if not (self.proc and p == self.proc.pid)]
                    decky.logger.warning(f"port {PORT} held by {stray or 'another process'}; stopping it")
                    await asyncio.to_thread(_stop_pids, stray)
                    await asyncio.sleep(1)
                    continue
                self._start()
            elif delay > 2:
                delay = 2
            await asyncio.sleep(3)

    async def _unload(self):
        self.stopping = True
        pids = _engine_pids()
        await asyncio.to_thread(_stop_pids, pids)
        if self.proc:
            try:
                self.proc.wait(timeout=1)
            except subprocess.TimeoutExpired:
                pass
        decky.logger.info(f"mercuryd stopped {pids}")

    async def store_app(self) -> int:
        """App ID of the Steam store page that is open, or 0."""
        import json
        import re
        import urllib.request
        try:
            tabs = json.load(urllib.request.urlopen("http://127.0.0.1:8080/json", timeout=2))
        except Exception:
            return 0
        for t in tabs:
            m = re.match(r"https://store\.steampowered\.com/app/(\d+)", t.get("url", ""))
            if m:
                return int(m.group(1))
        return 0
