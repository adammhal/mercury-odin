// Stand-ins for the @decky/ui components Mercury uses, with controller focus from ./nav.
import { cloneElement, CSSProperties, forwardRef, ReactElement, ReactNode, useEffect, useImperativeHandle, useRef, useState, useSyncExternalStore } from "react";
import { useParams as useRouterParams } from "react-router-dom";
import { current, focusFirst } from "./nav";
import { btn, usePadKind } from "./pad";

const C = { panel: "#1f242c", panel2: "#2a303a", text: "#dcdedf", dim: "#8b929a", accent: "#1a9fff" };
export const FOCUS_CLASS = "mercury-focus";
export const staticClasses = { Title: "" };

// ---------- tiny stores ----------
function store<T>(initial: T) {
  let v = initial; const subs = new Set<() => void>();
  return {
    get: () => v,
    set: (n: T) => { v = n; subs.forEach((s) => s()); },
    use: () => useSyncExternalStore((cb) => { subs.add(cb); return () => subs.delete(cb); }, () => v),
  };
}

// ---------- navigation ----------
let navigateFn: (to: string) => void = () => {};
export function setNavigate(f: (to: string) => void) { navigateFn = f; }
export const Navigation = {
  Navigate: (to: string) => navigateFn(to),
  NavigateBack: () => history.back(),
  CloseSideMenus: () => {},
  OpenQuickAccessMenu: () => {},
};
export const useParams = <T,>() => useRouterParams() as unknown as T;

// ---------- Focusable ----------
type FocusableProps = {
  children?: ReactNode; className?: string; style?: CSSProperties;
  onActivate?: (e: any) => void; onClick?: (e: any) => void; onCancel?: (e: any) => void;
  onFocus?: (e: any) => void; onBlur?: (e: any) => void;
  onOptionsButton?: (e: any) => void; onOptionsActionDescription?: ReactNode;
  focusClassName?: string; focusWithinClassName?: string; noFocusRing?: boolean; autoFocus?: boolean;
  "flow-children"?: string; disabled?: boolean;
};
export const Focusable = forwardRef<HTMLDivElement, FocusableProps>(function Focusable(props, ref) {
  const { children, className, style, onActivate, onClick, onFocus, onBlur, onOptionsButton, focusClassName, autoFocus, disabled } = props;
  const el = useRef<HTMLDivElement>(null);
  useImperativeHandle(ref, () => el.current as HTMLDivElement);
  const item = !!(onActivate || onClick);
  const h = useRef(props); h.current = props;
  const [focused, setFocused] = useState(false);
  useEffect(() => {
    const node = el.current; if (!node || !item) return;
    const act = (e: Event) => (h.current.onActivate ?? h.current.onClick)?.(e);
    const opt = (e: Event) => h.current.onOptionsButton?.(e);
    node.addEventListener("mercury-activate", act); node.addEventListener("mercury-options", opt);
    return () => { node.removeEventListener("mercury-activate", act); node.removeEventListener("mercury-options", opt); };
  }, [item]);
  useEffect(() => { if (autoFocus && item) el.current?.focus(); }, []);
  return (
    <div ref={el} className={[className, focused ? focusClassName ?? FOCUS_CLASS : ""].filter(Boolean).join(" ")} style={style}
      {...(item ? { tabIndex: 0, "data-focusable": "", ...(autoFocus ? { "data-autofocus": "" } : {}), ...(onOptionsButton ? { "data-options": "" } : {}), ...(disabled ? { "data-disabled": "" } : {}) } : {})}
      onClick={item ? (e) => (onClick ?? onActivate)?.(e) : undefined}
      onMouseEnter={item ? () => el.current?.focus() : undefined}
      onFocus={(e) => { if (e.target === el.current) setFocused(true); onFocus?.(e); }}
      onBlur={(e) => { if (e.target === el.current) setFocused(false); onBlur?.(e); }}>
      {children}
    </div>
  );
});

