import { fetchNoCors } from "@decky/api";

const BASE = "http://127.0.0.1:47800";

export type App = { appid: number; name: string; description: string; genres: string[]; release: string; developer: string; cover?: string | null };
export type Source = { name: string; provider: string; size: string; size_bytes: number; magnet?: string | null; url?: string | null; version?: string | null; urls?: string[]; supported?: boolean; repack?: boolean };
export type JobState = "queued" | "resolving" | "caching" | "downloading" | "paused" | "extracting" | "needs_setup" | "installing" | "ready" | "review" | "done" | "failed" | "cancelled";
export type Job = {
  id: number; appid: number; name: string; source: Source; state: JobState; error?: string | null;
  dir?: string | null; setup_exe?: string | null; exe?: string | null; candidates: string[]; shortcut_id?: number | null;
  cache_progress: number; done: number; total: number; speed: number; created: number; update_of?: number | null;
};
export type Card = { device: string; fstype: string; label: string; size: number };
export type ImportCandidate = { path: string; name: string; location: string; kind: "folder" | "archive"; exe?: string | null; installer: boolean; size: number };
export type UpdateInfo = { current?: string | null; provider: string; newer?: { version: string; source: Source } | null };
export type Availability = "cached" | "not_cached" | "blocked" | "unknown";
export type LocalFile = { path: string; name: string; size: number; modified: number; finished: boolean; archive: boolean };
export type Entry = { appid: number; name: string; dir: string; exe: string; shortcut_id: number; provider: string; size: number; version?: string | null;
  installed: number; source_name?: string | null; needs_repoint?: boolean; moving_to?: string | null; launch_options_fix?: string | null };
export type Location = { label: string; path: string; free: number; total: number; default: boolean };
export type SteamShortcut = { appid: number; name: string; exe: string; start_dir: string; launch_options: string };
export type Status = { version: string; rd_key_set: boolean; unrar: boolean; installer_launch_options?: string | null; storage: { total: number; free: number; mercury: number } };
export type Config = { browser_shortcut_id?: number | null; sgdb_key?: string; sgdb_key_set?: boolean; review_art?: boolean; launcher?: string; rd_key: string; rd_key_set: boolean; games_dir: string; downloads_dir: string; server_url: string; enable_steamrip: boolean; proton_tool: string; launch_options: string };
export type SgdbOpt = { url: string; thumb: string; score: number; width: number; height: number; author: string };

/** Rejects if `p` has not settled after `ms`. Nothing Mercury waits on may hang forever. */
export function withTimeout<T>(p: Promise<T>, ms: number, what: string): Promise<T> {
  return Promise.race([p, new Promise<T>((_, bad) => setTimeout(() => bad(new Error(`${what} timed out after ${ms / 1000}s`)), ms))]);
}

async function req<T>(path: string, method = "GET", body?: unknown): Promise<T> {
  return withTimeout(reqInner<T>(path, method, body), 30000, `Mercury engine ${path.split("?")[0]}`);
}

