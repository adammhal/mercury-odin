//! Time to full charge. Steam on this device reports none while charging (its Quick Access shows "?h ?m"),
//! but the kernel's battery driver estimates it.
use serde::Serialize;
use std::path::Path;

#[derive(Serialize, Default)]
pub struct Battery { pub charging: bool, pub percent: Option<i64>, pub seconds_to_full: Option<i64>, pub plugged: bool }

fn num(dir: &Path, f: &str) -> Option<i64> { std::fs::read_to_string(dir.join(f)).ok()?.trim().parse().ok() }

pub fn read() -> Battery {
    let Ok(rd) = std::fs::read_dir("/sys/class/power_supply") else { return Battery::default() };
    for d in rd.flatten().map(|e| e.path()) {
        if std::fs::read_to_string(d.join("type")).map(|t| t.trim() != "Battery").unwrap_or(true) { continue; }
        let charging = std::fs::read_to_string(d.join("status")).map(|s| s.trim() == "Charging").unwrap_or(false);
        // The driver's own average first; else charge still missing over the charge current (µAh / µA = hours).
        let secs = num(&d, "time_to_full_avg").filter(|s| *s > 0).or_else(|| {
            let (full, now, cur) = (num(&d, "charge_full")?, num(&d, "charge_now")?, num(&d, "current_now")?);
            (cur > 0 && full > now).then(|| (full - now) * 3600 / cur)
        });
        return Battery { charging, percent: num(&d, "capacity"), seconds_to_full: if charging { secs } else { None }, plugged: plugged() };
    }
    Battery::default()
}

/// On a charger, or on the dock's external display (which also counts when the dock does not charge).
fn plugged() -> bool {
    let online = |p: &str| std::fs::read_to_string(p).map(|s| s.trim() == "1").unwrap_or(false);
    let charger = std::fs::read_dir("/sys/class/power_supply").into_iter().flatten().flatten().any(|e| {
        let d = e.path();
        // "ucsi-source-…" is online when the Odin powers an accessory, not when it charges.
        !e.file_name().to_string_lossy().contains("source") && std::fs::read_to_string(d.join("type")).map(|t| matches!(t.trim(), "Mains" | "USB" | "Wireless")).unwrap_or(false) && online(&d.join("online").to_string_lossy())
    });
    let display = std::fs::read_dir("/sys/class/drm").into_iter().flatten().flatten().any(|e| {
        e.file_name().to_string_lossy().contains("-DP-") && std::fs::read_to_string(e.path().join("status")).map(|s| s.trim() == "connected").unwrap_or(false)
    });
    charger || display
}