// ---------- modals ----------
type ModalEntry = { id: number; el: ReactElement; prev: HTMLElement | null };
const modals = store<ModalEntry[]>([]);
let nextModal = 1;
export function showModal(el: ReactElement) {
  const id = nextModal++;
  const entry: ModalEntry = { id, el, prev: current() };
  modals.set([...modals.get(), entry]);
  const Close = () => {
    modals.set(modals.get().filter((m) => m.id !== id));
    setTimeout(() => { if (entry.prev?.isConnected) entry.prev.focus(); else focusFirst(); }, 0);
  };
  return { Close };
}
export function ModalHost() {
  const list = modals.use();
  return <>{list.map((m) => <ModalFrame key={m.id} entry={m} />)}</>;
}
function ModalFrame({ entry }: { entry: ModalEntry }) {
  const box = useRef<HTMLDivElement>(null);
  const close = () => { modals.set(modals.get().filter((m) => m.id !== entry.id)); setTimeout(() => { if (entry.prev?.isConnected) entry.prev.focus(); else focusFirst(); }, 0); };
  useEffect(() => {
    const node = box.current!; const cancel = () => close();
    node.addEventListener("mercury-cancel", cancel);
    setTimeout(() => { if (!node.contains(document.activeElement)) (node.querySelector<HTMLElement>("[data-autofocus]") ?? node.querySelector<HTMLElement>("[data-focusable]"))?.focus(); }, 0);
    return () => node.removeEventListener("mercury-cancel", cancel);
  }, []);
  return (
    <div ref={box} data-modal="" style={{ position: "absolute", inset: 0, zIndex: 50, background: "rgba(0,0,0,.65)", display: "flex", alignItems: "center", justifyContent: "center" }}>
      {cloneElement(entry.el as ReactElement<any>, { closeModal: close })}
    </div>
  );
}

const btnStyle: CSSProperties = { height: 34, padding: "0 18px", borderRadius: 4, display: "flex", alignItems: "center", justifyContent: "center", fontSize: 13, fontWeight: 600, color: "#fff", background: C.panel2, flex: 1 };

export function ConfirmModal({ strTitle, strDescription, strOKButtonText = "OK", strCancelButtonText = "Cancel", onOK, onCancel, closeModal }:
  { strTitle?: ReactNode; strDescription?: ReactNode; strOKButtonText?: string; strCancelButtonText?: string; onOK?: () => void; onCancel?: () => void; closeModal?: () => void }) {
  return (
    <div style={{ width: 520, maxWidth: "90%", background: "#171d25", borderRadius: 6, padding: "20px 22px", boxShadow: "0 10px 40px rgba(0,0,0,.6)" }}>
      <div style={{ fontSize: 17, fontWeight: 700, color: "#fff", marginBottom: 10 }}>{strTitle}</div>
      <div style={{ fontSize: 13, lineHeight: 1.5, color: "#c9cdd2", whiteSpace: "pre-wrap", marginBottom: 18 }}>{strDescription}</div>
      <div style={{ display: "flex", gap: 10 }}>
        <Focusable autoFocus onActivate={() => { closeModal?.(); onOK?.(); }} style={{ ...btnStyle, background: C.accent }}>{strOKButtonText}</Focusable>
        <Focusable onActivate={() => { closeModal?.(); onCancel?.(); }} style={btnStyle}>{strCancelButtonText}</Focusable>
      </div>
    </div>
  );
}

/** Decky's ModalRoot: the modal's contents. Closing on B is handled by the modal frame. */
export function ModalRoot({ children }: { children?: ReactNode; closeModal?: () => void; onCancel?: () => void }) {
  return <>{children}</>;
}

// ---------- toasts ----------
const toasts = store<{ id: number; title?: string; body: string }[]>([]);
let nextToast = 1;
export function toast(body: string, title?: string) {
  const id = nextToast++;
  toasts.set([...toasts.get(), { id, title, body }].slice(-4));
  setTimeout(() => toasts.set(toasts.get().filter((t) => t.id !== id)), 4500);
}
export function ToastHost() {
  const list = toasts.use();
  return (
    <div style={{ position: "absolute", right: 16, bottom: 52, zIndex: 60, display: "flex", flexDirection: "column", gap: 8, width: 300, pointerEvents: "none" }}>
      {list.map((t) => (
        <div key={t.id} style={{ background: "#23262e", borderLeft: `3px solid ${C.accent}`, borderRadius: 4, padding: "8px 12px", boxShadow: "0 6px 20px rgba(0,0,0,.5)" }}>
          {t.title && <div style={{ fontSize: 11, fontWeight: 700, color: C.dim, textTransform: "uppercase", letterSpacing: ".04em" }}>{t.title}</div>}
          <div style={{ fontSize: 12, color: "#fff", marginTop: 2 }}>{t.body}</div>
        </div>
      ))}
    </div>
  );
}

// ---------- simple widgets ----------
export function Spinner({ style }: { style?: CSSProperties }) {
  return <div style={{ width: 20, height: 20, border: "2px solid rgba(255,255,255,.2)", borderTopColor: "#fff", borderRadius: "50%", animation: "mercury-spin .8s linear infinite", ...style }} />;
}

