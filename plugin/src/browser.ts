import { toaster } from "@decky/api";
import { Navigation } from "@decky/ui";
import { api, Source } from "./api";
import { gameId } from "./steam";

declare const SteamClient: any;
declare const appStore: any;

/** A browser download Mercury is waiting for. Kept in localStorage so a plugin reload does not lose it. */
export type Pending = { appid: number; name: string; source: Source; since: number; shortcut: number; update?: boolean };
const KEY = "mercury.pendingBrowserDownload";
export let pending: Pending | undefined = (() => {
  try { return JSON.parse(localStorage.getItem(KEY) ?? "null") ?? undefined; } catch { return undefined; }
})();
function setPending(p: Pending | undefined) {
  pending = p;
  try { p ? localStorage.setItem(KEY, JSON.stringify(p)) : localStorage.removeItem(KEY); } catch { /* storage unavailable */ }
}

/** Progress of the download Firefox is writing right now, for the Downloads page. */
export let progress: { bytes: number; total: number; since: number } | undefined;
let navigatedFor = 0;
let firstSeen = 0;

/** Back to the Firefox window; it keeps running while Mercury's pages are in front. */
export function backToBrowser() {
  if (pending) SteamClient.Apps.RunGame(gameId(pending.shortcut), "", -1, 100);
}

/** Lines prefixed [Mercury] end up in ~/.local/share/Steam/logs/cef_log.txt on the Odin. */
const log = (...a: unknown[]) => console.log("[Mercury] browser download:", ...a);

const SAFE_URL = /^https:\/\/[A-Za-z0-9\-._~:\/?#\[\]@!&()*+,;=%]+$/;

/** SteamRIP's own page for a game. MegaDB and a few other hosts only serve visitors who come from steamrip.com, so the
 * download starts from there (a real click on its button). Titles with punctuation WordPress rewrites in its addresses
 * go to the site's search instead of a guessed address. */
export function steamripPage(sourceName: string): string {
  const title = sourceName.replace(/\s*\([^)]*\)\s*$/, "").trim();
  if (/^[A-Za-z0-9 ]+$/.test(title)) return `https://steamrip.com/${title.toLowerCase().replace(/ +/g, "-")}/`;
  return `https://steamrip.com/?s=${encodeURIComponent(title.replace(/\s*free download\s*$/i, "")).replace(/'/g, "%27")}`;
}

/** Steam Input layout for the browser shortcut: the stock Steam Deck "Web Browser" layout. The right stick moves the
 * pointer, the left stick scrolls, the triggers click, a face button opens the on-screen keyboard. Without it Firefox sees
 * a plain gamepad and nothing but the touchscreen works. Only Deck-family controllers (the Deck, the Odin) have it. */
function applyBrowserLayout(shortcut: number) {
  const store = (window as any).controllerConfiguratorStore;
  const controllers: any[] = (window as any).ControllerStore?.m_controllerList ?? [];
  for (const c of controllers) {
    if (c.unVendorID !== 0x28de || ![0x1205, 0x12f0].includes(c.unProductID)) continue;
    try { store?.SetActiveConfigForApp(shortcut, c.nControllerIndex, "template://controller_neptune_webbrowser.vdf", false); }
    catch (e: any) { log("could not set the browser controller layout:", e?.message); }
  }
}

/** The "Mercury Browser" shortcut runs Firefox natively. Steam's own browser cannot save downloads. */
async function browserShortcut(): Promise<number> {
  const cfg = await api.config();
  const id = cfg.browser_shortcut_id ?? 0;
  if (id && appStore?.GetAppOverviewByAppID?.(id)) return id;
  const { browser } = await api.status();
  if (!browser) throw new Error("Firefox is not installed. Install it from Discover (desktop mode), then try again.");
  const exe = browser.exe;
  const created = Number(await SteamClient.Apps.AddShortcut("Mercury Browser", exe, exe.slice(0, exe.lastIndexOf("/") + 1), ""));
  if (!created) throw new Error("Steam did not create the browser shortcut");
  SteamClient.Apps.SetShortcutName(created, "Mercury Browser");
  await api.saveConfig({ browser_shortcut_id: created });
  return created;
}

