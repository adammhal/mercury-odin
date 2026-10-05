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

pub async fn options(http: &reqwest::Client, key: &str, appid: u32, slot: u8) -> Result<Vec<Opt>> {
    let (kind, extra) = endpoint(slot)?;
    let game = get(http, key, &format!("/games/steam/{appid}")).await?;
    let id = game["data"]["id"].as_u64().ok_or_else(|| anyhow!("SteamGridDB does not know this game"))?;
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
    Ok(out)
}

/// Only SteamGridDB's own image hosts, so a chosen URL cannot point Mercury at anything else.
fn allowed(url: &str) -> bool {
    url.strip_prefix("https://").and_then(|r| r.split('/').next()).is_some_and(|h| h == "steamgriddb.com" || h.ends_with(".steamgriddb.com"))
}

/// Steam's own store art, with any slot in `choices` (slot -> SteamGridDB image URL) replaced by the picked image.
/// The result has the shape Steam wants: (slot, extension, base64).
pub async fn resolve(http: &reqwest::Client, appid: u32, choices: &HashMap<String, String>) -> Result<Vec<(u8, String, String)>> {
    let mut assets = crate::steam::art(http, appid).await.assets;
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
