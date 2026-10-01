import { ConfirmModal, DialogButton, Focusable, Navigation, showModal, Spinner, useParams } from "@decky/ui";
import { toaster } from "@decky/api";
import { useState } from "react";
import { api, bytes, cdn, Source } from "../api";
import { useOnce, usePoll } from "../hooks";
import { gameId, removeShortcut } from "../steam";
import { C, Chip, JobProgress, page, scrollIntoView } from "../ui";

declare const SteamClient: any;

const btn = { height: 36, borderRadius: 3, display: "flex", alignItems: "center", justifyContent: "center", fontWeight: 600, fontSize: 14, color: "#fff" } as const;

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
      strDescription={`${s.provider} · ${s.name}\n\nDownload ${s.size || "unknown size"}, about ${bytes(est)} installed. Needs about ${bytes(need)} free while installing; ${bytes(free)} is free.${!status?.rd_key_set ? "\n\nAdd your Real-Debrid key in Mercury settings first." : ""}`} />);
  };

  const uninstall = () => showModal(<ConfirmModal strTitle={`Uninstall ${entry?.name}?`} strOKButtonText="Uninstall"
    strDescription={`Deletes ${bytes(entry?.size ?? 0)} and removes the Steam shortcut.`}
    onOK={async () => { const e = await api.uninstall(appid); removeShortcut(e.shortcut_id); reloadLib(); }} />);

  return (
    <div style={page}>
      <div style={{ position: "relative", height: 180, background: `url(${cdn(appid, "library_hero.jpg")}) center 30%/cover` }}>
        <div style={{ position: "absolute", inset: 0, background: `linear-gradient(transparent 40%,${C.bg})` }} />
        <img src={cdn(appid, "logo.png")} style={{ position: "absolute", left: 28, top: 24, maxWidth: 260, maxHeight: 80, filter: "drop-shadow(0 4px 18px rgba(0,0,0,.7))" }}
          onError={(e) => { (e.target as HTMLImageElement).style.display = "none"; }} />
        <div style={{ position: "absolute", left: 28, bottom: 10, fontSize: 18, fontWeight: 700, color: "#fff" }}>{app?.name ?? ""}</div>
      </div>

      <div style={{ padding: "6px 28px 48px" }}>
        {entry && (
          <Focusable flow-children="horizontal" style={{ display: "flex", gap: 10, marginBottom: 18 }}>
            <Focusable autoFocus onActivate={() => SteamClient.Apps.RunGame(gameId(entry.shortcut_id), "", -1, 100)}
              style={{ ...btn, width: 140, background: "linear-gradient(90deg,#70d61d,#01a75b)" }}>Play</Focusable>
            <Focusable onActivate={uninstall} style={{ ...btn, width: 120, background: C.panel2 }}>Uninstall</Focusable>
            <div style={{ alignSelf: "center", color: C.dim, fontSize: 13 }}>{entry.provider}{entry.version ? ` ${entry.version}` : ""} · {bytes(entry.size)}</div>
          </Focusable>
        )}

        {job && <div style={{ background: C.panel, borderRadius: 6, padding: "10px 16px", marginBottom: 18 }}>
          <JobProgress job={job} label="In your downloads" />
          <DialogButton style={{ width: 160, marginTop: 6 }} onClick={() => Navigation.Navigate("/mercury/downloads")}>Open downloads</DialogButton>
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
            return (
              <Focusable key={`${s.provider}-${i}`} autoFocus={i === 0 && !entry} onActivate={() => !busy && !job && install(s, est)} onFocus={scrollIntoView}
                style={{ display: "flex", alignItems: "center", justifyContent: "space-between", minHeight: 44, padding: "6px 12px", borderRadius: 5, marginBottom: 6, background: C.panel }}>
                <div style={{ minWidth: 0 }}>
                  <div style={{ fontWeight: 600, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis", maxWidth: 520, fontSize: 13 }}>{s.name}</div>
                  <div style={{ fontSize: 11, color: C.dim, marginTop: 2 }}>{s.size_bytes ? `${s.size} download · ~${bytes(est)} installed` : "Size unknown"}</div>
                </div>
                <div style={{ flex: "none" }}>
                  <Chip tone="accent">{s.provider}</Chip>
                  {s.version && <Chip>{s.version}</Chip>}
                  {s.magnet ? <Chip>Torrent</Chip> : <Chip>Direct link</Chip>}
                  {tight && <Chip tone="bad">Not enough space</Chip>}
                </div>
              </Focusable>
            );
          })}
        </Focusable>
      </div>
    </div>
  );
}
