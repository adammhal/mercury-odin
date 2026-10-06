import { useEffect, useLayoutEffect, useState } from "react";
import { HashRouter, Navigate, Route, Routes, useLocation, useNavigate } from "react-router-dom";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ConfirmModal, Focusable, ModalHost, Navigation, setNavigate, showModal, ToastHost, toast } from "./shim/ui";
import { focusFirst, press, setGlobalHandlers } from "./shim/nav";
import { btn, PS_COLOR, usePadKind } from "./shim/pad";
import { api, Job, STATE_LABEL } from "@shared/api";
import { usePoll } from "@shared/hooks";
import { FOCUS } from "@shared/ui";
import { Home } from "@shared/pages/Home";
import { Search } from "@shared/pages/Search";
import { Game } from "./pages/Game";
import { Downloads } from "./pages/Downloads";
import { Library } from "./pages/Library";
import { Settings } from "./pages/Settings";
import { Review } from "@shared/pages/Review";
import { checkBrowserDownload } from "./browser";
import { pc } from "./pcapi";

// On the PC, a finished game goes into Steam (Big Picture) with its art.
STATE_LABEL.ready = "Adding to Steam";

/** The screens were designed for the Odin's 910x512 Big Picture viewport. Scale to the window's height
 * and let the width follow the screen's shape, so the app always fills the screen with no borders. */
const H = 512;
function useFrame() {
  const get = () => { const scale = window.innerHeight / H; return { scale, width: Math.max(640, window.innerWidth / scale) }; };
  const [f, setF] = useState(get);
  useEffect(() => { const r = () => setF(get()); window.addEventListener("resize", r); return () => window.removeEventListener("resize", r); }, []);
  return f;
}

function quit() {
  showModal(<ConfirmModal strTitle="Close Mercury?" strOKButtonText="Close" strDescription="Downloads keep going in the background, and finished games still appear in Steam."
    onOK={() => { getCurrentWindow().close().catch(() => window.close()); }} />);
}

const POWER: { key: "sleep" | "restart" | "shutdown" | "signout"; label: string; ask: string; ok: string }[] = [
  { key: "sleep", label: "Sleep", ask: "Put the PC to sleep?", ok: "Sleep" },
  { key: "restart", label: "Restart", ask: "Restart the PC?", ok: "Restart" },
  { key: "shutdown", label: "Shut down", ask: "Shut down the PC?", ok: "Shut down" },
  { key: "signout", label: "Sign out", ask: "Sign out of Windows?", ok: "Sign out" },
];

function PowerMenu({ closeModal }: { closeModal?: () => void }) {
  const pick = (o: typeof POWER[number]) => {
    closeModal?.();
    showModal(<ConfirmModal strTitle={o.ask} strOKButtonText={o.ok} strDescription="Downloads in progress will be interrupted."
      onOK={() => { pc.power(o.key).catch((e) => toast(String(e.message ?? e), "Power")); }} />);
  };
  return (
    <div style={{ width: 360, background: "#171d25", borderRadius: 6, padding: "18px 20px", boxShadow: "0 10px 40px rgba(0,0,0,.6)" }}>
      <div style={{ fontSize: 17, fontWeight: 700, color: "#fff", marginBottom: 12 }}>Power</div>
      <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
        {POWER.map((o, i) => (
          <Focusable key={o.key} autoFocus={i === 0} onActivate={() => pick(o)}
            style={{ height: 36, borderRadius: 4, display: "flex", alignItems: "center", padding: "0 14px", fontSize: 14, fontWeight: 600, color: "#fff", background: "#2a3340" }}>{o.label}</Focusable>
        ))}
      </div>
    </div>
  );
}
const openPower = () => showModal(<PowerMenu />);

function Clock() {
  const [t, setT] = useState(() => new Date());
  useEffect(() => { const i = setInterval(() => setT(new Date()), 15000); return () => clearInterval(i); }, []);
  return <span>{t.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })}</span>;
}

function Glyph({ k, label, onClick }: { k: string; label: string; onClick?: () => void }) {
  const kind = usePadKind();
  const ps = kind === "ps" && PS_COLOR[k];
  return <span onClick={onClick} style={{ display: "inline-flex", alignItems: "center", gap: 6, cursor: onClick ? "pointer" : undefined }}>
    <span style={{ minWidth: 18, height: 18, padding: "0 4px", boxSizing: "border-box", borderRadius: 9, background: ps ? "#1c2430" : "#e9ecef", color: ps || "#0e141b", border: ps ? `1.5px solid ${ps}` : undefined, fontSize: ps ? 11 : 10, fontWeight: 800, display: "inline-flex", alignItems: "center", justifyContent: "center" }}>{btn(k, kind)}</span>
    <span>{label}</span>
  </span>;
}

