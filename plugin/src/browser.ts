import { toaster } from "@decky/api";
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

/** Lines prefixed [Mercury] end up in ~/.local/share/Steam/logs/cef_log.txt on the Odin. */
const log = (...a: unknown[]) => console.log("[Mercury] browser download:", ...a);

const SAFE_URL = /^https:\/\/[A-Za-z0-9\-._~:\/?#\[\]@!&()*+,;=%]+$/;

/** The "Mercury Browser" shortcut runs Firefox natively. Steam's own browser cannot save downloads. */
async function browserShortcut(): Promise<number> {
  const cfg = await api.config();
  const id = cfg.browser_shortcut_id ?? 0;
  if (id && appStore?.GetAppOverviewByAppID?.(id)) return id;
  const created = Number(await SteamClient.Apps.AddShortcut("Mercury Browser", "/usr/bin/firefox", "/var/home/armada/", ""));
  if (!created) throw new Error("Steam did not create the browser shortcut");
  SteamClient.Apps.SetShortcutName(created, "Mercury Browser");
  await api.saveConfig({ browser_shortcut_id: created });
  return created;
}

/** Open the source's page in Firefox and wait for the user to download the file there. */
export async function downloadInBrowser(appid: number, name: string, source: Source, update = false) {
  const url = source.url ?? source.urls?.[0];
  // The URL lands in a Steam launch option, which Steam runs through /bin/sh.
  if (!url || !SAFE_URL.test(url)) throw new Error("This source has no usable link");
  const id = await browserShortcut();
  const gid = gameId(id);
  try { SteamClient.Apps.TerminateApp(gid, false); } catch { /* not running */ }
  SteamClient.Apps.SetShortcutLaunchOptions(id, `MOZ_ENABLE_WAYLAND=0 %command% "${url}"`);
  setPending({ appid, name, source, since: Math.floor(Date.now() / 1000) - 5, shortcut: id, update });
  log("waiting for", name, "from", url);
  await new Promise((r) => setTimeout(r, 800));
  SteamClient.Apps.RunGame(gid, "", -1, 100);
}

export function cancelBrowserDownload() { setPending(undefined); }

let lastSize = new Map<string, number>();
let failures = 0;

/** Called by the background watcher. Starts the install once a finished archive has stopped growing.
 * The waiting state is cleared only after the engine accepts the job, so a failed attempt is retried. */
export async function checkBrowserDownload() {
  const p = pending;
  if (!p) return;
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
  setPending(undefined); failures = 0; lastSize = new Map();
  // The file is Mercury's now, so take the user back to Steam.
  try { SteamClient.Apps.TerminateApp(gameId(p.shortcut), false); } catch { /* already closed */ }
  toaster.toast({ title: "Mercury", body: `Got ${pick.name}. Installing ${p.name} now.` });
}
