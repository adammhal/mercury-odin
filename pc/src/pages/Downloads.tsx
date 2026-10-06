import { ConfirmModal, Focusable, Navigation, ProgressBar, showModal } from "@decky/ui";
import { useEffect, useRef, useState } from "react";
import { toaster } from "@decky/api";
import { api, bytes, cdn, Job } from "@shared/api";
import { usePoll } from "@shared/hooks";
import { Btn, C, FocusStyle, JobProgress, page } from "@shared/ui";
import { cancelBrowserDownload, waitingFor } from "../browser";
import { pc } from "../pcapi";
import { btn, usePadKind } from "../shim/pad";

function Actions({ job, reload }: { job: Job; reload: () => void }) {
  const run = (fn: () => Promise<unknown>) => async () => { try { await fn(); } catch (e: any) { toaster.toast({ title: "Mercury", body: e.message }); } reload(); };
  const act = (a: string) => run(() => api.act(job.id, a));
  const b = (label: string, fn: () => void) => <Btn key={label} style={{ width: 110, height: 30, fontSize: 12 }} onClick={fn}>{label}</Btn>;
  const out = [];
  if (["queued", "resolving", "caching", "downloading"].includes(job.state)) out.push(b("Pause", act("pause")));
  if (["paused", "failed"].includes(job.state)) out.push(b(job.state === "failed" ? "Retry" : "Resume", act("resume")));
  if (job.state === "needs_setup") out.push(b("Run installer", run(() => pc.runSetup(job.id))));
  if (job.state === "installing") out.push(b("Installer finished", run(() => pc.finishSetup(job.id))));
  if (job.state === "needs_setup" && job.error) out.push(b("Run as admin", run(() => pc.runSetup(job.id, true))));
  if (job.state === "review") out.push(b("Review & add", () => Navigation.Navigate(`/mercury/review/${job.id}`)));
  const finished = ["done", "failed", "cancelled"].includes(job.state);
  if (!finished && job.state !== "installing" && job.state !== "review") out.push(b("Cancel", () => showModal(
    <ConfirmModal strTitle={`Cancel ${job.name}?`} strDescription="Stops the download, deletes its files, and removes it from this list." strOKButtonText="Cancel download" onOK={act("cancel")} />)));
  if (finished) out.push(b("Clear", act("remove")));
  return <Focusable flow-children="horizontal" style={{ display: "flex", gap: 8 }}>{out}</Focusable>;
}

/** A download made in Mercury's browser: the file grows in Downloads, so its size gives the progress. */
function BrowserProgress({ browser, onStop }: { browser: NonNullable<ReturnType<typeof waitingFor>>; onStop: () => void }) {
  const [s, setS] = useState({ bytes: 0, speed: 0 });
  const last = useRef<{ t: number; b: number }>();
  useEffect(() => {
    let alive = true;
    const tick = async () => {
      try {
        const files = await api.browserDownloads(browser.since);
        const got = files.filter((f) => f.archive).reduce((a, f) => a + f.size, 0);
        const now = Date.now(), prev = last.current;
        const inst = prev && now > prev.t ? Math.max(0, (got - prev.b) / ((now - prev.t) / 1000)) : 0;
        last.current = { t: now, b: got };
        if (alive) setS((o) => ({ bytes: got, speed: o.speed ? o.speed * 0.6 + inst * 0.4 : inst }));
      } catch { /* the engine is busy; try again next second */ }
    };
    tick();
    const i = setInterval(tick, 1000);
    return () => { alive = false; clearInterval(i); };
  }, [browser.since]);
  const total = browser.source.size_bytes || 0;
  const pct = total ? Math.min(100, (s.bytes / total) * 100) : 0;
  return (
    <div style={{ display: "flex", gap: 12, alignItems: "center", background: C.panel, borderRadius: 6, padding: 10, marginBottom: 8 }}>
      <div style={{ flex: 1, minWidth: 0 }}>
        {s.bytes === 0
          ? <div style={{ fontSize: 13 }}>Waiting for <b>{browser.name}</b> to start downloading in the browser.</div>
          : <>
              <div style={{ fontSize: 13, marginBottom: 6 }}><b>{browser.name}</b> <span style={{ color: C.dim }}>· Downloading · {total ? `${bytes(s.bytes)} of ${bytes(total)}` : bytes(s.bytes)}{s.speed > 1e5 ? ` · ${bytes(s.speed)}/s` : ""}{total ? ` · ${pct.toFixed(0)}%` : ""}</span></div>
              <ProgressBar nProgress={pct} indeterminate={!total} />
            </>}
      </div>
      <Btn style={{ width: 110, height: 30, fontSize: 12 }} onClick={onStop}>Stop waiting</Btn>
    </div>
  );
}

export function Downloads() {
  const kind = usePadKind();
  const [jobs, err, reload] = usePoll(api.jobs, 1000);
  const list = (jobs ?? []).slice().reverse();
  const browser = waitingFor();
  return (
    <div style={page}>
      <FocusStyle />
      <div style={{ padding: "16px 28px 48px" }}>
        <Focusable flow-children="horizontal" style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: 10 }}>
          <div style={{ fontSize: 18, fontWeight: 700, color: "#fff" }}>Downloads</div>
          {list.some((j) => ["done", "failed", "cancelled"].includes(j.state)) &&
            <Btn style={{ height: 28, fontSize: 12 }} onClick={async () => {
              try { const r = await api.clearJobs(); toaster.toast({ title: "Mercury", body: `Cleared ${r.cleared} finished ${r.cleared === 1 ? "download" : "downloads"}` }); }
              catch (e: any) { toaster.toast({ title: "Mercury", body: e.message }); }
              reload();
            }}>Clear finished</Btn>}
        </Focusable>
        {browser && <BrowserProgress browser={browser} onStop={() => { cancelBrowserDownload(); reload(); }} />}
        {err && <div style={{ color: C.bad }}>Mercury engine is not responding: {err}</div>}
        {jobs && !list.length && !browser && <div style={{ color: C.dim }}>Nothing downloading. Pick a game and a source to start.</div>}
        <Focusable flow-children="vertical">
          {list.map((j) => (
            <div key={j.id} style={{ display: "flex", gap: 12, alignItems: "center", background: C.panel, borderRadius: 6, padding: 10, marginBottom: 8 }}>
              <div style={{ flex: "none", width: 120, height: 56, borderRadius: 5, background: `url(${cdn(j.appid, "header.jpg")}) center/cover` }} />
              <div style={{ flex: 1, minWidth: 0 }}>
                <JobProgress job={j} />
                {j.state === "needs_setup" && <div style={{ fontSize: 12, color: C.warn, marginTop: 4 }}>Repack downloaded. Run its installer, accept the admin prompt, keep the folder it shows, and click through. Mercury adds the game to Steam when it closes.</div>}
                {j.state === "installing" && <div style={{ fontSize: 12, color: C.dim, marginTop: 4 }}>{`Installer running. It should be in front: left stick moves the mouse, ${btn("A", kind)} clicks, Start presses Next, ${btn("X", kind)} is Space, the D-pad sends the arrow keys, ${btn("RB", kind)} and ${btn("LB", kind)} are Tab and Shift+Tab.`}</div>}
                <div style={{ fontSize: 12, color: C.dim, marginTop: 4 }}>{j.source.provider} · {j.source.name}</div>
              </div>
              <Actions job={j} reload={reload} />
            </div>
          ))}
        </Focusable>
      </div>
    </div>
  );
}
