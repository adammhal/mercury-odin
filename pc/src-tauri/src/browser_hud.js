// A small hint bar for Mercury's browser. The controller itself is driven by the engine (real mouse and keyboard input).
(function () {
  if (window.top !== window) return;
  // Pop-ups and target=_blank links would open windows Mercury cannot show; keep everything in this one.
  window.open = function (u) { if (u) location.href = u; return null; };
  document.addEventListener("click", function (e) { var a = e.target && e.target.closest && e.target.closest("a[target]"); if (a) a.target = "_self"; }, true);
  function bar() {
    if (document.getElementById("mercury-hud")) return;
    var d = document.createElement("div"); d.id = "mercury-hud";
    d.style.cssText = "position:fixed;left:50%;bottom:10px;transform:translateX(-50%);z-index:2147483647;padding:6px 14px;border-radius:99px;background:rgba(14,20,27,.88);color:#dcdedf;font:600 12px system-ui,sans-serif;pointer-events:none;white-space:nowrap;opacity:1;transition:opacity .6s";
    d.textContent = "Left stick: mouse  \u00b7  A/\u2715: click  \u00b7  B/\u25cb: back  \u00b7  Y/\u25b3: keyboard  \u00b7  Share/View: close";
    (document.body || document.documentElement).appendChild(d);
    setTimeout(function () { d.style.opacity = "0"; }, 9000);
  }
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", bar); else bar();
})();
