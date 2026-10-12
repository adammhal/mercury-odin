import { ButtonItem, Dropdown, Field, Focusable, PanelSection, TextField, ToggleField } from "@decky/ui";
import { toaster } from "@decky/api";
import { useEffect, useState } from "react";
import { api, bytes, Config, Location } from "../api";
import { usePoll } from "../hooks";
import { Btn, C, confirmRebootAndroid, FocusStyle, page } from "../ui";

// Internal tool names as reported by SteamClient.Apps.GetAvailableCompatTools on the Odin (2026-10-01).
// A wrong name makes Steam run the .exe natively with no error, so never guess these.
declare const SteamClient: any;
// Used until Steam lists its compatibility tools (internal names: a wrong one runs the .exe natively, silently).
const PROTONS_ARM = [
  { data: "proton-experimental-arm64", label: "Proton Experimental (ARM64)" },
  { data: "proton_11-arm64", label: "Proton 11.0 (ARM64)" },
  { data: "proton-cachyos-11.0-arm64", label: "Proton 11.0 CachyOS (ARM64)" },
];
const PROTONS_X86 = [{ data: "proton_experimental", label: "Proton Experimental" }];

/** Proton versions this Steam actually has, by their internal names. */
function useProtons(arch?: string) {
  const [list, setList] = useState<{ data: string; label: string }[]>();
  useEffect(() => {
    Promise.resolve(SteamClient.Apps.GetAvailableCompatTools(0)).then((tools: any[]) => {
      const p = (tools ?? []).filter((t) => /proton/i.test(t.strToolName ?? "")).map((t) => ({ data: t.strToolName, label: t.strDisplayName || t.strToolName }));
      if (p.length) setList(p);
    }, () => {});
  }, []);
  return list ?? (arch === "aarch64" ? PROTONS_ARM : PROTONS_X86);
}

export function Settings() {
  const [cfg, setCfg] = useState<Config>();
  const [key, setKey] = useState("");
  const [sgdbKey, setSgdbKey] = useState("");
  const [status, , reloadStatus] = usePoll(api.status, 0);
  const protons = useProtons(status?.arch);
  const [locs, , reloadLocs] = usePoll(api.locations, 0);
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
        <PanelSection title="SteamGridDB (optional)">
          <Field label="API key" description={cfg.sgdb_key_set ? "A key is saved. Mercury uses SteamGridDB for art Steam lacks, and for game icons." : "Not set. Get one at steamgriddb.com, under Preferences then API. Without it, Mercury uses Steam's art only."} childrenLayout="below">
            <TextField bIsPassword value={sgdbKey} onChange={(e) => setSgdbKey(e.target.value)} />
          </Field>
          <Focusable flow-children="horizontal" style={{ display: "flex", gap: 10, margin: "8px 0 4px" }}>
            <Btn style={{ width: 180 }} disabled={!sgdbKey} onClick={async () => { await save({ sgdb_key: sgdbKey }); setSgdbKey(""); }}>Save key</Btn>
          </Focusable>
          <ToggleField label="Review before adding to Steam" description="After a game installs, check its title and choose SteamGridDB art before it goes to Steam. Off: it is added right away with Steam's art."
            checked={cfg.review_art ?? true} onChange={(v) => save({ review_art: v })} />
        </PanelSection>
        {status?.boot_switch && <PanelSection title="System">
          <Btn style={{ width: 220 }} onClick={confirmRebootAndroid}>Restart into Android</Btn>
        </PanelSection>}
        <PanelSection title="Battery">
          <ToggleField label="Limit charging to 80%" description="Slows battery wear when the Odin lives on the dock or charger. Charging stops at 80% and resumes below 77%. Turn it off before a trip to charge to 100%."
            checked={cfg.charge_limit ?? false} onChange={(v) => save({ charge_limit: v })} />
        </PanelSection>
        <PanelSection title="Games">
          <Field label="Proton for new games" childrenLayout="below">
            <Dropdown rgOptions={protons} selectedOption={cfg.proton_tool} onChange={(o) => save({ proton_tool: o.data })} />
          </Field>
          <Field label="Install new games on" description={`${cfg.games_dir}. Mount a Windows-formatted microSD card from Library, Import first.`} childrenLayout="below">
            <Dropdown rgOptions={(locs ?? []).map((l: Location) => ({ data: l.path, label: `${l.label} · ${bytes(l.free)} free` }))}
              selectedOption={cfg.games_dir} onChange={async (o) => { await save({ games_dir: o.data }); reloadLocs(); }} />
          </Field>
          <Field label="Launch options for new games" description={cfg.launch_options || "None"} />
          <ToggleField label="Search SteamRIP" description="Pre-installed games from the SteamRIP feed" checked={cfg.enable_steamrip} onChange={(v) => save({ enable_steamrip: v })} />
        </PanelSection>
        <PanelSection title="Status">
          <Field label="Storage" description={status ? `${bytes(status.storage.free)} free of ${bytes(status.storage.total)} · Mercury games ${bytes(status.storage.mercury)}` : "…"} />
          <Field label="RAR support" description={status?.unrar ? "unrar installed" : "unrar missing: RAR archives cannot be extracted"} />
          <Field label="Engine" description={status ? `mercuryd ${status.version}` : "Not responding"} />
          <ButtonItem layout="below" onClick={reloadStatus}>Refresh</ButtonItem>
        </PanelSection>
      </div>
    </div>
  );
}