export function ProgressBar({ nProgress = 0, indeterminate }: { nProgress?: number; indeterminate?: boolean }) {
  return (
    <div style={{ height: 6, borderRadius: 3, background: "rgba(255,255,255,.12)", overflow: "hidden", position: "relative" }}>
      <div style={indeterminate
        ? { position: "absolute", top: 0, bottom: 0, width: "30%", background: C.accent, animation: "mercury-indeterminate 1.2s ease-in-out infinite" }
        : { height: "100%", width: `${Math.max(0, Math.min(100, nProgress))}%`, background: C.accent, transition: "width .4s" }} />
    </div>
  );
}

export function Field({ label, description, children, childrenLayout, bottomSeparator }:
  { label?: ReactNode; description?: ReactNode; children?: ReactNode; childrenLayout?: "below" | "inline"; bottomSeparator?: string }) {
  return (
    <div style={{ padding: "8px 0", borderBottom: bottomSeparator === "none" ? undefined : "1px solid rgba(255,255,255,.06)" }}>
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 12 }}>
        <div style={{ minWidth: 0 }}>
          {label && <div style={{ fontSize: 14, color: "#fff" }}>{label}</div>}
          {description && <div style={{ fontSize: 12, color: C.dim, marginTop: 2, whiteSpace: "pre-wrap", wordBreak: "break-word" }}>{description}</div>}
        </div>
        {childrenLayout !== "below" && children}
      </div>
      {childrenLayout === "below" && <div style={{ marginTop: 6 }}>{children}</div>}
    </div>
  );
}

export function PanelSection({ title, children }: { title?: ReactNode; children?: ReactNode }) {
  return (
    <div style={{ margin: "14px 0 6px" }}>
      {title && <div style={{ fontSize: 12, fontWeight: 700, color: C.dim, textTransform: "uppercase", letterSpacing: ".06em", marginBottom: 4 }}>{title}</div>}
      <div style={{ background: C.panel, borderRadius: 6, padding: "4px 14px" }}>{children}</div>
    </div>
  );
}
export const PanelSectionRow = ({ children }: { children?: ReactNode }) => <div>{children}</div>;

export function ButtonItem({ children, onClick }: { children?: ReactNode; onClick?: () => void; layout?: string }) {
  return <div style={{ padding: "6px 0" }}><Focusable onActivate={onClick} style={{ ...btnStyle, flex: undefined }}>{children}</Focusable></div>;
}

export function ToggleField({ label, description, checked, onChange }: { label?: ReactNode; description?: ReactNode; checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <Focusable onActivate={() => onChange(!checked)} style={{ borderRadius: 4, margin: "2px -6px", padding: "0 6px" }}>
      <Field label={label} description={description}>
        <div style={{ width: 40, height: 22, borderRadius: 11, background: checked ? C.accent : "rgba(255,255,255,.2)", position: "relative", flex: "none", transition: "background .15s" }}>
          <div style={{ position: "absolute", top: 3, left: checked ? 21 : 3, width: 16, height: 16, borderRadius: 8, background: "#fff", transition: "left .15s" }} />
        </div>
      </Field>
    </Focusable>
  );
}

export function Dropdown({ rgOptions, selectedOption, onChange }: { rgOptions: { data: any; label: ReactNode }[]; selectedOption: any; onChange?: (o: { data: any; label: ReactNode }) => void; strDefaultLabel?: string }) {
  const sel = rgOptions.find((o) => o.data === selectedOption);
  const open = () => {
    const m = showModal(
      <div style={{ width: 420, background: "#171d25", borderRadius: 6, padding: 10 }}>
        {rgOptions.map((o) => (
          <Focusable key={String(o.data)} autoFocus={o.data === selectedOption} onActivate={() => { m.Close(); onChange?.(o); }}
            style={{ padding: "10px 12px", borderRadius: 4, fontSize: 14, color: "#fff", background: o.data === selectedOption ? "rgba(26,159,255,.18)" : undefined }}>{o.label}</Focusable>
        ))}
      </div>);
  };
  return <Focusable onActivate={open} style={{ ...btnStyle, justifyContent: "space-between", flex: undefined }}><span>{sel?.label ?? "Choose…"}</span><span>▾</span></Focusable>;
}

