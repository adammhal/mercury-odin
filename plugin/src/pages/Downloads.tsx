import { ConfirmModal, Field, Focusable, Navigation, ProgressBar, showModal } from "@decky/ui";
import { toaster } from "@decky/api";
import { api, bytes, cdn, Job } from "../api";
import { usePoll } from "../hooks";
import { backToBrowser, cancelBrowserDownload, pending, progress } from "../browser";
import { runInstaller } from "../steam";
import { Btn, C, FocusStyle, JobProgress, page } from "../ui";

function Actions({ job, reload }: { job: Job; reload: () => void }) {
  const act = (a: string) => async () => { try { await api.act(job.id, a); } catch (e: any) { toaster.toast({ title: "Mercury", body: e.message }); } reload(); };
  const b = (label: string, fn: () => void) => <Btn key={label} style={{ width: 110, height: 30, fontSize: 12 }} onClick={fn}>{label}</Btn>;
  const out = [];
  if (["queued", "resolving", "caching", "downloading"].includes(job.state)) out.push(b("Pause", act("pause")));
  if (["paused", "failed"].includes(job.state)) out.push(b(job.state === "failed" ? "Retry" : "Resume", act("resume")));
  if (job.state === "needs_setup") out.push(b("Run installer", async () => {
    try { await runInstaller(job); } catch (e: any) { toaster.toast({ title: "Mercury", body: e.message }); }
    reload();
  }));
  if (job.state === "installing") out.push(b("Installer done", act("setup-done")));
  if (job.state === "review") out.push(b("Review", () => Navigation.Navigate(`/mercury/review/${job.id}`)));
  const finished = ["done", "failed", "cancelled"].includes(job.state);
  if (!finished) out.push(b("Cancel", () => showModal(
    <ConfirmModal strTitle={`Cancel ${job.name}?`} strDescription="Stops the download, deletes its files, and removes it from this list." strOKButtonText="Cancel download" onOK={act("cancel")} />)));
  if (finished) out.push(b("Clear", act("remove")));
  return <Focusable flow-children="horizontal" style={{ display: "flex", gap: 8 }}>{out}</Focusable>;
}

export function Downloads() {
  const [jobs, err, reload] = usePoll(api.jobs, 1000);
  const list = (jobs ?? []).slice().reverse();
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
        {err && <div style={{ color: C.bad }}>Mercury engine is not responding: {err}</div>}
        {jobs && !list.length && !pending && <div style={{ color: C.dim }}>Nothing downloading. Pick a game and a source to start.</div>}
        {pending && (
          <div style={{ display: "flex", gap: 12, alignItems: "center", background: C.panel, borderRadius: 6, padding: 10, marginBottom: 8 }}>
            <div style={{ flex: "none", width: 120, height: 56, borderRadius: 5, background: `url(${cdn(pending.appid, "header.jpg")}) center/cover` }} />
            <div style={{ flex: 1, minWidth: 0 }}>
              <Field label={pending.name} childrenLayout="below" bottomSeparator="none"
                description={progress
                  ? `Downloading in Firefox · ${bytes(progress.bytes)}${progress.total ? ` of ${bytes(progress.total)}` : ""}`
                  : "Waiting for the download to start in Firefox"}>
                <ProgressBar nProgress={progress?.total ? Math.min(99, (progress.bytes / progress.total) * 100) : 0} indeterminate={!progress?.total} />
              </Field>
              <div style={{ fontSize: 12, color: C.dim, marginTop: 4 }}>Mercury installs it when the file finishes. Firefox can stay in the background.</div>
            </div>
            <Focusable flow-children="horizontal" style={{ display: "flex", gap: 8 }}>
              <Btn style={{ width: 120, height: 30, fontSize: 12 }} onClick={backToBrowser}>Back to Firefox</Btn>
              <Btn style={{ width: 110, height: 30, fontSize: 12 }} onClick={() => { cancelBrowserDownload(); reload(); }}>Stop waiting</Btn>
            </Focusable>
          </div>
        )}
        <Focusable flow-children="vertical">
          {list.map((j) => (
            <div key={j.id} style={{ display: "flex", gap: 12, alignItems: "center", background: C.panel, borderRadius: 6, padding: 10, marginBottom: 8 }}>
              <div style={{ flex: "none", width: 120, height: 56, borderRadius: 5, background: `url(${cdn(j.appid, "header.jpg")}) center/cover` }} />
              <div style={{ flex: 1, minWidth: 0 }}>
                <JobProgress job={j} />
                {j.state === "needs_setup" && <div style={{ fontSize: 12, color: C.warn, marginTop: 4 }}>This is a repack. Run its installer and click through it. Mercury adds the game to Steam when the installer closes.</div>}
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
