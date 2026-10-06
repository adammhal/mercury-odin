//! Windows: while a repack's installer runs (or Mercury's browser window is open), bring its window to the front and let
//! the controller drive it, because those windows are plain mouse-and-keyboard programs.
//!   left stick: mouse · A: click · X: space · D-pad: arrow keys (repeat while held) · RB / LB: Tab / Shift+Tab · Start: Enter (Next) · right stick: scroll
//! Works only on a non-elevated installer (Windows does not let a normal program send input to an elevated window).
use std::{path::{Path, PathBuf}, sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}}, time::{Duration, Instant}};
use windows_sys::Win32::{
    Devices::{DeviceAndDriverInstallation::*, HumanInterfaceDevice::*},
    Foundation::{BOOL, CloseHandle, GENERIC_READ, GetLastError, HWND, INVALID_HANDLE_VALUE, LPARAM, MAX_PATH, POINT},
    Storage::FileSystem::{CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING, ReadFile},
    System::Threading::{AttachThreadInput, GetCurrentThreadId, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW},
    UI::{Input::{KeyboardAndMouse::*, XboxController::*},
        WindowsAndMessaging::*},
};

pub struct Assist(Arc<AtomicBool>);
impl Drop for Assist { fn drop(&mut self) { self.0.store(true, Ordering::Relaxed); } }

/// How the helper runs: elevated in its own process (so it can drive installers that restarted themselves as admin),
/// or inside the engine if that process could not be started.
/// What the helper is driving: a repack installer (found by its folder) or Mercury's own browser window.
#[derive(Clone)]
pub enum Mode { Installer(PathBuf), Browser }
impl Mode {
    fn arg(&self) -> String { match self { Mode::Installer(d) => d.display().to_string(), Mode::Browser => "browser".into() } }
    fn from_arg(a: &str) -> Mode { if a == "browser" { Mode::Browser } else { Mode::Installer(PathBuf::from(a)) } }
    fn stop_name(&self) -> &'static str { match self { Mode::Installer(_) => "mercury-assist.stop", Mode::Browser => "mercury-assist-browser.stop" } }
    fn windows(&self) -> Vec<HWND> { match self { Mode::Installer(d) => installer_windows(d), Mode::Browser => browser_windows() } }
}

pub enum Guard { InProc(Assist), Helper(PathBuf) }
impl Drop for Guard { fn drop(&mut self) { if let Guard::Helper(p) = self { let _ = std::fs::write(p, b"stop"); } } }

fn stop_file(m: &Mode) -> PathBuf { std::env::temp_dir().join(m.stop_name()) }

/// Start the elevated helper (`mercuryd.exe --assist <dir> <engine pid>`); fall back to helping from inside the engine.
pub fn spawn(mode: Mode) -> Guard {
    let stop = stop_file(&mode);
    let _ = std::fs::remove_file(&stop);
    if let Ok(exe) = std::env::current_exe() {
        let q = |s: &str| s.replace('\'', "''");
        let script = format!("Start-Process -FilePath '{}' -ArgumentList '--assist','\"{}\"','{}' -Verb RunAs -WindowStyle Hidden",
            q(&exe.display().to_string()), q(&mode.arg()), std::process::id());
        let mut c = std::process::Command::new("powershell");
        c.args(["-NoProfile", "-NonInteractive", "-Command", &script]);
        std::os::windows::process::CommandExt::creation_flags(&mut c, 0x0800_0000);
        if c.spawn().is_ok() { tracing::info!("assist: started the elevated helper"); return Guard::Helper(stop); }
    }
    tracing::warn!("assist: could not start the elevated helper; helping from the engine (cannot drive admin installers)");
    Guard::InProc(start(mode))
}

/// Entry point of the elevated helper process.
pub fn helper_main(arg: String, parent: u32) {
    let mode = Mode::from_arg(&arg);
    let stop = Arc::new(AtomicBool::new(false));
    let s = stop.clone();
    let file = stop_file(&mode);
    std::thread::spawn(move || loop {
        let alive = unsafe {
            let h = OpenProcess(0x0010_0000 /* SYNCHRONIZE */, 0, parent);
            let a = !h.is_null() && windows_sys::Win32::System::Threading::WaitForSingleObject(h, 0) != 0;
            if !h.is_null() { CloseHandle(h); }
            a
        };
        if file.exists() || !alive { s.store(true, Ordering::Relaxed); break; }
        std::thread::sleep(Duration::from_millis(500));
    });
    run(&mode, &stop);
}

