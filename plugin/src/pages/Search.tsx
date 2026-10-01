import { Focusable, Navigation, TextField } from "@decky/ui";
import { useEffect, useState } from "react";
import { api, App, cdn } from "../api";
import { C, page, scrollIntoView } from "../ui";

export function Search() {
  const [q, setQ] = useState("");
  const [res, setRes] = useState<App[]>();
  const [err, setErr] = useState<string>();
  useEffect(() => {
    if (q.trim().length < 2) { setRes(undefined); return; }
    const t = setTimeout(() => api.search(q.trim()).then((r) => { setRes(r); setErr(undefined); }, (e) => setErr(e.message)), 400);
    return () => clearTimeout(t);
  }, [q]);
  return (
    <div style={page}>
      <div style={{ padding: "16px 28px 48px" }}>
        <div style={{ fontSize: 18, fontWeight: 700, color: "#fff", marginBottom: 8 }}>Search</div>
        <TextField label="Game name" value={q} focusOnMount onChange={(e) => setQ(e.target.value)} />
        {err && <div style={{ color: C.bad, marginTop: 10 }}>{err}</div>}
        {res && !res.length && <div style={{ color: C.dim, marginTop: 14 }}>No Steam games match.</div>}
        <Focusable flow-children="grid" style={{ display: "grid", gridTemplateColumns: "repeat(7, 1fr)", gap: 10, marginTop: 12 }}>
          {(res ?? []).map((a) => (
            <Focusable key={a.appid} onActivate={() => Navigation.Navigate(`/mercury/game/${a.appid}`)} onFocus={scrollIntoView}>
              <div style={{ aspectRatio: "2/3", borderRadius: 6, background: `${C.panel} url(${cdn(a.appid, "library_600x900.jpg")}) center/cover` }} />
              <div style={{ fontSize: 11, marginTop: 4, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{a.name}</div>
            </Focusable>
          ))}
        </Focusable>
      </div>
    </div>
  );
}
