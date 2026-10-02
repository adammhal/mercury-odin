//! Archive extraction with 7z (zip, 7z, tar) and unrar (rar, multi-part rar).
use anyhow::{Context, Result, bail};
use regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

/// Extraction progress in thousandths (0..=1000) across all archives of a job.
pub type Permille = AtomicU64;

/// Password OnlineFix uses for its inner archives (carried over from the old app).
const INNER_PASSWORDS: &[&str] = &["online-fix.me"];

fn is_archive(p: &Path) -> bool {
    let n = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
    [".zip", ".rar", ".7z", ".tar", ".gz", ".bz2", ".xz"].iter().any(|e| n.ends_with(e)) || Regex::new(r"\.(r\d{2}|z\d{2}|\d{3})$").unwrap().is_match(&n)
}

/// For a multi-part set only the first part is extracted; the tool finds the rest.
fn first_parts(files: &[PathBuf]) -> Vec<PathBuf> {
    let part = Regex::new(r"(?i)\.part(\d+)\.rar$").unwrap();
    let mut out = vec![];
    for f in files {
        let n = f.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if let Some(c) = part.captures(n) {
            if c[1].parse::<u32>().unwrap_or(0) == 1 { out.push(f.clone()); }
        } else if Regex::new(r"(?i)\.(r\d{2}|z\d{2})$").unwrap().is_match(n) || Regex::new(r"(?i)\.(00[2-9]|0[1-9]\d|[1-9]\d\d)$").unwrap().is_match(n) {
            continue;
        } else if is_archive(f) {
            out.push(f.clone());
        }
    }
    out
}

pub fn unrar_path() -> PathBuf {
    crate::config::home().join(".local/share/mercury/bin/unrar")
}

/// `share` = (index, count): this archive's slice of the overall bar.
async fn run(archive: &Path, dest: &Path, password: Option<&str>, progress: Option<(&Permille, usize, usize)>) -> Result<()> {
    tokio::fs::create_dir_all(dest).await?;
    let lower = archive.to_string_lossy().to_lowercase();
    let is_rar = lower.ends_with(".rar") || Regex::new(r"\.r\d{2}$").unwrap().is_match(&lower);
    let mut cmd = if is_rar {
        let mut c = Command::new(unrar_path());
        // -idcd hides the banner and "Done" but keeps the running percentage, which we read for progress.
        c.arg("x").arg("-o+").arg("-y").arg("-idcd");
        c.arg(format!("-p{}", password.unwrap_or("-")));
        c.arg(archive).arg(format!("{}/", dest.display()));
        c
    } else {
        let mut c = Command::new("7z");
        // -bsp1 sends the running percentage to stdout; -bso0 drops the file list.
        c.arg("x").arg("-y").arg("-bsp1").arg("-bso0").arg(format!("-o{}", dest.display()));
        if let Some(pw) = password { c.arg(format!("-p{pw}")); }
        c.arg(archive);
        c
    };
    cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn().with_context(|| format!("could not start extractor for {}", archive.display()))?;
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let pct = Regex::new(r"(\d{1,3})%").unwrap();
    let mut tail = String::new();
    let mut buf = [0u8; 4096];
    loop {
        let n = stdout.read(&mut buf).await?;
        if n == 0 { break; }
        let chunk = String::from_utf8_lossy(&buf[..n]);
        if let (Some((p, i, count)), Some(m)) = (progress, pct.captures_iter(&chunk).last()) {
            let v = m[1].parse::<u64>().unwrap_or(0).min(100);
            p.store(((i as u64 * 1000) + v * 10) / count.max(1) as u64, Ordering::Relaxed);
        }
        tail.push_str(&chunk);
        if tail.len() > 4000 { tail = tail[tail.len() - 2000..].to_string(); }
    }
    let mut err = String::new();
    let _ = stderr.read_to_string(&mut err).await;
    let status = child.wait().await?;
    if !status.success() {
        let src = if err.trim().is_empty() { &tail } else { &err };
        let msg = src.replace('\u{8}', "").lines().map(str::trim).filter(|l| !l.is_empty()).last().unwrap_or("").to_string();
        bail!("extracting {} failed: {msg}", archive.file_name().unwrap_or_default().to_string_lossy());
    }
    Ok(())
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() { walk(&p, out) } else { out.push(p) }
        }
    }
}

/// Release clutter that sits beside the game folder (SteamRIP adds a readme, a link and the redistributables).
fn is_clutter(p: &Path) -> bool {
    let n = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
    if p.is_dir() { return n == "_commonredist" || n == "redist" || n == "_redist"; }
    [".txt", ".url", ".nfo", ".html", ".htm", ".lnk", ".diz", ".jpg", ".png"].iter().any(|e| n.ends_with(e))
}

