import { ConfirmModal, DialogButton, Focusable, showModal } from "@decky/ui";
import { toaster } from "@decky/api";
import { api, cdn, Job } from "../api";
import { usePoll } from "../hooks";
import { runInstaller } from "../steam";
import { C, JobProgress, page } from "../ui";

function Actions({ job, reload }: { job: Job; reload: () => void }) {
  const act = (a: string) => async () => { try { await api.act(job.id, a); } catch (e: any) { toaster.toast({ title: "Mercury", body: e.message }); } reload(); };
  const b = (label: string, fn: () => void) => <DialogButton key={label} style={{ minWidth: 0, width: 110, height: 30, fontSize: 12, padding: "0 8px" }} onClick={fn}>{label}</DialogButton>;
  const out = [];
  if (["queued", "resolving", "caching", "downloading"].includes(job.state)) out.push(b("Pause", act("pause")));
  if (["paused", "failed"].includes(job.state)) out.push(b(job.state === "failed" ? "Retry" : "Resume", act("resume")));
  if (job.state === "needs_setup") out.push(b("Run installer", async () => {
    try { await runInstaller(job); } catch (e: any) { toaster.toast({ title: "Mercury", body: e.message }); }
    reload();
  }));
  if (job.state === "installing") out.push(b("Installer done", act("setup-done")));
  if (job.state !== "done" && job.state !== "cancelled") out.push(b("Cancel", () => showModal(
    <ConfirmModal strTitle={`Cancel ${job.name}?`} strDescription="Stops the download and deletes its files." strOKButtonText="Cancel download" onOK={act("cancel")} />)));
  return <Focusable flow-children="horizontal" style={{ display: "flex", gap: 8 }}>{out}</Focusable>;
}

export function Downloads() {
  const [jobs, err, reload] = usePoll(api.jobs, 1000);
  const list = (jobs ?? []).slice().reverse();
  return (
    <div style={page}>
      <div style={{ padding: "16px 28px 48px" }}>
        <div style={{ fontSize: 18, fontWeight: 700, color: "#fff", marginBottom: 10 }}>Downloads</div>
        {err && <div style={{ color: C.bad }}>Mercury engine is not responding: {err}</div>}
        {jobs && !list.length && <div style={{ color: C.dim }}>Nothing downloading. Pick a game and a source to start.</div>}
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
