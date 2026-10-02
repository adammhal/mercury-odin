import { fetchNoCors } from "@decky/api";

const BASE = "http://127.0.0.1:47800";

export type App = { appid: number; name: string; description: string; genres: string[]; release: string; developer: string };
export type Source = { name: string; provider: string; size: string; size_bytes: number; magnet?: string | null; url?: string | null; version?: string | null; urls?: string[]; supported?: boolean; repack?: boolean };
export type JobState = "queued" | "resolving" | "caching" | "downloading" | "paused" | "extracting" | "needs_setup" | "installing" | "ready" | "done" | "failed" | "cancelled";
export type Job = {
  id: number; appid: number; name: string; source: Source; state: JobState; error?: string | null;
  dir?: string | null; setup_exe?: string | null; exe?: string | null; candidates: string[]; shortcut_id?: number | null;
  cache_progress: number; done: number; total: number; speed: number; created: number;
};
export type LocalFile = { path: string; name: string; size: number; modified: number; finished: boolean; archive: boolean };
export type Entry = { appid: number; name: string; dir: string; exe: string; shortcut_id: number; provider: string; size: number; version?: string | null; installed: number };
export type Status = { version: string; rd_key_set: boolean; unrar: boolean; installer_launch_options?: string | null; storage: { total: number; free: number; mercury: number } };
export type Config = { browser_shortcut_id?: number | null; rd_key: string; rd_key_set: boolean; games_dir: string; downloads_dir: string; server_url: string; enable_steamrip: boolean; proton_tool: string; launch_options: string };

async function req<T>(path: string, method = "GET", body?: unknown): Promise<T> {
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
  art: (id: number) => req<{ assets: [number, string, string][] }>(`/steam/art/${id}`),
  sources: (name: string) => req<{ sources: Source[]; installed_estimate: number[]; errors: string[] }>(`/sources?name=${encodeURIComponent(name)}`),
  jobs: () => req<Job[]>("/jobs"),
  install: (appid: number, name: string, source: Source, local_file?: string) => req<Job>("/jobs", "POST", { appid, name, source, local_file }),
  browserDownloads: (since: number) => req<LocalFile[]>(`/browser/downloads?since=${since}`),
  act: (id: number, act: string, body?: object) => req<unknown>(`/jobs/${id}/${act}`, "POST", body ?? {}),
  clearJobs: () => req<{ cleared: number }>("/jobs/clear", "POST", {}),
  library: () => req<Entry[]>("/library"),
  uninstall: (appid: number) => req<Entry>(`/library/${appid}/uninstall`, "POST", {}),
  installerFiles: (appid: number) => req<{ dir: string; size: number } | null>(`/library/${appid}/installer-files`),
  deleteInstallerFiles: (appid: number) => req<{ freed: number }>(`/library/${appid}/installer-files`, "DELETE"),
};

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
  ready: "Adding to Steam", done: "Ready to play", failed: "Failed", cancelled: "Cancelled",
};

/** 0-100 progress for whichever step a job is in. */
export function jobPercent(j: Job): number {
  if (j.state === "caching") return j.cache_progress;
  if (j.total > 0) return Math.min(100, (j.done / j.total) * 100);
  return ["done", "ready"].includes(j.state) ? 100 : 0;
}
