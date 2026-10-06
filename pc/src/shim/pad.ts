// Which kind of controller is connected, so the app can show PlayStation or Xbox button names.
// The Gamepad API only lists a controller after its first button press; until then Xbox names are shown.
import { useSyncExternalStore } from "react";

export type PadKind = "ps" | "xbox";
const PS = /054c|dualshock|dualsense|wireless controller|playstation|sony/i;
let kind: PadKind = "xbox";
const subs = new Set<() => void>();

function detect() {
  const pads = Array.from(navigator.getGamepads?.() ?? []).filter((p): p is Gamepad => !!p);
  if (!pads.length) return; // keep showing the last controller when it sleeps or disconnects
  const next: PadKind = pads.some((p) => PS.test(p.id)) ? "ps" : "xbox";
  if (next !== kind) { kind = next; subs.forEach((f) => f()); }
}
window.addEventListener("gamepadconnected", detect);
setInterval(detect, 1000);

export function usePadKind(): PadKind {
  return useSyncExternalStore((cb) => { subs.add(cb); return () => { subs.delete(cb); }; }, () => kind);
}

const PS_NAMES: Record<string, string> = { A: "✕", B: "○", X: "□", Y: "△", LB: "L1", RB: "R1", LT: "L2", RT: "R2" };
/** The label for a button on the connected controller: "A" on Xbox, a cross on PlayStation. */
export const btn = (k: string, kind: PadKind) => (kind === "ps" ? PS_NAMES[k] ?? k : k);
// PlayStation face-button colours.
export const PS_COLOR: Record<string, string> = { A: "#7aa7ff", B: "#ff6b7a", X: "#f29bd0", Y: "#6fe0a8" };
