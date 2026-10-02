import { Navigation } from "@decky/ui";
import { fetchNoCors } from "@decky/api";

/**
 * Touch-only "Get with Mercury" button on Steam store app pages (spike S4: the D-pad cannot reach injected
 * elements; the Quick Access panel covers controllers). Same technique as protondb-decky: talk to the store
 * tab over Steam's local debug port, inject a button, and receive taps through a Runtime binding.
 */

const BINDING = "mercuryGet";
const INJECT = `(() => {
  document.getElementById("mercury-get")?.remove();
  const m = location.href.match(/store\\.steampowered\\.com\\/app\\/(\\d+)/);
  if (!m) return "no app";
  const b = document.createElement("a");
  b.id = "mercury-get"; b.href = "#";
  b.textContent = "\\u2193  Get with Mercury";
  b.style.cssText = "position:fixed;right:24px;top:96px;z-index:99999;padding:10px 18px;border-radius:4px;" +
    "font:700 15px 'Motiva Sans',Arial,sans-serif;color:#fff;text-decoration:none;" +
    "background:linear-gradient(90deg,#70d61d,#01a75b);box-shadow:0 4px 16px rgba(0,0,0,.5)";
  b.onclick = (e) => { e.preventDefault(); window.${BINDING}(m[1]); };
  document.body.appendChild(b);
  return "button " + m[1];
})()`;

let ws: WebSocket | undefined;
let tabId = "";
let msgId = 0;

const onStorePage = () =>
  ((window as any).SteamUIStore?.WindowStore?.GamepadUIMainWindowInstance?.m_history?.location?.pathname ?? "").startsWith("/steamweb");

function send(method: string, params: object = {}) {
  if (ws?.readyState === WebSocket.OPEN) ws.send(JSON.stringify({ id: ++msgId, method, params }));
}

function disconnect() {
  try { ws?.close(); } catch { /* already closed */ }
  ws = undefined;
  tabId = "";
}

async function connect() {
  let tabs: { id: string; url: string; webSocketDebuggerUrl?: string }[];
  try { tabs = await (await fetchNoCors("http://127.0.0.1:8080/json")).json(); } catch { return; }
  const tab = tabs.find((t) => t.url.includes("store.steampowered.com") && t.webSocketDebuggerUrl);
  if (!tab || tab.id === tabId) return;
  disconnect();
  tabId = tab.id;
  const sock = new WebSocket(tab.webSocketDebuggerUrl!);
  ws = sock;
  sock.onopen = () => {
    send("Page.enable");
    send("Runtime.enable");
    send("Runtime.addBinding", { name: BINDING });
    send("Runtime.evaluate", { expression: INJECT });
  };
  sock.onmessage = (m) => {
    let d: any;
    try { d = JSON.parse(m.data); } catch { return; }
    if (d.method === "Runtime.bindingCalled" && d.params?.name === BINDING) {
      const appid = Number(d.params.payload);
      if (appid) Navigation.Navigate(`/mercury/game/${appid}`);
    }
    // Full navigations and in-page route changes both need the button again.
    if ((d.method === "Page.frameNavigated" && !d.params?.frame?.parentId) || d.method === "Page.navigatedWithinDocument") {
      setTimeout(() => send("Runtime.evaluate", { expression: INJECT }), 1200);
    }
  };
  sock.onclose = () => { if (ws === sock) { ws = undefined; tabId = ""; } };
  sock.onerror = () => { try { sock.close(); } catch { /* ignore */ } };
}

/** Called every couple of seconds by the plugin's background watcher. */
export function tickStoreButton() {
  if (onStorePage()) connect();
  else if (ws) disconnect();
}

export const stopStoreButton = disconnect;
