import os
import socket
import subprocess

import decky

PORT = 47800
BIN = os.path.join(decky.DECKY_PLUGIN_DIR, "bin", "mercuryd")
LOG = os.path.join(decky.DECKY_USER_HOME, ".local/share/mercury/mercuryd.log")


def _listening() -> bool:
    with socket.socket() as s:
        s.settimeout(0.3)
        return s.connect_ex(("127.0.0.1", PORT)) == 0


class Plugin:
    """Starts the native mercuryd engine and stops it with the plugin. The UI talks to it over HTTP."""

    proc = None

    async def _main(self):
        if _listening():
            decky.logger.info("mercuryd already running")
            return
        os.makedirs(os.path.dirname(LOG), exist_ok=True)
        env = {**os.environ, "HOME": decky.DECKY_USER_HOME, "RUST_LOG": "mercuryd=info"}
        self.proc = subprocess.Popen([BIN], stdout=open(LOG, "ab"), stderr=subprocess.STDOUT, env=env, start_new_session=True)
        decky.logger.info(f"started mercuryd pid {self.proc.pid}")

    async def _unload(self):
        if self.proc and self.proc.poll() is None:
            self.proc.terminate()
            try:
                self.proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.proc.kill()
        decky.logger.info("mercuryd stopped")

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
