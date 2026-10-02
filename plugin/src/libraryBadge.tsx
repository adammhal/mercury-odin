import { afterPatch, appDetailsClasses, createReactTreePatcher, findInReactTree, Focusable, Navigation, useParams } from "@decky/ui";
import { routerHook } from "@decky/api";
import { useEffect, useState } from "react";
import { api, Entry, UpdateInfo } from "./api";
import { FOCUS } from "./ui";

/** Installed-games list shared by every badge, refreshed at most every 10 s. */
let cache: { at: number; lib: Entry[] } = { at: 0, lib: [] };
async function library(): Promise<Entry[]> {
  if (Date.now() - cache.at > 10000) cache = { at: Date.now(), lib: await api.library().catch(() => cache.lib) };
  return cache.lib;
}

/** Shown on a Steam library page when the game (a shortcut) was installed by Mercury. */
function Badge() {
  const { appid } = useParams<{ appid: string }>();
  const [entry, setEntry] = useState<Entry>();
  const [update, setUpdate] = useState<UpdateInfo>();
  useEffect(() => {
    let live = true;
    library().then((lib) => {
      const e = lib.find((x) => String(x.shortcut_id) === appid);
      if (!live || !e) return;
      setEntry(e);
      api.updateCheck(e.appid).then((u) => live && setUpdate(u), () => {});
    });
    return () => { live = false; };
  }, [appid]);
  if (!entry) return null;
  const version = update?.current ?? entry.version;
  return (
    <Focusable focusClassName={FOCUS} noFocusRing onActivate={() => Navigation.Navigate(`/mercury/game/${entry.appid}`)}
      onClick={() => Navigation.Navigate(`/mercury/game/${entry.appid}`)}
      style={{ position: "absolute", top: 24, right: 24, zIndex: 3, display: "flex", alignItems: "center", gap: 8, padding: "6px 12px",
        borderRadius: 6, background: "rgba(14,20,27,.82)", border: "1px solid rgba(26,159,255,.45)", fontSize: 12, color: "#dcdedf" }}>
      <b style={{ color: "#8ccfff", letterSpacing: ".06em" }}>MERCURY</b>
      <span>{entry.provider}{version ? ` · ${version}` : ""}</span>
      {update?.newer && <span style={{ color: "#a6ec6b", fontWeight: 700 }}>Update {update.newer.version}</span>}
    </Focusable>
  );
}

/** Same technique as protondb-decky: patch the app page's render and insert into its inner container. */
export function patchLibraryPage() {
  return routerHook.addPatch("/library/app/:appid", (tree: any) => {
    const routeProps = findInReactTree(tree, (x: any) => x?.renderFunc);
    if (routeProps) {
      const handler = createReactTreePatcher([
        (t: any) => findInReactTree(t, (x: any) => x?.props?.children?.props?.overview)?.props?.children,
      ], (_: unknown, ret: any) => {
        const container = findInReactTree(ret, (x: any) => Array.isArray(x?.props?.children) &&
          x?.props?.className?.includes(appDetailsClasses.InnerContainer));
        if (typeof container !== "object") return ret;
        if (!container.props.children.some((c: any) => c?.key === "mercury-badge")) {
          container.props.children.splice(2, 0, <Badge key="mercury-badge" />);
        }
        return ret;
      });
      afterPatch(routeProps, "renderFunc", handler);
    }
    return tree;
  });
}
