import { Focusable, ModalRoot, Navigation, showModal, Spinner, TextField, useParams } from "@decky/ui";
import { toaster } from "@decky/api";
import { useEffect, useState } from "react";
import { api, cdn, SgdbOpt } from "../api";
import { usePoll } from "../hooks";
import { confirmReview } from "../review";
import { Btn, C, FOCUS, FocusStyle, page, scrollIntoView } from "../ui";

export type Slot = { slot: number; label: string; file: Parameters<typeof cdn>[1]; w: number; h: number; fit: "cover" | "contain" };
export const SLOTS: Slot[] = [
  { slot: 0, label: "Cover", file: "library_600x900.jpg", w: 104, h: 156, fit: "cover" },
  { slot: 1, label: "Hero", file: "library_hero.jpg", w: 252, h: 82, fit: "cover" },
  { slot: 2, label: "Logo", file: "logo.png", w: 150, h: 82, fit: "contain" },
  { slot: 3, label: "Wide", file: "header.jpg", w: 176, h: 82, fit: "cover" },
];

export function Art({ src, s, tile }: { src: string; s: Slot; tile?: { w: number; h: number } }) {
  const [bad, setBad] = useState(false);
  const w = tile?.w ?? s.w, h = tile?.h ?? s.h;
  useEffect(() => setBad(false), [src]);
  return <div style={{ width: w, height: h, borderRadius: 4, background: "#161b22", overflow: "hidden", display: "flex", alignItems: "center", justifyContent: "center" }}>
    {bad ? <span style={{ fontSize: 10, color: C.dim }}>No image</span>
      : <img src={src} alt="" onError={() => setBad(true)} style={{ width: "100%", height: "100%", objectFit: s.fit, display: "block" }} />}
  </div>;
}

/** Choose one SteamGridDB image for a slot. The first tile keeps Steam's own store art. */
export function ArtPicker({ appid, s, name, onPick, closeModal }: { appid: number; s: Slot; name?: string; onPick: (o: SgdbOpt | null) => void; closeModal?: () => void }) {
  const [opts, setOpts] = useState<SgdbOpt[]>();
  const [err, setErr] = useState<string>();
  useEffect(() => { api.sgdb(appid, s.slot, name).then((r) => setOpts(r.options), (e) => setErr(e.message)); }, []);
  const scale = s.slot === 0 ? 0.9 : s.slot === 1 ? 0.8 : 0.9;
  const tile = { w: Math.round(s.w * scale), h: Math.round(s.h * scale) };
  const pick = (o: SgdbOpt | null) => { closeModal?.(); onPick(o); };
  return (
    <ModalRoot closeModal={closeModal}>
      <div style={{ width: 760, maxWidth: "96%", background: "#171d25", borderRadius: 6, padding: "16px 18px" }}>
        <div style={{ fontSize: 16, fontWeight: 700, color: "#fff", marginBottom: 10 }}>Choose {s.label.toLowerCase()} art</div>
        {err && <div style={{ color: C.bad, fontSize: 13, marginBottom: 8 }}>{err}</div>}
        {!opts && !err && <div style={{ display: "flex", alignItems: "center", gap: 8, color: C.dim }}><Spinner style={{ width: 18, height: 18 }} /> Loading SteamGridDB…</div>}
        <Focusable flow-children="grid" style={{ display: "flex", flexWrap: "wrap", gap: 10, maxHeight: 340, overflowY: "auto", padding: 4, scrollPaddingBottom: 12 }}>
          <Focusable autoFocus focusClassName={FOCUS} noFocusRing onActivate={() => pick(null)} onFocus={scrollIntoView} style={{ borderRadius: 4 }}>
            <div style={{ width: tile.w, height: tile.h, borderRadius: 4, background: C.panel2, display: "flex", alignItems: "center", justifyContent: "center", fontSize: 11, fontWeight: 700, color: "#fff", textAlign: "center", padding: 6, boxSizing: "border-box" }}>Steam&apos;s own art</div>
          </Focusable>
          {(opts ?? []).map((o) => (
            <Focusable key={o.url} focusClassName={FOCUS} noFocusRing onActivate={() => pick(o)} onFocus={scrollIntoView} style={{ borderRadius: 4 }}>
              <Art src={o.thumb} s={s} tile={tile} />
            </Focusable>
          ))}
        </Focusable>
        {opts && !opts.length && <div style={{ color: C.dim, fontSize: 13, marginTop: 8 }}>SteamGridDB has no {s.label.toLowerCase()} images for this game.</div>}
      </div>
    </ModalRoot>
  );
}

