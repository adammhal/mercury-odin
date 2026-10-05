#![cfg_attr(not(windows), allow(dead_code))]
//! Adds installed games to warmUP's library (warmUP has no import API).
//!
//! warmUP keeps its library in SQLite at %APPDATA%\dev.warmup.console\warmup.db. A row with
//! `platform = 'manual'` shows up after warmUP's library sync and launches only when its `id` is
//! warmUP's own hash of the exe path. Verified against warmUP 1.3.5 on 2026-10-02 (spikes W1, W2).

/// warmUP's manual-game ID: `"manual-" + abs(JS string hash of the exe path)`, path with forward slashes.
pub fn manual_id(exe_path: &str) -> String {
    let mut h: i32 = 0;
    for unit in exe_path.encode_utf16() {
        h = h.wrapping_shl(5).wrapping_sub(h).wrapping_add(unit as i32);
    }
    format!("manual-{}", (h as i64).abs())
}

/// Steam's "Sep 2, 2026" style dates become warmUP's ISO dates.
pub fn iso_date(s: &str) -> Option<String> {
    let s = s.trim().replace(',', "");
    let months = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
    let parts: Vec<&str> = s.split_whitespace().collect();
    let month = |m: &str| months.iter().position(|x| m.to_lowercase().starts_with(x)).map(|i| i + 1);
    let (d, m, y) = match parts.as_slice() {
        [m, d, y] if month(m).is_some() => (d.parse::<u32>().ok()?, month(m)?, y.parse::<u32>().ok()?),
        [d, m, y] if month(m).is_some() => (d.parse::<u32>().ok()?, month(m)?, y.parse::<u32>().ok()?),
        _ => return None,
    };
    Some(format!("{y:04}-{m:02}-{d:02}"))
}

#[cfg(windows)]
mod db {
    use super::*;
    use crate::steam::App;
    use anyhow::{Context, Result, bail};
    use rusqlite::{Connection, OptionalExtension, params};
    use std::path::{Path, PathBuf};

    const CDN: &str = "https://shared.akamai.steamstatic.com/store_item_assets/steam/apps";
    /// Columns Mercury writes. If warmUP renames any, Mercury refuses rather than guessing.
    const COLUMNS: &[&str] = &["id", "title", "genre", "platform", "cover_image_url", "hero_image_url", "cover_gradient", "hero_gradient",
        "accent_color", "playtime", "is_installed", "executable_path", "description", "release_date", "install_path", "install_state",
        "source_kind", "last_scanned_at", "display_name_override"];

    pub fn db_path() -> Option<PathBuf> {
        let p = PathBuf::from(std::env::var_os("APPDATA")?).join("dev.warmup.console").join("warmup.db");
        p.exists().then_some(p)
    }

    fn open() -> Result<Connection> {
        let p = db_path().context("warmUP is not installed (no warmup.db)")?;
        let c = Connection::open(p)?;
        c.busy_timeout(std::time::Duration::from_secs(15))?;
        let have: Vec<String> = c.prepare("select name from pragma_table_info('games')")?.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;
        if let Some(missing) = COLUMNS.iter().find(|c| !have.iter().any(|h| h == *c)) {
            bail!("warmUP's library format changed (no games.{missing}); Mercury did not write to it");
        }
        Ok(c)
    }

    /// Online backup into %APPDATA%\Mercury\warmup-backups, keeping the newest five.
    fn backup(c: &Connection) -> Result<()> {
        let dir = crate::config::data_dir().join("warmup-backups");
        std::fs::create_dir_all(&dir)?;
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_millis();
        let mut dst = Connection::open(dir.join(format!("warmup-{stamp}.db")))?;
        rusqlite::backup::Backup::new(c, &mut dst)?.run_to_completion(256, std::time::Duration::from_millis(5), None)?;
        let mut old: Vec<_> = std::fs::read_dir(&dir)?.flatten().map(|e| e.path()).collect();
        old.sort();
        while old.len() > 5 { let _ = std::fs::remove_file(old.remove(0)); }
        Ok(())
    }

