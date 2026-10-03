//! Time to full charge. Steam on this device reports none while charging (its Quick Access shows "?h ?m"),
//! but the kernel's battery driver estimates it.
use serde::Serialize;
use std::path::Path;

#[derive(Serialize, Default)]
pub struct Battery { pub charging: bool, pub seconds_to_full: Option<i64> }

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
        return Battery { charging, seconds_to_full: if charging { secs } else { None } };
    }
    Battery::default()
}
