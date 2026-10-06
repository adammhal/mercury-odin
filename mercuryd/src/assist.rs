//! Windows: while a repack's installer runs, bring its window to the front and let the controller drive it,
//! because installer windows are plain mouse-and-keyboard programs.
//!   left stick: mouse · A: click · X: space · D-pad down/up: Tab / Shift+Tab · Start: Enter (Next) · right stick: scroll
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

/// One controller reading, whichever way it was obtained.
#[derive(Default, Clone, Copy)]
struct Pad { lx: f32, ly: f32, ry: f32, a: bool, x: bool, start: bool, up: bool, down: bool }

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

fn start_hid(stop: Arc<AtomicBool>) -> Hid {
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
        tracing::info!("assist: reading Sony HID {vid:04x}:{pid:04x}, input report {len} bytes");
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
    Some(Pad { lx: stick(d[1]), ly: -stick(d[2]), ry: -stick(*d.get(4)?), a: b1 & 0x20 != 0, x: b1 & 0x10 != 0, start: b2 & 0x20 != 0,
        up: matches!(hat, 7 | 0 | 1), down: matches!(hat, 3 | 4 | 5) })
}

fn xinput_pad() -> Option<Pad> {
    let mut st: XINPUT_STATE = unsafe { std::mem::zeroed() };
    (0..4).find(|&i| unsafe { XInputGetState(i, &mut st) } == 0)?;
    let g = st.Gamepad; let b = g.wButtons;
    Some(Pad { lx: axis(g.sThumbLX), ly: axis(g.sThumbLY), ry: axis(g.sThumbRY), a: b & XINPUT_GAMEPAD_A != 0, x: b & XINPUT_GAMEPAD_X != 0,
        start: b & XINPUT_GAMEPAD_START != 0, up: b & XINPUT_GAMEPAD_DPAD_UP != 0, down: b & XINPUT_GAMEPAD_DPAD_DOWN != 0 })
}

const DEAD: f32 = 7849.0;
fn axis(v: i16) -> f32 {
    let v = v as f32;
    if v.abs() < DEAD { 0.0 } else { ((v.abs() - DEAD) / (32767.0 - DEAD)) * v.signum() }
}

fn run(setup_dir: &Path, stop: &Arc<AtomicBool>) {
    tracing::info!("assist: started for {}", setup_dir.display());
    let hid = start_hid(stop.clone());
    let (mut prev, mut last_front, mut last_scan, mut last_log) = (Pad::default(), Instant::now() - Duration::from_secs(10), Instant::now() - Duration::from_secs(10), Instant::now() - Duration::from_secs(10));
    let mut wheel = 0f32;
    let mut source = "";
    while !stop.load(Ordering::Relaxed) {
        if last_scan.elapsed() > Duration::from_millis(700) {
            last_scan = Instant::now();
            let wins = installer_windows(setup_dir);
            let fg = unsafe { GetForegroundWindow() };
            if wins.is_empty() && last_log.elapsed() > Duration::from_secs(5) { last_log = Instant::now(); tracing::info!("assist: no installer window yet"); }
            // Keep the installer on top, but not more than every few seconds so the user can still switch away.
            if let Some(&w) = wins.first() {
                if last_log.elapsed() > Duration::from_secs(5) { last_log = Instant::now(); tracing::info!("assist: {} installer window(s); foreground is installer: {}", wins.len(), wins.contains(&fg)); }
                if !wins.contains(&fg) && last_front.elapsed() > Duration::from_secs(4) { front(w); last_front = Instant::now(); tracing::info!("assist: brought the installer to the front"); }
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
            if p.down && !prev.down { tap(VK_TAB); }
            if p.up && !prev.up { send(&[key(VK_SHIFT, false), key(VK_TAB, false), key(VK_TAB, true), key(VK_SHIFT, true)]); }
            prev = p;
        }
        std::thread::sleep(Duration::from_millis(16));
    }
    // Closing the handles ends the reader threads.
    for h in hid.handles { unsafe { CloseHandle(h as _); } }
    // Release the always-on-top flag so a finished installer does not stay above everything.
    for w in installer_windows(setup_dir) { unsafe { SetWindowPos(w, HWND_NOTOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE); } }
    tracing::info!("assist: stopped");
}
