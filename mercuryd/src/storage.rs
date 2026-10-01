use serde::Serialize;
use std::{ffi::CString, path::Path};

#[derive(Serialize, Default)]
pub struct Storage {
    pub total: u64,
    pub free: u64,
}

pub fn of(path: &Path) -> Storage {
    let mut p = path.to_path_buf();
    while !p.exists() {
        if !p.pop() { break; }
    }
    let Ok(c) = CString::new(p.to_string_lossy().as_bytes()) else { return Storage::default() };
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut s) } != 0 {
        return Storage::default();
    }
    Storage { total: s.f_blocks as u64 * s.f_frsize as u64, free: s.f_bavail as u64 * s.f_frsize as u64 }
}

/// Ported from the old app: repacks inflate a lot when small, little when huge.
pub fn estimate_installed(compressed: u64) -> u64 {
    let gb = compressed as f64 / (1u64 << 30) as f64;
    let ratio = if gb < 0.5 { 4.0 } else if gb < 5.0 { 4.0 - gb / 5.0 * 1.5 } else if gb < 20.0 { 2.5 - (gb - 5.0) / 15.0 } else { 1.5 - ((gb - 20.0) / 60.0).min(1.0) * 0.4 };
    (compressed as f64 * ratio) as u64
}

pub fn dir_size(p: &Path) -> u64 {
    let Ok(rd) = std::fs::read_dir(p) else { return 0 };
    rd.flatten().map(|e| match e.file_type() {
        Ok(t) if t.is_dir() => dir_size(&e.path()),
        Ok(t) if t.is_file() => e.metadata().map(|m| m.len()).unwrap_or(0),
        _ => 0,
    }).sum()
}
