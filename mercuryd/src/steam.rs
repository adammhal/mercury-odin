//! Steam metadata: wishlist, store search, app details, and library art. No API key needed.
use crate::config::{data_dir, home};
use anyhow::{Context, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, sync::Mutex};

const CDN: &str = "https://shared.akamai.steamstatic.com/store_item_assets/steam/apps";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct App {
    pub appid: u32,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub genres: Vec<String>,
    #[serde(default)]
    pub release: String,
    #[serde(default)]
    pub developer: String,
}

static DETAILS: Mutex<Option<HashMap<u32, App>>> = Mutex::new(None);

fn cache_path() -> std::path::PathBuf {
    data_dir().join("appdetails.json")
}

fn with_cache<T>(f: impl FnOnce(&mut HashMap<u32, App>) -> T) -> T {
    let mut g = DETAILS.lock().unwrap();
    let map = g.get_or_insert_with(|| fs::read(cache_path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default());
    f(map)
}

/// SteamID64 of the logged-in user, from the only numeric folder under userdata.
/// With several accounts, the one whose localconfig.vdf changed last is the one in use.
pub fn steam_id64() -> Option<u64> {
    let dir = if cfg!(windows) { std::path::PathBuf::from(r"C:\Program Files (x86)\Steam\userdata") } else { home().join(".local/share/Steam/userdata") };
    let mtime = |id: &u64| fs::metadata(dir.join(id.to_string()).join("config/localconfig.vdf")).and_then(|m| m.modified()).ok();
    let ids: Vec<u64> = fs::read_dir(&dir).ok()?.flatten().filter_map(|e| e.file_name().to_str()?.parse::<u64>().ok()).filter(|&id| id > 0).collect();
    ids.into_iter().max_by_key(|id| mtime(id)).map(|id| 76561197960265728 + id)
}

pub async fn details(http: &reqwest::Client, appid: u32) -> Result<App> {
    if let Some(a) = with_cache(|m| m.get(&appid).cloned()) {
        return Ok(a);
    }
    let v: serde_json::Value = http
        .get(format!("https://store.steampowered.com/api/appdetails?appids={appid}&l=english"))
        .send().await?.json().await?;
    let d = &v[appid.to_string()]["data"];
    let app = App {
        appid,
        name: d["name"].as_str().context("app not found")?.to_string(),
        description: d["short_description"].as_str().unwrap_or_default().to_string(),
        genres: d["genres"].as_array().map(|g| g.iter().filter_map(|x| x["description"].as_str().map(String::from)).take(3).collect()).unwrap_or_default(),
        release: d["release_date"]["date"].as_str().unwrap_or_default().to_string(),
        developer: d["developers"][0].as_str().unwrap_or_default().to_string(),
    };
    with_cache(|m| {
        m.insert(appid, app.clone());
        let _ = fs::create_dir_all(data_dir());
        let _ = fs::write(cache_path(), serde_json::to_vec(m).unwrap_or_default());
    });
    Ok(app)
}

pub async fn wishlist(http: &reqwest::Client) -> Result<Vec<App>> {
    let id = steam_id64().context("no Steam user found")?;
    let v: serde_json::Value = http
        .get(format!("https://api.steampowered.com/IWishlistService/GetWishlist/v1/?steamid={id}"))
        .send().await?.json().await?;
    let mut items: Vec<(u64, u32)> = v["response"]["items"].as_array().cloned().unwrap_or_default().iter()
        .filter_map(|i| Some((i["priority"].as_u64().unwrap_or(0), i["appid"].as_u64()? as u32))).collect();
    items.sort();
    // Uncached lookups are throttled by Steam, so fetch a few at a time.
    use futures_util::StreamExt;
    let out: Vec<App> = futures_util::stream::iter(items.into_iter().map(|(_, appid)| async move { details(http, appid).await.ok() }))
        .buffered(6).filter_map(|a| async move { a }).collect().await;
    Ok(out)
}

pub async fn search(http: &reqwest::Client, q: &str) -> Result<Vec<App>> {
    let v: serde_json::Value = http
        .get(format!("https://store.steampowered.com/api/storesearch/?term={}&l=english&cc=US", urlencoding::encode(q)))
        .send().await?.json().await?;
    Ok(v["items"].as_array().cloned().unwrap_or_default().iter()
        .filter(|i| i["type"].as_str() == Some("app"))
        .filter_map(|i| Some(App { appid: i["id"].as_u64()? as u32, name: i["name"].as_str()?.to_string(), description: String::new(), genres: vec![], release: String::new(), developer: String::new() }))
        .collect())
}

#[derive(Serialize)]
pub struct Art {
    /// (steam asset type, file extension, base64 data). Types: 0 portrait, 1 hero, 2 logo, 3 wide.
    pub assets: Vec<(u8, String, String)>,
}

pub async fn art(http: &reqwest::Client, appid: u32) -> Art {
    let files = [(0u8, "library_600x900.jpg"), (1, "library_hero.jpg"), (2, "logo.png"), (3, "header.jpg")];
    let mut assets = vec![];
    for (t, f) in files {
        if let Ok(r) = http.get(format!("{CDN}/{appid}/{f}")).send().await {
            if r.status().is_success() {
                if let Ok(b) = r.bytes().await {
                    let ext = f.rsplit('.').next().unwrap_or("jpg").to_string();
                    assets.push((t, ext, base64::engine::general_purpose::STANDARD.encode(&b)));
                }
            }
        }
    }
    Art { assets }
}