/// Start helping with the installer whose files are in `setup_dir`. Stops when the returned guard is dropped.
pub fn start(mode: Mode) -> Assist {
    let stop = Arc::new(AtomicBool::new(false));
    let s = stop.clone();
    std::thread::spawn(move || run(&mode, &s));
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

fn browser_windows() -> Vec<HWND> {
    let mut all: Vec<(HWND, u32)> = vec![];
    unsafe { EnumWindows(Some(collect), &mut all as *mut _ as LPARAM); }
    all.into_iter().map(|(h, _)| h).filter(|&h| unsafe {
        let mut buf = [0u16; 64];
        let n = GetWindowTextW(h, buf.as_mut_ptr(), 64);
        n > 0 && String::from_utf16_lossy(&buf[..n as usize]) == "Mercury Browser"
    }).collect()
}

fn installer_windows(setup_dir: &Path) -> Vec<HWND> {
    let mut all: Vec<(HWND, u32)> = vec![];
    unsafe { EnumWindows(Some(collect), &mut all as *mut _ as LPARAM); }
    all.into_iter().filter(|(_, pid)| image_of(*pid).is_some_and(|p| is_installer(&p, setup_dir))).map(|(h, _)| h).collect()
}

fn front(h: HWND, topmost: bool) {
    unsafe {
        ShowWindow(h, SW_RESTORE);
        if topmost { SetWindowPos(h, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW); }
        // Windows only lets the foreground app hand focus on. Drop the focus-stealing lock, tap Alt (which counts as
        // the user's input), borrow the foreground app's input queue, then ask for focus.
        SystemParametersInfoW(SPI_SETFOREGROUNDLOCKTIMEOUT, 0, std::ptr::null_mut(), SPIF_SENDCHANGE);
        AllowSetForegroundWindow(ASFW_ANY);
        keybd_event(VK_MENU as u8, 0, 0, 0);
        keybd_event(VK_MENU as u8, 0, KEYEVENTF_KEYUP, 0);
        let fg = GetForegroundWindow();
        let ft = if fg.is_null() { 0 } else { GetWindowThreadProcessId(fg, std::ptr::null_mut()) };
        let me = GetCurrentThreadId();
        if ft != 0 && ft != me { AttachThreadInput(me, ft, 1); }
        SetForegroundWindow(h);
        BringWindowToTop(h);
        SwitchToThisWindow(h, 1);
        SetFocus(h);
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

/// One controller reading, whichever way it was obtained.
#[derive(Default, Clone, Copy)]
struct Pad { lx: f32, ly: f32, ry: f32, a: bool, b: bool, x: bool, y: bool, start: bool, select: bool, up: bool, down: bool, left: bool, right: bool, lb: bool, rb: bool }

// ---- HID: Apollo emulates a DualShock 4 (not an Xbox pad), which XInput cannot see ----
struct Hid { latest: Arc<Mutex<Option<(u16, Vec<u8>)>>>, handles: Vec<usize> }

fn wide(p: *const u16) -> String {
    let mut n = 0; unsafe { while *p.add(n) != 0 { n += 1; } String::from_utf16_lossy(std::slice::from_raw_parts(p, n)) }
}

fn open_hid(path: &str, access: u32) -> windows_sys::Win32::Foundation::HANDLE {
    let w: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
    unsafe { CreateFileW(w.as_ptr(), access, FILE_SHARE_READ | FILE_SHARE_WRITE, std::ptr::null(), OPEN_EXISTING, 0, std::ptr::null_mut()) }
}

/// Sony pads (real or ViGEm's emulated DualShock 4): vendor 054C.
fn hid_pads() -> Vec<(String, u16, u16)> {
    let mut out = vec![];
    unsafe {
        let mut guid: windows_sys::core::GUID = std::mem::zeroed();
        HidD_GetHidGuid(&mut guid);
        let set = SetupDiGetClassDevsW(&guid, std::ptr::null(), std::ptr::null_mut(), DIGCF_PRESENT | DIGCF_DEVICEINTERFACE);
        let mut i = 0;
        loop {
            let mut ifd: SP_DEVICE_INTERFACE_DATA = std::mem::zeroed();
            ifd.cbSize = std::mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as u32;
            if SetupDiEnumDeviceInterfaces(set, std::ptr::null(), &guid, i, &mut ifd) == 0 { break; }
            i += 1;
            let mut need = 0u32;
            SetupDiGetDeviceInterfaceDetailW(set, &ifd, std::ptr::null_mut(), 0, &mut need, std::ptr::null_mut());
            let mut buf = vec![0u32; need as usize / 4 + 2];
            let det = buf.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
            (*det).cbSize = if cfg!(target_pointer_width = "64") { 8 } else { 6 };
            if SetupDiGetDeviceInterfaceDetailW(set, &ifd, det, need, std::ptr::null_mut(), std::ptr::null_mut()) == 0 { continue; }
            let path = wide((*det).DevicePath.as_ptr());
            let h = open_hid(&path, 0);
            if h == INVALID_HANDLE_VALUE { continue; }
            let mut at: HIDD_ATTRIBUTES = std::mem::zeroed();
            at.Size = std::mem::size_of::<HIDD_ATTRIBUTES>() as u32;
            if HidD_GetAttributes(h, &mut at) != 0 && at.VendorID == 0x054C { out.push((path, at.VendorID, at.ProductID)); }
            CloseHandle(h);
        }
        SetupDiDestroyDeviceInfoList(set);
    }
    out
}

fn start_hid(stop: Arc<AtomicBool>, verbose: bool) -> Hid {
    let latest: Arc<Mutex<Option<(u16, Vec<u8>)>>> = Arc::new(Mutex::new(None));
    let mut handles = vec![];
    for (path, vid, pid) in hid_pads() {
        let h = open_hid(&path, GENERIC_READ);
        if h == INVALID_HANDLE_VALUE { tracing::info!("assist: found Sony HID {vid:04x}:{pid:04x} but could not open it (error {})", unsafe { GetLastError() }); continue; }
        let len = unsafe {
            let mut pp: isize = 0; let mut caps: HIDP_CAPS = std::mem::zeroed();
            if HidD_GetPreparsedData(h, &mut pp) != 0 { HidP_GetCaps(pp, &mut caps); HidD_FreePreparsedData(pp); }
            caps.InputReportByteLength as usize
        };
        if verbose { tracing::info!("assist: reading Sony HID {vid:04x}:{pid:04x}, input report {len} bytes"); }
        if len < 10 { unsafe { CloseHandle(h); } continue; }
        handles.push(h as usize);
        let (latest, stop, hh) = (latest.clone(), stop.clone(), h as usize);
        std::thread::spawn(move || {
            let mut buf = vec![0u8; len];
            while !stop.load(Ordering::Relaxed) {
                let mut got = 0u32;
                let ok = unsafe { ReadFile(hh as _, buf.as_mut_ptr(), len as u32, &mut got, std::ptr::null_mut()) };
                if ok == 0 { break; }
                *latest.lock().unwrap() = Some((pid, buf[..got as usize].to_vec()));
            }
        });
    }
    Hid { latest, handles }
}

fn stick(b: u8) -> f32 { let v = (b as f32 - 128.0) / 127.0; if v.abs() < 0.12 { 0.0 } else { v } }

/// DualShock 4 and DualSense (USB layout) input report 0x01.
fn parse_sony(pid: u16, d: &[u8]) -> Option<Pad> {
    if d.first() != Some(&1) { return None; }
    let dual_sense = matches!(pid, 0x0CE6 | 0x0DF2);
    let (b1, b2) = if dual_sense { (*d.get(8)?, *d.get(9)?) } else { (*d.get(5)?, *d.get(6)?) };
    let hat = b1 & 0x0F;
    Some(Pad { lx: stick(d[1]), ly: -stick(d[2]), ry: -stick(*d.get(4)?), a: b1 & 0x20 != 0, b: b1 & 0x40 != 0, x: b1 & 0x10 != 0, y: b1 & 0x80 != 0, start: b2 & 0x20 != 0, select: b2 & 0x10 != 0,
        up: matches!(hat, 7 | 0 | 1), down: matches!(hat, 3 | 4 | 5), left: matches!(hat, 5 | 6 | 7), right: matches!(hat, 1 | 2 | 3), lb: b2 & 0x01 != 0, rb: b2 & 0x02 != 0 })
}

/// The PlayStation / Home button on a DualShock 4 (byte 7) or DualSense (byte 10), report 0x01.
fn ps_button(pid: u16, d: &[u8]) -> bool {
    d.first() == Some(&1) && d.get(if matches!(pid, 0x0CE6 | 0x0DF2) { 10 } else { 7 }).is_some_and(|b| b & 1 != 0)
}

fn window_titled(title: &str) -> Option<HWND> {
    let mut all: Vec<(HWND, u32)> = vec![];
    unsafe { EnumWindows(Some(collect), &mut all as *mut _ as LPARAM); }
    all.into_iter().map(|(h, _)| h).find(|&h| unsafe {
        let mut buf = [0u16; 64];
        let n = GetWindowTextW(h, buf.as_mut_ptr(), 64);
        n > 0 && String::from_utf16_lossy(&buf[..n as usize]) == title
    })
}

/// Steam's overlay cannot attach to Mercury (a web-view app, not a game), so Steam's Home-button menu would open behind
/// Mercury's full-screen window. While Mercury is in front, the Home / PS button raises Steam's Big Picture window instead.
/// Runs for the life of the engine; real games keep Steam's own overlay.
pub fn start_home_watch() {
    if std::env::args().any(|a| a == "--assist") { return; } // the elevated helper must not run a second watcher
    std::thread::spawn(|| {
        let never = Arc::new(AtomicBool::new(false));
        loop {
            let hid = start_hid(never.clone(), false);
            if hid.handles.is_empty() { std::thread::sleep(Duration::from_secs(4)); continue; }
            let rescan = Instant::now() + Duration::from_secs(30);
            let mut was = false;
            while Instant::now() < rescan {
                let now = hid.latest.lock().unwrap().as_ref().is_some_and(|(pid, d)| ps_button(*pid, d));
                if now && !was {
                    unsafe {
                        let fg = GetForegroundWindow();
                        let mut pid = 0u32;
                        GetWindowThreadProcessId(fg, &mut pid);
                        let mercury_in_front = image_of(pid).is_some_and(|p| p.to_lowercase().ends_with("\\mercury.exe"));
                        if mercury_in_front {
                            if let Some(bp) = window_titled("Steam Big Picture Mode") { front(bp, false); tracing::info!("home: raised Steam over Mercury"); }
                        }
                    }
                }
                was = now;
                std::thread::sleep(Duration::from_millis(20));
            }
            // Closing the handles ends the reader threads; reconnecting a controller is picked up on the next pass.
            for h in hid.handles { unsafe { CloseHandle(h as _); } }
        }
    });
}

fn xinput_pad() -> Option<Pad> {
    let mut st: XINPUT_STATE = unsafe { std::mem::zeroed() };
    (0..4).find(|&i| unsafe { XInputGetState(i, &mut st) } == 0)?;
    let g = st.Gamepad; let b = g.wButtons;
    Some(Pad { lx: axis(g.sThumbLX), ly: axis(g.sThumbLY), ry: axis(g.sThumbRY), a: b & XINPUT_GAMEPAD_A != 0, b: b & XINPUT_GAMEPAD_B != 0, x: b & XINPUT_GAMEPAD_X != 0, y: b & XINPUT_GAMEPAD_Y != 0,
        start: b & XINPUT_GAMEPAD_START != 0, select: b & XINPUT_GAMEPAD_BACK != 0, up: b & XINPUT_GAMEPAD_DPAD_UP != 0, down: b & XINPUT_GAMEPAD_DPAD_DOWN != 0,
        left: b & XINPUT_GAMEPAD_DPAD_LEFT != 0, right: b & XINPUT_GAMEPAD_DPAD_RIGHT != 0,
        lb: b & XINPUT_GAMEPAD_LEFT_SHOULDER != 0, rb: b & XINPUT_GAMEPAD_RIGHT_SHOULDER != 0 })
}

const DEAD: f32 = 7849.0;
fn axis(v: i16) -> f32 {
    let v = v as f32;
    if v.abs() < DEAD { 0.0 } else { ((v.abs() - DEAD) / (32767.0 - DEAD)) * v.signum() }
}

fn run(mode: &Mode, stop: &Arc<AtomicBool>) {
    tracing::info!("assist: started ({})", mode.arg());
    let browser = matches!(mode, Mode::Browser);
    let hid = start_hid(stop.clone(), true);
    let mut tries = 0u32;
    let (mut prev, mut last_front, mut last_scan, mut last_log) = (Pad::default(), Instant::now() - Duration::from_secs(10), Instant::now() - Duration::from_secs(10), Instant::now() - Duration::from_secs(10));
    let mut wheel = 0f32;
    let mut close_due: Option<Instant> = None;
    let mut held: [Option<Instant>; 4] = [None; 4];
    let mut source = "";
    while !stop.load(Ordering::Relaxed) {
        if last_scan.elapsed() > Duration::from_millis(700) {
            last_scan = Instant::now();
            let wins = mode.windows();
            let fg = unsafe { GetForegroundWindow() };
            if wins.is_empty() && last_log.elapsed() > Duration::from_secs(5) { last_log = Instant::now(); tracing::info!("assist: no installer window yet"); }
            // Keep the installer on top, but not more than every few seconds so the user can still switch away.
            if let Some(&w) = wins.first() {
                if last_log.elapsed() > Duration::from_secs(5) { last_log = Instant::now(); tracing::info!("assist: {} installer window(s); foreground is installer: {}", wins.len(), wins.contains(&fg)); }
                // Retry quickly until the installer really has focus, then only now and then (so the user can switch away).
                let wait = if tries < 8 { Duration::from_millis(700) } else { Duration::from_secs(5) };
                if wins.contains(&fg) { tries = 0; }
                else if last_front.elapsed() > wait { front(w, true); last_front = Instant::now(); tries += 1; tracing::info!("assist: brought the installer to the front (try {tries})"); }
            }
        }
        let sony = hid.latest.lock().unwrap().as_ref().and_then(|(pid, d)| parse_sony(*pid, d));
        let (pad, src) = match (sony, xinput_pad()) { (Some(p), _) => (Some(p), "hid"), (None, Some(p)) => (Some(p), "xinput"), _ => (None, "") };
        if src != source { source = src; tracing::info!("assist: controller source is now {}", if src.is_empty() { "none" } else { src }); }
        if let Some(p) = pad {
            if p.lx != 0.0 || p.ly != 0.0 {
                let mut pt = POINT { x: 0, y: 0 };
                unsafe {
                    GetCursorPos(&mut pt);
                    let sp = 4.0 + 20.0 * (p.lx * p.lx + p.ly * p.ly).sqrt();
                    SetCursorPos(pt.x + (p.lx * sp) as i32, pt.y - (p.ly * sp) as i32);
                }
            }
            wheel += p.ry * 40.0;
            if wheel.abs() >= 1.0 { send(&[mouse(MOUSEEVENTF_WHEEL, wheel as i32)]); wheel = 0.0; }
            if p.a && !prev.a { send(&[mouse(MOUSEEVENTF_LEFTDOWN, 0)]); }
            if !p.a && prev.a { send(&[mouse(MOUSEEVENTF_LEFTUP, 0)]); }
            if p.x && !prev.x { tap(VK_SPACE); }
            if p.start && !prev.start { tap(VK_RETURN); }
            if browser {
                // Browser: B goes back, Y shows the on-screen keyboard, Select (Share/View) closes the browser window.
                if p.b && !prev.b { send(&[key(VK_MENU, false), key(VK_LEFT, false), key(VK_LEFT, true), key(VK_MENU, true)]); }
                if p.y && !prev.y { let _ = std::process::Command::new("osk.exe").spawn(); }
                if p.select && !prev.select {
                    // Close the browser window; if it is stuck and still there two seconds later, let go of the screen.
                    for w in mode.windows() { unsafe { PostMessageW(w, WM_CLOSE, 0, 0); } }
                    close_due = Some(Instant::now() + Duration::from_secs(2));
                }
                if let Some(t) = close_due { if Instant::now() >= t {
                    close_due = None;
                    for w in mode.windows() { unsafe { SetWindowPos(w, HWND_NOTOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE); ShowWindow(w, SW_MINIMIZE); } }
                } }
            }
            if p.rb && !prev.rb { tap(VK_TAB); }
            if p.lb && !prev.lb { send(&[key(VK_SHIFT, false), key(VK_TAB, false), key(VK_TAB, true), key(VK_SHIFT, true)]); }
            // D-pad: arrow keys, repeating while held (after a short delay, like a keyboard).
            for (down, was, vk, i) in [(p.up, prev.up, VK_UP, 0), (p.down, prev.down, VK_DOWN, 1), (p.left, prev.left, VK_LEFT, 2), (p.right, prev.right, VK_RIGHT, 3)] {
                if !down { held[i] = None; continue; }
                if !was { tap(vk); held[i] = Some(Instant::now() + Duration::from_millis(400)); }
                else if let Some(due) = held[i] { if Instant::now() >= due { tap(vk); held[i] = Some(Instant::now() + Duration::from_millis(90)); } }
            }
            prev = p;
        }
        std::thread::sleep(Duration::from_millis(16));
    }
    // Closing the handles ends the reader threads.
    for h in hid.handles { unsafe { CloseHandle(h as _); } }
    // Release the always-on-top flag so a finished installer does not stay above everything.
    for w in mode.windows() { unsafe { SetWindowPos(w, HWND_NOTOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE); } }
    tracing::info!("assist: stopped");
}