/** Shown after a game finishes installing and before it goes to Steam: confirm or change the title and artwork. */
export function Review() {
  const { id } = useParams<{ id: string }>();
  const [jobs] = usePoll(api.jobs, 2500);
  const job = jobs?.find((j) => j.id === Number(id));
  const [name, setName] = useState<string>();
  const [picked, setPicked] = useState<Record<number, SgdbOpt>>({});
  const [busy, setBusy] = useState(false);
  useEffect(() => { if (job && name === undefined) setName(job.name.replace(/[\u2122\u00ae\u00a9]/g, "").trim()); }, [job?.id]);

  if (!jobs) return <div style={page}><div style={{ padding: 48, color: C.dim }}>Loading…</div></div>;
  if (!job || job.state !== "review") return (
    <div style={page}><FocusStyle /><div style={{ padding: "28px" }}>
      <div style={{ color: C.dim, marginBottom: 14 }}>{job ? "This game is no longer waiting for review." : "This game is no longer in Mercury's list."}</div>
      <Btn autoFocus onClick={() => Navigation.Navigate("/mercury/downloads")}>Back to downloads</Btn>
    </div></div>
  );

  const choose = (s: Slot) => showModal(<ArtPicker appid={job.appid} s={s} name={job.name} onPick={(o) => setPicked((p) => { const n = { ...p }; if (o) n[s.slot] = o; else delete n[s.slot]; return n; })} />);
  const confirm = async () => {
    setBusy(true);
    try {
      const art: Record<string, string> = {};
      for (const [k, o] of Object.entries(picked)) art[k] = o.url;
      await confirmReview(job, { name: (name ?? job.name).trim() || job.name, art });
      toaster.toast({ title: "Mercury", body: `${name ?? job.name} sent to Steam` });
      Navigation.Navigate("/mercury/downloads");
    } catch (e: any) { toaster.toast({ title: "Mercury", body: e.message }); setBusy(false); }
  };

  return (
    <div style={page}>
      <FocusStyle />
      <div style={{ padding: "14px 28px 48px" }}>
        <div style={{ fontSize: 18, fontWeight: 700, color: "#fff" }}>Review before adding to Steam</div>
        <div style={{ fontSize: 12, color: C.dim, margin: "2px 0 10px" }}>{job.name} is installed. Check the title and artwork, change what you like, then confirm.</div>
        <TextField label="Title in Steam" value={name ?? ""} onChange={(e) => setName(e.target.value)} />
        <Focusable flow-children="horizontal" style={{ display: "flex", gap: 14, margin: "14px 0", alignItems: "flex-end" }}>
          {SLOTS.map((s) => {
            const o = picked[s.slot];
            return (
              <Focusable key={s.slot} focusClassName={FOCUS} noFocusRing onActivate={() => choose(s)} onFocus={scrollIntoView} style={{ borderRadius: 4, padding: 2 }}>
                <Art src={o ? o.thumb : cdn(job.appid, s.file)} s={s} />
                <div style={{ fontSize: 11, marginTop: 4, color: o ? C.accent : C.dim }}>{s.label} · {o ? "SteamGridDB" : "Steam"}</div>
              </Focusable>
            );
          })}
        </Focusable>
        <Focusable flow-children="horizontal" style={{ display: "flex", gap: 10 }}>
          <Btn autoFocus disabled={busy} style={{ width: 200 }} onClick={confirm}>{busy ? "Sending…" : "Add to Steam"}</Btn>
          <Btn disabled={busy} style={{ width: 140 }} onClick={() => Navigation.Navigate("/mercury/downloads")}>Not now</Btn>
        </Focusable>
      </div>
    </div>
  );
}
