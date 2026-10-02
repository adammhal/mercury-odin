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
/// GOG offline installers are `setup_<game>_<version>.exe`, often beside `setup_..-1.bin` parts.
pub fn find_setup(dir: &Path) -> Option<PathBuf> {
    let files: Vec<PathBuf> = std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect();
    let name = |p: &PathBuf| p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
    if let Some(p) = files.iter().find(|p| name(p) == "setup.exe") { return Some(p.clone()); }
    let gog: Vec<&PathBuf> = files.iter().filter(|p| { let n = name(p); n.starts_with("setup_") && n.ends_with(".exe") }).collect();
    // With several, take the one whose .bin parts are present.
    gog.iter().find(|p| {
        let stem = name(p).trim_end_matches(".exe").to_string();
        files.iter().any(|f| { let n = name(f); n.starts_with(&stem) && n.ends_with(".bin") })
    }).or(gog.first()).map(|p| (*p).clone())
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

/// Folders Wine and Proton fill in a prefix. A game is never installed under these.
pub fn is_wine_dir(p: &Path) -> bool {
    let s = p.to_string_lossy().to_lowercase();
    ["/drive_c/windows/", "/drive_c/users/", "/drive_c/programdata/", "/common files/", "/internet explorer/", "/windows media player/",
     "/windows nt/", "/windows mail/", "/windows photo viewer/", "/windows defender/", "/windowspowershell/", "/msbuild/", "/reference assemblies/",
     // Proton adds its own steam.exe helper and VR stubs to every game prefix.
     "/program files (x86)/steam/", "/program files/steam/", "/drive_c/vrclient/", "/drive_c/openxr/"]
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

/// OnlineFix hooks in through a proxy of a Windows system DLL (winmm.dll and the like) next to the game, which then
/// loads OnlineFix64.dll and the DLLs in dlllist.txt. Wine prefers its own system DLLs, so without an override the
/// fix never loads and multiplayer fails. Returns the WINEDLLOVERRIDES value, and switches the fix to English.
pub fn onlinefix_overrides(exe: &Path) -> Option<String> {
    let dir = exe.parent()?;
    let names: std::collections::HashMap<String, String> = std::fs::read_dir(dir).ok()?.flatten()
        .map(|e| { let n = e.file_name().to_string_lossy().to_string(); (n.to_lowercase(), n) }).collect();
    let fix = ["onlinefix64.dll", "onlinefix.dll"].into_iter().find(|n| names.contains_key(*n))?;
    let mut out: Vec<String> = ["winmm", "version", "winhttp", "dinput8", "dnet"].iter()
        .filter(|p| names.contains_key(&format!("{p}.dll"))).map(|p| format!("{p}=n,b")).collect();
    let mut native = vec![names[fix].trim_end_matches(".dll").trim_end_matches(".DLL").to_string()];
    if let Some(list) = names.get("dlllist.txt").and_then(|n| std::fs::read_to_string(dir.join(n)).ok()) {
        for l in list.lines().map(str::trim).filter(|l| !l.is_empty()) {
            let stem = l.strip_suffix(".dll").or_else(|| l.strip_suffix(".DLL")).unwrap_or(l).to_string();
            if !native.iter().chain(out.iter()).any(|n| n.split('=').next().unwrap().eq_ignore_ascii_case(&stem)) { native.push(stem); }
        }
    }
    out.extend(native.into_iter().map(|n| format!("{n}=n")));
    if let Some(ini) = names.get("onlinefix.ini").map(|n| dir.join(n)) {
        if let Ok(text) = std::fs::read_to_string(&ini) {
            let fixed: Vec<String> = text.lines().map(|l| if l.trim_start().to_lowercase().starts_with("language=") { "Language=english".into() } else { l.to_string() }).collect();
            let fixed = fixed.join("\n") + if text.ends_with('\n') { "\n" } else { "" };
            if fixed != text { let _ = std::fs::write(&ini, fixed); }
        }
    }
    Some(out.join(";"))
}

/// `base` (the configured launch options) with OnlineFix's DLL overrides in front when the game needs them.
pub fn launch_options(base: &str, exe: &Path) -> String {
    match onlinefix_overrides(exe) {
        Some(o) if !base.contains("WINEDLLOVERRIDES") => format!("WINEDLLOVERRIDES=\"{o}\" {base}").trim_end().to_string(),
        _ => base.to_string(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn onlinefix() {
        let d = std::env::temp_dir().join(format!("mercury-of-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        for f in ["Game.exe", "winmm.dll", "OnlineFix64.dll"] { std::fs::write(d.join(f), b"").unwrap(); }
        std::fs::write(d.join("dlllist.txt"), "SteamOverlay64.dll\nOnlineFix64.dll").unwrap();
        std::fs::write(d.join("OnlineFix.ini"), "[Main]\nLanguage=russian\nBuildId=0\n").unwrap();
        let o = super::launch_options("run %command%", &d.join("Game.exe"));
        assert_eq!(o, r#"WINEDLLOVERRIDES="winmm=n,b;OnlineFix64=n;SteamOverlay64=n" run %command%"#);
        assert_eq!(std::fs::read_to_string(d.join("OnlineFix.ini")).unwrap(), "[Main]\nLanguage=english\nBuildId=0\n");
        std::fs::write(d.join("Other.exe"), b"").unwrap();
        assert_eq!(super::launch_options(&o, &d.join("Game.exe")), o);
        std::fs::remove_dir_all(&d).unwrap();
    }

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
        assert!(is_wine_dir(Path::new("/x/pfx/drive_c/Program Files (x86)/Steam/steam.exe")));
        assert!(!is_wine_dir(Path::new("/x/pfx/drive_c/Games/Skate Story/SkateStory.exe")));
        std::fs::write(d.join("setup.exe"), b"MZ").unwrap();
        assert!(find_setup(&d).is_some());
        let g = d.join("gog");
        std::fs::create_dir_all(&g).unwrap();
        for f in ["setup_core_keeper_1.3.0.4_(64bit)_(94213).exe", "setup_core_keeper_1.3.0.4_(64bit)_(94213)-1.bin", "readme.txt"] {
            std::fs::write(g.join(f), b"MZ").unwrap();
        }
        assert!(find_setup(&g).unwrap().to_string_lossy().ends_with("(94213).exe"), "GOG installer recognised");
        std::fs::remove_dir_all(d).unwrap();
    }
}

/// FEX config for running repack installers (32-bit Inno Setup under FEX's WoW64 backend).
/// Armada's default profile sets X87ReducedPrecision=1. Delphi's Move copies 8 bytes through the x87
/// stack, so reduced precision corrupts strings: the installer opened `X:\\var\\homd\\arlada\\...`
/// instead of its own path and failed with "path not found". Everything else stays at Armada's defaults.
pub fn installer_fex_config() -> anyhow::Result<std::path::PathBuf> {
    use serde_json::{Value, json};
    let read = |p: &str| std::fs::read(p).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok());
    let mut cfg = read("/usr/share/fex-emu/Config.json").unwrap_or_else(|| json!({ "Config": {} }));
    if let Some(c) = cfg["Config"].as_object_mut() {
        // Each Proton's FEX resolves these itself; Armada's values break it (see armada-game-launch).
        for k in ["RootFS", "ThunkGuestLibs", "ThunkHostLibs"] { c.remove(k); }
        if let Some(Value::Object(d)) = read("/usr/share/armada/fex-profiles.json").map(|v| v["profiles"]["default"]["config"].clone()) {
            c.extend(d);
        }
        c.insert("X87ReducedPrecision".into(), json!("0"));
    }
    // pressure-vessel shares ~/.cache into the container, so FEX can see the file there.
    let dir = crate::config::home().join(".cache/mercury-fex");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("installer.json");
    std::fs::write(&path, serde_json::to_vec_pretty(&cfg)?)?;
    Ok(path)
}