    fn slashes(p: &Path) -> String { p.to_string_lossy().replace('\\', "/") }

    /// Adds (or refreshes) one game. Returns warmUP's ID for it.
    pub fn add(exe: &Path, install_dir: &Path, name: &str, app: Option<&App>, appid: u32) -> Result<String> {
        let c = open()?;
        let exe_s = slashes(exe);
        let id = manual_id(&exe_s);
        let exists: Option<String> = c.query_row("select id from games where id = ?1 or lower(executable_path) = lower(?2)",
            params![id, exe_s], |r| r.get(0)).optional()?;
        backup(&c)?;
        if let Some(found) = exists {
            c.execute("update games set is_installed = 1, install_state = 'installed', install_path = ?2 where id = ?1", params![found, slashes(install_dir)])?;
            return Ok(found);
        }
        // Reuse the look of an existing manual game so the new tile matches the rest.
        let (cg, hg, ac): (String, String, String) = c.query_row(
            "select cover_gradient, hero_gradient, accent_color from games where platform = 'manual' limit 1", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap_or_else(|_| ("linear-gradient(160deg,#0b1220 0%,#14223a 100%)".into(), "linear-gradient(135deg,#070b14 0%,#14223a 100%)".into(), "#1a9fff".into()));
        let scanned: Option<i64> = c.query_row("select max(last_scanned_at) from games where platform = 'manual'", [], |r| r.get(0)).ok().flatten();
        let scanned = scanned.unwrap_or_else(|| std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0));
        let genre = app.and_then(|a| a.genres.first().cloned()).unwrap_or_else(|| "Action".into());
        let desc = app.map(|a| a.description.clone()).unwrap_or_default();
        let release = app.and_then(|a| iso_date(&a.release));
        let (cover, hero) = if appid > 0 { (Some(format!("{CDN}/{appid}/library_600x900.jpg")), Some(format!("{CDN}/{appid}/library_hero.jpg"))) } else { (None, None) };
        c.execute(
            "insert into games (id, title, genre, platform, cover_image_url, hero_image_url, cover_gradient, hero_gradient, accent_color, playtime,
                is_installed, executable_path, description, release_date, install_path, install_state, source_kind, last_scanned_at)
             values (?1, ?2, ?3, 'manual', ?4, ?5, ?6, ?7, ?8, 0, 1, ?9, ?10, ?11, ?12, 'installed', 'native', ?13)",
            params![id, name, genre, cover, hero, cg, hg, ac, exe_s, desc, release, slashes(install_dir), scanned],
        )?;
        tracing::info!("warmUP: added {name} as {id}");
        Ok(id)
    }

    pub fn remove(exe: &Path) -> Result<bool> {
        let c = open()?;
        backup(&c)?;
        let id = manual_id(&slashes(exe));
        Ok(c.execute("delete from games where id = ?1 and platform = 'manual'", params![id])? > 0)
    }
}

#[cfg(windows)]
pub use db::{add, db_path, remove};

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ids_match_warmup() {
        // IDs warmUP itself assigned on Adam's PC (read 2026-10-02).
        assert_eq!(manual_id("C:/Games/Hades II/Ship/Hades2.exe"), "manual-57919538");
        assert_eq!(manual_id("C:/Games/Moonlighter 2 - The Endless Vault/Moonlighter 2 The Endless Vault.exe"), "manual-228219794");
        assert_eq!(manual_id("C:/Users/adamm/Desktop/Everything Extra/Game Folders/2_Fights_in_2_Tight_Spaces/2FightsIn2TightSpaces.exe"), "manual-53523133");
    }
    #[test]
    fn dates() {
        assert_eq!(iso_date("Sep 2, 2026").as_deref(), Some("2026-09-02"));
        assert_eq!(iso_date("25 Sep, 2025").as_deref(), Some("2025-09-25"));
        assert_eq!(iso_date("Coming soon"), None);
    }
}
