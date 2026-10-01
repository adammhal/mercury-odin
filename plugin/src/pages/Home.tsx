import { DialogButton, Focusable, Navigation } from "@decky/ui";
import { useEffect, useMemo, useState } from "react";
import { FaCog, FaDownload, FaSearch, FaThLarge } from "react-icons/fa";
import { api, App, cdn } from "../api";
import { usePoll } from "../hooks";
import { C, page, scrollIntoView } from "../ui";

type Item = { appid: number; name: string; tag?: string };

function Row({ title, items, onFocus, first }: { title: string; items: Item[]; onFocus: (i: Item) => void; first?: boolean }) {
  if (!items.length) return null;
  return (
    <div style={{ marginBottom: 10 }}>
      <div style={{ fontSize: 13, fontWeight: 600, color: "#fff", margin: "0 0 6px 32px" }}>{title}</div>
      <Focusable flow-children="horizontal" style={{ display: "flex", gap: 10, overflowX: "auto", padding: "3px 32px 6px", scrollbarWidth: "none" }}>
        {items.map((it, i) => (
          <Focusable key={it.appid} autoFocus={first && i === 0}
            onFocus={(e) => { onFocus(it); scrollIntoView(e); }}
            onActivate={() => Navigation.Navigate(`/mercury/game/${it.appid}`)}
            onOptionsButton={() => Navigation.Navigate("/mercury/search")} onOptionsActionDescription="Search"
            style={{ flex: "none", width: 88, height: 132, borderRadius: 4, position: "relative", background: `${C.panel} url(${cdn(it.appid, "library_600x900.jpg")}) center/cover` }}>
            {it.tag && <span style={{ position: "absolute", left: 4, bottom: 4, fontSize: 9, padding: "1px 4px", borderRadius: 3, background: "rgba(0,0,0,.75)" }}>{it.tag}</span>}
          </Focusable>
        ))}
      </Focusable>
    </div>
  );
}

export function Home() {
  const [wish] = usePoll(api.wishlist, 0);
  const [lib] = usePoll(api.library, 5000);
  const [jobs] = usePoll(api.jobs, 2000);
  const [cur, setCur] = useState<Item>();
  const [info, setInfo] = useState<App>();

  const installed: Item[] = useMemo(() => (lib ?? []).map((e) => ({ appid: e.appid, name: e.name, tag: "Installed" })), [lib]);
  const wishlist: Item[] = useMemo(() => {
    const have = new Set(installed.map((i) => i.appid));
    return (wish ?? []).filter((a) => !have.has(a.appid)).map((a) => ({ appid: a.appid, name: a.name }));
  }, [wish, installed]);
  const active = (jobs ?? []).filter((j) => !["done", "failed", "cancelled"].includes(j.state)).length;

  useEffect(() => { if (!cur) setCur(installed[0] ?? wishlist[0]); }, [installed, wishlist]);
  useEffect(() => {
    if (!cur) return;
    const w = wish?.find((a) => a.appid === cur.appid);
    if (w?.description) { setInfo(w); return; }
    let live = true;
    api.app(cur.appid).then((a) => live && setInfo(a), () => {});
    return () => { live = false; };
  }, [cur?.appid]);

  const g = cur;
  const nav: [string, any, string][] = [["Search", FaSearch, "/mercury/search"], [`Downloads${active ? ` (${active})` : ""}`, FaDownload, "/mercury/downloads"], ["Library", FaThLarge, "/mercury/library"], ["Settings", FaCog, "/mercury/settings"]];
  // Steam renders pages in a 910x512 CSS viewport on the Odin (2.11x scale), minus its 40px header.
  return (
    <div style={{ ...page, overflow: "hidden", display: "flex", flexDirection: "column" }}>
      {g && <div key={g.appid} style={{ position: "absolute", inset: "0 0 auto 0", height: "75%", background: `url(${cdn(g.appid, "library_hero.jpg")}) center 30%/cover` }} />}
      <div style={{ position: "absolute", inset: 0, background: `linear-gradient(90deg,rgba(14,20,27,.92),rgba(14,20,27,.45) 45%,transparent 75%),linear-gradient(transparent 30%,${C.bg} 72%)` }} />

      <Focusable flow-children="horizontal" style={{ position: "relative", display: "flex", gap: 6, justifyContent: "flex-end", padding: "10px 20px 0" }}>
        {nav.map(([t, Icon, to]) => (
          <DialogButton key={to} style={{ minWidth: 0, width: "auto", height: 28, padding: "0 10px", fontSize: 12, display: "flex", alignItems: "center", gap: 6, background: "rgba(14,20,27,.72)", backdropFilter: "blur(6px)" }} onClick={() => Navigation.Navigate(to)}><Icon size={11} />{t}</DialogButton>
        ))}
      </Focusable>

      <div style={{ position: "relative", flex: 1, minHeight: 0, padding: "4px 32px 0", width: 470 }}>
        {g && <>
          <img key={g.appid} src={cdn(g.appid, "logo.png")} style={{ maxWidth: 300, maxHeight: 76, display: "block", filter: "drop-shadow(0 3px 12px rgba(0,0,0,.6))" }}
            onError={(e) => { (e.target as HTMLImageElement).style.display = "none"; }} />
          <div style={{ fontSize: 17, fontWeight: 700, marginTop: 6, color: "#fff" }}>{g.name}</div>
          <div style={{ color: "#b9bfc6", fontSize: 11, margin: "3px 0" }}>{[info?.genres?.join(" · "), info?.release, info?.developer].filter(Boolean).join(" · ")}</div>
          <div style={{ fontSize: 11, lineHeight: 1.45, color: "#c9cdd2", maxHeight: 32, overflow: "hidden" }}>{info?.description}</div>
        </>}
      </div>

      <div style={{ position: "relative", flex: "none", height: 172, marginBottom: 40, overflowY: "auto", scrollbarWidth: "none" }}>
        {!wish && !lib && <div style={{ marginLeft: 32, color: C.dim, fontSize: 12 }}>Loading your Steam wishlist…</div>}
        <Row title="Installed with Mercury" items={installed} onFocus={setCur} first />
        <Row title="Your Steam wishlist" items={wishlist} onFocus={setCur} first={!installed.length} />
      </div>
    </div>
  );
}
