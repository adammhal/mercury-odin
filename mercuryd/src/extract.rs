//! Archive extraction with 7z (zip, 7z, tar) and unrar (rar, multi-part rar).
use anyhow::{Context, Result, bail};
use regex::Regex;
use std::path::{Path, PathBuf};
use tokio::process::Command;

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

async fn run(archive: &Path, dest: &Path, password: Option<&str>) -> Result<()> {
    tokio::fs::create_dir_all(dest).await?;
    let lower = archive.to_string_lossy().to_lowercase();
    let is_rar = lower.ends_with(".rar") || Regex::new(r"\.r\d{2}$").unwrap().is_match(&lower);
    let mut cmd = if is_rar {
        let mut c = Command::new(unrar_path());
        c.arg("x").arg("-o+").arg("-y").arg("-idq");
        c.arg(format!("-p{}", password.unwrap_or("-")));
        c.arg(archive).arg(format!("{}/", dest.display()));
        c
    } else {
        let mut c = Command::new("7z");
        c.arg("x").arg("-y").arg(format!("-o{}", dest.display()));
        if let Some(pw) = password { c.arg(format!("-p{pw}")); }
        c.arg(archive);
        c
    };
    cmd.stdin(std::process::Stdio::null());
    let out = cmd.output().await.with_context(|| format!("could not start extractor for {}", archive.display()))?;
    if !out.status.success() {
        let msg = String::from_utf8_lossy(if out.stderr.is_empty() { &out.stdout } else { &out.stderr }).lines().last().unwrap_or("").to_string();
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

/// If a folder holds exactly one directory and nothing else, lift its contents up one level.
fn flatten(dir: &Path) -> Result<()> {
    loop {
        let items: Vec<_> = std::fs::read_dir(dir)?.flatten().collect();
        if items.len() != 1 || !items[0].path().is_dir() {
            return Ok(());
        }
        let inner = items[0].path();
        let tmp = dir.join(".mercury_flatten");
        std::fs::rename(&inner, &tmp)?;
        for e in std::fs::read_dir(&tmp)?.flatten() {
            std::fs::rename(e.path(), dir.join(e.file_name()))?;
        }
        std::fs::remove_dir(&tmp)?;
    }
}

/// Extract every archive found in `src` into `dest`. Files that are not archives are moved as-is.
/// Archives found inside the result (OnlineFix nests one) are extracted in place.
pub async fn extract_all(src: &Path, dest: &Path) -> Result<()> {
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
        for a in &archives {
            run(a, dest, None).await?;
        }
    }
    let mut inner = vec![];
    walk(dest, &mut inner);
    for a in first_parts(&inner) {
        let at = a.parent().unwrap_or(dest).to_path_buf();
        let mut ok = run(&a, &at, None).await.is_ok();
        for pw in INNER_PASSWORDS {
            if ok { break; }
            ok = run(&a, &at, Some(pw)).await.is_ok();
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
        extract_all(&src, &dest).await.unwrap();
        assert!(dest.join("bin/game.exe").exists(), "single top folder is flattened");
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
