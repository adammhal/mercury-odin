import { Focusable, Navigation, Spinner, TextField } from "@decky/ui";
import { toaster } from "@decky/api";
import { useEffect, useState } from "react";
import { api, App, bytes, cdn, ImportCandidate } from "../api";
import { useOnce } from "../hooks";
import { Btn, C, Chip, FOCUS, FocusStyle, page, scrollIntoView } from "../ui";

/** "Hollow.Knight.Silksong-RUNE (v1.2)" -> "Hollow Knight Silksong": a starting point for the Steam search. */
function guessTitle(name: string) {
  return name.replace(/\.(zip|rar|7z)$/i, "").replace(/[._]/g, " ").replace(/[\[(].*?[\])]/g, " ")
    .replace(/\b(v?\d+(\.\d+)+|build \d+|repack|fitgirl|dodi|steamrip|gog|-[A-Z0-9]+$)\b/gi, " ").replace(/\s+/g, " ").trim();
}

function Pick({ c, onDone }: { c: ImportCandidate; onDone: () => void }) {
  const [q, setQ] = useState(guessTitle(c.name));
  const [res, setRes] = useState<App[]>();
  const [match, setMatch] = useState<App>();
  const [keep, setKeep] = useState(c.location === "microSD");
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (q.trim().length < 2) return;
    const t = setTimeout(() => api.search(q.trim()).then((r) => { setRes(r); setMatch((m) => m ?? r[0]); }, () => setRes([])), 400);
    return () => clearTimeout(t);
  }, [q]);
  const go = async () => {
    if (!match) return;
    setBusy(true);
    try {
      await api.importGame(c.path, match.appid, match.name, keep);
      toaster.toast({ title: "Mercury", body: `Importing ${match.name}` });
      Navigation.Navigate("/mercury/downloads");
    } catch (e: any) { toaster.toast({ title: "Mercury", body: e.message }); }
    setBusy(false);
    onDone();
  };
  return (
    <div>
      <div style={{ fontSize: 13, color: C.dim, margin: "4px 0 8px" }}>
        {c.location} · {c.name}{c.exe ? ` · ${c.installer ? "installer" : "game"}: ${c.exe}` : ""}
      </div>
      <TextField label="Which Steam game is this? (for art, icon and update checks)" value={q} onChange={(e) => setQ(e.target.value)} />
      <Focusable flow-children="horizontal" style={{ display: "flex", gap: 10, overflowX: "auto", padding: "10px 2px", scrollbarWidth: "none" }}>
        {(res ?? []).slice(0, 10).map((a) => (
          <Focusable key={a.appid} focusClassName={FOCUS} noFocusRing onFocus={scrollIntoView} onActivate={() => setMatch(a)} onClick={() => setMatch(a)}
            style={{ flex: "none", width: 80, borderRadius: 4, outline: match?.appid === a.appid ? `2px solid ${C.ok}` : "none" }}>
            <div style={{ height: 120, borderRadius: 4, background: `${C.panel} url(${cdn(a.appid, "library_600x900.jpg")}) center/cover` }} />
            <div style={{ fontSize: 10, marginTop: 3, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{a.name}</div>
          </Focusable>
        ))}
        {res && !res.length && <div style={{ color: C.dim, fontSize: 12 }}>No Steam match. Try another name.</div>}
      </Focusable>
      {c.kind === "folder" && (
        <Focusable flow-children="horizontal" style={{ display: "flex", gap: 8, alignItems: "center", margin: "4px 0 10px" }}>
          <Btn onClick={() => setKeep(true)} style={{ background: keep ? "rgba(26,159,255,.35)" : undefined }}>Keep it where it is</Btn>
          <Btn onClick={() => setKeep(false)} style={{ background: !keep ? "rgba(26,159,255,.35)" : undefined }}>Move into Mercury's games folder</Btn>
          <span style={{ fontSize: 11, color: C.dim }}>{keep ? "The game runs from its current folder. Keep the card inserted to play." : "Moved to internal storage (copied, then removed, if it is on the card)."}</span>
        </Focusable>
      )}
      <Btn disabled={!match || busy} onClick={go} style={{ height: 34, background: "linear-gradient(90deg,#70d61d,#01a75b)" }}>
        {busy ? "Importing…" : match ? `Import as ${match.name}` : "Pick the matching game"}
      </Btn>
    </div>
  );
}

export function Import() {
  const [tick, setTick] = useState(0);
  const [data, err] = useOnce(api.importCandidates, [tick]);
  const [sel, setSel] = useState<ImportCandidate>();
  return (
    <div style={page}>
      <FocusStyle />
      <div style={{ padding: "16px 28px 48px" }}>
        <Focusable flow-children="horizontal" style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: 6 }}>
          <div style={{ fontSize: 18, fontWeight: 700, color: "#fff" }}>Import a game</div>
          <Btn style={{ height: 28, fontSize: 12 }} onClick={() => { setSel(undefined); setTick((n) => n + 1); }}>Rescan</Btn>
        </Focusable>
        <div style={{ fontSize: 12, color: C.dim, marginBottom: 12, lineHeight: 1.5 }}>
          For games installed or downloaded somewhere else, such as a repack installed on your PC. Put the game folder (or its .zip, .rar or .7z) on the microSD card,
          in {data?.drop_folder ?? "~/Games/Import"} over SSH, or in Downloads.
        </div>
        {!data && !err && <div style={{ display: "flex", gap: 8, color: C.dim }}><Spinner style={{ width: 18 }} />Looking for games…</div>}
        {err && <div style={{ color: C.bad }}>{err}</div>}
        {data && !data.candidates.length && <div style={{ color: C.dim }}>Nothing to import yet.</div>}
        {sel ? <Pick c={sel} onDone={() => setSel(undefined)} /> : (
          <Focusable flow-children="vertical">
            {(data?.candidates ?? []).map((c) => (
              <Focusable key={c.path} focusClassName={FOCUS} noFocusRing onFocus={scrollIntoView} onActivate={() => setSel(c)} onClick={() => setSel(c)}
                style={{ display: "grid", gridTemplateColumns: "minmax(0,1fr) auto", gap: 12, alignItems: "center", padding: "8px 12px", borderRadius: 5, marginBottom: 6, background: C.panel }}>
                <div style={{ minWidth: 0 }}>
                  <div style={{ fontWeight: 600, fontSize: 13, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{c.name}</div>
                  <div style={{ fontSize: 11, color: C.dim, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>{c.kind === "archive" ? `Archive · ${bytes(c.size)}` : c.exe}</div>
                </div>
                <div style={{ whiteSpace: "nowrap" }}>
                  <Chip tone="accent">{c.location}</Chip>
                  {c.installer && <Chip tone="warn">Installer</Chip>}
                </div>
              </Focusable>
            ))}
          </Focusable>
        )}
      </div>
    </div>
  );
}
