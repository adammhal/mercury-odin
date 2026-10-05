import { Focusable, Navigation, TextField } from "@decky/ui";
import { CSSProperties, useEffect, useState } from "react";
import { api, App, cdn } from "../api";
import { C, FOCUS, FocusStyle, page, scrollIntoView } from "../ui";

/** Kept across visits so backing out of a result returns to the same search and result. */
let saved: { q: string; res?: App[]; resQ?: string; focus?: number } = { q: "" };

/** Cover art with fallbacks: many Steam apps have no 600x900 library image, so try the wide header, then show the name. */
function Cover({ id, name }: { id: number; name: string }) {
  const [step, setStep] = useState(0);
  const base: CSSProperties = { aspectRatio: "2/3", borderRadius: 6, background: C.panel, width: "100%", display: "block", overflow: "hidden" };
  if (step < 2) {
    const wide = step === 1;
    return <div style={{ ...base, position: "relative" }}>
      {wide && <img src={cdn(id, "header.jpg")} alt="" aria-hidden style={{ position: "absolute", inset: 0, width: "100%", height: "100%", objectFit: "cover", filter: "blur(14px) brightness(.55)", transform: "scale(1.2)" }} />}
      <img key={step} src={cdn(id, wide ? "header.jpg" : "library_600x900.jpg")} alt="" onError={() => setStep(step + 1)}
        style={{ position: "absolute", inset: 0, width: "100%", height: "100%", objectFit: wide ? "contain" : "cover" }} />
    </div>;
  }
  return <div style={{ ...base, display: "flex", alignItems: "center", justifyContent: "center", padding: 8, boxSizing: "border-box", textAlign: "center", fontSize: 12, fontWeight: 700, color: C.dim }}>{name}</div>;
}

export function Search() {
  const [q, setQ] = useState(saved.q);
  const [res, setRes] = useState<App[] | undefined>(saved.res);
  const [err, setErr] = useState<string>();
  useEffect(() => {
    saved.q = q;
    const term = q.trim();
    if (term.length < 2) { setRes(undefined); saved.res = undefined; saved.resQ = undefined; return; }
    if (saved.res && saved.resQ === term) { setRes(saved.res); return; }
    saved.focus = undefined;
    const t = setTimeout(() => api.search(term).then((r) => { setRes(r); saved.res = r; saved.resQ = term; setErr(undefined); }, (e) => setErr(e.message)), 400);
    return () => clearTimeout(t);
  }, [q]);
  return (
    <div style={page}>
      <FocusStyle />
      <div style={{ padding: "16px 28px 48px" }}>
        <div style={{ fontSize: 18, fontWeight: 700, color: "#fff", marginBottom: 8 }}>Search</div>
        <TextField label="Game name" value={q} focusOnMount={!saved.focus} onChange={(e) => setQ(e.target.value)} />
        {err && <div style={{ color: C.bad, marginTop: 10 }}>{err}</div>}
        {res && !res.length && <div style={{ color: C.dim, marginTop: 14 }}>No Steam games match.</div>}
        <Focusable flow-children="grid" style={{ display: "grid", gridTemplateColumns: "repeat(7, minmax(0, 1fr))", gap: 10, marginTop: 12 }}>
          {(res ?? []).map((a) => (
            <Focusable key={a.appid} autoFocus={a.appid === saved.focus} focusClassName={FOCUS} noFocusRing style={{ borderRadius: 6, minWidth: 0 }}
              onActivate={() => Navigation.Navigate(`/mercury/game/${a.appid}`)} onFocus={(e) => { saved.focus = a.appid; scrollIntoView(e); }}>
              <Cover id={a.appid} name={a.name} />
              <div style={{ fontSize: 11, marginTop: 4, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{a.name}</div>
            </Focusable>
          ))}
        </Focusable>
      </div>
    </div>
  );
}
