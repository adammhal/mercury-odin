// Mercury for Windows: a full-screen, controller-first window around the mercuryd engine.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

const ENGINE: &str = "127.0.0.1:47800";

fn engine_up() -> bool {
    TcpStream::connect_timeout(&ENGINE.parse::<SocketAddr>().unwrap(), Duration::from_millis(300)).is_ok()
}

/// Starts mercuryd.exe from next to Mercury.exe unless one is already running. The engine is not a child
/// of the window's lifetime: downloads keep going after the window closes and still reach warmUP.
fn start_engine() {
    if engine_up() { return; }
    let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(PathBuf::from)) else { return };
    let exe = dir.join("mercuryd.exe");
    let log_dir = std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("Mercury")).unwrap_or_else(|| dir.clone());
    let _ = std::fs::create_dir_all(&log_dir);
    let log = std::fs::OpenOptions::new().create(true).append(true).open(log_dir.join("mercuryd.log"));
    let mut cmd = std::process::Command::new(&exe);
    cmd.current_dir(&dir).env("RUST_LOG", "mercuryd=info");
    if let Ok(f) = log { if let Ok(f2) = f.try_clone() { cmd.stdout(f).stderr(f2); } }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW
        cmd.creation_flags(0x0000_0200 | 0x0800_0000);
    }
    if cmd.spawn().is_err() { return; }
    for _ in 0..50 { if engine_up() { break; } std::thread::sleep(Duration::from_millis(100)); }
}

/// Downloads the browser window has started and not yet finished: (url, file name).
static ACTIVE: std::sync::Mutex<Vec<(String, String)>> = std::sync::Mutex::new(Vec::new());

/// Tells the engine to start or stop the controller helper for the browser window.
fn browser_assist(action: &str) {
    use std::io::{Read, Write};
    if let Ok(mut s) = TcpStream::connect_timeout(&ENGINE.parse::<SocketAddr>().unwrap(), Duration::from_millis(500)) {
        let _ = write!(s, "POST /assist/browser/{action} HTTP/1.1\r\nHost: 127.0.0.1:47800\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        let _ = s.set_read_timeout(Some(Duration::from_secs(3)));
        let _ = s.read(&mut [0u8; 256]);
    }
}

/// A full-screen browser for hosts Real-Debrid cannot fetch. The engine's helper drives it with the controller; a finished
/// download is saved to Downloads, where Mercury's watcher picks it up, and the window closes itself.
// async: building a window inside a synchronous command freezes the whole app on Windows.
#[tauri::command]
async fn open_browser(app: tauri::AppHandle, url: String) -> Result<(), String> {
    use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent, webview::DownloadEvent};
    if !(url.starts_with("https://") || url.starts_with("http://")) { return Err("not a web link".into()); }
    let parsed: tauri::Url = url.parse().map_err(|_| "bad link".to_string())?;
    if let Some(w) = app.get_webview_window("browser") { let _ = w.destroy(); }
    let downloads = std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join("Downloads")).unwrap_or_default();
    let a_dl = app.clone();
    let win = WebviewWindowBuilder::new(&app, "browser", WebviewUrl::External(parsed))
        .title("Mercury Browser")
        .fullscreen(true)
        .initialization_script(include_str!("browser_hud.js"))
        .on_download(move |_wv, ev| {
            match ev {
                DownloadEvent::Requested { url, destination } => {
                    let name = destination.file_name().map(|n| n.to_os_string()).unwrap_or_else(|| "download".into());
                    // The same file twice (a page that starts it again, or a second click) is refused.
                    let key = name.to_string_lossy().to_string();
                    { let mut act = ACTIVE.lock().unwrap(); if act.iter().any(|(_, k)| *k == key) { return false; } act.push((url.to_string(), key)); }
                    *destination = downloads.join(name);
                    // The download carries on while the window is hidden (closing it would cancel it). Hand the screen back to Mercury.
                    if let Some(w) = a_dl.get_webview_window("browser") { let _ = w.hide(); }
                    if let Some(m) = a_dl.get_webview_window("main") { let _ = m.set_focus(); }
                    std::thread::spawn(|| browser_assist("stop"));
                    let _ = a_dl.emit("browser-download", "started");
                }
                DownloadEvent::Finished { url, success, .. } => {
                    let left = { let mut act = ACTIVE.lock().unwrap(); act.retain(|(u, _)| *u != url.to_string()); act.len() };
                    let _ = a_dl.emit("browser-download", if success { "finished" } else { "failed" });
                    if left == 0 { if let Some(w) = a_dl.get_webview_window("browser") { let _ = w.destroy(); } }
                }
                _ => {}
            }
            true
        })
        .build()
        .map_err(|e| e.to_string())?;
    win.on_window_event(|e| { if let WindowEvent::Destroyed = e { std::thread::spawn(|| browser_assist("stop")); } });
    std::thread::spawn(|| browser_assist("start"));
    Ok(())
}

fn main() {
    start_engine();
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![open_browser])
        .run(tauri::generate_context!())
        .expect("Mercury failed to start");
}
