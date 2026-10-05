import { ButtonItem, Field, Focusable, PanelSection, TextField, ToggleField } from "@decky/ui";
import { toaster } from "@decky/api";
import { useEffect, useState } from "react";
import { api, bytes, Config } from "@shared/api";
import { usePoll } from "@shared/hooks";
import { Btn, C, FocusStyle, page } from "@shared/ui";

export function Settings() {
  const [cfg, setCfg] = useState<Config>();
  const [key, setKey] = useState("");
  const [sgdb, setSgdb] = useState("");
  const [status, , reloadStatus] = usePoll(api.status as () => Promise<any>, 0);
  useEffect(() => { api.config().then(setCfg, () => {}); }, []);

  const save = async (patch: Partial<Config>) => {
    try { setCfg(await api.saveConfig(patch)); reloadStatus(); toaster.toast({ title: "Mercury", body: "Settings saved" }); }
    catch (e: any) { toaster.toast({ title: "Mercury", body: e.message }); }
  };
  const check = async () => {
    try { const u = await api.rdCheck(); toaster.toast({ title: "Real-Debrid", body: `${u.username} · ${u.type} until ${u.expiration?.slice(0, 10)}` }); }
    catch (e: any) { toaster.toast({ title: "Real-Debrid", body: e.message }); }
  };
  if (!cfg) return <div style={page}><div style={{ padding: 48, color: C.dim }}>Loading settings…</div></div>;
  return (
    <div style={page}>
      <FocusStyle />
      <div style={{ padding: "16px 28px 48px", maxWidth: 820 }}>
        <div style={{ fontSize: 18, fontWeight: 700, color: "#fff", marginBottom: 4 }}>Settings</div>
        <PanelSection title="Real-Debrid">
          <Field label="API key" description={cfg.rd_key_set ? "A key is saved. Type a new one to replace it." : "Not set. Get it from real-debrid.com/apitoken."} childrenLayout="below">
            <TextField bIsPassword value={key} onChange={(e) => setKey(e.target.value)} />
          </Field>
          <Focusable flow-children="horizontal" style={{ display: "flex", gap: 10, margin: "8px 0 4px" }}>
            <Btn style={{ width: 180 }} disabled={!key} onClick={async () => { await save({ rd_key: key }); setKey(""); }}>Save key</Btn>
            <Btn style={{ width: 180 }} disabled={!cfg.rd_key_set} onClick={check}>Test key</Btn>
          </Focusable>
        </PanelSection>
        <PanelSection title="Artwork">
          <Field label="SteamGridDB key" description={cfg.sgdb_key_set ? "A key is saved. Type a new one to replace it." : "Not set. Create one at steamgriddb.com under Preferences, then API."} childrenLayout="below">
            <TextField bIsPassword value={sgdb} onChange={(e) => setSgdb(e.target.value)} />
          </Field>
          <Focusable flow-children="horizontal" style={{ display: "flex", gap: 10, margin: "8px 0 4px" }}>
            <Btn style={{ width: 180 }} disabled={!sgdb} onClick={async () => { await save({ sgdb_key: sgdb }); setSgdb(""); }}>Save key</Btn>
          </Focusable>
          <ToggleField label="Review before adding to Steam" description="After a game installs, confirm its title and artwork before it goes to Steam" checked={cfg.review_art !== false} onChange={(v) => save({ review_art: v })} />
        </PanelSection>
        <PanelSection title="Games">
          <Field label="Install folder" description={cfg.games_dir} />
          <Field label="Steam" description={!status?.steam?.found ? "Steam not found. Games still install, but are not added to a launcher." : status.launcher === "warmup" ? "Installed games go to warmUP." : status.steam.flag ? "Found. Installed games are added to your Steam library with their art." : "Found. Mercury turns on its Steam connection the first time it adds a game."} />
          <ToggleField label="Search SteamRIP" description="Pre-installed games from the SteamRIP feed" checked={cfg.enable_steamrip} onChange={(v) => save({ enable_steamrip: v })} />
        </PanelSection>
        <PanelSection title="Status">
          <Field label="Storage" description={status ? `${bytes(status.storage.free)} free of ${bytes(status.storage.total)} · Mercury games ${bytes(status.storage.mercury)}` : "…"} />
          <Field label="Archives" description={status?.unrar ? "7-Zip found (zip, 7z and rar)" : "7-Zip missing: install it from 7-zip.org"} />
          <Field label="Engine" description={status ? `mercuryd ${status.version}` : "Not responding"} />
          <ButtonItem layout="below" onClick={reloadStatus}>Refresh</ButtonItem>
        </PanelSection>
      </div>
    </div>
  );
}
