import { Focusable, Navigation, showModal, TextField, useParams } from "@decky/ui";
import { toaster } from "@decky/api";
import { useRef, useState } from "react";
import { api, cdn, gridArt, SgdbOpt } from "@shared/api";
import { usePoll } from "@shared/hooks";
import { Art, ArtPicker, Slot, SLOTS } from "@shared/pages/Review";
import { Btn, C, FOCUS, FocusStyle, page, scrollIntoView } from "@shared/ui";

/** Change the title and artwork of a game that is already in Steam. Slots you do not touch stay as they are. */
export function EditArt() {
  const { appid: raw } = useParams<{ appid: string }>();
  const appid = Number(raw);
  const [lib] = usePoll(api.library, 0);
  const entry = lib?.find((e) => e.appid === appid);
  const stamp = useRef(Date.now());
  const [name, setName] = useState<string>();
  const [picked, setPicked] = useState<Record<number, SgdbOpt>>({});
  const [busy, setBusy] = useState(false);

  if (!lib) return <div style={page}><div style={{ padding: 48, color: C.dim }}>Loading…</div></div>;
  if (!entry) return <div style={page}><FocusStyle /><div style={{ padding: 28 }}><div style={{ color: C.dim, marginBottom: 14 }}>This game is not installed with Mercury.</div>
    <Btn autoFocus onClick={() => Navigation.Navigate("/mercury/library")}>Back to library</Btn></div></div>;

  const title = name ?? entry.name;
  const changed = Object.keys(picked).length > 0 || title.trim() !== entry.name;
  const choose = (s: Slot) => showModal(<ArtPicker appid={appid} s={s} onPick={(o) =>
    setPicked((p) => ({ ...p, [s.slot]: o ?? { url: "default", thumb: cdn(appid, s.file), score: 0, width: 0, height: 0, author: "" } }))} />);
  const save = async () => {
    setBusy(true);
    try {
      const art: Record<string, string> = {};
      for (const [k, o] of Object.entries(picked)) art[k] = o.url;
      await api.editArt(appid, { name: title.trim() !== entry.name ? title.trim() : undefined, art });
      toaster.toast({ title: "Mercury", body: `${title.trim() || entry.name} updated in Steam` });
      Navigation.Navigate(`/mercury/game/${appid}`);
    } catch (e: any) { toaster.toast({ title: "Mercury", body: e.message }); setBusy(false); }
  };

  return (
    <div style={page}>
      <FocusStyle />
      <div style={{ padding: "14px 28px 48px" }}>
        <div style={{ fontSize: 18, fontWeight: 700, color: "#fff" }}>Edit artwork</div>
        <div style={{ fontSize: 12, color: C.dim, margin: "2px 0 10px" }}>Pick a slot to replace it. Only the slots you change are sent to Steam.</div>
        <TextField label="Title in Steam" value={title} onChange={(e) => setName(e.target.value)} />
        <Focusable flow-children="horizontal" style={{ display: "flex", gap: 14, margin: "14px 0", alignItems: "flex-end" }}>
          {SLOTS.map((s) => {
            const o = picked[s.slot];
            return (
              <Focusable key={s.slot} focusClassName={FOCUS} noFocusRing onActivate={() => choose(s)} onFocus={scrollIntoView} style={{ borderRadius: 4, padding: 2 }}>
                <Art src={o ? o.thumb : gridArt(entry.shortcut_id, s.slot, stamp.current)} s={s} />
                <div style={{ fontSize: 11, marginTop: 4, color: o ? C.accent : C.dim }}>{s.label} · {o ? (o.url === "default" ? "Steam's own" : "SteamGridDB") : "Current"}</div>
              </Focusable>
            );
          })}
        </Focusable>
        <Focusable flow-children="horizontal" style={{ display: "flex", gap: 10 }}>
          <Btn autoFocus disabled={busy || !changed} style={{ width: 200 }} onClick={save}>{busy ? "Sending…" : "Save to Steam"}</Btn>
          <Btn disabled={busy} style={{ width: 140 }} onClick={() => Navigation.Navigate(`/mercury/game/${appid}`)}>Cancel</Btn>
        </Focusable>
      </div>
    </div>
  );
}
