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
    /// Every mirror for a direct-link source; `url` is the preferred one.
    #[serde(default)]
    pub urls: Vec<String>,
    /// False when Real-Debrid supports none of the source's hosts, so it cannot be downloaded.
    #[serde(default = "yes")]
    pub supported: bool,
}

fn yes() -> bool { true }

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
    #[serde(default)]
    uris: Vec<String>,
    version: Option<String>,
}

static RIP_CACHE: Mutex<Option<(Instant, Vec<RipItem>)>> = Mutex::new(None);
static RD_HOSTS: Mutex<Option<(Instant, Vec<String>)>> = Mutex::new(None);

fn host(url: &str) -> String {
    url.split("://").nth(1).unwrap_or(url).split('/').next().unwrap_or("").trim_start_matches("www.").to_lowercase()
}

/// Domains Real-Debrid can download from. Public endpoint, cached for a day.
async fn rd_hosts(http: &reqwest::Client) -> Vec<String> {
    if let Some((t, v)) = RD_HOSTS.lock().unwrap().as_ref() {
        if t.elapsed() < Duration::from_secs(86400) { return v.clone(); }
    }
    match http.get("https://api.real-debrid.com/rest/1.0/hosts/domains").send().await {
        Ok(r) => match r.json::<Vec<String>>().await {
            Ok(v) => { *RD_HOSTS.lock().unwrap() = Some((Instant::now(), v.clone())); v }
            Err(_) => vec![],
        },
        Err(_) => vec![],
    }
}

/// A version is short ("v1.0.3", "Build 10371237"). Anything else is scraped page text.
pub fn clean_version(v: &str) -> Option<String> {
    let mut v = clean_title(v);
    for sep in [" + ", " (", " |", " ["] {
        if let Some(i) = v.find(sep) { v.truncate(i); }
    }
    let lower = v.to_lowercase();
    let shaped = lower.starts_with('v') || lower.starts_with("build") || lower.starts_with("b.") || v.contains('.');
    let ok = shaped && v.chars().count() <= 24 && v.chars().any(|c| c.is_ascii_digit());
    ok.then_some(v)
}

/// Scraped titles sometimes carry the store page text ("... Storage: 12 GB GAME INFO Genre: ...").
pub fn clean_title(t: &str) -> String {
    let mut s = t.trim().to_string();
    for marker in [" Storage:", " GAME INFO", " Genre:", " Developer:", " Platform:", " Game Size:", " Released By:", " Release Date:", " Size:"] {
        if let Some(i) = s.find(marker) { s.truncate(i); }
    }
    let s = s.trim_end_matches(|c: char| c == '-' || c == '|' || c.is_whitespace()).to_string();
    if s.chars().count() > 110 { s.chars().take(107).collect::<String>() + "…" } else { s }
}

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
                    name: clean_title(&i.name),
                    provider: if i.provider.is_empty() { "Server".into() } else { i.provider },
                    magnet: i.magnet,
                    url: i.url,
                    version: None,
                    urls: vec![],
                    supported: true,
                }));
            }
        }
        Err(e) => errors.push(e.to_string()),
    }
    let hosts = if rip.is_empty() { vec![] } else { rd_hosts(http).await };
    for r in rip {
        let score = similarity(name, &r.title);
        if score >= 0.75 {
            let mut urls = r.uris.clone();
            if !urls.contains(&r.url) { urls.insert(0, r.url.clone()); }
            // Put mirrors Real-Debrid can fetch first. An empty host list means the check failed; assume yes.
            urls.sort_by_key(|u| !hosts.contains(&host(u)));
            let supported = hosts.is_empty() || urls.iter().any(|u| hosts.contains(&host(u)));
            out.push((score, Source {
                size_bytes: parse_size(&r.size),
                size: r.size,
                name: clean_title(&r.title),
                provider: "SteamRIP".into(),
                magnet: None,
                url: urls.first().cloned(),
                version: r.version.as_deref().and_then(clean_version),
                urls,
                supported,
            }));
        }
    }
    // Best match first; sources that cannot be downloaded go to the bottom.
    out.sort_by(|a, b| b.1.supported.cmp(&a.1.supported).then(b.0.partial_cmp(&a.0).unwrap()));
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
    fn titles() {
        assert_eq!(clean_title("Skate Story Free Download 12 Storage: 12 GB available space GAME INFO Genre: Action"), "Skate Story Free Download 12");
        assert_eq!(host("https://www.gofile.io/d/abc"), "gofile.io");
        assert_eq!(clean_version("v1.0.3").as_deref(), Some("v1.0.3"));
        assert_eq!(clean_version("12 Storage: 12 GB available space GAME INFO Genre: Action"), None);
        assert_eq!(clean_version("Unknown"), None);
        assert_eq!(clean_version("Build 10371237").as_deref(), Some("Build 10371237"));
        assert_eq!(clean_version("1.0.28324").as_deref(), Some("1.0.28324"));
        assert_eq!(clean_version("Build 1286980 + Multiplayer").as_deref(), Some("Build 1286980"));
    }
    #[test]
    fn matching() {
        assert!(similarity("Hollow Knight: Silksong", "Hollow Knight: Silksong – v1.0.28324") >= 0.85);
        assert!(similarity("Hollow Knight: Silksong", "Maseylia: Echoes of the Past") < 0.3);
        assert!(similarity("Hollow Knight", "Hollow Knight Silksong Free Download") >= 0.8);
    }
}