/// If a folder holds one directory (plus release clutter), lift that directory's contents up one level.
/// SteamRIP's `Game/Game/Game.exe` becomes `Game/Game.exe`. Two real folders (OnlineFix's game + "Fix Repair")
/// are left alone.
fn flatten(dir: &Path) -> Result<()> {
    loop {
        let items: Vec<_> = std::fs::read_dir(dir)?.flatten().map(|e| e.path()).collect();
        let dirs: Vec<&PathBuf> = items.iter().filter(|p| p.is_dir() && !is_clutter(p)).collect();
        if dirs.len() != 1 || items.iter().any(|p| !p.is_dir() && !is_clutter(p)) {
            return Ok(());
        }
        let inner = dirs[0].clone();
        let tmp = dir.join(".mercury_flatten");
        std::fs::rename(&inner, &tmp)?;
        for e in std::fs::read_dir(&tmp)?.flatten() {
            let to = dir.join(e.file_name());
            // A clutter file with the same name as a game file loses; the game's copy wins.
            if to.exists() && !to.is_dir() { std::fs::remove_file(&to)?; }
            std::fs::rename(e.path(), to)?;
        }
        std::fs::remove_dir(&tmp)?;
    }
}

/// Extract every archive found in `src` into `dest`. Files that are not archives are moved as-is.
/// Archives found inside the result (OnlineFix nests one) are extracted in place.
pub async fn extract_all(src: &Path, dest: &Path, progress: Option<&Permille>) -> Result<()> {
    let mut files = vec![];
    walk(src, &mut files);
    let archives = first_parts(&files);
    tokio::fs::create_dir_all(dest).await?;
    if archives.is_empty() {
        for f in &files {
            let rel = f.strip_prefix(src).unwrap_or(f);
            let to = dest.join(rel);
            if let Some(p) = to.parent() { tokio::fs::create_dir_all(p).await?; }
            tokio::fs::rename(f, &to).await?;
        }
    } else {
        for (i, a) in archives.iter().enumerate() {
            run(a, dest, None, progress.map(|p| (p, i, archives.len()))).await?;
        }
        if let Some(p) = progress { p.store(1000, Ordering::Relaxed); }
    }
    let mut inner = vec![];
    walk(dest, &mut inner);
    for a in first_parts(&inner) {
        let at = a.parent().unwrap_or(dest).to_path_buf();
        let mut ok = run(&a, &at, None, None).await.is_ok();
        for pw in INNER_PASSWORDS {
            if ok { break; }
            ok = run(&a, &at, Some(pw), None).await.is_ok();
        }
        if ok {
            let _ = tokio::fs::remove_file(&a).await;
        } else {
            tracing::warn!("left inner archive as-is: {}", a.display());
        }
    }
    flatten(dest)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command as Std;

    #[tokio::test]
    async fn zip_with_inner_password_archive_and_single_folder() {
        let root = std::env::temp_dir().join(format!("mercury-x-{}", std::process::id()));
        let (src, dest, stage) = (root.join("src"), root.join("dest"), root.join("stage"));
        std::fs::create_dir_all(stage.join("Game/bin")).unwrap();
        std::fs::write(stage.join("Game/bin/game.exe"), b"MZ").unwrap();
        let fix = root.join("fix");
        std::fs::create_dir_all(&fix).unwrap();
        std::fs::write(fix.join("fix.dll"), b"x").unwrap();
        assert!(Std::new("7z").args(["a", "-y", "-bso0", "-ponline-fix.me"]).arg(stage.join("Game/Fix.7z")).arg(fix.join("fix.dll")).status().unwrap().success());
        std::fs::create_dir_all(&src).unwrap();
        assert!(Std::new("7z").args(["a", "-y", "-bso0"]).arg(src.join("game.zip")).arg(stage.join("Game")).status().unwrap().success());
        let p = Permille::new(0);
        extract_all(&src, &dest, Some(&p)).await.unwrap();
        assert_eq!(p.load(Ordering::Relaxed), 1000, "progress reaches 100%");
        assert!(dest.join("bin/game.exe").exists(), "single top folder is flattened");

        // SteamRIP layout: game folder beside a readme, a .url and _CommonRedist.
        let rip = root.join("rip");
        std::fs::create_dir_all(rip.join("Game/Data")).unwrap();
        std::fs::create_dir_all(rip.join("_CommonRedist")).unwrap();
        std::fs::write(rip.join("Game/Game.exe"), b"MZ").unwrap();
        std::fs::write(rip.join("Read_Me_Instructions.txt"), b"x").unwrap();
        std::fs::write(rip.join("STEAMRIP.url"), b"x").unwrap();
        flatten(&rip).unwrap();
        assert!(rip.join("Game.exe").exists() && rip.join("Data").is_dir(), "game folder lifted beside clutter");
        // Two real folders stay as they are.
        let ofx = root.join("ofx");
        std::fs::create_dir_all(ofx.join("Game")).unwrap();
        std::fs::create_dir_all(ofx.join("Fix Repair")).unwrap();
        flatten(&ofx).unwrap();
        assert!(ofx.join("Game").is_dir() && ofx.join("Fix Repair").is_dir());
        assert!(dest.join("fix.dll").exists(), "password-protected inner archive is extracted");
        assert!(!dest.join("Fix.7z").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn multipart_first_parts() {
        let f = |v: &[&str]| first_parts(&v.iter().map(PathBuf::from).collect::<Vec<_>>()).iter().map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>();
        assert_eq!(f(&["a.part1.rar", "a.part2.rar", "a.part10.rar"]), vec!["a.part1.rar"]);
        assert_eq!(f(&["b.rar", "b.r00", "b.r01"]), vec!["b.rar"]);
        assert_eq!(f(&["c.7z.001", "c.7z.002"]), vec!["c.7z.001"]);
        assert!(f(&["readme.txt"]).is_empty());
    }
}
