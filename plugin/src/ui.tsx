import { Field, ProgressBar } from "@decky/ui";
import { CSSProperties, ReactNode } from "react";
import { bytes, Job, jobPercent, STATE_LABEL } from "./api";

export const C = { bg: "#0e141b", panel: "#1f242c", panel2: "#2a303a", text: "#dcdedf", dim: "#8b929a", accent: "#1a9fff", ok: "#a6ec6b", warn: "#f2c35b", bad: "#ff6b6b" };

/** Route pages render below Steam's 40px header. */
export const page: CSSProperties = { position: "absolute", inset: 0, marginTop: 40, overflowY: "auto", background: C.bg, color: C.text };

export function Chip({ children, tone }: { children: ReactNode; tone?: "ok" | "warn" | "bad" | "accent" }) {
  const col = tone ? C[tone] : C.text;
  return <span style={{ display: "inline-block", fontSize: 11, fontWeight: 600, padding: "2px 8px", borderRadius: 99, marginRight: 6, color: col, background: "rgba(255,255,255,.08)" }}>{children}</span>;
}

/** The working progress layout from spike S3: a Field with the bar below. ProgressBarItem overflows panels. */
export function JobProgress({ job, label }: { job: Job; label?: ReactNode }) {
  const pct = jobPercent(job);
  const detail = job.state === "downloading" && job.total
    ? `${bytes(job.done)} of ${bytes(job.total)}${job.speed ? ` · ${bytes(job.speed)}/s` : ""}`
    : job.state === "caching" ? `${pct.toFixed(0)}%` : "";
  return (
    <Field label={label ?? job.name} description={job.error ?? `${STATE_LABEL[job.state]}${detail ? ` · ${detail}` : ""}`} childrenLayout="below" bottomSeparator="none">
      <ProgressBar nProgress={pct} indeterminate={["resolving", "extracting", "queued"].includes(job.state)} />
    </Field>
  );
}

export const scrollIntoView = (e: { currentTarget?: unknown; target?: unknown }) =>
  ((e.currentTarget ?? e.target) as HTMLElement | undefined)?.scrollIntoView?.({ behavior: "smooth", block: "nearest", inline: "center" });