/** Open the source's page in Firefox and wait for the user to download the file there. */
export async function downloadInBrowser(appid: number, name: string, source: Source, update = false) {
  const direct = source.url ?? source.urls?.[0];
  // SteamRIP sources open SteamRIP's own game page, where the host buttons are.
  const url = direct && source.provider === "SteamRIP" ? steamripPage(source.name) : direct;
  // The URL lands in a Steam launch option, which Steam runs through /bin/sh.
  if (!url || !SAFE_URL.test(url)) throw new Error("This source has no usable link");
  const id = await browserShortcut();
  applyBrowserLayout(id);
  const gid = gameId(id);
  try { SteamClient.Apps.TerminateApp(gid, false); } catch { /* not running */ }
  const { browser } = await api.status();
  // The Flatpak Firefox (SteamOS) runs as "flatpak run org.mozilla.firefox <url>".
  SteamClient.Apps.SetShortcutLaunchOptions(id, `MOZ_ENABLE_WAYLAND=0 %command% ${browser?.args ? browser.args + " " : ""}"${url}"`);
  setPending({ appid, name, source, since: Math.floor(Date.now() / 1000) - 5, shortcut: id, update });
  log("waiting for", name, "from", url);
  await new Promise((r) => setTimeout(r, 800));
  SteamClient.Apps.RunGame(gid, "", -1, 100);
}

export function cancelBrowserDownload() { setPending(undefined); progress = undefined; navigatedFor = 0; firstSeen = 0; }

let lastSize = new Map<string, number>();
let failures = 0;

/** Called by the background watcher. Starts the install once a finished archive has stopped growing.
 * The waiting state is cleared only after the engine accepts the job, so a failed attempt is retried. */
export async function checkBrowserDownload() {
  const p = pending;
  if (!p) return;
  // A download is running: show its progress in Mercury, and a few seconds after it starts take the user back to Mercury.
  // Firefox keeps downloading in the background, and "Back to Firefox" on the Downloads page returns to it.
  try {
    const parts = await api.browserProgress(p.since);
    if (parts.length) {
      progress = { bytes: parts.reduce((n, x) => n + x.size, 0), total: p.source.size_bytes || 0, since: p.since };
      if (!firstSeen) firstSeen = Date.now();
      if (navigatedFor !== p.since && Date.now() - firstSeen > 6000) {
        navigatedFor = p.since;
        Navigation.Navigate("/mercury/downloads");
        toaster.toast({ title: "Mercury", body: `Downloading ${p.name} in Firefox. Mercury installs it when it finishes.` });
      }
    } else { progress = undefined; firstSeen = 0; }
  } catch { /* the file check below reports engine problems */ }
  let files;
  try { files = await api.browserDownloads(p.since); } catch (e: any) { log("engine not reachable:", e.message); return; }
  const archives = files.filter((f) => f.archive);
  if (!archives.length || archives.some((f) => !f.finished)) return;
  const pick = archives.find((f) => /\.part0*1\.rar$|\.001$/i.test(f.name)) ?? archives[archives.length - 1];
  // Two polls in a row with the same size, so a download Firefox has just renamed is complete on disk.
  const prev = lastSize.get(pick.path);
  lastSize.set(pick.path, pick.size);
  if (prev !== pick.size || !pick.size) return;
  log("file finished:", pick.name, pick.size, "bytes; starting install");
  try {
    await api.install(p.appid, p.name, p.source, pick.path, p.update ?? false);
  } catch (e: any) {
    failures++;
    log(`install request failed (attempt ${failures}):`, e.message);
    if (failures === 1 || failures % 15 === 0) toaster.toast({ title: "Mercury", body: `Waiting to install ${p.name}: ${e.message}` });
    if (failures >= 30) {
      toaster.toast({ title: "Mercury", body: `Gave up installing ${p.name} from ${pick.name}. It is still in Downloads.` });
      log("gave up after", failures, "attempts");
      setPending(undefined); failures = 0; lastSize = new Map();
    }
    return;
  }
  log("install started for", p.name);
  setPending(undefined); failures = 0; lastSize = new Map(); progress = undefined; firstSeen = 0; navigatedFor = 0;
  // The file is Mercury's now, so take the user back to Steam.
  try { SteamClient.Apps.TerminateApp(gameId(p.shortcut), false); } catch { /* already closed */ }
  toaster.toast({ title: "Mercury", body: `Got ${pick.name}. Installing ${p.name} now.` });
}
