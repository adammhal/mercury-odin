//! Files the user downloads in Firefox, for hosts Real-Debrid cannot fetch.
//! Firefox writes `<name>.<random>.part` beside a download while it runs, so a file is finished when no
//! `.part` file shares its stem.
use regex::Regex;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Serialize, Clone, Debug)]
pub struct LocalFile {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub modified: u64,
    pub finished: bool,
    pub archive: bool,
}

fn mtime(p: &Path) -> u64 {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0)
}

/// Name without any multi-part suffix: "Game.part02.rar" -> "Game", "Game.7z.003" -> "Game.7z", "Game.r01" -> "Game".
pub fn part_stem(name: &str) -> String {
    let re = Regex::new(r"(?i)(\.part\d+\.rar|\.r\d{2}|\.rar|\.\d{3}|\.z\d{2}|\.zip|\.7z)$").unwrap();
    let mut s = name.to_string();
    while let Some(m) = re.find(&s) { s.truncate(m.start()); }
    s
}

pub fn list(dir: &Path, since: u64) -> Vec<LocalFile> {
    let Ok(rd) = std::fs::read_dir(dir) else { return vec![] };
    let entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect();
    let partials: Vec<String> = entries.iter().filter_map(|p| p.file_name()?.to_str().map(String::from))
        .filter(|n| n.ends_with(".part") || n.ends_with(".crdownload")).collect();
    let archive_re = Regex::new(r"(?i)\.(zip|rar|7z|r\d{2}|\d{3})$").unwrap();
    let mut out: Vec<LocalFile> = entries.iter().filter_map(|p| {
        let name = p.file_name()?.to_str()?.to_string();
        if name.ends_with(".part") || name.ends_with(".crdownload") || name.starts_with('.') { return None; }
        let modified = mtime(p);
        if modified < since { return None; }
        let stem = part_stem(&name);
        // Finished only when no Firefox partial for this file, or any part of the same set, remains.
        let busy = partials.iter().any(|pn| pn.starts_with(&format!("{}.", name.rsplit_once('.').map(|x| x.0).unwrap_or(&name))) || part_stem(pn.trim_end_matches(".part")).starts_with(&format!("{stem}.")));
        Some(LocalFile { size: std::fs::metadata(p).map(|m| m.len()).unwrap_or(0), archive: archive_re.is_match(&name), finished: !busy, path: p.clone(), name, modified })
    }).collect();
    out.sort_by(|a, b| b.modified.cmp(&a.modified));
    out
}

/// The chosen file plus the other parts of the same multi-part set, if any.
pub fn with_siblings(file: &Path) -> Vec<PathBuf> {
    let Some(dir) = file.parent() else { return vec![file.to_path_buf()] };
    let Some(name) = file.file_name().and_then(|n| n.to_str()) else { return vec![file.to_path_buf()] };
    let stem = part_stem(name);
    let multi = Regex::new(r"(?i)(\.part\d+\.rar|\.r\d{2}|\.\d{3}|\.z\d{2})$").unwrap();
    let mut out = vec![file.to_path_buf()];
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            let Some(n) = p.file_name().and_then(|n| n.to_str()) else { continue };
            if p != file && part_stem(n) == stem && (multi.is_match(n) || multi.is_match(name)) && !n.ends_with(".part") {
                out.push(p);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stems() {
        assert_eq!(part_stem("Game.part02.rar"), "Game");
        assert_eq!(part_stem("Game.r01"), "Game");
        assert_eq!(part_stem("Game.7z.003"), "Game");
        assert_eq!(part_stem("TUNIC-SteamRIP.com.rar"), "TUNIC-SteamRIP.com");
    }
    #[test]
    fn finished_and_siblings() {
        let d = std::env::temp_dir().join(format!("mercury-b-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        for f in ["G.part1.rar", "G.part2.rar", "Other.zip", "Game2.zip"] { std::fs::write(d.join(f), b"x").unwrap(); }
        std::fs::write(d.join("Game2.x7Yq.zip.part"), b"x").unwrap();
        std::fs::write(d.join("G.part3.rar.aB12.part"), b"x").unwrap();
        let l = list(&d, 0);
        assert!(!l.iter().find(|f| f.name == "G.part1.rar").unwrap().finished, "a part still downloading");
        assert!(l.iter().find(|f| f.name == "Other.zip").unwrap().finished);
        assert!(!l.iter().find(|f| f.name == "Game2.zip").unwrap().finished, "Firefox placeholder while .part exists");
        std::fs::remove_file(d.join("Game2.x7Yq.zip.part")).unwrap();
        std::fs::remove_file(d.join("G.part3.rar.aB12.part")).unwrap();
        std::fs::write(d.join("G.part3.rar"), b"x").unwrap();
        assert!(list(&d, 0).iter().all(|f| f.finished));
        let mut s = with_siblings(&d.join("G.part1.rar"));
        s.sort();
        assert_eq!(s.len(), 3);
        assert_eq!(with_siblings(&d.join("Other.zip")).len(), 1);
        std::fs::remove_dir_all(d).unwrap();
    }
}
