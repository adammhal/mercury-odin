import { staticClasses } from "@decky/ui";
import { definePlugin, routerHook, toaster } from "@decky/api";
import type { FC } from "react";

declare const SteamClient: any;
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
import { Review } from "./pages/Review";
import { EditArt } from "./pages/EditArt";
import { setReviewHandler } from "./review";
import { Navigation } from "@decky/ui";
import { checkBrowserDownload } from "./browser";
import { stopStoreButton, tickStoreButton } from "./storeButton";
import { patchLibraryPage } from "./libraryBadge";

// On the Odin only the plugin can reach Steam's client, so a confirmed review adds the game here,
// with the chosen title and Steam's art plus the user's SteamGridDB picks.
setReviewHandler(async (job, r) => {
  const { assets } = await api.resolveArt(job.appid, r.art);
  await addToSteam(job, job.exe ?? "", { name: r.name, assets });
});

const ROUTES: [string, FC][] = [
  ["/mercury", Home], ["/mercury/game/:appid", Game], ["/mercury/downloads", Downloads],
  ["/mercury/library", Library], ["/mercury/import", Import], ["/mercury/search", Search], ["/mercury/settings", Settings],
  ["/mercury/review/:id", Review], ["/mercury/edit-art/:appid", EditArt],
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
        if (j.state === "review") {
          // Open the Review screen if Mercury is on screen; otherwise (say, mid-game) just tell the user.
          const path = (window as any).SteamUIStore?.WindowStore?.GamepadUIMainWindowInstance?.m_history?.location?.pathname ?? "";
          if (path.startsWith("/mercury")) Navigation.Navigate(`/mercury/review/${j.id}`);
          else toaster.toast({ title: "Mercury", body: `${j.name} is installed. Review its title and art to add it to Steam.`, onClick: () => Navigation.Navigate(`/mercury/review/${j.id}`) } as any);
        }
        if (j.state === "failed") toaster.toast({ title: "Mercury", body: `${j.name} failed: ${j.error ?? "unknown error"}` });
      }
    }
  };
  // A game whose shortcut the user deleted in Steam leaves its files behind; offer to clean up.
  const missingSince = new Map<number, number>();
  const announced = new Set<number>(JSON.parse(localStorage.getItem("mercury.removedAnnounced") ?? "[]"));
  let n = 0;
  const checkRemoved = async () => {
    if (++n % 3) return;
    let lib;
    try { lib = await api.library(); } catch { return; }
    for (const e of lib.filter((x) => x.needs_repoint)) {
      // Mercury moved the game; point the same shortcut (same Proton prefix, same saves) at the new place.
      try {
        await SteamClient.Apps.SetShortcutExe(e.shortcut_id, `"${e.exe}"`);
        await SteamClient.Apps.SetShortcutStartDir(e.shortcut_id, e.exe.slice(0, e.exe.lastIndexOf("/") + 1));
        await api.repointed(e.appid);
        toaster.toast({ title: "Mercury", body: `${e.name} moved` });
      } catch (err: any) { console.log("[Mercury] repoint failed", e.name, err?.message); }
    }
    for (const e of lib.filter((x) => x.launch_options_fix)) {
      try {
        await SteamClient.Apps.SetShortcutLaunchOptions(e.shortcut_id, e.launch_options_fix);
        await api.launchOptionsSet(e.appid);
        console.log("[Mercury] launch options for", e.name, e.launch_options_fix);
      } catch (err: any) { console.log("[Mercury] launch options failed", e.name, err?.message); }
    }
    for (const e of lib) {
      const exists = shortcutExists(e.shortcut_id);
      if (exists !== false) { missingSince.delete(e.appid); announced.delete(e.appid); continue; }
      const first = missingSince.get(e.appid) ?? Date.now();
      missingSince.set(e.appid, first);
      // Missing for 30 s (several checks) before believing it, and one notification per removal.
      if (Date.now() - first < 30000 || announced.has(e.appid)) continue;
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
  const stopBattery = fillChargingTime();
  const stopReconnect = reconnectAfterSleep();
  const stopAutoPerf = performanceWhenPlugged();
  return () => { clearInterval(timer); clearInterval(daily); stopExit(); stopStoreButton(); stopBattery(); stopReconnect(); stopAutoPerf(); };
}

/** Two gaps in what Steam's client reports on this device, filled from the kernel:
 * - charging time: it sends -1 while charging, so Quick Access shows "?h ?m".
 * - full: the battery keeps reporting Charging at 100 % (it trickle-tops off), and Steam caps the header at 99 %
 *   until the state is Full. Report Full once the kernel says 100 % on the charger.
 * Discharging time is Steam's own and left alone. */
function fillChargingTime(): () => void {
  const CHARGING = 2, FULL = 3; // Steam's EBatteryState
  // The last value Mercury put there, kept on window so a reloaded plugin still knows it is ours.
  // Keep refreshing it; never replace a value Steam sent.
  const w = window as any;
  const fill = async () => {
    const s = (window as any).SystemPowerStore;
    if (!s || s.m_eBatteryState !== CHARGING) return;
    try {
      const b = await api.battery();
      if (!b.charging || s.m_eBatteryState !== CHARGING) return;
      if ((b.percent ?? 0) >= 100) { s.m_eBatteryState = FULL; s.m_bSayFull = true; return; }
      if (s.m_nBatterySecondsRemaining >= 0 && s.m_nBatterySecondsRemaining !== w.__mercuryChargeSecs) return;
      if (b.seconds_to_full) s.m_nBatterySecondsRemaining = w.__mercuryChargeSecs = b.seconds_to_full;
    } catch { /* engine restarting */ }
  };
  // Steam overwrites the value on each battery update; refill right after it.
  const h = SteamClient.System.RegisterForBatteryStateChanges(() => setTimeout(fill, 0));
  const t = setInterval(fill, 15000);
  fill();
  return () => { clearInterval(t); h?.unregister?.(); };
}

/** After the Odin wakes, Steam logs back in on its own but the new connection can be half dead: it reports
 * "Logged On" while its requests (cloud sync, stats) keep failing, and Reconnect() is ignored in that state.
 * Going offline and back online gives it a fresh connection. Sleep freezes Steam, so a long gap between timer
 * ticks means the Odin was asleep. */
function reconnectAfterSleep(): () => void {
  let last = Date.now();
  let busy = false;
  const cycle = async () => {
    busy = true;
    try {
      // Wait for Wi-Fi to come back (up to 2 minutes), then give Steam's own logon a moment.
      for (let i = 0; i < 60 && !navigator.onLine; i++) await new Promise((r) => setTimeout(r, 2000));
      await new Promise((r) => setTimeout(r, 15000));
      console.log("[Mercury] woke from sleep; refreshing Steam's connection");
      await SteamClient.User.GoOffline();
      await new Promise((r) => setTimeout(r, 4000));
      await SteamClient.User.GoOnline();
    } catch (e: any) {
      console.log("[Mercury] reconnect after sleep failed", e?.message);
      try { await SteamClient.User.GoOnline(); } catch { /* Steam is reconnecting on its own */ }
    } finally { busy = false; }
  };
  const t = setInterval(() => {
    const now = Date.now();
    if (now - last > 30000 && !busy) cycle();
    last = now;
  }, 5000);
  return () => clearInterval(t);
}

/** Performance profile on a charger or the dock's display; the previous profile back when unplugged, unless the
 * user picked another one meanwhile. Set through Steam's own setting (as Quick Access does): Steam keeps its own copy
 * and pushes it to Armada, so changing Armada's side directly leaves Quick Access wrong and gets overwritten. */
function performanceWhenPlugged(): () => void {
  const KEY = "mercury.profileBeforePlug";
  const current = (): string | undefined => (window as any).settingsStore?.clientSettings?.steamos_platform_performance_profile;
  const setProfile = (name: string) => {
    // CMsgClientSettings field 22010 (steamos_platform_performance_profile), a string, protobuf-encoded.
    const varint = (n: number) => { const o: number[] = []; while (n > 127) { o.push((n & 127) | 128); n >>>= 7; } o.push(n); return o; };
    const s = Array.from(new TextEncoder().encode(name));
    const bytes = [...varint((22010 << 3) | 2), ...varint(s.length), ...s];
    return SteamClient.Settings.SetSetting(btoa(String.fromCharCode(...bytes)));
  };
  let last: boolean | undefined;
  const check = async () => {
    let plugged: boolean;
    try { plugged = (await api.battery()).plugged; } catch { return; }
    const cur = current();
    if (!cur || plugged === last) return;
    if (plugged) {
      if (cur !== "Performance") { localStorage.setItem(KEY, cur); await setProfile("Performance"); console.log("[Mercury] plugged in: Performance"); }
    } else if (last !== undefined) {
      const before = localStorage.getItem(KEY);
      if (before && cur === "Performance") { await setProfile(before); console.log("[Mercury] unplugged:", before); }
      localStorage.removeItem(KEY);
    }
    last = plugged;
  };
  const t = setInterval(check, 3000);
  check();
  return () => clearInterval(t);
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

