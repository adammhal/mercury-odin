use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub rd_key: String,
    /// SteamGridDB API key (steamgriddb.com/profile/preferences/api). Optional: fills art Steam's CDN lacks.
    pub sgdb_key: String,
    pub games_dir: PathBuf,
    pub downloads_dir: PathBuf,
    pub server_url: String,
    pub steamrip_url: String,
    pub enable_steamrip: bool,
    pub proton_tool: String,
    pub launch_options: String,
    /// Steam shortcut that runs Firefox for hosts Real-Debrid cannot download from.
    pub browser_shortcut_id: Option<u32>,
    /// How many jobs may download or extract at the same time.
    pub parallel_jobs: usize,
    /// Windows: where finished games go, "steam" (Big Picture) or "warmup".
    pub launcher: String,
    /// Windows: the Steam shortcut for Mercury itself.
    pub steam_self_id: Option<u32>,
    /// Ask to confirm the title and artwork before a finished game goes to Steam.
    pub review_art: bool,
}

impl Default for Config {
    fn default() -> Self {
        let home = home();
        // On Windows games go where Adam already keeps them; on the Odin under ~/Games/Mercury.
        let games_dir = if cfg!(windows) { PathBuf::from(r"C:\Games") } else { home.join("Games/Mercury") };
        Self {
            rd_key: String::new(),
            sgdb_key: String::new(),
            downloads_dir: games_dir.join(if cfg!(windows) { ".mercury-downloads" } else { ".downloads" }),
            games_dir,
            server_url: "https://mercury-server-production.up.railway.app".into(),
            steamrip_url: "https://adammhal.github.io/mercury-db/steamrip_data.json".into(),
            enable_steamrip: true,
            proton_tool: "proton-experimental-arm64".into(),
            launch_options: "/usr/libexec/armada/armada-game-launch ~/.lsfg %command%".into(),
            browser_shortcut_id: None,
            parallel_jobs: 2,
            launcher: "steam".into(),
            steam_self_id: None,
            review_art: true,
        }
    }
}

pub fn home() -> PathBuf {
    #[cfg(windows)]
    if let Some(p) = std::env::var_os("USERPROFILE") { return PathBuf::from(p); }
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| "/var/home/armada".into())
}

pub fn browser_downloads_dir() -> PathBuf {
    home().join("Downloads")
}

pub fn data_dir() -> PathBuf {
    #[cfg(windows)]
    if let Some(p) = std::env::var_os("APPDATA") { return PathBuf::from(p).join("Mercury"); }
    home().join(".local/share/mercury")
}

fn path() -> PathBuf {
    data_dir().join("config.json")
}

impl Config {
    pub fn load() -> Self {
        fs::read(path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn save(&self) -> Result<()> {
        fs::create_dir_all(data_dir())?;
        let p = path();
        fs::write(&p, serde_json::to_vec_pretty(self)?)?;
        // %APPDATA% is already private to the user on Windows.
        #[cfg(unix)]
        { use std::os::unix::fs::PermissionsExt; fs::set_permissions(&p, fs::Permissions::from_mode(0o600))?; }
        Ok(())
    }

    /// Config as sent to the UI: the key is never echoed back, only whether it is set.
    pub fn public(&self) -> serde_json::Value {
        let mut v = serde_json::to_value(self).unwrap_or_default();
        v["rd_key"] = serde_json::Value::String(String::new());
        v["sgdb_key"] = serde_json::Value::String(String::new());
        v["sgdb_key_set"] = serde_json::Value::Bool(!self.sgdb_key.is_empty());
        v["rd_key_set"] = serde_json::Value::Bool(!self.rd_key.is_empty());
        v["sgdb_key"] = serde_json::Value::String(String::new());
        v["sgdb_key_set"] = serde_json::Value::Bool(!self.sgdb_key.is_empty());
        v
    }
}
