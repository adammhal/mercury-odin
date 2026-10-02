//! After extraction: is this a repack installer, and which .exe is the game?
use regex::Regex;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Serialize, Clone, Debug)]
pub struct Candidate {
    pub path: PathBuf,
    pub score: i64,
}

const SKIP: &str = r"(?i)(unins|setup|install|redist|vc_?redist|dxsetup|directx|dotnet|crash|report|ue4prereq|prereq|vcruntime|physx|easyanticheat|eac|battleye|uplay|launcherhelper|unitycrashhandler|quicksfv|notification_helper|cefprocess|helper)";

fn walk(dir: &Path, depth: usize, out: &mut Vec<(PathBuf, usize)>) {
    if depth > 5 { return; }
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            let n = e.file_name().to_string_lossy().to_lowercase();
            if p.is_dir() {
                if n.starts_with("_commonredist") || n == "redist" || n == "directx" || n == "__installer" { continue; }
                walk(&p, depth + 1, out);
            } else if n.ends_with(".exe") {
                out.push((p, depth));
            }
        }
    }
}

/// A repack (FitGirl, DODI, ...) ships `setup.exe` at the top with data files, not a playable game.
pub fn find_setup(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| {
        p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.eq_ignore_ascii_case("setup.exe"))
    })
}

fn words(s: &str) -> Vec<String> {
    s.to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect::<String>().split_whitespace().filter(|w| w.len() > 2).map(String::from).collect()
}

/// Wine fills every prefix with stand-in programs (wmplayer.exe, iexplore.exe, notepad.exe, ...).
/// They carry this marker, so they can never be mistaken for a game.
pub fn is_wine_stub(p: &Path) -> bool {
    let Ok(b) = std::fs::read(p) else { return true };
    let hay = &b[..b.len().min(4096)];
    hay.windows(16).any(|w| w == b"Wine placeholder") || hay.windows(12).any(|w| w == b"Wine builtin")
}

/// Folders Wine creates in a fresh prefix. A game is never installed under these.
pub fn is_wine_dir(p: &Path) -> bool {
    let s = p.to_string_lossy().to_lowercase();
    ["/drive_c/windows/", "/drive_c/users/", "/drive_c/programdata/", "/common files/", "/internet explorer/", "/windows media player/",
     "/windows nt/", "/windows mail/", "/windows photo viewer/", "/windows defender/", "/windowspowershell/", "/msbuild/", "/reference assemblies/"]
        .iter().any(|d| s.contains(d))
}

/// Rank .exe files: name close to the game title, shallow, large, and not a helper.
pub fn find_game_exe(dir: &Path, title: &str) -> Vec<Candidate> {
    let skip = Regex::new(SKIP).unwrap();
    let tw = words(title);
    let mut found = vec![];
    walk(dir, 0, &mut found);
    let mut out: Vec<Candidate> = found.into_iter().filter_map(|(p, depth)| {
        let stem = p.file_stem()?.to_string_lossy().to_string();
        let mut score = 0i64;
        if skip.is_match(&stem) { score -= 1000; }
        let sw = words(&stem);
        let joined = stem.to_lowercase().replace([' ', '_', '-', '.'], "");
        score += tw.iter().filter(|w| sw.contains(w) || joined.contains(w.as_str())).count() as i64 * 50;
        if stem.to_lowercase().contains("launcher") { score -= 30; }
        if stem.ends_with("-Win64-Shipping") || stem.ends_with("-Win64-Test") { score += 40; }
        score -= depth as i64 * 10;
        let mb = std::fs::metadata(&p).map(|m| m.len() >> 20).unwrap_or(0) as i64;
        score += mb.min(200) / 4;
        Some(Candidate { path: p, score })
    }).collect();
    out.sort_by(|a, b| b.score.cmp(&a.score));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn picks_the_game_not_helpers() {
        let d = std::env::temp_dir().join(format!("mercury-i-{}", std::process::id()));
        for f in ["Silksong.exe", "UnityCrashHandler64.exe", "_CommonRedist/vcredist_x64.exe", "unins000.exe"] {
            let p = d.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, b"MZ").unwrap();
        }
        let c = find_game_exe(&d, "Hollow Knight: Silksong");
        assert_eq!(c[0].path.file_name().unwrap(), "Silksong.exe");
        assert!(c.iter().all(|x| !x.path.to_string_lossy().contains("vcredist")));
        std::fs::write(d.join("setup.exe"), b"MZ").unwrap();
        assert!(find_setup(&d).is_some());
        std::fs::remove_dir_all(d).unwrap();
    }
}
