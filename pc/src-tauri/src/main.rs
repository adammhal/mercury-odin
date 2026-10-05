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

fn main() {
    start_engine();
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("Mercury failed to start");
}
