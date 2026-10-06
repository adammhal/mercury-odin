// Windows-only engine calls, next to the shared client in ../plugin/src/api.ts.
const BASE = "http://127.0.0.1:47800";
async function post(path: string, body: unknown = {}) {
  const r = await fetch(BASE + path, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body) });
  const d = await r.json().catch(() => ({}));
  if (!r.ok) throw new Error(d?.error ?? `HTTP ${r.status}`);
  return d;
}
export const pc = {
  play: (appid: number) => post(`/library/${appid}/play`),
  openUrl: (url: string) => post("/open", { url }),
  power: (action: "sleep" | "restart" | "shutdown" | "signout") => post(`/power/${action}`),
  finishSetup: (id: number) => post(`/jobs/${id}/finish-setup`),
  runSetup: (id: number, admin = false) => post(`/jobs/${id}/${admin ? "run-setup-admin" : "run-setup"}`),
};
