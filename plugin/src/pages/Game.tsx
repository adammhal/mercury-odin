import { ConfirmModal, Focusable, Navigation, showModal, Spinner, useParams } from "@decky/ui";
import { toaster } from "@decky/api";
import { useEffect, useState } from "react";
import { api, Availability, bytes, cdn, Source } from "../api";
import { useOnce, usePoll } from "../hooks";
import { downloadInBrowser } from "../browser";
import { gameId, removeShortcut } from "../steam";
import { Btn, C, Chip, FOCUS, FocusStyle, JobProgress, page, scrollIntoView } from "../ui";

declare const SteamClient: any;

export function Game() {
  const { appid: raw } = useParams<{ appid: string }>();
  const appid = Number(raw);
  const [app] = useOnce(() => api.app(appid), [appid]);
  const [status] = usePoll(api.status, 10000);
  const [refreshTick, setRefreshTick] = useState(0);
  const [found, srcErr] = useOnce(async () => (app ? api.sources(app.name, refreshTick > 0) : undefined), [app?.name, refreshTick]);
  const [avail, setAvail] = useState<Record<string, Availability>>({});
  // Ask Real-Debrid which torrent sources are already cached (instant). Probes are remembered for a day.
  useEffect(() => {
    const magnets = (found?.sources ?? []).filter((s) => s.magnet).map((s) => s.magnet!).slice(0, 6);
    if (!magnets.length || !status?.rd_key_set) return;
    let live = true;
    api.cached(magnets).then((r) => live && setAvail(r), () => {});
    return () => { live = false; };
  }, [found, status?.rd_key_set]);
  const [lib, , reloadLib] = usePoll(api.library, 5000);
  const [jobs] = usePoll(api.jobs, 1500);
  const [busy, setBusy] = useState(false);

  const entry = lib?.find((e) => e.appid === appid);
  const [setupFiles, , reloadSetupFiles] = useOnce(() => (entry ? api.installerFiles(appid) : Promise.resolve(null)), [entry?.appid]);
  const job = jobs?.find((j) => j.appid === appid && !["done", "cancelled"].includes(j.state));
  const free = status?.storage.free ?? 0;

  const [update] = useOnce(() => (entry ? api.updateCheck(appid) : Promise.resolve(undefined)), [entry?.appid, entry?.installed]);

  /** Replace the installed game's files with a release from `s`. Keeps the shortcut, Proton prefix and saves. */
  const startUpdate = (s: Source, via?: () => Promise<void>) => showModal(<ConfirmModal strTitle={`Update ${app?.name}?`} strOKButtonText="Update"
    strDescription={`${s.provider} · ${s.name}\n\nMercury downloads this release, then copies its files over the installed game. Your saves, Steam shortcut, Proton choice and frame generation stay. If the download or extraction fails, the installed game is not touched.`}
    onOK={async () => {
      setBusy(true);
      try {
        if (via) await via();
        else { await api.install(appid, app!.name, s, undefined, true); toaster.toast({ title: "Mercury", body: `Updating ${app!.name}` }); }
      } catch (e: any) { toaster.toast({ title: "Mercury", body: e.message }); }
      setBusy(false);
    }} />);

  const install = (s: Source, est: number) => {
    const need = s.size_bytes + est;
    const go = async () => {
      setBusy(true);
      try { await api.install(appid, app!.name, s); toaster.toast({ title: "Mercury", body: `${app!.name} added to downloads` }); }
      catch (e: any) { toaster.toast({ title: "Mercury", body: e.message }); }
      setBusy(false);
    };
    showModal(<ConfirmModal strTitle={`Install ${app?.name}?`} strOKButtonText="Install" onOK={go}
      strDescription={`${s.provider} · ${s.name}\n\n${s.repack ? "This is a repack. Its installer currently gets stuck unpacking on the Odin (32-bit x86 emulation). A pre-installed source is more likely to work.\n\n" : ""}Download ${s.size || "unknown size"}, about ${bytes(est)} installed. Needs about ${bytes(need)} free while installing; ${bytes(free)} is free.${!status?.rd_key_set ? "\n\nAdd your Real-Debrid key in Mercury settings first." : ""}`} />);
  };

  const viaBrowser = (s: Source) => showModal(<ConfirmModal strTitle="Download in your browser" strOKButtonText="Open Firefox"
    strDescription={`Real-Debrid cannot fetch from this host, so you download it yourself.\n\n1. Mercury opens Firefox on the download page.\n2. Tap the host's Download button (touch is easiest). Some hosts show a short check first.\n3. When the file finishes, Mercury closes Firefox and installs ${app?.name ?? "the game"} on its own.`}
    onOK={async () => {
      try { await downloadInBrowser(appid, app!.name, s); }
      catch (e: any) { toaster.toast({ title: "Mercury", body: e.message }); }
    }} />);

  const uninstall = () => showModal(<ConfirmModal strTitle={`Uninstall ${entry?.name}?`} strOKButtonText="Uninstall"
    strDescription={`Deletes ${bytes(entry?.size ?? 0)} and removes the Steam shortcut.`}
    onOK={async () => { const e = await api.uninstall(appid); removeShortcut(e.shortcut_id); reloadLib(); }} />);

  return (
    <div style={page}>
      <FocusStyle />
      <div style={{ position: "relative", height: 180, background: `url(${cdn(appid, "library_hero.jpg")}) center 30%/cover` }}>
        <div style={{ position: "absolute", inset: 0, background: `linear-gradient(transparent 40%,${C.bg})` }} />
        <img src={cdn(appid, "logo.png")} style={{ position: "absolute", left: 28, top: 24, maxWidth: 260, maxHeight: 80, filter: "drop-shadow(0 4px 18px rgba(0,0,0,.7))" }}
          onError={(e) => { (e.target as HTMLImageElement).style.display = "none"; }} />
        <div style={{ position: "absolute", left: 28, bottom: 10, fontSize: 18, fontWeight: 700, color: "#fff" }}>{app?.name ?? ""}</div>
      </div>

      <div style={{ padding: "6px 28px 48px" }}>
        {entry && (
          <Focusable flow-children="horizontal" style={{ display: "flex", gap: 10, marginBottom: 18 }}>
            <Btn autoFocus onClick={() => SteamClient.Apps.RunGame(gameId(entry.shortcut_id), "", -1, 100)}
              style={{ height: 36, width: 140, fontSize: 14, background: "linear-gradient(90deg,#70d61d,#01a75b)" }}>Play</Btn>
            <Btn onClick={uninstall} style={{ height: 36, width: 120, fontSize: 14 }}>Uninstall</Btn>
            {setupFiles && <Btn style={{ height: 36 }} onClick={() => showModal(<ConfirmModal strTitle="Delete installer files?" strOKButtonText="Delete"
              strDescription={`The repack's setup files (${bytes(setupFiles.size)}) are not needed to play. Delete them only after the game launches and works.`}
              onOK={async () => { const r = await api.deleteInstallerFiles(appid); toaster.toast({ title: "Mercury", body: `Freed ${bytes(r.freed)}` }); reloadSetupFiles(); }} />)}>
              Delete installer files ({bytes(setupFiles.size)})</Btn>}
            <div style={{ alignSelf: "center", color: C.dim, fontSize: 13 }}>{entry.provider}{entry.version ? ` ${entry.version}` : ""} · {bytes(entry.size)}</div>
          </Focusable>
        )}

        {entry && update?.newer && !job && (
          <Focusable flow-children="horizontal" style={{ display: "flex", alignItems: "center", gap: 12, background: "rgba(112,214,29,.12)", border: "1px solid rgba(112,214,29,.35)", borderRadius: 6, padding: "8px 12px", marginBottom: 14 }}>
            <div style={{ flex: 1, fontSize: 13 }}>
              <b style={{ color: C.ok }}>Update available:</b> {update.newer.version} from {update.newer.source.provider}
              <span style={{ color: C.dim }}> · installed {update.current ?? "unknown version"}</span>
            </div>
            <Btn style={{ height: 30, background: "linear-gradient(90deg,#70d61d,#01a75b)" }}
              onClick={() => { const s = update.newer!.source; startUpdate(s, s.supported === false ? () => downloadInBrowser(appid, app!.name, s, true) : undefined); }}>Update</Btn>
          </Focusable>
        )}
        {entry && <div style={{ color: C.dim, fontSize: 12, marginBottom: 6 }}>Pick any source below to update or reinstall from it.</div>}
        {job && <div style={{ background: C.panel, borderRadius: 6, padding: "10px 16px", marginBottom: 18 }}>
          <JobProgress job={job} label="In your downloads" />
          <Btn style={{ width: 160, marginTop: 6 }} onClick={() => Navigation.Navigate("/mercury/downloads")}>Open downloads</Btn>
        </div>}

        <Focusable flow-children="horizontal" style={{ display: "flex", alignItems: "center", gap: 10, margin: "4px 0 8px" }}>
          <div style={{ fontSize: 13, fontWeight: 600, color: "#fff" }}>
            Sources <span style={{ color: C.dim, fontWeight: 500, fontSize: 13, marginLeft: 8 }}>{bytes(free)} free</span>
          </div>
          <Btn style={{ height: 26, fontSize: 12, marginLeft: "auto" }} disabled={!found && !srcErr}
            onClick={() => { setAvail({}); setRefreshTick((n) => n + 1); }}>Refresh sources</Btn>
        </Focusable>
        {!found && !srcErr && <div style={{ display: "flex", gap: 10, alignItems: "center", color: C.dim }}><Spinner style={{ width: 22 }} />Searching sources. The Mercury server can take a minute to wake up.</div>}
        {srcErr && <div style={{ color: C.bad }}>{srcErr}</div>}
        {found?.errors.map((e) => <div key={e} style={{ color: C.warn, fontSize: 13, marginBottom: 6 }}>{e}</div>)}
        {found && !found.sources.length && <div style={{ color: C.dim }}>No sources found for this game.</div>}
        <Focusable flow-children="vertical">
          {found?.sources.map((s, i) => {
            const est = found.installed_estimate[i] ?? 0;
            const tight = s.size_bytes + est > free;
            const blocked = s.supported === false;
            return (
              <Focusable key={`${s.provider}-${i}`} autoFocus={i === 0 && !entry} focusClassName={FOCUS} noFocusRing onFocus={scrollIntoView}
                onActivate={() => {
                  if (busy || job) return;
                  if (entry) return startUpdate(s, blocked ? () => downloadInBrowser(appid, app!.name, s, true) : undefined);
                  return blocked ? viaBrowser(s) : install(s, est);
                }}
                style={{ display: "grid", gridTemplateColumns: "minmax(0, 1fr) auto", alignItems: "center", gap: 12, minHeight: 44,
                  padding: "6px 12px", borderRadius: 5, marginBottom: 6, background: C.panel, opacity: 1 }}>
                <div style={{ minWidth: 0 }}>
                  <div style={{ fontWeight: 600, fontSize: 13, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{s.name}</div>
                  <div style={{ fontSize: 11, color: C.dim, marginTop: 2, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
                    {blocked ? `${s.size || "Unknown size"} · Real-Debrid can't fetch this host; download it in Firefox` : s.size_bytes ? `${s.size} download · ~${bytes(est)} installed` : "Size unknown"}
                  </div>
                </div>
                <div style={{ whiteSpace: "nowrap", maxWidth: 380, overflow: "hidden", textAlign: "right" }}>
                  <Chip tone="accent">{s.provider}</Chip>
                  {s.version && <Chip>{s.version}</Chip>}
                  {s.magnet && avail[s.magnet] === "cached" && <Chip tone="ok">Cached · instant</Chip>}
                  {s.magnet && avail[s.magnet] === "not_cached" && <Chip>Not cached</Chip>}
                  {s.magnet && avail[s.magnet] === "blocked" && <Chip tone="bad">Blocked by Real-Debrid</Chip>}
                  {s.magnet ? <Chip>Torrent</Chip> : <Chip>Direct link</Chip>}
                  {blocked && <Chip tone="warn">Browser download</Chip>}
                  {s.repack && !blocked && <Chip tone="warn">May not install on ARM</Chip>}
                  {tight && !blocked && <Chip tone="bad">Not enough space</Chip>}
                </div>
              </Focusable>
            );
          })}
        </Focusable>
      </div>
    </div>
  );
}
