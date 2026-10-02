import { ButtonItem, Navigation, PanelSection, PanelSectionRow } from "@decky/ui";
import { callable, useQuickAccessVisible } from "@decky/api";
import { useEffect, useState } from "react";
import { api } from "./api";
import { cancelBrowserDownload, pending } from "./browser";
import { usePoll } from "./hooks";
import { C, JobProgress } from "./ui";

const storeApp = callable<[], number>("store_app");

const go = (to: string) => { Navigation.Navigate(to); Navigation.CloseSideMenus(); };

/** The store's browser tab stays alive after you leave it, so only trust it while the store is on screen. */
const onStorePage = () =>
  ((window as any).SteamUIStore?.WindowStore?.GamepadUIMainWindowInstance?.m_history?.location?.pathname ?? "").startsWith("/steamweb");

export function Panel() {
  const visible = useQuickAccessVisible();
  const [jobs, err] = usePoll(api.jobs, visible ? 1500 : 0, [visible]);
  const [store, setStore] = useState<{ appid: number; name: string }>();
  useEffect(() => {
    let last = 0;
    const check = () => {
      if (!onStorePage()) { last = 0; setStore(undefined); return; }
      storeApp().then(async (id) => {
        if (id === last) return;
        last = id;
        setStore(id ? { appid: id, name: (await api.app(id)).name } : undefined);
      }, () => setStore(undefined));
    };
    check();
    const t = setInterval(check, 1500);
    return () => clearInterval(t);
  }, []);

  const active = (jobs ?? []).filter((j) => !["done", "cancelled"].includes(j.state));
  return (
    <>
      {store && (
        <PanelSection title="Store page">
          <PanelSectionRow><ButtonItem layout="below" onClick={() => go(`/mercury/game/${store.appid}`)}>Get {store.name}</ButtonItem></PanelSectionRow>
        </PanelSection>
      )}
      {pending && (
        <PanelSection title="Browser download">
          <PanelSectionRow><div style={{ fontSize: 12, color: C.dim }}>Waiting for {pending.name} in Firefox. Mercury installs it as soon as the file finishes.</div></PanelSectionRow>
          <PanelSectionRow><ButtonItem layout="below" onClick={() => { cancelBrowserDownload(); setStore((x) => x); }}>Stop waiting</ButtonItem></PanelSectionRow>
        </PanelSection>
      )}
      <PanelSection title={active.length ? "Downloads" : "Mercury"}>
        {err && <PanelSectionRow><div style={{ color: C.bad, fontSize: 12 }}>Engine not responding</div></PanelSectionRow>}
        {active.map((j) => <PanelSectionRow key={j.id}><JobProgress job={j} /></PanelSectionRow>)}
        {jobs && !active.length && <PanelSectionRow><div style={{ color: C.dim, fontSize: 12 }}>Nothing downloading.</div></PanelSectionRow>}
        <PanelSectionRow><ButtonItem layout="below" onClick={() => go("/mercury")}>Open Mercury</ButtonItem></PanelSectionRow>
        {active.length > 0 && <PanelSectionRow><ButtonItem layout="below" onClick={() => go("/mercury/downloads")}>All downloads</ButtonItem></PanelSectionRow>}
      </PanelSection>
    </>
  );
}
