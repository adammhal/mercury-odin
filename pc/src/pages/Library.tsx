import { Focusable, Navigation, TextField } from "@decky/ui";
import { toaster } from "@decky/api";
import { useRef, useState } from "react";
import { api, bytes, cdn, gridArt } from "@shared/api";
import { usePoll } from "@shared/hooks";
import { Btn, C, FocusStyle, page, scrollIntoView } from "@shared/ui";
import { pc } from "../pcapi";

export function Library() {
  const [lib] = usePoll(api.library, 5000);
  const [q, setQ] = useState("");
  const stamp = useRef(Date.now()).current;
  const norm = (s: string) => s.toLowerCase().replace(/[^a-z0-9]/g, "");
  const shown = (lib ?? []).filter((e) => !q.trim() || norm(e.name).includes(norm(q)));
  const play = (id: number) => pc.play(id).catch((e) => toaster.toast({ title: "Mercury", body: e.message }));
  return (
    <div style={page}>
      <FocusStyle />
      <div style={{ padding: "16px 28px 48px" }}>
        <div style={{ fontSize: 18, fontWeight: 700, color: "#fff", marginBottom: 10 }}>
          Installed with Mercury <span style={{ fontSize: 14, color: C.dim, fontWeight: 500, marginRight: 8 }}>{shown.length === (lib ?? []).length ? `${shown.length} games` : `${shown.length} of ${(lib ?? []).length}`}</span><span style={{ fontSize: 14, color: C.dim, fontWeight: 500 }}>{bytes((lib ?? []).reduce((a, e) => a + e.size, 0))}</span>
        </div>
        {lib && lib.length > 0 && <TextField label="Search your library" value={q} onChange={(e) => setQ(e.target.value)} />}
        {lib && !lib.length && <div style={{ color: C.dim }}>No games installed with Mercury yet. Games you install also appear in Steam.</div>}
        {lib && lib.length > 0 && !shown.length && <div style={{ color: C.dim, marginTop: 12 }}>No installed game matches &ldquo;{q}&rdquo;.</div>}
        <Focusable flow-children="vertical">
          {shown.map((e) => (
            <Focusable key={e.appid} flow-children="horizontal" onFocus={scrollIntoView}
              style={{ display: "flex", gap: 12, alignItems: "center", background: C.panel, borderRadius: 6, padding: 10, marginBottom: 8 }}>
              <div style={{ flex: "none", width: 120, height: 56, borderRadius: 5, background: `${e.shortcut_id ? `url(${gridArt(e.shortcut_id, 3, stamp)}) center/cover, ` : ""}url(${cdn(e.appid, "header.jpg")}) center/cover` }} />
              <div style={{ flex: 1 }}>
                <div style={{ fontWeight: 700, fontSize: 14 }}>{e.name}</div>
                <div style={{ fontSize: 12, color: C.dim, marginTop: 4 }}>{e.provider}{e.version ? ` ${e.version}` : ""} · {bytes(e.size)} · {new Date(e.installed * 1000).toLocaleDateString()}</div>
              </div>
              <Btn style={{ width: 90 }} onClick={() => play(e.appid)}>Play</Btn>
              <Btn style={{ width: 90 }} onClick={() => Navigation.Navigate(`/mercury/game/${e.appid}`)}>Details</Btn>
            </Focusable>
          ))}
        </Focusable>
      </div>
    </div>
  );
}
