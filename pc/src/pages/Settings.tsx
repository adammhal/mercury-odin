import { ButtonItem, Field, Focusable, PanelSection, TextField, ToggleField } from "@decky/ui";
import { toaster } from "@decky/api";
import { useEffect, useState } from "react";
import { api, bytes, Config } from "@shared/api";
import { usePoll } from "@shared/hooks";
import { Btn, C, FocusStyle, page } from "@shared/ui";

export function Settings() {
  const [cfg, setCfg] = useState<Config>();
  const [key, setKey] = useState("");
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
        <PanelSection title="Games">
          <Field label="Install folder" description={cfg.games_dir} />
          <Field label="warmUP" description={status?.warmup ? "Found. Installed games are added to warmUP; press library sync there to see them." : "warmUP not found. Games still install, but are not added to a launcher."} />
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
