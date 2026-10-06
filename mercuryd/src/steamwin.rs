//! Windows: put games into Steam (Big Picture) through Steam's own client functions, the same calls the Odin plugin uses.
//! Steam only opens its CEF debug port (127.0.0.1:8080) when `.cef-enable-remote-debugging` exists in its folder, so
//! `ensure_ready` creates that file and restarts Steam once if the port is closed.
use anyhow::{Result, anyhow, bail};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::{path::{Path, PathBuf}, time::Duration};
use tokio_tungstenite::tungstenite::Message;

const DEBUG: &str = "http://127.0.0.1:8080";

pub fn steam_dir() -> Option<PathBuf> {
    let mut c: Vec<PathBuf> = vec![];
    for v in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Some(p) = std::env::var_os(v) { c.push(PathBuf::from(p).join("Steam")); }
    }
    c.push(PathBuf::from(r"C:\Program Files (x86)\Steam"));
    c.into_iter().find(|p| p.join("steam.exe").is_file())
}

fn flag(dir: &Path) -> PathBuf { dir.join(".cef-enable-remote-debugging") }

pub fn status() -> Value {
    match steam_dir() {
        Some(d) => json!({ "found": true, "flag": flag(&d).exists() }),
        None => json!({ "found": false, "flag": false }),
    }
}

async fn debug_up(http: &reqwest::Client) -> bool {
    http.get(format!("{DEBUG}/json/version")).timeout(Duration::from_millis(1500)).send().await.is_ok_and(|r| r.status().is_success())
}

fn ps(script: &str) -> Result<String> {
    use std::os::windows::process::CommandExt;
    let o = std::process::Command::new("powershell").args(["-NoProfile", "-NonInteractive", "-Command", script]).creation_flags(0x0800_0000).output()?;
    Ok(String::from_utf8_lossy(&o.stdout).to_string())
}

fn steam_running() -> bool {
    ps("(Get-Process steam -ErrorAction SilentlyContinue | Measure-Object).Count").map(|s| s.trim() != "0").unwrap_or(false)
}

/// A game launched from Steam is a child of steam.exe. Steam's helpers are children too and do not count.
fn game_running() -> bool {
    ps("$s=(Get-Process steam -ErrorAction SilentlyContinue).Id; if(-not $s){0}else{ (Get-CimInstance Win32_Process | Where-Object { $s -contains $_.ParentProcessId -and $_.Name -notmatch '^(steamwebhelper|steamservice|steamerrorreporter|gameoverlayui|vulkandriverquery|steamsysinfo)' } | Measure-Object).Count }")
        .map(|s| s.trim() != "0").unwrap_or(false)
}

fn launch(dir: &Path, args: &[&str]) -> Result<()> {
    use std::os::windows::process::CommandExt;
    // Detached, outside Mercury's job object, so Steam outlives Mercury and its engine.
    let mut c = std::process::Command::new(dir.join("steam.exe"));
    c.args(args).current_dir(dir);
    if c.creation_flags(0x0000_0008 | 0x0000_0200 | 0x0100_0000).spawn().is_err() {
        std::process::Command::new(dir.join("steam.exe")).args(args).current_dir(dir).creation_flags(0x0000_0008 | 0x0000_0200).spawn()?;
    }
    Ok(())
}

/// Make sure Steam is running with its debug port open. Restarts Steam once if it was started without the flag file.
pub async fn ensure_ready(http: &reqwest::Client) -> Result<()> {
    let dir = steam_dir().ok_or_else(|| anyhow!("Steam is not installed"))?;
    if !flag(&dir).exists() { std::fs::write(flag(&dir), b"")?; }
    if debug_up(http).await { return Ok(()); }
    if steam_running() {
        if game_running() { bail!("Steam has to restart once to let Mercury add games. Close your game and try again."); }
        launch(&dir, &["-shutdown"])?;
        for _ in 0..30 { if !steam_running() { break; } tokio::time::sleep(Duration::from_secs(2)).await; }
        if steam_running() { bail!("Steam did not close"); }
    }
    launch(&dir, &[])?;
    for _ in 0..60 {
        tokio::time::sleep(Duration::from_secs(2)).await;
        if debug_up(http).await { return Ok(()); }
    }
    bail!("Steam did not start in time")
}

