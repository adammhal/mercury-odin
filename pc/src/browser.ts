// Hosts Real-Debrid cannot fetch: Mercury opens the page in the default browser, Adam presses Download,
// and Mercury picks the finished file up from %USERPROFILE%\Downloads. Same flow as the Odin's Firefox one.
import { toaster } from "@decky/api";
import { api, Source } from "@shared/api";
import { pc } from "./pcapi";

type Pending = { appid: number; name: string; source: Source; since: number };
const KEY = "mercury.pendingBrowserDownload";
let pending: Pending | undefined = (() => { try { return JSON.parse(localStorage.getItem(KEY) ?? "null") ?? undefined; } catch { return undefined; } })();
function setPending(p?: Pending) { pending = p; try { p ? localStorage.setItem(KEY, JSON.stringify(p)) : localStorage.removeItem(KEY); } catch { /* storage off */ } }
export const waitingFor = () => pending;

export async function downloadInBrowser(appid: number, name: string, source: Source) {
  const url = source.url ?? source.urls?.[0];
  if (!url) throw new Error("This source has no link");
  setPending({ appid, name, source, since: Math.floor(Date.now() / 1000) - 5 });
  await pc.openUrl(url);
}
export function cancelBrowserDownload() { setPending(undefined); }

let lastSize = new Map<string, number>();
export async function checkBrowserDownload() {
  const p = pending; if (!p) return;
  let files; try { files = await api.browserDownloads(p.since); } catch { return; }
  const archives = files.filter((f) => f.archive);
  if (!archives.length || archives.some((f) => !f.finished)) return;
  const pick = archives.find((f) => /\.part0*1\.rar$|\.001$/i.test(f.name)) ?? archives[archives.length - 1];
  const prev = lastSize.get(pick.path); lastSize.set(pick.path, pick.size);
  if (prev !== pick.size || !pick.size) return;
  try { await api.install(p.appid, p.name, p.source, pick.path); }
  catch (e: any) { toaster.toast({ title: "Mercury", body: `Waiting to install ${p.name}: ${e.message}` }); return; }
  setPending(undefined); lastSize = new Map();
  toaster.toast({ title: "Mercury", body: `Got ${pick.name}. Installing ${p.name}.` });
}
