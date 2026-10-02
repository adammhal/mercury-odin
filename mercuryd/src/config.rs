use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};

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
}

impl Default for Config {
    fn default() -> Self {
        let home = home();
        Self {
            rd_key: String::new(),
            sgdb_key: String::new(),
            games_dir: home.join("Games/Mercury"),
            downloads_dir: home.join("Games/Mercury/.downloads"),
            server_url: "https://mercury-server-production.up.railway.app".into(),
            steamrip_url: "https://adammhal.github.io/mercury-db/steamrip_data.json".into(),
            enable_steamrip: true,
            proton_tool: "proton-experimental-arm64".into(),
            launch_options: "/usr/libexec/armada/armada-game-launch ~/.lsfg %command%".into(),
            browser_shortcut_id: None,
        }
    }
}

pub fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| "/var/home/armada".into())
}

pub fn browser_downloads_dir() -> PathBuf {
    home().join("Downloads")
}

pub fn data_dir() -> PathBuf {
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
        fs::set_permissions(&p, fs::Permissions::from_mode(0o600))?;
        Ok(())
    }

    /// Config as sent to the UI: the key is never echoed back, only whether it is set.
    pub fn public(&self) -> serde_json::Value {
        let mut v = serde_json::to_value(self).unwrap_or_default();
        v["rd_key"] = serde_json::Value::String(String::new());
        v["rd_key_set"] = serde_json::Value::Bool(!self.rd_key.is_empty());
        v["sgdb_key"] = serde_json::Value::String(String::new());
        v["sgdb_key_set"] = serde_json::Value::Bool(!self.sgdb_key.is_empty());
        v
    }
}
