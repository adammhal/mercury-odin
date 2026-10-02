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
import { Import } from "./pages/Import";
import { Search } from "./pages/Search";
import { Settings } from "./pages/Settings";
import { addToSteam, onAppExit, shortcutExists } from "./steam";
import { Navigation } from "@decky/ui";
import { checkBrowserDownload } from "./browser";
import { stopStoreButton, tickStoreButton } from "./storeButton";
import { patchLibraryPage } from "./libraryBadge";

const ROUTES: [string, FC][] = [
  ["/mercury", Home], ["/mercury/game/:appid", Game], ["/mercury/downloads", Downloads],
  ["/mercury/library", Library], ["/mercury/import", Import], ["/mercury/search", Search], ["/mercury/settings", Settings],
];

/** Background work that must happen even when no Mercury page is open. */
function startWatcher() {
  const busy = new Set<number>();
  const attempts = new Map<number, number>();
  const seen = new Map<number, string>();
  let jobs: Job[] = [];
  const tick = async () => {
    try { jobs = await api.jobs(); } catch { return; }
    for (const j of jobs) {
      const prev = seen.get(j.id);
      seen.set(j.id, j.state);
      if (j.state === "ready" && !busy.has(j.id) && (attempts.get(j.id) ?? 0) < 5) {
        busy.add(j.id);
        const n = (attempts.get(j.id) ?? 0) + 1;
        attempts.set(j.id, n);
        addToSteam(j).then(
          () => toaster.toast({ title: "Mercury", body: `${j.name} is in your Steam library` }),
          (e) => {
            console.log("[Mercury] add to Steam failed", j.name, `attempt ${n}:`, e.message);
            if (n >= 5) toaster.toast({ title: "Mercury", body: `Could not add ${j.name} to Steam: ${e.message}` });
          },
        ).finally(() => busy.delete(j.id));
      }
      if (prev && prev !== j.state) {
        if (j.state === "needs_setup") toaster.toast({ title: "Mercury", body: `${j.name} downloaded. Run its installer from Downloads.` });
        if (j.state === "failed") toaster.toast({ title: "Mercury", body: `${j.name} failed: ${j.error ?? "unknown error"}` });
      }
    }
  };
  // A game whose shortcut the user deleted in Steam leaves its files behind; offer to clean up.
  const missingSince = new Map<number, number>();
  const announced = new Set<number>(JSON.parse(localStorage.getItem("mercury.removedAnnounced") ?? "[]"));
  let n = 0;
  const checkRemoved = async () => {
    if (++n % 15) return;
    let lib;
    try { lib = await api.library(); } catch { return; }
    for (const e of lib) {
      const exists = shortcutExists(e.shortcut_id);
      if (exists !== false) { missingSince.delete(e.appid); announced.delete(e.appid); continue; }
      const first = missingSince.get(e.appid) ?? Date.now();
      missingSince.set(e.appid, first);
      // Two checks in a row (30 s apart) before believing it, and one notification per removal.
      if (Date.now() - first < 25000 || announced.has(e.appid)) continue;
      announced.add(e.appid);
      toaster.toast({ title: "Mercury", body: `${e.name} was removed from Steam. Its files are still installed. Tap to clean up.`,
        onClick: () => Navigation.Navigate("/mercury/library") } as any);
    }
    localStorage.setItem("mercury.removedAnnounced", JSON.stringify([...announced]));
  };
  // Once a day, look for newer releases of installed games and say so once per version.
  const checkUpdates = async () => {
    const last = Number(localStorage.getItem("mercury.updatesCheckedAt") ?? 0);
    if (Date.now() - last < 24 * 3600 * 1000) return;
    localStorage.setItem("mercury.updatesCheckedAt", String(Date.now()));
    const told: Record<string, string> = JSON.parse(localStorage.getItem("mercury.updatesAnnounced") ?? "{}");
    let lib;
    try { lib = await api.library(); } catch { return; }
    for (const e of lib) {
      try {
        const u = await api.updateCheck(e.appid);
        const v = u.newer?.version;
        if (!v || told[e.appid] === v) continue;
        told[e.appid] = v;
        toaster.toast({ title: "Mercury", body: `Update available for ${e.name}: ${v}`, onClick: () => Navigation.Navigate(`/mercury/game/${e.appid}`) } as any);
      } catch { /* engine busy or source down; try tomorrow */ }
    }
    localStorage.setItem("mercury.updatesAnnounced", JSON.stringify(told));
  };
  setTimeout(checkUpdates, 60000);
  const daily = setInterval(checkUpdates, 3600 * 1000);
  const timer = setInterval(() => { tick(); checkBrowserDownload(); tickStoreButton(); checkRemoved(); }, 2000);
  // When a repack installer closes, look for the installed game and add it.
  const stopExit = onAppExit(async (appid) => {
    const j = jobs.find((x) => x.state === "installing" && x.shortcut_id === appid);
    if (!j) return;
    try { await api.act(j.id, "setup-done"); }
    catch (e: any) { toaster.toast({ title: "Mercury", body: e.message }); }
  });
  return () => { clearInterval(timer); clearInterval(daily); stopExit(); stopStoreButton(); };
}

export default definePlugin(() => {
  for (const [path, C] of ROUTES) routerHook.addRoute(path, C, { exact: true });
  const libraryPatch = patchLibraryPage();
  const stop = startWatcher();
  return {
    name: "Mercury",
    titleView: <div className={staticClasses.Title}>Mercury</div>,
    content: <Panel />,
    icon: <FaMeteor />,
    onDismount() {
      stop();
      for (const [path] of ROUTES) routerHook.removeRoute(path);
      routerHook.removePatch("/library/app/:appid", libraryPatch);
    },
  };
});

