import { staticClasses } from "@decky/ui";
import { definePlugin, routerHook, toaster } from "@decky/api";
import type { FC } from "react";
import { FaMeteor } from "react-icons/fa";
import { api, Job } from "./api";
import { Panel } from "./Panel";
import { Downloads } from "./pages/Downloads";
import { Game } from "./pages/Game";
import { Home } from "./pages/Home";
import { Library } from "./pages/Library";
import { Search } from "./pages/Search";
import { Settings } from "./pages/Settings";
import { addToSteam, onAppExit } from "./steam";
import { checkBrowserDownload } from "./browser";
import { stopStoreButton, tickStoreButton } from "./storeButton";

const ROUTES: [string, FC][] = [
  ["/mercury", Home], ["/mercury/game/:appid", Game], ["/mercury/downloads", Downloads],
  ["/mercury/library", Library], ["/mercury/search", Search], ["/mercury/settings", Settings],
];

/** Background work that must happen even when no Mercury page is open. */
function startWatcher() {
  const busy = new Set<number>();
  const seen = new Map<number, string>();
  let jobs: Job[] = [];
  const tick = async () => {
    try { jobs = await api.jobs(); } catch { return; }
    for (const j of jobs) {
      const prev = seen.get(j.id);
      seen.set(j.id, j.state);
      if (j.state === "ready" && !busy.has(j.id)) {
        busy.add(j.id);
        addToSteam(j).then(
          () => toaster.toast({ title: "Mercury", body: `${j.name} is in your Steam library` }),
          (e) => toaster.toast({ title: "Mercury", body: `Could not add ${j.name} to Steam: ${e.message}` }),
        ).finally(() => busy.delete(j.id));
      }
      if (prev && prev !== j.state) {
        if (j.state === "needs_setup") toaster.toast({ title: "Mercury", body: `${j.name} downloaded. Run its installer from Downloads.` });
        if (j.state === "failed") toaster.toast({ title: "Mercury", body: `${j.name} failed: ${j.error ?? "unknown error"}` });
      }
    }
  };
  const timer = setInterval(() => { tick(); checkBrowserDownload(); tickStoreButton(); }, 2000);
  // When a repack installer closes, look for the installed game and add it.
  const stopExit = onAppExit(async (appid) => {
    const j = jobs.find((x) => x.state === "installing" && x.shortcut_id === appid);
    if (!j) return;
    try { await api.act(j.id, "setup-done"); }
    catch (e: any) { toaster.toast({ title: "Mercury", body: e.message }); }
  });
  return () => { clearInterval(timer); stopExit(); stopStoreButton(); };
}

export default definePlugin(() => {
  for (const [path, C] of ROUTES) routerHook.addRoute(path, C, { exact: true });
  const stop = startWatcher();
  return {
    name: "Mercury",
    titleView: <div className={staticClasses.Title}>Mercury</div>,
    content: <Panel />,
    icon: <FaMeteor />,
    onDismount() {
      stop();
      for (const [path] of ROUTES) routerHook.removeRoute(path);
    },
  };
});

