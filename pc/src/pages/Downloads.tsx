import { ConfirmModal, Focusable, Navigation, showModal } from "@decky/ui";
import { toaster } from "@decky/api";
import { api, cdn, Job } from "@shared/api";
import { usePoll } from "@shared/hooks";
import { Btn, C, FocusStyle, JobProgress, page } from "@shared/ui";
import { cancelBrowserDownload, waitingFor } from "../browser";
import { pc } from "../pcapi";

function Actions({ job, reload }: { job: Job; reload: () => void }) {
  const run = (fn: () => Promise<unknown>) => async () => { try { await fn(); } catch (e: any) { toaster.toast({ title: "Mercury", body: e.message }); } reload(); };
  const act = (a: string) => run(() => api.act(job.id, a));
  const b = (label: string, fn: () => void) => <Btn key={label} style={{ width: 110, height: 30, fontSize: 12 }} onClick={fn}>{label}</Btn>;
  const out = [];
  if (["queued", "resolving", "caching", "downloading"].includes(job.state)) out.push(b("Pause", act("pause")));
  if (["paused", "failed"].includes(job.state)) out.push(b(job.state === "failed" ? "Retry" : "Resume", act("resume")));
  if (job.state === "needs_setup") out.push(b("Run installer", run(() => pc.runSetup(job.id))));
  if (job.state === "needs_setup" && job.error) out.push(b("Run as admin", run(() => pc.runSetup(job.id, true))));
  if (job.state === "review") out.push(b("Review & add", () => Navigation.Navigate(`/mercury/review/${job.id}`)));
  const finished = ["done", "failed", "cancelled"].includes(job.state);
  if (!finished && job.state !== "installing" && job.state !== "review") out.push(b("Cancel", () => showModal(
    <ConfirmModal strTitle={`Cancel ${job.name}?`} strDescription="Stops the download, deletes its files, and removes it from this list." strOKButtonText="Cancel download" onOK={act("cancel")} />)));
  if (finished) out.push(b("Clear", act("remove")));
  return <Focusable flow-children="horizontal" style={{ display: "flex", gap: 8 }}>{out}</Focusable>;
}

export function Downloads() {
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
        {browser && <div style={{ display: "flex", gap: 12, alignItems: "center", background: C.panel, borderRadius: 6, padding: 10, marginBottom: 8 }}>
          <div style={{ flex: 1, fontSize: 13 }}>Waiting for <b>{browser.name}</b> in your browser. Save it to your Downloads folder; Mercury takes it from there.</div>
          <Btn style={{ width: 110, height: 30, fontSize: 12 }} onClick={() => { cancelBrowserDownload(); reload(); }}>Stop waiting</Btn>
        </div>}
        {err && <div style={{ color: C.bad }}>Mercury engine is not responding: {err}</div>}
        {jobs && !list.length && !browser && <div style={{ color: C.dim }}>Nothing downloading. Pick a game and a source to start.</div>}
        <Focusable flow-children="vertical">
          {list.map((j) => (
            <div key={j.id} style={{ display: "flex", gap: 12, alignItems: "center", background: C.panel, borderRadius: 6, padding: 10, marginBottom: 8 }}>
              <div style={{ flex: "none", width: 120, height: 56, borderRadius: 5, background: `url(${cdn(j.appid, "header.jpg")}) center/cover` }} />
              <div style={{ flex: 1, minWidth: 0 }}>
                <JobProgress job={j} />
                {j.state === "needs_setup" && <div style={{ fontSize: 12, color: C.warn, marginTop: 4 }}>Repack downloaded. Run its installer, accept the admin prompt, keep the folder it shows, and click through. Mercury adds the game to Steam when it closes.</div>}
                {j.state === "installing" && <div style={{ fontSize: 12, color: C.dim, marginTop: 4 }}>Installer running. It should be in front: left stick moves the mouse, A clicks, Start presses Next, X is Space, D-pad up/down is Tab.</div>}
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
