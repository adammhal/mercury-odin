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
    /// Portrait cover URL. Newer games keep art under hashed paths, so the plain CDN path is often a 404.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<String>,
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
pub fn steam_id64() -> Option<u64> {
    let dir = home().join(".local/share/Steam/userdata");
    fs::read_dir(dir).ok()?.flatten().filter_map(|e| e.file_name().to_str()?.parse::<u64>().ok()).find(|&id| id > 0).map(|id| 76561197960265728 + id)
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
        cover: None,
    };
    with_cache(|m| {
        m.insert(appid, app.clone());
        let _ = fs::create_dir_all(data_dir());
        let _ = fs::write(cache_path(), serde_json::to_vec(m).unwrap_or_default());
    });
    Ok(app)
}

static COVERS: Mutex<Option<HashMap<u32, Option<String>>>> = Mutex::new(None);

/// Fill `cover` with each game's real portrait (or header if it has none) from the store's asset list.
pub async fn with_covers(http: &reqwest::Client, apps: &mut [App]) {
    let missing: Vec<u32> = { let g = COVERS.lock().unwrap(); apps.iter().map(|a| a.appid).filter(|id| !g.as_ref().is_some_and(|m| m.contains_key(id))).collect() };
    for chunk in missing.chunks(50) {
        let ids: Vec<serde_json::Value> = chunk.iter().map(|id| serde_json::json!({ "appid": id })).collect();
        let input = serde_json::json!({ "ids": ids, "context": { "language": "english", "country_code": "US" }, "data_request": { "include_assets": true } });
        let Ok(r) = http.get("https://api.steampowered.com/IStoreBrowseService/GetItems/v1/").query(&[("input_json", input.to_string())]).send().await else { continue };
        let Ok(v) = r.json::<serde_json::Value>().await else { continue };
        let mut g = COVERS.lock().unwrap();
        let m = g.get_or_insert_with(HashMap::new);
        for it in v["response"]["store_items"].as_array().cloned().unwrap_or_default() {
            let Some(id) = it["appid"].as_u64() else { continue };
            let a = &it["assets"];
            let url = a["asset_url_format"].as_str().and_then(|f| {
                let file = a["library_capsule"].as_str().or(a["header"].as_str())?;
                Some(format!("https://shared.akamai.steamstatic.com/store_item_assets/{}", f.replace("${FILENAME}", file)))
            });
            m.insert(id as u32, url);
        }
    }
    let g = COVERS.lock().unwrap();
    if let Some(m) = g.as_ref() { for a in apps.iter_mut() { a.cover = m.get(&a.appid).cloned().flatten(); } }
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
    let mut out = out;
    with_covers(http, &mut out).await;
    Ok(out)
}

pub async fn search(http: &reqwest::Client, q: &str) -> Result<Vec<App>> {
    let v: serde_json::Value = http
        .get(format!("https://store.steampowered.com/api/storesearch/?term={}&l=english&cc=US", urlencoding::encode(q)))
        .send().await?.json().await?;
    let mut out: Vec<App> = v["items"].as_array().cloned().unwrap_or_default().iter()
        .filter(|i| i["type"].as_str() == Some("app"))
        .filter_map(|i| Some(App { appid: i["id"].as_u64()? as u32, name: i["name"].as_str()?.to_string(), description: String::new(), genres: vec![], release: String::new(), developer: String::new(), cover: None }))
        .collect();
    with_covers(http, &mut out).await;
    Ok(out)
}

#[derive(Serialize)]
pub struct Art {
    /// (steam asset type, file extension, base64 data). Types: 0 portrait, 1 hero, 2 logo, 3 wide.
    pub assets: Vec<(u8, String, String)>,
    /// Icon as a file Steam can point the shortcut at (Steam has no artwork slot for shortcut icons).
    pub icon: Option<String>,
}

async fn fetch(http: &reqwest::Client, url: &str) -> Option<Vec<u8>> {
    let r = http.get(url).send().await.ok()?;
    if !r.status().is_success() { return None; }
    r.bytes().await.ok().map(|b| b.to_vec()).filter(|b| !b.is_empty())
}

/// First SteamGridDB image URL of `kind` (grids, heroes, logos, icons) for a Steam app.
async fn sgdb(http: &reqwest::Client, key: &str, appid: u32, kind: &str, query: &str) -> Option<String> {
    let v: serde_json::Value = http.get(format!("https://www.steamgriddb.com/api/v2/{kind}/steam/{appid}{query}"))
        .bearer_auth(key).send().await.ok()?.json().await.ok()?;
    v["data"].as_array()?.first()?["url"].as_str().map(String::from)
}

/// Steam CDN art first; SteamGridDB (with a key) for anything missing, and for the icon.
pub async fn art(http: &reqwest::Client, appid: u32, sgdb_key: &str) -> Art {
    let files = [(0u8, "library_600x900.jpg"), (1, "library_hero.jpg"), (2, "logo.png"), (3, "header.jpg")];
    let fallback = [(0u8, "grids", "?dimensions=600x900"), (1, "heroes", ""), (2, "logos", ""), (3, "grids", "?dimensions=920x430,460x215")];
    let mut assets = vec![];
    for ((t, f), (_, kind, q)) in files.iter().zip(fallback.iter()) {
        let mut got = fetch(http, &format!("{CDN}/{appid}/{f}")).await.map(|b| (f.rsplit('.').next().unwrap_or("jpg").to_string(), b));
        if got.is_none() && !sgdb_key.is_empty() {
            if let Some(url) = sgdb(http, sgdb_key, appid, kind, q).await {
                let ext = url.rsplit('.').next().unwrap_or("png").to_lowercase();
                got = fetch(http, &url).await.map(|b| (ext, b));
            }
        }
        if let Some((ext, b)) = got {
            assets.push((*t, ext, base64::engine::general_purpose::STANDARD.encode(&b)));
        }
    }
    let mut icon = None;
    if !sgdb_key.is_empty() {
        if let Some(url) = sgdb(http, sgdb_key, appid, "icons", "").await {
            if let Some(b) = fetch(http, &url).await {
                let ext = url.rsplit('.').next().unwrap_or("png").to_lowercase();
                let dir = data_dir().join("icons");
                let path = dir.join(format!("{appid}.{ext}"));
                if std::fs::create_dir_all(&dir).is_ok() && std::fs::write(&path, b).is_ok() {
                    icon = Some(path.to_string_lossy().to_string());
                }
            }
        }
    }
    Art { assets, icon }
}