async function reqInner<T>(path: string, method = "GET", body?: unknown): Promise<T> {
  const r = await fetchNoCors(BASE + path, {
    method,
    headers: body === undefined ? undefined : { "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  // While the engine restarts, Decky answers for it with a non-JSON error page.
  const text = await r.text();
  let data: any;
  try { data = JSON.parse(text); } catch { throw new Error("Mercury engine is restarting. Try again in a moment."); }
  if (!r.ok) throw new Error(data?.error ?? `HTTP ${r.status}`);
  return data as T;
}

export const api = {
  status: () => req<Status>("/status"),
  config: () => req<Config>("/config"),
  saveConfig: (c: Partial<Config>) => req<Config>("/config", "PUT", c),
  rdCheck: () => req<{ username: string; type: string; expiration: string }>("/rd/check"),
  wishlist: () => req<App[]>("/steam/wishlist"),
  search: (q: string) => req<App[]>(`/steam/search?q=${encodeURIComponent(q)}`),
  app: (id: number) => req<App>(`/steam/app/${id}`),
  art: (id: number) => req<{ assets: [number, string, string][]; icon?: string | null }>(`/steam/art/${id}`),
  sgdb: (appid: number, slot: number) => req<{ options: SgdbOpt[] }>(`/sgdb/${appid}/${slot}`),
  editArt: (appid: number, body: { name?: string; art: Record<string, string> }) => req<{ ok: boolean; changed: boolean }>(`/library/${appid}/art`, "POST", body),
  resolveArt: (appid: number, choices: Record<string, string>) => req<{ assets: [number, string, string][] }>("/art/resolve", "POST", { appid, choices }),
  sources: (name: string, refresh = false) =>
    req<{ sources: Source[]; installed_estimate: number[]; errors: string[] }>(`/sources?name=${encodeURIComponent(name)}${refresh ? "&refresh=true" : ""}`),
  cached: (magnets: string[]) => req<Record<string, Availability>>("/sources/cached", "POST", { magnets }),
  jobs: () => req<Job[]>("/jobs"),
  install: (appid: number, name: string, source: Source, local_file?: string, update = false) =>
    req<Job>("/jobs", "POST", { appid, name, source, local_file, update }),
  updateCheck: (appid: number) => req<UpdateInfo>(`/library/${appid}/update`),
  browserDownloads: (since: number) => req<LocalFile[]>(`/browser/downloads?since=${since}`),
  act: (id: number, act: string, body?: object) => req<unknown>(`/jobs/${id}/${act}`, "POST", body ?? {}),
  clearJobs: () => req<{ cleared: number }>("/jobs/clear", "POST", {}),
  library: () => req<Entry[]>("/library"),
  importCandidates: () => req<{ drop_folder: string; candidates: ImportCandidate[]; unmounted_cards: Card[] }>("/import"),
  mountCard: (device: string) => req<{ message: string }>("/import/mount", "POST", { device }),
  importGame: (path: string, appid: number, name: string, keep_in_place: boolean) => req<Job>("/import", "POST", { path, appid, name, keep_in_place }),
  uninstall: (appid: number) => req<Entry>(`/library/${appid}/uninstall`, "POST", {}),
  battery: () => req<{ charging: boolean; percent: number | null; seconds_to_full: number | null }>("/battery"),
  locations: () => req<Location[]>("/locations"),
  moveGame: (appid: number, to: string) => req<Entry>(`/library/${appid}/move`, "POST", { to }),
  launchOptions: (exe: string) => req<{ launch_options: string }>("/launch-options", "POST", { exe }),
  launchOptionsSet: (appid: number) => req<unknown>(`/library/${appid}/launch-options-set`, "POST", {}),
  repointed: (appid: number) => req<unknown>(`/library/${appid}/repointed`, "POST", {}),
  steamShortcuts: () => req<SteamShortcut[]>("/steam/shortcuts"),
  adopt: (b: { appid: number; name: string; dir: string; exe: string; shortcut_id: number; provider?: string }) => req<Entry>("/library/adopt", "POST", b),
  setShortcut: (appid: number, shortcut_id: number) => req<Entry>(`/library/${appid}/shortcut`, "POST", { shortcut_id }),
  installerFiles: (appid: number) => req<{ dir: string; size: number } | null>(`/library/${appid}/installer-files`),
  deleteInstallerFiles: (appid: number) => req<{ freed: number }>(`/library/${appid}/installer-files`, "DELETE"),
};

/** The artwork Steam currently shows for a shortcut (PC engine). */
export const gridArt = (shortcut: number, slot: number, stamp: number) => `${BASE}/steam/grid/${shortcut}/${slot}?t=${stamp}`;

export const cdn = (id: number, f: "library_600x900.jpg" | "library_hero.jpg" | "logo.png" | "header.jpg") =>
  `https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/${id}/${f}`;

export function bytes(n: number): string {
  if (!n) return "0 B";
  const u = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.min(u.length - 1, Math.floor(Math.log(n) / Math.log(1024)));
  return `${(n / 1024 ** i).toFixed(i >= 3 ? 1 : 0)} ${u[i]}`;
}

export const STATE_LABEL: Record<JobState, string> = {
  queued: "Waiting", resolving: "Contacting Real-Debrid", caching: "Caching on Real-Debrid", downloading: "Downloading",
  paused: "Paused", extracting: "Extracting", needs_setup: "Installer ready", installing: "Installer running",
  ready: "Adding to Steam", review: "Waiting for your review", done: "Ready to play", failed: "Failed", cancelled: "Cancelled",
};

/** 0-100 progress for whichever step a job is in. */
export function jobPercent(j: Job): number {
  if (j.state === "caching") return j.cache_progress;
  if (j.total > 0) return Math.min(100, (j.done / j.total) * 100);
  return ["done", "ready", "review"].includes(j.state) ? 100 : 0;
}