/** Background work that runs whichever page is open. */
function useWatcher() {
  useEffect(() => {
    const seen = new Map<number, string>();
    const prompted = new Set<number>();
    const tick = async () => {
      let jobs: Job[]; try { jobs = await api.jobs(); } catch { return; }
      for (const j of jobs) {
        const prev = seen.get(j.id); seen.set(j.id, j.state);
        // A finished install waits for the user to confirm its title and art before going to Steam.
        if (j.state === "review" && !prompted.has(j.id)) { prompted.add(j.id); Navigation.Navigate(`/mercury/review/${j.id}`); }
        if (!prev || prev === j.state) continue;
        if (j.state === "done") toast(j.error ?? `${j.name} is installed and in your Steam library.`, "Mercury");
        if (j.state === "needs_setup") toast(j.error ?? `${j.name} downloaded. Run its installer from Downloads.`, "Mercury");
        if (j.state === "failed") toast(`${j.name} failed: ${j.error ?? "unknown error"}`, "Mercury");
      }
      checkBrowserDownload();
    };
    tick(); const t = setInterval(tick, 2000); return () => clearInterval(t);
  }, []);
}

function Shell() {
  const navigate = useNavigate();
  const loc = useLocation();
  const { scale, width } = useFrame();
  const [jobs] = usePoll(api.jobs, 3000);
  const active = (jobs ?? []).filter((j) => !["done", "failed", "cancelled"].includes(j.state));
  useWatcher();
  useLayoutEffect(() => { setNavigate((to) => navigate(to)); }, [navigate]);
  const home = loc.pathname === "/mercury";
  useEffect(() => {
    setGlobalHandlers({ back: () => (home ? quit() : navigate(-1)), options: () => navigate("/mercury/search"), menu: quit });
    // Pages focus their own first item when data arrives; this covers pages with nothing marked.
    const t = setTimeout(() => { if (!document.activeElement || document.activeElement === document.body) focusFirst(); }, 250);
    return () => clearTimeout(t);
  }, [loc.pathname, home]);

  return (
    <div style={{ position: "fixed", inset: 0, display: "flex", alignItems: "center", justifyContent: "center", background: "#0b1016" }}>
      <div style={{ width, height: H, zoom: scale, position: "relative", overflow: "hidden", background: "#0e141b" } as React.CSSProperties}>
        <style>{`
          .${FOCUS} { box-shadow: 0 0 0 2px #fff, 0 0 14px rgba(255,255,255,.35) !important; }
          .mercury-input:focus { box-shadow: 0 0 0 2px #fff; }
          @keyframes mercury-spin { to { transform: rotate(360deg) } }
          @keyframes mercury-indeterminate { 0% { left: -30% } 100% { left: 100% } }
          @keyframes mercury-fade { from { opacity: 0 } to { opacity: 1 } }
        `}</style>
        <Routes>
          <Route path="/mercury" element={<Home />} />
          <Route path="/mercury/game/:appid" element={<Game />} />
          <Route path="/mercury/downloads" element={<Downloads />} />
          <Route path="/mercury/library" element={<Library />} />
          <Route path="/mercury/search" element={<Search />} />
          <Route path="/mercury/settings" element={<Settings />} />
          <Route path="/mercury/review/:id" element={<Review />} />
          <Route path="*" element={<Navigate to="/mercury" replace />} />
        </Routes>

        <div style={{ position: "absolute", top: 0, left: 0, right: 0, height: 40, zIndex: 20, display: "flex", alignItems: "center", justifyContent: "space-between", padding: "0 20px",
          background: "linear-gradient(rgba(8,11,15,.92),rgba(8,11,15,.6))", fontSize: 13, color: "#c7cbd1" }}>
          <Focusable onActivate={() => navigate("/mercury")} style={{ fontWeight: 800, letterSpacing: ".08em", color: "#fff", padding: "4px 6px", borderRadius: 4 }}>MERCURY</Focusable>
          <div style={{ display: "flex", gap: 14, alignItems: "center" }}>
            {active.length > 0 && <span onClick={() => navigate("/mercury/downloads")} style={{ cursor: "pointer", background: "rgba(26,159,255,.16)", color: "#8ccfff", padding: "2px 10px", borderRadius: 99, fontSize: 11, fontWeight: 600 }}>
              ↓ {active[0].name}{active.length > 1 ? ` +${active.length - 1}` : ""}</span>}
            <Focusable onActivate={openPower} style={{ padding: "4px 10px", borderRadius: 4, fontWeight: 700, color: "#fff" }}>⏻ Power</Focusable>
            <Clock />
          </div>
        </div>

        <div style={{ position: "absolute", left: 0, right: 0, bottom: 0, height: 40, zIndex: 20, display: "flex", alignItems: "center", justifyContent: "space-between", padding: "0 20px",
          background: "linear-gradient(transparent,rgba(8,11,15,.95) 40%)", fontSize: 12, color: "#c7cbd1" }}>
          <Glyph k="☰" label="Close Mercury" onClick={quit} />
          <div style={{ display: "flex", gap: 18 }}>
            <Glyph k="Y" label="Search" onClick={() => press("y")} />
            <Glyph k="A" label="Select" />
            <Glyph k="B" label={home ? "Close" : "Back"} onClick={() => press("b")} />
          </div>
        </div>

        <ModalHost />
        <ToastHost />
      </div>
    </div>
  );
}

export function App() {
  return <HashRouter><Shell /></HashRouter>;
}
