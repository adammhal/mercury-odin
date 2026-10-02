import { api, Job } from "./api";

declare const SteamClient: any;

/** Non-Steam shortcuts launch by 64-bit game ID: (appid << 32) | 0x02000000. */
export const gameId = (appid: number) => ((BigInt(appid >>> 0) << 32n) | 0x02000000n).toString();

const dirOf = (p: string) => p.slice(0, p.lastIndexOf("/") + 1);

async function applyArt(shortcut: number, steamAppid: number) {
  const { assets } = await api.art(steamAppid);
  for (const [type, ext, data] of assets) {
    await SteamClient.Apps.SetCustomArtworkForApp(shortcut, data, ext, type);
  }
}

async function configure(shortcut: number, name: string) {
  const cfg = await api.config();
  SteamClient.Apps.SetShortcutName(shortcut, name);
  if (cfg.launch_options) SteamClient.Apps.SetShortcutLaunchOptions(shortcut, cfg.launch_options);
  await SteamClient.Apps.SpecifyCompatTool(shortcut, cfg.proton_tool);
}

/** Create the Steam shortcut for a finished job, or repoint the installer shortcut at the game. */
export async function addToSteam(job: Job, exe = job.exe ?? ""): Promise<number> {
  if (!exe) throw new Error("No game .exe to add");
  let id = job.shortcut_id ?? 0;
  if (id) {
    SteamClient.Apps.SetShortcutExe(id, `"${exe}"`);
    SteamClient.Apps.SetShortcutStartDir(id, dirOf(exe));
  } else {
    // AddShortcut takes plain paths and quotes the exe itself (spike S2).
    id = Number(await SteamClient.Apps.AddShortcut(job.name, exe, dirOf(exe), ""));
    if (!id) throw new Error("Steam did not create the shortcut");
  }
  await configure(id, job.name);
  await applyArt(id, job.appid);
  await api.act(job.id, "steam-added", { shortcut_id: id, exe });
  return id;
}

/** Repacks: run setup.exe through Proton from a Steam shortcut and let the user click through it.
 * A second attempt reuses the same shortcut, so the same Proton prefix. */
export async function runInstaller(job: Job): Promise<number> {
  if (!job.setup_exe) throw new Error("No installer");
  let id = job.shortcut_id ?? 0;
  if (!id) {
    id = Number(await SteamClient.Apps.AddShortcut(job.name, job.setup_exe, dirOf(job.setup_exe), ""));
    if (!id) throw new Error("Steam did not create the shortcut");
    const cfg = await api.config();
    SteamClient.Apps.SetShortcutName(id, `${job.name} (installer)`);
    await SteamClient.Apps.SpecifyCompatTool(id, cfg.proton_tool);
  }
  // No frame-generation wrapper for an installer. PROTON_LOG writes ~/steam-<gameid>.log; +file records every path
  // the installer opens, which is what a "path not found" needs.
  SteamClient.Apps.SetShortcutLaunchOptions(id, "PROTON_LOG=1 WINEDEBUG=+timestamp,+pid,+tid,+seh,+loaddll,+file /usr/libexec/armada/armada-game-launch %command%");
  await api.act(job.id, "setup-launched", { shortcut_id: id });
  SteamClient.Apps.RunGame(gameId(id), "", -1, 100);
  return id;
}

export function removeShortcut(id: number) {
  if (id) SteamClient.Apps.RemoveShortcut(id);
}

/** Calls back when a Steam app (or shortcut) stops running. */
export function onAppExit(cb: (appid: number) => void): () => void {
  const h = SteamClient.GameSessions.RegisterForAppLifetimeNotifications((n: { unAppID: number; bRunning: boolean }) => {
    if (!n.bRunning) cb(n.unAppID);
  });
  return () => h?.unregister?.();
}
