//! SteamGridDB: alternative artwork for a game, looked up by its Steam app id (needs a free API key).
use anyhow::{Result, anyhow, bail};
use base64::Engine;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

const API: &str = "https://www.steamgriddb.com/api/v2";

#[derive(Serialize)]
pub struct Opt { pub url: String, pub thumb: String, pub score: i64, pub width: u64, pub height: u64, pub author: String }

/// Art slots, numbered as Steam's SetCustomArtworkForApp does: 0 cover, 1 hero, 2 logo, 3 wide capsule.
fn endpoint(slot: u8) -> Result<(&'static str, &'static str)> {
    Ok(match slot {
        0 => ("grids", "&dimensions=600x900,342x482,660x930"),
        1 => ("heroes", ""),
        2 => ("logos", ""),
        3 => ("grids", "&dimensions=920x430,460x215"),
        _ => bail!("unknown art slot"),
    })
}

async fn get(http: &reqwest::Client, key: &str, path: &str) -> Result<Value> {
    if key.is_empty() { bail!("Add your SteamGridDB key in Mercury settings."); }
    let r = http.get(format!("{API}{path}")).bearer_auth(key).timeout(std::time::Duration::from_secs(20)).send().await?;
    if r.status().as_u16() == 401 { bail!("SteamGridDB rejected the key."); }
    if !r.status().is_success() { bail!("SteamGridDB answered {}", r.status()); }
    Ok(r.json().await?)
}

/// SteamGridDB's id for a game: by Steam app id, or (when it does not know that id) by searching its name.
async fn game_id(http: &reqwest::Client, key: &str, appid: u32, name: Option<&str>, by_name: bool) -> Result<(u64, String)> {
    if !by_name {
        if let Ok(g) = get(http, key, &format!("/games/steam/{appid}")).await {
            if let Some(id) = g["data"]["id"].as_u64() { return Ok((id, g["data"]["name"].as_str().unwrap_or_default().to_string())); }
        }
    }
    let name = name.map(str::trim).filter(|n| !n.is_empty()).ok_or_else(|| anyhow!("SteamGridDB does not know this game"))?;
    let hits = get(http, key, &format!("/search/autocomplete/{}", urlencoding::encode(name))).await?;
    let norm = |s: &str| s.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase();
    let list = hits["data"].as_array().cloned().unwrap_or_default();
    let exact = list.iter().find(|h| h["name"].as_str().is_some_and(|n| norm(n) == norm(name)));
    // An automatic lookup only accepts a real match (the same name, or one containing the other); a search the user typed
    // takes SteamGridDB's best guess, and the picker shows which game it found.
    let close = list.iter().find(|h| h["name"].as_str().is_some_and(|n| { let (a, b) = (norm(n), norm(name)); !a.is_empty() && (a.contains(&b) || b.contains(&a)) }));
    exact.or(close).or(if by_name { list.first() } else { None })
        .and_then(|h| Some((h["id"].as_u64()?, h["name"].as_str().unwrap_or_default().to_string()))).ok_or_else(|| anyhow!("SteamGridDB has nothing for \"{name}\""))
}

/// `by_name` searches SteamGridDB for `name` instead of using the Steam app id. Returns the images and the game they are for.
pub async fn options(http: &reqwest::Client, key: &str, appid: u32, slot: u8, name: Option<&str>, by_name: bool) -> Result<(Vec<Opt>, String)> {
    let (kind, extra) = endpoint(slot)?;
    let (id, game) = game_id(http, key, appid, name, by_name).await?;
    let mut list = get(http, key, &format!("/{kind}/game/{id}?nsfw=false&humor=any&epilepsy=any{extra}")).await?;
    let mut out: Vec<Opt> = list["data"].as_array_mut().map(std::mem::take).unwrap_or_default().iter().filter_map(|o| Some(Opt {
        url: o["url"].as_str()?.to_string(),
        thumb: o["thumb"].as_str().or(o["url"].as_str())?.to_string(),
        score: o["score"].as_i64().unwrap_or(0),
        width: o["width"].as_u64().unwrap_or(0),
        height: o["height"].as_u64().unwrap_or(0),
        author: o["author"]["name"].as_str().unwrap_or("").to_string(),
    })).collect();
    out.sort_by(|a, b| b.score.cmp(&a.score));
    out.truncate(40);
    Ok((out, game))
}

