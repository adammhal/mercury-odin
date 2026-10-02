// Controller and keyboard navigation for the PC app, standing in for Steam's focus engine on the Odin.
// Focusable items carry data-focusable. D-pad / left stick / arrow keys move focus to the nearest
// item in that direction; A / Enter activates; B / Escape goes back; Y / F runs the item's options action.

type Dir = "up" | "down" | "left" | "right";
export type Button = Dir | "a" | "b" | "x" | "y" | "lb" | "rb" | "start" | "select";

type Handlers = { back: () => void; options: () => void; menu: () => void };
let handlers: Handlers = { back: () => history.back(), options: () => {}, menu: () => {} };
export function setGlobalHandlers(h: Partial<Handlers>) { handlers = { ...handlers, ...h }; }

/** The topmost open modal limits navigation to its own items. */
function scope(): ParentNode {
  const modals = document.querySelectorAll<HTMLElement>("[data-modal]");
  return modals.length ? modals[modals.length - 1] : document;
}

function visible(el: HTMLElement) {
  const r = el.getBoundingClientRect();
  if (!r.width || !r.height) return false;
  const st = getComputedStyle(el);
  return st.visibility !== "hidden" && st.display !== "none";
}

function items(): HTMLElement[] {
  return Array.from(scope().querySelectorAll<HTMLElement>("[data-focusable]")).filter((e) => !e.hasAttribute("data-disabled") && visible(e));
}

export function current(): HTMLElement | null {
  const a = document.activeElement as HTMLElement | null;
  return a && a.hasAttribute("data-focusable") && scope().contains(a) ? a : null;
}

export function focusFirst() {
  const list = items();
  const preferred = list.find((e) => e.hasAttribute("data-autofocus")) ?? list[0];
  preferred?.focus();
}

/** Nearest item whose centre lies in the direction pressed; distance along the axis counts once,
 * sideways drift counts more, so a straight line beats a closer diagonal. */
function move(dir: Dir) {
  const from = current();
  if (!from) { focusFirst(); return; }
  const r = from.getBoundingClientRect();
  const cx = r.left + r.width / 2, cy = r.top + r.height / 2;
  const v = { up: [0, -1], down: [0, 1], left: [-1, 0], right: [1, 0] }[dir];
  let best: HTMLElement | null = null, bestScore = Infinity;
  for (const el of items()) {
    if (el === from) continue;
    const q = el.getBoundingClientRect();
    const dx = q.left + q.width / 2 - cx, dy = q.top + q.height / 2 - cy;
    const along = dx * v[0] + dy * v[1];
    if (along <= 2) continue;
    // Overlap on the cross axis (same row or column) is free, which keeps rows feeling like rows.
    const overlap = v[0] ? Math.min(r.bottom, q.bottom) - Math.max(r.top, q.top) : Math.min(r.right, q.right) - Math.max(r.left, q.left);
    const side = overlap > 0 ? 0 : Math.abs(dx * v[1] + dy * v[0]);
    const score = along + side * 3;
    if (score < bestScore) { bestScore = score; best = el; }
  }
  best?.focus();
}

export function press(b: Button) {
  if (b === "up" || b === "down" || b === "left" || b === "right") return move(b);
  const el = current();
  if (b === "a") { el?.dispatchEvent(new CustomEvent("mercury-activate", { bubbles: false })); return; }
  if (b === "b") {
    const modal = scope();
    if (modal !== document) { (modal as HTMLElement).dispatchEvent(new CustomEvent("mercury-cancel")); return; }
    handlers.back(); return;
  }
  if (b === "start") { handlers.menu(); return; }
  if (b === "y") {
    if (el?.hasAttribute("data-options")) el.dispatchEvent(new CustomEvent("mercury-options"));
    else handlers.options();
  }
}

// ---- keyboard ----
window.addEventListener("keydown", (e) => {
  const typing = (e.target as HTMLElement)?.tagName === "INPUT";
  const map: Record<string, Button> = { ArrowUp: "up", ArrowDown: "down", ArrowLeft: "left", ArrowRight: "right", Enter: "a", Escape: "b", Backspace: "b" };
  let b = map[e.key];
  if (!b && !typing && (e.key === "f" || e.key === "F")) b = "y";
  if (!b) return;
  if (typing && (b === "left" || b === "right" || e.key === "Backspace")) return;
  e.preventDefault();
  press(b);
});

// ---- gamepad (standard mapping: 0 A, 1 B, 2 X, 3 Y, 4 LB, 5 RB, 8 Back, 9 Start, 12-15 D-pad) ----
const BUTTONS: [number, Button][] = [[0, "a"], [1, "b"], [2, "x"], [3, "y"], [4, "lb"], [5, "rb"], [8, "select"], [9, "start"], [12, "up"], [13, "down"], [14, "left"], [15, "right"]];
const held = new Map<Button, number>();
const FIRST_REPEAT = 380, REPEAT = 90;

function poll(t: number) {
  const down = new Set<Button>();
  for (const pad of navigator.getGamepads()) {
    if (!pad) continue;
    for (const [i, b] of BUTTONS) if (pad.buttons[i]?.pressed) down.add(b);
    const [x = 0, y = 0] = pad.axes;
    if (y < -0.6) down.add("up"); if (y > 0.6) down.add("down");
    if (x < -0.6) down.add("left"); if (x > 0.6) down.add("right");
  }
  for (const b of down) {
    const since = held.get(b);
    if (since === undefined) { held.set(b, t); press(b); continue; }
    // Only directions repeat while held.
    if (["up", "down", "left", "right"].includes(b) && t - since > FIRST_REPEAT) { held.set(b, t - FIRST_REPEAT + REPEAT); press(b); }
  }
  for (const b of [...held.keys()]) if (!down.has(b)) held.delete(b);
  requestAnimationFrame(poll);
}
requestAnimationFrame(poll);
