import { ConfirmModal, Focusable, Navigation, showModal, Spinner, useParams } from "@decky/ui";
import { toaster } from "@decky/api";
import { useRef, useState } from "react";
import { api, bytes, cdn, gridArt, Source } from "@shared/api";
import { useOnce, usePoll } from "@shared/hooks";
import { Btn, C, Chip, FOCUS, FocusStyle, JobProgress, page, scrollIntoView } from "@shared/ui";
import { downloadInBrowser } from "../browser";
import { pc } from "../pcapi";

export function Game() {
  const { appid: raw } = useParams<{ appid: string }>();
  const appid = Number(raw);
  const [app] = useOnce(() => api.app(appid), [appid]);
  const [found, srcErr] = useOnce(async () => (app ? api.sources(app.name) : undefined), [app?.name]);
  const [status] = usePoll(api.status, 10000);
  const [lib, , reloadLib] = usePoll(api.library, 5000);
  const [jobs] = usePoll(api.jobs, 1500);
  const [busy, setBusy] = useState(false);

  const entry = lib?.find((e) => e.appid === appid);
  const [setupFiles, , reloadSetupFiles] = useOnce(() => (entry ? api.installerFiles(appid) : Promise.resolve(null)), [entry?.appid]);
  const job = jobs?.find((j) => j.appid === appid && !["done", "cancelled"].includes(j.state));
  const free = status?.storage.free ?? 0;

  const install = (s: Source, est: number) => {
    const need = s.size_bytes + est;
    const go = async () => {
      setBusy(true);
      try { await api.install(appid, app!.name, s); toaster.toast({ title: "Mercury", body: `${app!.name} added to downloads` }); }
      catch (e: any) { toaster.toast({ title: "Mercury", body: e.message }); }
      setBusy(false);
    };
    showModal(<ConfirmModal strTitle={`Install ${app?.name}?`} strOKButtonText="Install" onOK={go}
      strDescription={`${s.provider} · ${s.name}\n\n${s.repack ? "This is a repack. When it finishes downloading, Mercury runs its installer; accept the admin prompt and click through it.\n\n" : ""}Download ${s.size || "unknown size"}, about ${bytes(est)} installed. Needs about ${bytes(need)} free while installing; ${bytes(free)} is free.${!status?.rd_key_set ? "\n\nAdd your Real-Debrid key in Mercury settings first." : ""}`} />);
  };

  const viaBrowser = (s: Source) => showModal(<ConfirmModal strTitle="Download in your browser" strOKButtonText="Open browser"
    strDescription={`Real-Debrid cannot fetch from this host, so you download it yourself.\n\n1. Mercury opens the download page in your browser.\n2. Press the host's Download button and save to your Downloads folder.\n3. When the file finishes, Mercury installs ${app?.name ?? "the game"} and adds it to Steam.`}
    onOK={async () => { try { await downloadInBrowser(appid, app!.name, s); } catch (e: any) { toaster.toast({ title: "Mercury", body: e.message }); } }} />);

  const uninstall = () => showModal(<ConfirmModal strTitle={`Uninstall ${entry?.name}?`} strOKButtonText="Uninstall"
    strDescription={`Deletes ${bytes(entry?.size ?? 0)} and removes it from Steam.`}
    onOK={async () => { try { await api.uninstall(appid); reloadLib(); } catch (e: any) { toaster.toast({ title: "Mercury", body: e.message }); } }} />);

  // Games Mercury added without a Steam store page (or whose store art is wrong) use the art Steam shows for the shortcut.
  const stamp = useRef(Date.now()).current;
  const sid = entry?.shortcut_id;
  const play = async () => { try { await pc.play(appid); } catch (e: any) { toaster.toast({ title: "Mercury", body: e.message }); } };

  return (
    <div style={page}>
      <FocusStyle />
      <div style={{ position: "relative", height: 180, background: `${sid ? `url(${gridArt(sid, 1, stamp)}) center 30%/cover, ` : ""}url(${cdn(appid, "library_hero.jpg")}) center 30%/cover` }}>
        <div style={{ position: "absolute", inset: 0, background: `linear-gradient(transparent 40%,${C.bg})` }} />
        <img src={sid ? gridArt(sid, 2, stamp) : cdn(appid, "logo.png")} style={{ position: "absolute", left: 28, top: 24, maxWidth: 260, maxHeight: 80, filter: "drop-shadow(0 4px 18px rgba(0,0,0,.7))" }}
          onError={(e) => { (e.target as HTMLImageElement).style.display = "none"; }} />
        <div style={{ position: "absolute", left: 28, bottom: 10, fontSize: 18, fontWeight: 700, color: "#fff" }}>{app?.name ?? entry?.name ?? ""}</div>
      </div>

      <div style={{ padding: "6px 28px 48px" }}>
        {entry && (
          <Focusable flow-children="horizontal" style={{ display: "flex", gap: 10, marginBottom: 18 }}>
            <Btn autoFocus onClick={play} style={{ height: 36, width: 140, fontSize: 14, background: "linear-gradient(90deg,#70d61d,#01a75b)" }}>Play</Btn>
            <Btn onClick={() => Navigation.Navigate(`/mercury/edit-art/${appid}`)} style={{ height: 36, width: 150, fontSize: 14 }}>Edit artwork</Btn>
            <Btn onClick={uninstall} style={{ height: 36, width: 120, fontSize: 14 }}>Uninstall</Btn>
            {setupFiles && <Btn style={{ height: 36 }} onClick={() => showModal(<ConfirmModal strTitle="Delete installer files?" strOKButtonText="Delete"
              strDescription={`The repack's setup files (${bytes(setupFiles.size)}) are not needed to play. Delete them only after the game launches and works.`}
              onOK={async () => { const r = await api.deleteInstallerFiles(appid); toaster.toast({ title: "Mercury", body: `Freed ${bytes(r.freed)}` }); reloadSetupFiles(); }} />)}>
              Delete installer files ({bytes(setupFiles.size)})</Btn>}
            <div style={{ alignSelf: "center", color: C.dim, fontSize: 13 }}>{entry.provider}{entry.version ? ` ${entry.version}` : ""} · {bytes(entry.size)}</div>
          </Focusable>
        )}

        {job && <div style={{ background: C.panel, borderRadius: 6, padding: "10px 16px", marginBottom: 18 }}>
          <JobProgress job={job} label="In your downloads" />
          <Btn style={{ width: 160, marginTop: 6 }} onClick={() => Navigation.Navigate("/mercury/downloads")}>Open downloads</Btn>
        </div>}

        <div style={{ fontSize: 13, fontWeight: 600, color: "#fff", margin: "4px 0 8px" }}>
          Sources <span style={{ color: C.dim, fontWeight: 500, fontSize: 13, marginLeft: 8 }}>{bytes(free)} free</span>
        </div>
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
                onActivate={() => !busy && !job && (blocked ? viaBrowser(s) : install(s, est))}
                style={{ display: "grid", gridTemplateColumns: "minmax(0, 1fr) auto", alignItems: "center", gap: 12, minHeight: 44,
                  padding: "6px 12px", borderRadius: 5, marginBottom: 6, background: C.panel }}>
                <div style={{ minWidth: 0 }}>
                  <div style={{ fontWeight: 600, fontSize: 13, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{s.name}</div>
                  <div style={{ fontSize: 11, color: C.dim, marginTop: 2, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
                    {blocked ? `${s.size || "Unknown size"} · Real-Debrid can't fetch this host; download it in your browser` : s.size_bytes ? `${s.size} download · ~${bytes(est)} installed` : "Size unknown"}
                  </div>
                </div>
                <div style={{ whiteSpace: "nowrap", maxWidth: 380, overflow: "hidden", textAlign: "right" }}>
                  <Chip tone="accent">{s.provider}</Chip>
                  {s.version && <Chip>{s.version}</Chip>}
                  {s.magnet ? <Chip>Torrent</Chip> : <Chip>Direct link</Chip>}
                  {blocked && <Chip tone="warn">Browser download</Chip>}
                  {s.repack && !blocked && <Chip>Repack · installer</Chip>}
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
