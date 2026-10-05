import { Focusable, Navigation } from "@decky/ui";
import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { FaCog, FaDownload, FaSearch, FaThLarge } from "react-icons/fa";
import { api, App, cdn } from "../api";
import { usePoll } from "../hooks";
import { Btn, C, Cover, FOCUS, FocusStyle, page, scrollIntoView } from "../ui";

type Item = { appid: number; name: string; tag?: string; cover?: string | null };

/** Survives leaving the page (module state lives as long as the plugin), so B returns to the same cover. */
let lastFocus: Item | undefined;

const preloaded = new Set<number>();
function preload(appid?: number) {
  if (!appid || preloaded.has(appid)) return;
  preloaded.add(appid);
  for (const f of ["library_hero.jpg", "logo.png"] as const) new Image().src = cdn(appid, f);
}

/** Memoised so moving focus re-renders only the hero, not every cover. */
const Row = memo(function Row({ title, items, onFocus, focusId }: { title: string; items: Item[]; onFocus: (i: Item, idx: number, list: Item[]) => void; focusId?: number }) {
  // Bring the restored cover into view before Steam focuses it, so the row does not jump.
  const restore = useCallback((el: HTMLDivElement | null) => { el?.scrollIntoView({ block: "nearest", inline: "center" }); }, []);
  if (!items.length) return null;
  return (
    <div style={{ marginBottom: 10 }}>
      <div style={{ fontSize: 13, fontWeight: 600, color: "#fff", margin: "0 0 6px 32px" }}>{title}</div>
      <Focusable flow-children="horizontal" style={{ display: "flex", gap: 10, overflowX: "auto", padding: "4px 32px 6px", scrollbarWidth: "none" }}>
        {items.map((it, i) => (
          <Focusable key={it.appid} autoFocus={it.appid === focusId} ref={it.appid === focusId ? restore : undefined} focusClassName={FOCUS} noFocusRing
            onFocus={(e) => { onFocus(it, i, items); scrollIntoView(e); }}
            onActivate={() => Navigation.Navigate(`/mercury/game/${it.appid}`)}
            onOptionsButton={() => Navigation.Navigate("/mercury/search")} onOptionsActionDescription="Search"
            style={{ flex: "none", width: 88, borderRadius: 4, position: "relative" }}>
            <Cover app={it} width={88} radius={4} />
            {it.tag && <span style={{ position: "absolute", left: 4, bottom: 4, fontSize: 9, padding: "1px 4px", borderRadius: 3, background: "rgba(0,0,0,.75)" }}>{it.tag}</span>}
          </Focusable>
        ))}
      </Focusable>
    </div>
  );
});

export function Home() {
  const [wish] = usePoll(api.wishlist, 0);
  const [lib] = usePoll(api.library, 5000);
  const [jobs] = usePoll(api.jobs, 2000);
  const [cur, setCur] = useState<Item | undefined>(lastFocus);
  const [info, setInfo] = useState<App>();
  const settle = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  // One row: the Steam wishlist. Installed games stay in it with a tag; the Library page lists them on their own.
  const wishlist: Item[] = useMemo(() => {
    const have = new Set((lib ?? []).map((e) => e.appid));
    return (wish ?? []).map((a) => ({ appid: a.appid, name: a.name, cover: a.cover, tag: have.has(a.appid) ? "Installed" : undefined }));
  }, [wish, lib]);
  const active = (jobs ?? []).filter((j) => !["done", "failed", "cancelled"].includes(j.state)).length;

  // Focus moves instantly (Steam draws the outline); the hero follows once the D-pad stops.
  const onFocus = useCallback((it: Item, i: number, list: Item[]) => {
    lastFocus = it;
    clearTimeout(settle.current);
    settle.current = setTimeout(() => {
      setCur(it);
      preload(list[i + 1]?.appid);
      preload(list[i - 1]?.appid);
    }, 120);
  }, []);
  useEffect(() => () => clearTimeout(settle.current), []);

  useEffect(() => { if (!cur) setCur(wishlist[0]); }, [wishlist]);
  // Which cover gets focus when the page opens: the one you left from, if it is still listed.
  const focusId = useMemo(() => wishlist.find((i) => i.appid === lastFocus?.appid)?.appid ?? wishlist[0]?.appid, [wishlist]);
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
    <div style={{ ...page, overflow: "hidden", paddingBottom: 0, display: "flex", flexDirection: "column" }}>
      <FocusStyle />
      {g && <div key={g.appid} style={{ position: "absolute", inset: "0 0 auto 0", height: "75%", background: `url(${cdn(g.appid, "library_hero.jpg")}) center 30%/cover`, animation: "mercury-fade .25s ease" }} />}
      <div style={{ position: "absolute", inset: 0, background: `linear-gradient(90deg,rgba(14,20,27,.92),rgba(14,20,27,.45) 45%,transparent 75%),linear-gradient(transparent 30%,${C.bg} 72%)` }} />

      <Focusable flow-children="horizontal" style={{ position: "relative", display: "flex", gap: 8, justifyContent: "flex-end", padding: "10px 20px 0" }}>
        {nav.map(([t, Icon, to]) => (
          <Btn key={to} onClick={() => Navigation.Navigate(to)} style={{ height: 28, padding: "0 11px", fontSize: 12, background: "rgba(14,20,27,.55)", backdropFilter: "blur(6px)" }}><Icon size={11} />{t}</Btn>
        ))}
      </Focusable>

      <div style={{ position: "relative", flex: 1, minHeight: 0, padding: "4px 32px 0", width: 470 }}>
        {g && <div key={g.appid} style={{ animation: "mercury-fade .25s ease" }}>
          <img src={cdn(g.appid, "logo.png")} style={{ maxWidth: 300, maxHeight: 76, display: "block", filter: "drop-shadow(0 3px 12px rgba(0,0,0,.6))" }}
            onError={(e) => { (e.target as HTMLImageElement).style.display = "none"; }} />
          <div style={{ fontSize: 17, fontWeight: 700, marginTop: 6, color: "#fff" }}>{g.name}</div>
          <div style={{ color: "#b9bfc6", fontSize: 11, margin: "3px 0" }}>{[info?.genres?.join(" · "), info?.release, info?.developer].filter(Boolean).join(" · ")}</div>
          <div style={{ fontSize: 11, lineHeight: 1.45, color: "#c9cdd2", maxHeight: 32, overflow: "hidden" }}>{info?.description}</div>
        </div>}
      </div>

      <div style={{ position: "relative", flex: "none", height: 176, marginBottom: 40, overflowY: "auto", scrollbarWidth: "none" }}>
        {!wish && <div style={{ marginLeft: 32, color: C.dim, fontSize: 12 }}>Loading your Steam wishlist…</div>}
        <Row title="Your Steam wishlist" items={wishlist} onFocus={onFocus} focusId={focusId} />
      </div>
    </div>
  );
}
