//! Games installed or copied elsewhere: a repack installed on a PC and carried over on the microSD card,
//! or copied over SSH into the drop folder. Mercury adds them to its library and to Steam like any install.
use crate::{config::home, install};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Serialize, Clone, Debug)]
pub struct Candidate {
    pub path: PathBuf,
    pub name: String,
    /// "microSD" | "Import folder" | "Downloads"
    pub location: String,
    /// "folder" | "archive"
    pub kind: String,
    /// Best guess at the game's .exe (folders only), or the installer if it is one.
    pub exe: Option<String>,
    pub installer: bool,
    pub size: u64,
}

pub fn drop_folder() -> PathBuf {
    home().join("Games/Import")
}

/// Every place Mercury looks, with a label. microSD cards are whatever udisks mounted under /run/media.
pub fn roots() -> Vec<(String, PathBuf)> {
    let mut out = vec![("Import folder".to_string(), drop_folder()), ("Downloads".to_string(), home().join("Downloads"))];
    let user = std::env::var("USER").unwrap_or_else(|_| "armada".into());
    for base in [PathBuf::from("/run/media").join(&user), PathBuf::from("/run/media")] {
        if let Ok(rd) = std::fs::read_dir(&base) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() && p != PathBuf::from("/run/media").join(&user) && !out.iter().any(|(_, r)| r == &p) {
                    out.push(("microSD".to_string(), p));
                }
            }
        }
    }
    out
}

/// True when `p` is inside one of the import roots (only those may be imported or moved).
pub fn allowed(p: &Path) -> bool {
    let Ok(p) = p.canonicalize() else { return false };
    roots().iter().any(|(_, r)| r.canonicalize().is_ok_and(|r| p.starts_with(&r) && p != r))
}

fn is_archive(name: &str) -> bool {
    let n = name.to_lowercase();
    [".zip", ".rar", ".7z"].iter().any(|e| n.ends_with(e)) && !regex::Regex::new(r"(?i)\.part(0*[2-9]|\d{2,})\.rar$").unwrap().is_match(&n)
}

pub fn candidates() -> Vec<Candidate> {
    let mut out = vec![];
    for (label, root) in roots() {
        let Ok(rd) = std::fs::read_dir(&root) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || name.ends_with(".part") { continue; }
            if p.is_dir() {
                let setup = install::find_setup(&p);
                let exe = if setup.is_some() { setup.clone() } else {
                    install::find_game_exe(&p, &name).into_iter().find(|c| c.score > -500).map(|c| c.path)
                };
                let Some(exe) = exe else { continue };
                out.push(Candidate { name, location: label.clone(), kind: "folder".into(), installer: setup.is_some(),
                    exe: exe.strip_prefix(&p).ok().map(|r| r.to_string_lossy().to_string()), size: 0, path: p });
            } else if is_archive(&name) {
                out.push(Candidate { size: e.metadata().map(|m| m.len()).unwrap_or(0), name, location: label.clone(),
                    kind: "archive".into(), exe: None, installer: false, path: p });
            }
        }
    }
    out
}

/// Move a folder, falling back to copy-then-delete when it crosses filesystems (microSD to internal).
pub fn move_dir(from: &Path, to: &Path) -> anyhow::Result<()> {
    if let Some(parent) = to.parent() { std::fs::create_dir_all(parent)?; }
    if std::fs::rename(from, to).is_ok() { return Ok(()); }
    copy_dir(from, to)?;
    std::fs::remove_dir_all(from)?;
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)?.flatten() {
        let (s, d) = (e.path(), to.join(e.file_name()));
        if s.is_dir() { copy_dir(&s, &d)?; } else { std::fs::copy(&s, &d)?; }
    }
    Ok(())
}

#[derive(Serialize, Clone, Debug)]
pub struct Card {
    pub device: String,
    pub fstype: String,
    pub label: String,
    pub size: u64,
}

/// microSD partitions with a filesystem that nothing has mounted. Armada auto-mounts only ext4, so a card
/// formatted on Windows (exFAT, NTFS) shows up here instead.
pub fn unmounted_cards() -> Vec<Card> {
    let Ok(out) = std::process::Command::new("lsblk").args(["-J", "-b", "-o", "NAME,FSTYPE,LABEL,SIZE,MOUNTPOINTS"]).output() else { return vec![] };
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(&out.stdout) else { return vec![] };
    let mut cards = vec![];
    let mut walk = vec![v["blockdevices"].clone()];
    while let Some(serde_json::Value::Array(list)) = walk.pop() {
        for d in list {
            let name = d["name"].as_str().unwrap_or("");
            let mounted = d["mountpoints"].as_array().is_some_and(|m| m.iter().any(|x| x.is_string()));
            if name.starts_with("mmcblk") && name.contains('p') && !mounted {
                if let Some(fs) = d["fstype"].as_str().filter(|f| !f.is_empty()) {
                    cards.push(Card { device: format!("/dev/{name}"), fstype: fs.into(), label: d["label"].as_str().unwrap_or("").into(), size: d["size"].as_u64().unwrap_or(0) });
                }
            }
            walk.push(d["children"].clone());
        }
    }
    cards
}

/// Mount a card through udisks as the current user, the same mechanism Armada's automount uses.
pub fn mount(device: &str) -> anyhow::Result<String> {
    if !unmounted_cards().iter().any(|c| c.device == device) { anyhow::bail!("{device} is not an unmounted microSD partition"); }
    let out = std::process::Command::new("udisksctl").args(["mount", "-b", device, "--no-user-interaction"]).output()?;
    let text = String::from_utf8_lossy(if out.status.success() { &out.stdout } else { &out.stderr }).trim().to_string();
    if !out.status.success() { anyhow::bail!("Could not mount {device}: {text}"); }
    Ok(text)
}
