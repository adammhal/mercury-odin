import { toaster } from "@decky/api";
import { api, Source } from "./api";
import { gameId } from "./steam";

declare const SteamClient: any;
declare const appStore: any;

/** A browser download Mercury is waiting for. Module state: lives as long as the plugin. */
export type Pending = { appid: number; name: string; source: Source; since: number };
export let pending: Pending | undefined;

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
export async function downloadInBrowser(appid: number, name: string, source: Source) {
  const url = source.url ?? source.urls?.[0];
  // The URL lands in a Steam launch option, which Steam runs through /bin/sh.
  if (!url || !SAFE_URL.test(url)) throw new Error("This source has no usable link");
  const id = await browserShortcut();
  const gid = gameId(id);
  try { SteamClient.Apps.TerminateApp(gid, false); } catch { /* not running */ }
  SteamClient.Apps.SetShortcutLaunchOptions(id, `MOZ_ENABLE_WAYLAND=0 %command% "${url}"`);
  pending = { appid, name, source, since: Math.floor(Date.now() / 1000) - 5 };
  await new Promise((r) => setTimeout(r, 800));
  SteamClient.Apps.RunGame(gid, "", -1, 100);
}

export function cancelBrowserDownload() { pending = undefined; }

let lastSize = new Map<string, number>();

/** Called by the background watcher. Starts the install once a finished archive has stopped growing. */
export async function checkBrowserDownload() {
  const p = pending;
  if (!p) return;
  let files;
  try { files = await api.browserDownloads(p.since); } catch { return; }
  const archives = files.filter((f) => f.archive);
  if (!archives.length || archives.some((f) => !f.finished)) return;
  const pick = archives.find((f) => /\.part0*1\.rar$|\.001$/i.test(f.name)) ?? archives[0];
  // Two polls in a row with the same size, so a download Firefox has just renamed is complete on disk.
  const prev = lastSize.get(pick.path);
  lastSize.set(pick.path, pick.size);
  if (prev !== pick.size || !pick.size) return;
  pending = undefined;
  lastSize = new Map();
  try {
    await api.install(p.appid, p.name, p.source, pick.path);
    toaster.toast({ title: "Mercury", body: `Got ${pick.name}. Installing ${p.name} now; exit Firefox whenever you like.` });
  } catch (e: any) {
    toaster.toast({ title: "Mercury", body: `Could not install from ${pick.name}: ${e.message}` });
  }
}
