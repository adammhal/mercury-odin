import { Field, Focusable, ProgressBar } from "@decky/ui";
import { CSSProperties, ReactNode } from "react";
import { bytes, Job, jobPercent, STATE_LABEL } from "./api";

export const C = { bg: "#0e141b", panel: "#1f242c", panel2: "#2a303a", text: "#dcdedf", dim: "#8b929a", accent: "#1a9fff", ok: "#a6ec6b", warn: "#f2c35b", bad: "#ff6b6b" };

/** Route pages sit between Steam's 40px header and its ~40px button bar. The bottom padding and
 * scroll padding keep the last item (and anything scrolled into view) clear of the button bar. */
export const page: CSSProperties = {
  position: "absolute", top: 40, left: 0, right: 0, bottom: 0, overflowY: "auto",
  paddingBottom: 56, scrollPaddingBottom: 56, scrollPaddingTop: 12, boxSizing: "border-box", background: C.bg, color: C.text,
};

export function Chip({ children, tone }: { children: ReactNode; tone?: "ok" | "warn" | "bad" | "accent" }) {
  const col = tone ? C[tone] : C.text;
  return <span style={{ display: "inline-block", maxWidth: 140, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", verticalAlign: "middle",
    fontSize: 11, fontWeight: 600, padding: "2px 8px", borderRadius: 99, marginLeft: 6, color: col, background: "rgba(255,255,255,.08)" }}>{children}</span>;
}

/** The working progress layout from spike S3: a Field with the bar below. ProgressBarItem overflows panels. */
export function JobProgress({ job, label }: { job: Job; label?: ReactNode }) {
  const pct = jobPercent(job);
  const detail = job.state === "downloading" && job.total
    ? `${bytes(job.done)} of ${bytes(job.total)}${job.speed ? ` · ${bytes(job.speed)}/s` : ""}`
    : job.state === "caching" || (job.state === "extracting" && job.total) ? `${pct.toFixed(0)}%` : "";
  return (
    <Field label={label ?? (job.update_of ? `Updating ${job.name}` : job.name)} description={job.error ?? `${STATE_LABEL[job.state]}${detail ? ` · ${detail}` : ""}`} childrenLayout="below" bottomSeparator="none">
      <ProgressBar nProgress={pct} indeterminate={["resolving", "queued"].includes(job.state) || (job.state === "extracting" && !job.total)} />
    </Field>
  );
}

let lastScroll = 0;
/** Smooth for single steps; instant while a direction is held, so scrolling never queues up behind focus. */
export const scrollIntoView = (e: { currentTarget?: unknown; target?: unknown }) => {
  const now = Date.now();
  const held = now - lastScroll < 180;
  lastScroll = now;
  ((e.currentTarget ?? e.target) as HTMLElement | undefined)?.scrollIntoView?.({ behavior: held ? "auto" : "smooth", block: "nearest", inline: "nearest" });
};

/** One focus look for every Mercury control: a white outline with a soft glow, no dark fill. */
export const FOCUS = "mercury-focus";
export function FocusStyle() {
  return <style>{`
    .${FOCUS} { box-shadow: 0 0 0 2px #fff, 0 0 14px rgba(255,255,255,.35) !important; }
    .mercury-btn { transition: background .1s; }
    .mercury-btn.${FOCUS} { background: rgba(255,255,255,.2) !important; }
    @keyframes mercury-fade { from { opacity: 0 } to { opacity: 1 } }
  `}</style>;
}

export function Btn({ children, onClick, disabled, style, autoFocus }: { children: ReactNode; onClick: () => void; disabled?: boolean; style?: CSSProperties; autoFocus?: boolean }) {
  return (
    <Focusable className="mercury-btn" focusClassName={FOCUS} noFocusRing autoFocus={autoFocus}
      onActivate={() => !disabled && onClick()} onClick={() => !disabled && onClick()}
      style={{ height: 32, padding: "0 14px", borderRadius: 4, display: "flex", alignItems: "center", justifyContent: "center", gap: 6,
        fontSize: 13, fontWeight: 600, color: "#fff", background: "rgba(255,255,255,.1)", opacity: disabled ? 0.45 : 1, flex: "none", ...style }}>
      {children}
    </Focusable>
  );
}
