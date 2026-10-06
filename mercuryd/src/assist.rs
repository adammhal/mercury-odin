//! Windows: while a repack's installer runs, bring its window to the front and let the controller drive it,
//! because installer windows are plain mouse-and-keyboard programs.
//!   left stick: mouse · A: click · X: space · D-pad down/up: Tab / Shift+Tab · Start: Enter (Next) · right stick: scroll
//! Works only on a non-elevated installer (Windows does not let a normal program send input to an elevated window).
use std::{path::{Path, PathBuf}, sync::{Arc, atomic::{AtomicBool, Ordering}}, time::{Duration, Instant}};
use windows_sys::Win32::{
    Foundation::{BOOL, CloseHandle, HWND, LPARAM, MAX_PATH, POINT},
    System::Threading::{AttachThreadInput, GetCurrentThreadId, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW},
    UI::{Input::{KeyboardAndMouse::*, XboxController::*},
        WindowsAndMessaging::*},
};

pub struct Assist(Arc<AtomicBool>);
impl Drop for Assist { fn drop(&mut self) { self.0.store(true, Ordering::Relaxed); } }

/// Start helping with the installer whose files are in `setup_dir`. Stops when the returned guard is dropped.
pub fn start(setup_dir: PathBuf) -> Assist {
    let stop = Arc::new(AtomicBool::new(false));
    let s = stop.clone();
    std::thread::spawn(move || run(&setup_dir, &s));
    Assist(stop)
}

fn image_of(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() { return None; }
        let mut buf = [0u16; MAX_PATH as usize * 2];
        let mut n = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut n);
        CloseHandle(h);
        (ok != 0).then(|| String::from_utf16_lossy(&buf[..n as usize]))
    }
}

/// An installer process: runs from the setup folder, or is Inno/NSIS's temp copy (`...\is-XXXX.tmp\setup.tmp`, `~nsu.tmp`).
fn is_installer(path: &str, setup_dir: &Path) -> bool {
    let l = path.to_lowercase();
    l.starts_with(&setup_dir.to_string_lossy().to_lowercase()) || l.contains("\\is-") && l.contains(".tmp") || l.contains("~nsu.tmp") || l.ends_with("\\setup.exe")
}

unsafe extern "system" fn collect(h: HWND, lp: LPARAM) -> BOOL {
    unsafe {
        let v = &mut *(lp as *mut Vec<(HWND, u32)>);
        if IsWindowVisible(h) != 0 && GetWindow(h, GW_OWNER).is_null() {
            let mut pid = 0u32;
            GetWindowThreadProcessId(h, &mut pid);
            v.push((h, pid));
        }
    }
    1
}

fn installer_windows(setup_dir: &Path) -> Vec<HWND> {
    let mut all: Vec<(HWND, u32)> = vec![];
    unsafe { EnumWindows(Some(collect), &mut all as *mut _ as LPARAM); }
    all.into_iter().filter(|(_, pid)| image_of(*pid).is_some_and(|p| is_installer(&p, setup_dir))).map(|(h, _)| h).collect()
}

fn front(h: HWND) {
    unsafe {
        ShowWindow(h, SW_RESTORE);
        SetWindowPos(h, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW);
        // Windows only lets the foreground app hand focus on: borrow its input queue for a moment.
        let fg = GetForegroundWindow();
        let ft = if fg.is_null() { 0 } else { GetWindowThreadProcessId(fg, std::ptr::null_mut()) };
        let me = GetCurrentThreadId();
        if ft != 0 && ft != me { AttachThreadInput(me, ft, 1); }
        SetForegroundWindow(h);
        if ft != 0 && ft != me { AttachThreadInput(me, ft, 0); }
    }
}

fn key(vk: u16, up: bool) -> INPUT {
    INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: if up { KEYEVENTF_KEYUP } else { 0 }, time: 0, dwExtraInfo: 0 } } }
}
fn mouse(flags: u32, data: i32) -> INPUT {
    INPUT { r#type: INPUT_MOUSE, Anonymous: INPUT_0 { mi: MOUSEINPUT { dx: 0, dy: 0, mouseData: data as u32, dwFlags: flags, time: 0, dwExtraInfo: 0 } } }
}
fn send(inputs: &[INPUT]) { unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32); } }
fn tap(vk: u16) { send(&[key(vk, false), key(vk, true)]); }

const DEAD: f32 = 7849.0;
fn axis(v: i16) -> f32 {
    let v = v as f32;
    if v.abs() < DEAD { 0.0 } else { ((v.abs() - DEAD) / (32767.0 - DEAD)) * v.signum() }
}

fn run(setup_dir: &Path, stop: &AtomicBool) {
    let (mut prev, mut last_front, mut last_scan) = (0u16, Instant::now() - Duration::from_secs(10), Instant::now() - Duration::from_secs(10));
    let mut wheel = 0f32;
    while !stop.load(Ordering::Relaxed) {
        if last_scan.elapsed() > Duration::from_millis(700) {
            last_scan = Instant::now();
            let wins = installer_windows(setup_dir);
            let fg = unsafe { GetForegroundWindow() };
            // Keep the installer on top, but not more than every few seconds so the user can still switch away.
            if let Some(&w) = wins.first() { if !wins.contains(&fg) && last_front.elapsed() > Duration::from_secs(4) { front(w); last_front = Instant::now(); } }
        }
        let mut st: XINPUT_STATE = unsafe { std::mem::zeroed() };
        let pad = (0..4).find(|&i| unsafe { XInputGetState(i, &mut st) } == 0);
        if pad.is_some() {
            let g = st.Gamepad;
            let (x, y) = (axis(g.sThumbLX), axis(g.sThumbLY));
            if x != 0.0 || y != 0.0 {
                let mut p = POINT { x: 0, y: 0 };
                unsafe {
                    GetCursorPos(&mut p);
                    let sp = 4.0 + 20.0 * (x * x + y * y).sqrt();
                    SetCursorPos(p.x + (x * sp) as i32, p.y - (y * sp) as i32);
                }
            }
            wheel += axis(g.sThumbRY) * 40.0;
            if wheel.abs() >= 1.0 { send(&[mouse(MOUSEEVENTF_WHEEL, wheel as i32)]); wheel = 0.0; }
            let b = g.wButtons;
            let pressed = |m: u16| b & m != 0 && prev & m == 0;
            let released = |m: u16| b & m == 0 && prev & m != 0;
            if pressed(XINPUT_GAMEPAD_A) { send(&[mouse(MOUSEEVENTF_LEFTDOWN, 0)]); }
            if released(XINPUT_GAMEPAD_A) { send(&[mouse(MOUSEEVENTF_LEFTUP, 0)]); }
            if pressed(XINPUT_GAMEPAD_X) { tap(VK_SPACE); }
            if pressed(XINPUT_GAMEPAD_START) { tap(VK_RETURN); }
            if pressed(XINPUT_GAMEPAD_DPAD_DOWN) { tap(VK_TAB); }
            if pressed(XINPUT_GAMEPAD_DPAD_UP) { send(&[key(VK_SHIFT, false), key(VK_TAB, false), key(VK_TAB, true), key(VK_SHIFT, true)]); }
            prev = b;
        }
        std::thread::sleep(Duration::from_millis(16));
    }
    // Release the always-on-top flag so a finished installer does not stay above everything.
    for w in installer_windows(setup_dir) { unsafe { SetWindowPos(w, HWND_NOTOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE); } }
}
