import { Focusable, Navigation, TextField } from "@decky/ui";
import { useEffect, useState } from "react";
import { api, App } from "../api";
import { C, Cover, FOCUS, FocusStyle, page, scrollIntoView } from "../ui";

/** Kept across visits so backing out of a result returns to the same search and result. */
let saved: { q: string; res?: App[]; resQ?: string; focus?: number } = { q: "" };

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
              <Cover app={a} />
              <div style={{ fontSize: 11, marginTop: 4, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{a.name}</div>
            </Focusable>
          ))}
        </Focusable>
      </div>
    </div>
  );
}
