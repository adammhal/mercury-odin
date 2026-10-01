//! Resumable HTTP download. Pausing stops the stream and keeps the partial file;
//! resuming continues with a Range request from the bytes already on disk.
use anyhow::{Result, bail};
use futures_util::StreamExt;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::io::AsyncWriteExt;

#[derive(Default)]
pub struct Progress {
    pub done: AtomicU64,
    pub total: AtomicU64,
    pub speed: AtomicU64,
}

pub enum Outcome {
    Finished,
    Stopped,
}

pub async fn fetch(http: &reqwest::Client, url: &str, dest: &Path, prog: &Arc<Progress>, stop: &Arc<AtomicBool>) -> Result<Outcome> {
    let have = tokio::fs::metadata(dest).await.map(|m| m.len()).unwrap_or(0);
    let mut req = http.get(url);
    if have > 0 {
        req = req.header(reqwest::header::RANGE, format!("bytes={have}-"));
    }
    let resp = req.send().await?;
    let status = resp.status();
    if status.as_u16() == 416 {
        return Ok(Outcome::Finished);
    }
    if !status.is_success() {
        bail!("download failed: HTTP {status}");
    }
    let resumed = status.as_u16() == 206;
    if have > 0 && !resumed {
        // Server ignored the Range header and is sending the whole file again.
        prog.done.fetch_sub(have.min(prog.done.load(Ordering::Relaxed)), Ordering::Relaxed);
    }
    let mut file = tokio::fs::OpenOptions::new().create(true).write(true).append(resumed).truncate(!resumed).open(dest).await?;
    let mut stream = resp.bytes_stream();
    let mut window = (std::time::Instant::now(), 0u64);
    while let Some(chunk) = stream.next().await {
        if stop.load(Ordering::Relaxed) {
            file.flush().await?;
            return Ok(Outcome::Stopped);
        }
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        prog.done.fetch_add(chunk.len() as u64, Ordering::Relaxed);
        window.1 += chunk.len() as u64;
        let el = window.0.elapsed().as_secs_f64();
        if el >= 1.0 {
            prog.speed.store((window.1 as f64 / el) as u64, Ordering::Relaxed);
            window = (std::time::Instant::now(), 0);
        }
    }
    file.flush().await?;
    prog.speed.store(0, Ordering::Relaxed);
    Ok(Outcome::Finished)
}
