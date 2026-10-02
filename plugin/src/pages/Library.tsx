import { Focusable, Navigation } from "@decky/ui";
import { useEffect, useState } from "react";
import { api, bytes, cdn, UpdateInfo } from "../api";
import { usePoll } from "../hooks";
import { gameId } from "../steam";
import { Btn, C, FocusStyle, page, scrollIntoView } from "../ui";

declare const SteamClient: any;

export function Library() {
  const [lib] = usePoll(api.library, 5000);
  const [updates, setUpdates] = useState<Record<number, UpdateInfo>>({});
  useEffect(() => {
    for (const e of lib ?? []) {
      if (updates[e.appid]) continue;
      api.updateCheck(e.appid).then((u) => setUpdates((m) => ({ ...m, [e.appid]: u })), () => {});
    }
  }, [lib?.length]);
  return (
    <div style={page}>
      <FocusStyle />
      <div style={{ padding: "16px 28px 48px" }}>
        <div style={{ fontSize: 18, fontWeight: 700, color: "#fff", marginBottom: 10 }}>
          Installed with Mercury <span style={{ fontSize: 14, color: C.dim, fontWeight: 500 }}>{bytes((lib ?? []).reduce((a, e) => a + e.size, 0))}</span>
        </div>
        {lib && !lib.length && <div style={{ color: C.dim }}>No games installed with Mercury yet.</div>}
        <Focusable flow-children="vertical">
          {(lib ?? []).map((e) => (
            <Focusable key={e.appid} flow-children="horizontal" onFocus={scrollIntoView}
              style={{ display: "flex", gap: 12, alignItems: "center", background: C.panel, borderRadius: 6, padding: 10, marginBottom: 8 }}>
              <div style={{ flex: "none", width: 120, height: 56, borderRadius: 5, background: `url(${cdn(e.appid, "header.jpg")}) center/cover` }} />
              <div style={{ flex: 1 }}>
                <div style={{ fontWeight: 700, fontSize: 14 }}>{e.name}</div>
                <div style={{ fontSize: 12, color: C.dim, marginTop: 4 }}>{e.provider}{e.version ? ` ${e.version}` : ""} · {bytes(e.size)} · {new Date(e.installed * 1000).toLocaleDateString()}</div>
                {updates[e.appid]?.newer && <div style={{ fontSize: 12, color: C.ok, marginTop: 3 }}>Update available: {updates[e.appid].newer!.version}</div>}
              </div>
              <Btn style={{ width: 90 }} onClick={() => SteamClient.Apps.RunGame(gameId(e.shortcut_id), "", -1, 100)}>Play</Btn>
              <Btn style={{ width: 90 }} onClick={() => Navigation.Navigate(`/mercury/game/${e.appid}`)}>Details</Btn>
            </Focusable>
          ))}
        </Focusable>
      </div>
    </div>
  );
}
