//! Is a torrent already cached on Real-Debrid? RD disabled its bulk `instantAvailability` endpoint
//! (see dionysus-server-py), so Mercury probes: add the magnet, select all files, read the status, delete it.
//! A cached torrent reports `downloaded` at once. Results are kept for a day.
use crate::rd::Rd;
use serde::Serialize;
use std::{collections::HashMap, sync::Mutex, time::{Duration, Instant}};

#[derive(Serialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Availability { Cached, NotCached, Blocked, Unknown }

static SEEN: Mutex<Option<HashMap<String, (Instant, Availability)>>> = Mutex::new(None);
const KEEP: Duration = Duration::from_secs(24 * 3600);

pub fn info_hash(magnet: &str) -> Option<String> {
    let i = magnet.to_lowercase().find("xt=urn:btih:")? + "xt=urn:btih:".len();
    let h: String = magnet[i..].chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
    (h.len() >= 32).then(|| h.to_lowercase())
}

async fn probe(rd: &Rd, magnet: &str) -> Availability {
    let id = match rd.add_magnet(magnet).await {
        Ok(id) => id,
        Err(e) if e.to_string().contains("451") => return Availability::Blocked,
        Err(_) => return Availability::Unknown,
    };
    let _ = rd.select_all(&id).await;
    let mut out = Availability::Unknown;
    for _ in 0..3 {
        tokio::time::sleep(Duration::from_millis(1200)).await;
        match rd.info(&id).await.map(|i| i.status) {
            Ok(s) if s == "downloaded" => { out = Availability::Cached; break; }
            Ok(s) if ["downloading", "queued", "waiting_files_selection"].contains(&s.as_str()) => out = Availability::NotCached,
            Ok(s) if ["magnet_error", "error", "virus", "dead"].contains(&s.as_str()) => { out = Availability::Unknown; break; }
            _ => {}
        }
    }
    let _ = rd.delete(&id).await;
    out
}

/// Availability per magnet, probing at most three at a time.
pub async fn check(rd: &Rd, magnets: Vec<String>) -> HashMap<String, Availability> {
    use futures_util::StreamExt;
    let mut out = HashMap::new();
    let mut todo = vec![];
    {
        let mut g = SEEN.lock().unwrap();
        let seen = g.get_or_insert_with(HashMap::new);
        for m in magnets {
            let Some(h) = info_hash(&m) else { continue };
            match seen.get(&h) {
                Some((t, a)) if t.elapsed() < KEEP && *a != Availability::Unknown => { out.insert(m, *a); }
                _ => todo.push((h, m)),
            }
        }
    }
    let results: Vec<(String, String, Availability)> = futures_util::stream::iter(todo.into_iter().map(|(h, m)| async move {
        let a = probe(rd, &m).await;
        (h, m, a)
    })).buffer_unordered(3).collect().await;
    let mut g = SEEN.lock().unwrap();
    let seen = g.get_or_insert_with(HashMap::new);
    for (h, m, a) in results {
        tracing::info!("rd cache {h}: {a:?}");
        seen.insert(h, (Instant::now(), a));
        out.insert(m, a);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hashes() {
        assert_eq!(info_hash("magnet:?xt=urn:btih:DE5CA5350AB6181CB3196E8F080FBD7D61DAB937&dn=x").as_deref(), Some("de5ca5350ab6181cb3196e8f080fbd7d61dab937"));
        assert_eq!(info_hash("https://example.com"), None);
    }
}
