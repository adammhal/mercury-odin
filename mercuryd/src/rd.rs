//! Real-Debrid REST client. Only the calls Mercury needs.
use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;

const BASE: &str = "https://api.real-debrid.com/rest/1.0";

#[derive(Clone)]
pub struct Rd {
    http: reqwest::Client,
    key: String,
}

#[derive(Debug, Deserialize)]
pub struct TorrentInfo {
    pub status: String,
    #[serde(default)]
    pub progress: f64,
    #[serde(default)]
    pub links: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct Unrestricted {
    pub download: String,
    pub filename: String,
    #[serde(default)]
    pub filesize: u64,
}

impl Rd {
    pub fn new(http: reqwest::Client, key: &str) -> Result<Self> {
        if key.is_empty() {
            bail!("Real-Debrid key is not set. Add it in Mercury settings.");
        }
        Ok(Self { http, key: key.to_string() })
    }

    async fn send(&self, req: reqwest::RequestBuilder) -> Result<reqwest::Response> {
        let resp = req.bearer_auth(&self.key).send().await?;
        let status = resp.status();
        if status.is_success() {
            return Ok(resp);
        }
        let body = resp.text().await.unwrap_or_default();
        let code = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| v["error"].as_str().map(String::from))
            .unwrap_or_else(|| body.chars().take(120).collect());
        if status.as_u16() == 451 || code.contains("infringing") {
            bail!("blocked by Real-Debrid (451 infringing_file)");
        }
        bail!("Real-Debrid {status}: {code}")
    }

    pub async fn add_magnet(&self, magnet: &str) -> Result<String> {
        let r = self.send(self.http.post(format!("{BASE}/torrents/addMagnet")).form(&[("magnet", magnet)])).await?;
        let v: serde_json::Value = r.json().await?;
        v["id"].as_str().map(String::from).ok_or_else(|| anyhow!("no torrent id"))
    }

    pub async fn select_all(&self, id: &str) -> Result<()> {
        self.send(self.http.post(format!("{BASE}/torrents/selectFiles/{id}")).form(&[("files", "all")])).await?;
        Ok(())
    }

    pub async fn info(&self, id: &str) -> Result<TorrentInfo> {
        let r = self.send(self.http.get(format!("{BASE}/torrents/info/{id}"))).await?;
        r.json().await.context("torrent info")
    }

    pub async fn unrestrict(&self, link: &str) -> Result<Unrestricted> {
        let r = self.send(self.http.post(format!("{BASE}/unrestrict/link")).form(&[("link", link)])).await?;
        r.json().await.context("unrestrict")
    }

    pub async fn delete(&self, id: &str) -> Result<()> {
        self.send(self.http.delete(format!("{BASE}/torrents/delete/{id}"))).await?;
        Ok(())
    }

    pub async fn user(&self) -> Result<serde_json::Value> {
        Ok(self.send(self.http.get(format!("{BASE}/user"))).await?.json().await?)
    }
}
