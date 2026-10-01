//! Game sources: Adam's Mercury server (FitGirl, OnlineFix, TorrentGames) and the SteamRIP feed.
use crate::config::Config;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Source {
    pub name: String,
    pub provider: String,
    #[serde(default)]
    pub size: String,
    #[serde(default)]
    pub size_bytes: u64,
    #[serde(default)]
    pub magnet: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Deserialize)]
struct ServerItem {
    name: String,
    #[serde(default)]
    size: String,
    magnet: Option<String>,
    url: Option<String>,
    #[serde(default)]
    provider: String,
}

#[derive(Deserialize)]
struct RipFeed {
    games: Vec<RipItem>,
}

#[derive(Clone, Deserialize)]
struct RipItem {
    title: String,
    #[serde(default)]
    size: String,
    url: String,
    version: Option<String>,
}

static RIP_CACHE: Mutex<Option<(Instant, Vec<RipItem>)>> = Mutex::new(None);

pub fn parse_size(s: &str) -> u64 {
    let s = s.trim().to_uppercase();
    let num: f64 = s.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect::<String>().parse().unwrap_or(0.0);
    let mul = if s.contains("TB") { 1u64 << 40 } else if s.contains("GB") { 1 << 30 } else if s.contains("MB") { 1 << 20 } else if s.contains("KB") { 1 << 10 } else { 0 };
    (num * mul as f64) as u64
}

fn norm(s: &str) -> String {
    let s = s.to_lowercase();
    let s = s.split(" free download").next().unwrap_or(&s).to_string();
    s.chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Token overlap: share of the query's words that appear in the candidate, penalising extra words lightly.
pub fn similarity(query: &str, candidate: &str) -> f64 {
    let q: Vec<String> = norm(query).split(' ').filter(|w| !w.is_empty()).map(String::from).collect();
    let c: Vec<String> = norm(candidate).split(' ').filter(|w| !w.is_empty()).map(String::from).collect();
    if q.is_empty() || c.is_empty() {
        return 0.0;
    }
    let hit = q.iter().filter(|w| c.contains(w)).count() as f64;
    let recall = hit / q.len() as f64;
    let precision = hit / c.len().min(q.len() + 4) as f64;
    0.8 * recall + 0.2 * precision.min(1.0)
}

async fn steamrip(http: &reqwest::Client, cfg: &Config) -> Vec<RipItem> {
    if let Some((t, v)) = RIP_CACHE.lock().unwrap().as_ref() {
        if t.elapsed() < Duration::from_secs(6 * 3600) {
            return v.clone();
        }
    }
    match http.get(&cfg.steamrip_url).send().await {
        Ok(r) => match r.json::<RipFeed>().await {
            Ok(f) => {
                *RIP_CACHE.lock().unwrap() = Some((Instant::now(), f.games.clone()));
                f.games
            }
            Err(e) => { tracing::warn!("steamrip parse: {e}"); vec![] }
        },
        Err(e) => { tracing::warn!("steamrip fetch: {e}"); vec![] }
    }
}

async fn server(http: &reqwest::Client, cfg: &Config, name: &str) -> anyhow::Result<Vec<ServerItem>> {
    let url = format!("{}/api/games/search?query={}", cfg.server_url, urlencoding::encode(name));
    // The Railway server sleeps when idle; the first request can 502 while it wakes.
    for attempt in 0..4 {
        match http.get(&url).timeout(Duration::from_secs(45)).send().await {
            Ok(r) if r.status().is_success() => {
                let v: serde_json::Value = r.json().await?;
                return Ok(serde_json::from_value(v["data"].clone()).unwrap_or_default());
            }
            Ok(r) => tracing::info!("server {} (attempt {attempt})", r.status()),
            Err(e) => tracing::info!("server error {e} (attempt {attempt})"),
        }
        tokio::time::sleep(Duration::from_secs(8)).await;
    }
    anyhow::bail!("Mercury server did not respond")
}

pub async fn search(http: &reqwest::Client, cfg: &Config, name: &str) -> (Vec<Source>, Vec<String>) {
    let mut errors = vec![];
    let (srv, rip) = tokio::join!(server(http, cfg, name), async {
        if cfg.enable_steamrip { steamrip(http, cfg).await } else { vec![] }
    });
    let mut out: Vec<(f64, Source)> = vec![];
    match srv {
        Ok(items) => {
            for i in items {
                let score = similarity(name, &i.name);
                if score < 0.6 || (i.magnet.is_none() && i.url.is_none()) {
                    continue;
                }
                out.push((score, Source {
                    size_bytes: parse_size(&i.size),
                    size: i.size,
                    name: i.name,
                    provider: if i.provider.is_empty() { "Server".into() } else { i.provider },
                    magnet: i.magnet,
                    url: i.url,
                    version: None,
                }));
            }
        }
        Err(e) => errors.push(e.to_string()),
    }
    for r in rip {
        let score = similarity(name, &r.title);
        if score >= 0.75 {
            out.push((score, Source {
                size_bytes: parse_size(&r.size),
                size: r.size,
                name: r.title,
                provider: "SteamRIP".into(),
                magnet: None,
                url: Some(r.url),
                version: r.version.filter(|v| v != "Unknown"),
            }));
        }
    }
    out.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    (out.into_iter().map(|(_, s)| s).collect(), errors)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sizes() {
        assert_eq!(parse_size("1.5 GB"), (1.5 * (1u64 << 30) as f64) as u64);
        assert_eq!(parse_size("814 MB"), 814 << 20);
        assert_eq!(parse_size("N/A"), 0);
    }
    #[test]
    fn matching() {
        assert!(similarity("Hollow Knight: Silksong", "Hollow Knight: Silksong – v1.0.28324") >= 0.85);
        assert!(similarity("Hollow Knight: Silksong", "Maseylia: Echoes of the Past") < 0.3);
        assert!(similarity("Hollow Knight", "Hollow Knight Silksong Free Download") >= 0.8);
    }
}