/// Run JavaScript inside Steam's client (SharedJSContext) and return its value.
async fn eval(http: &reqwest::Client, expr: &str) -> Result<Value> {
    let tabs: Value = http.get(format!("{DEBUG}/json")).timeout(Duration::from_secs(5)).send().await?.json().await?;
    let ws = tabs.as_array().and_then(|t| t.iter().find(|t| t["title"] == "SharedJSContext")).and_then(|t| t["webSocketDebuggerUrl"].as_str())
        .ok_or_else(|| anyhow!("Steam's client page is not ready yet"))?.to_string();
    let (mut sock, _) = tokio_tungstenite::connect_async(ws).await?;
    sock.send(Message::Text(json!({ "id": 1, "method": "Runtime.evaluate",
        "params": { "expression": expr, "awaitPromise": true, "returnByValue": true } }).to_string().into())).await?;
    let r = tokio::time::timeout(Duration::from_secs(60), async {
        while let Some(m) = sock.next().await {
            if let Message::Text(t) = m? {
                let v: Value = serde_json::from_str(&t)?;
                if v["id"] == 1 { return Ok::<Value, anyhow::Error>(v); }
            }
        }
        bail!("Steam closed the connection")
    }).await.map_err(|_| anyhow!("Steam did not answer"))??;
    if let Some(e) = r["result"]["exceptionDetails"].as_object() { bail!("Steam rejected it: {}", e.get("text").and_then(|t| t.as_str()).unwrap_or("error")); }
    Ok(r["result"]["result"]["value"].clone())
}

/// Create a non-Steam shortcut with its art and return its app id. `art` = (type, ext, base64) as sent by `/steam/art`.
pub async fn add_shortcut(http: &reqwest::Client, name: &str, exe: &Path, art: &[(u8, String, String)]) -> Result<u32> {
    ensure_ready(http).await?;
    let dir = exe.parent().map(|p| p.display().to_string()).unwrap_or_default();
    let js = format!(r#"(async () => {{
        const id = await SteamClient.Apps.AddShortcut({n}, {e}, {d}, "");
        if (!id) throw new Error("Steam did not create the shortcut");
        SteamClient.Apps.SetShortcutName(id, {n});
        for (const [t, ext, data] of {a}) await SteamClient.Apps.SetCustomArtworkForApp(id, data, ext, t);
        return id;
    }})()"#, n = json!(name), e = json!(exe.display().to_string()), d = json!(dir), a = json!(art));
    let id = eval(http, &js).await?.as_u64().ok_or_else(|| anyhow!("Steam did not return a shortcut id"))?;
    Ok(id as u32)
}

pub async fn shortcut_exists(http: &reqwest::Client, id: u32) -> bool {
    if !debug_up(http).await { return false; }
    eval(http, &format!("!!window.appStore?.GetAppOverviewByAppID({id})")).await.ok().and_then(|v| v.as_bool()).unwrap_or(false)
}

/// Best effort: only when Steam is already running with the debug port open.
pub async fn remove_shortcut(http: &reqwest::Client, id: u32) -> Result<()> {
    if id == 0 { return Ok(()); }
    if !debug_up(http).await { bail!("Steam is not running with its debug port open; remove the shortcut in Steam"); }
    eval(http, &format!("SteamClient.Apps.RemoveShortcut({id})")).await?;
    Ok(())
}

/// Change the title and/or artwork of a shortcut that is already in Steam. `art` = (slot, ext, base64).
pub async fn update_shortcut(http: &reqwest::Client, id: u32, name: Option<&str>, art: &[(u8, String, String)]) -> Result<()> {
    ensure_ready(http).await?;
    let rename = name.map(|n| format!("SteamClient.Apps.SetShortcutName(id, {});", json!(n))).unwrap_or_default();
    let js = format!(r#"(async () => {{
        const id = {id};
        {rename}
        for (const [t, ext, data] of {a}) await SteamClient.Apps.SetCustomArtworkForApp(id, data, ext, t);
        return true;
    }})()"#, a = json!(art));
    eval(http, &js).await?;
    Ok(())
}

/// The artwork Steam currently shows for a shortcut: its file in the user's grid folder.
pub fn grid_file(shortcut_id: u32, slot: u8) -> Option<(PathBuf, &'static str)> {
    let user = crate::steam::steam_id64()? - 76561197960265728;
    let dir = steam_dir()?.join("userdata").join(user.to_string()).join("config").join("grid");
    let names: [String; 4] = match slot {
        0 => [format!("{shortcut_id}p.jpg"), format!("{shortcut_id}p.png"), String::new(), String::new()],
        1 => [format!("{shortcut_id}_hero.jpg"), format!("{shortcut_id}_hero.png"), String::new(), String::new()],
        2 => [format!("{shortcut_id}_logo.png"), format!("{shortcut_id}_logo.jpg"), String::new(), String::new()],
        3 => [format!("{shortcut_id}.jpg"), format!("{shortcut_id}.png"), String::new(), String::new()],
        _ => return None,
    };
    names.iter().filter(|n| !n.is_empty()).map(|n| dir.join(n)).find(|p| p.is_file())
        .map(|p| { let t = if p.extension().is_some_and(|e| e == "png") { "image/png" } else { "image/jpeg" }; (p, t) })
}