// ---------- text input with an on-screen keyboard ----------
const ROWS = ["1234567890", "qwertyuiop", "asdfghjkl'", "zxcvbnm-.:"];
function Keyboard({ initial, password, onChange, closeModal }: { initial: string; password?: boolean; onChange: (v: string) => void; closeModal?: () => void }) {
  const [v, setV] = useState(initial);
  const [shift, setShift] = useState(false);
  const kind = usePadKind();
  const root = useRef<HTMLDivElement>(null);
  const set = (n: string) => { setV(n); onChange(n); };
  // Controller shortcuts: X backspace, Y space, LB shift, Start done (A types the highlighted key, B closes).
  const live = useRef({ v, shift });
  live.current = { v, shift };
  useEffect(() => {
    const modal = root.current?.closest("[data-modal]");
    if (!modal) return;
    const on = (e: Event) => {
      const b = (e as CustomEvent<string>).detail;
      const { v: cur, shift: sh } = live.current;
      if (b === "x") set(cur.slice(0, -1));
      else if (b === "y") set(cur + " ");
      else if (b === "lb") setShift(!sh);
      else if (b === "start") closeModal?.();
      else return;
      e.preventDefault();
    };
    modal.addEventListener("mercury-button", on);
    return () => modal.removeEventListener("mercury-button", on);
  }, []);
  const hint = (b: string) => <span style={{ marginLeft: 6, minWidth: 16, height: 16, padding: "0 4px", boxSizing: "border-box", borderRadius: 8, border: "1px solid rgba(255,255,255,.55)", fontSize: 10, fontWeight: 800, display: "inline-flex", alignItems: "center", justifyContent: "center", opacity: 0.85 }}>{b}</span>;
  const key = (label: string, fn: () => void, w = 1, autoFocus = false, hintBtn?: string) =>
    <Focusable key={label} autoFocus={autoFocus} onActivate={fn} style={{ flex: w, height: 38, borderRadius: 4, background: C.panel2, color: "#fff", fontSize: 15, display: "flex", alignItems: "center", justifyContent: "center" }}>
      {label}{hintBtn && hint(btn(hintBtn, kind))}
    </Focusable>;
  return (
    <div ref={root} style={{ width: 640, background: "#171d25", borderRadius: 6, padding: 14 }}>
      <div style={{ height: 38, borderRadius: 4, background: "#0e141b", padding: "0 12px", display: "flex", alignItems: "center", fontSize: 16, color: "#fff", marginBottom: 10, overflow: "hidden", whiteSpace: "nowrap" }}>
        {password ? "•".repeat(v.length) : v}<span style={{ opacity: 0.6 }}>▏</span>
      </div>
      {ROWS.map((row, r) => (
        <div key={row} style={{ display: "flex", gap: 6, marginBottom: 6 }}>
          {row.split("").map((ch, i) => { const c = shift ? ch.toUpperCase() : ch; return key(c, () => set(v + c), 1, r === 1 && i === 0); })}
        </div>
      ))}
      <div style={{ display: "flex", gap: 6 }}>
        {key(shift ? "abc" : "ABC", () => setShift(!shift), 1.5, false, "LB")}
        {key("Space", () => set(v + " "), 4, false, "Y")}
        {key("⌫", () => set(v.slice(0, -1)), 1.5, false, "X")}
        {key("Clear", () => set(""), 1.5)}
        {key("Done", () => closeModal?.(), 1.5)}
      </div>
      <div style={{ marginTop: 10, fontSize: 11, color: C.dim, textAlign: "center" }}>{`${btn("A", kind)} type  ·  ${btn("B", kind)} close  ·  ${btn("X", kind)} backspace  ·  ${btn("Y", kind)} space  ·  ${btn("LB", kind)} shift  ·  ${kind === "ps" ? "Options" : "Start"} done`}</div>
    </div>
  );
}

export function TextField({ label, description, value = "", onChange, bIsPassword, focusOnMount }:
  { label?: ReactNode; description?: ReactNode; value?: string; onChange?: (e: { target: { value: string } }) => void; bIsPassword?: boolean; focusOnMount?: boolean }) {
  const input = useRef<HTMLInputElement>(null);
  const change = (v: string) => onChange?.({ target: { value: v } });
  useEffect(() => { if (focusOnMount) input.current?.focus(); }, []);
  useEffect(() => {
    const node = input.current!; const act = () => showModal(<Keyboard initial={node.value} password={bIsPassword} onChange={change} />);
    node.addEventListener("mercury-activate", act); return () => node.removeEventListener("mercury-activate", act);
  }, [onChange, bIsPassword]);
  return (
    <div style={{ margin: "4px 0" }}>
      {label && <div style={{ fontSize: 13, color: "#fff", marginBottom: 4 }}>{label}</div>}
      <input ref={input} data-focusable="" type={bIsPassword ? "password" : "text"} value={value} onChange={(e) => change(e.target.value)}
        className="mercury-input" placeholder="Press A to type"
        style={{ width: "100%", boxSizing: "border-box", height: 36, borderRadius: 4, border: "none", background: "#0e141b", color: "#fff", fontSize: 14, padding: "0 12px" }} />
      {description && <div style={{ fontSize: 12, color: C.dim, marginTop: 4 }}>{description}</div>}
    </div>
  );
}