/// Only SteamGridDB's own image hosts, so a chosen URL cannot point Mercury at anything else.
fn allowed(url: &str) -> bool {
    url.strip_prefix("https://").and_then(|r| r.split('/').next()).is_some_and(|h| h == "steamgriddb.com" || h.ends_with(".steamgriddb.com"))
}

/// Steam's own store art, with any slot in `choices` (slot -> SteamGridDB image URL) replaced by the picked image.
/// The result has the shape Steam wants: (slot, extension, base64).
pub async fn resolve(http: &reqwest::Client, appid: u32, choices: &HashMap<String, String>) -> Result<Vec<(u8, String, String)>> {
    // The same base art the plain add uses: Steam's CDN, with SteamGridDB filling slots Steam lacks.
    let key = crate::config::Config::load().sgdb_key;
    let mut assets = crate::steam::art(http, appid, &key).await.assets;
    for (slot, url) in choices {
        let slot: u8 = slot.parse().map_err(|_| anyhow!("bad art slot"))?;
        if url.is_empty() { continue; }
        if !allowed(url) { bail!("that image is not from SteamGridDB"); }
        let r = http.get(url).timeout(std::time::Duration::from_secs(30)).send().await?;
        if !r.status().is_success() { bail!("could not download the chosen image ({})", r.status()); }
        let ext = match r.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("") {
            t if t.contains("png") => "png", t if t.contains("webp") => "webp", t if t.contains("gif") => "gif", _ => "jpg",
        }.to_string();
        let b64 = base64::engine::general_purpose::STANDARD.encode(r.bytes().await?);
        assets.retain(|a| a.0 != slot);
        assets.push((slot, ext, b64));
    }
    Ok(assets)
}

/// Download one chosen image: (extension, base64).
async fn download(http: &reqwest::Client, url: &str) -> Result<(String, String)> {
    if !allowed(url) { bail!("that image is not from SteamGridDB"); }
    let r = http.get(url).timeout(std::time::Duration::from_secs(30)).send().await?;
    if !r.status().is_success() { bail!("could not download the chosen image ({})", r.status()); }
    let ext = match r.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("") {
        t if t.contains("png") => "png", t if t.contains("webp") => "webp", t if t.contains("gif") => "gif", _ => "jpg",
    }.to_string();
    Ok((ext, base64::engine::general_purpose::STANDARD.encode(r.bytes().await?)))
}

/// Only the slots the user changed. A value of "default" means Steam's own store art for that slot.
pub async fn chosen(http: &reqwest::Client, appid: u32, choices: &HashMap<String, String>) -> Result<Vec<(u8, String, String)>> {
    let mut out = vec![];
    let mut store: Option<Vec<(u8, String, String)>> = None;
    for (slot, url) in choices {
        let slot: u8 = slot.parse().map_err(|_| anyhow!("bad art slot"))?;
        if url.is_empty() { continue; }
        if url == "default" {
            if store.is_none() {
                let key = crate::config::Config::load().sgdb_key;
                store = Some(crate::steam::art(http, appid, &key).await.assets);
            }
            match store.as_ref().and_then(|s| s.iter().find(|a| a.0 == slot)) {
                Some(a) => out.push(a.clone()),
                None => bail!("Steam has no store art for that slot"),
            }
        } else {
            let (ext, b64) = download(http, url).await?;
            out.push((slot, ext, b64));
        }
    }
    Ok(out)
}
