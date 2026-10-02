mod browser;
mod config;
mod download;
mod extract;
mod install;
mod jobs;
mod rd;
mod sources;
mod steam;
mod storage;

use axum::{Json, Router, extract::{Path, Query, State}, http::StatusCode, response::{IntoResponse, Response}, routing::{get, post}};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

pub const PORT: u16 = 47800;

#[derive(Clone)]
struct App { m: Arc<jobs::Manager> }

struct ApiError(anyhow::Error);
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let msg = format!("{:#}", self.0);
        // Every refused request lands in mercuryd.log, so the log explains what the UI showed.
        tracing::warn!("request failed: {msg}");
        (StatusCode::BAD_REQUEST, Json(json!({ "error": msg }))).into_response()
    }
}
impl<E: Into<anyhow::Error>> From<E> for ApiError { fn from(e: E) -> Self { Self(e.into()) } }
type R = Result<Json<Value>, ApiError>;

fn cfg(a: &App) -> config::Config { a.m.cfg.lock().unwrap().clone() }

async fn status(State(a): State<App>) -> R {
    let c = cfg(&a);
    let s = storage::of(&c.games_dir);
    let mercury: u64 = a.m.library().iter().map(|e| e.size).sum();
    Ok(Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "rd_key_set": !c.rd_key.is_empty(),
        "unrar": extract::unrar_path().exists(),
        // Launch options for repack installers: full x87 precision, Proton's default log, no Armada wrapper
        // (the wrapper would replace FEX_APP_CONFIG with its own).
        "installer_launch_options": install::installer_fex_config().ok().map(|p| format!("PROTON_LOG=1 FEX_APP_CONFIG={} %command%", p.display())),
        "storage": { "total": s.total, "free": s.free, "mercury": mercury },
    })))
}

async fn get_config(State(a): State<App>) -> R { Ok(Json(cfg(&a).public())) }

async fn put_config(State(a): State<App>, Json(mut v): Json<Value>) -> R {
    let mut c = cfg(&a);
    // An empty key from the UI means "unchanged"; the UI never sees the stored key.
    if v["rd_key"].as_str().is_none_or(|k| k.is_empty()) { v["rd_key"] = json!(c.rd_key); }
    let mut merged = serde_json::to_value(&c)?;
    if let (Some(m), Some(n)) = (merged.as_object_mut(), v.as_object()) {
        for (k, val) in n { if m.contains_key(k) { m.insert(k.clone(), val.clone()); } }
    }
    c = serde_json::from_value(merged)?;
    c.save()?;
    *a.m.cfg.lock().unwrap() = c.clone();
    Ok(Json(c.public()))
}

async fn rd_check(State(a): State<App>) -> R {
    let u = rd::Rd::new(a.m.http.clone(), &cfg(&a).rd_key)?.user().await?;
    Ok(Json(json!({ "username": u["username"], "type": u["type"], "expiration": u["expiration"] })))
}

async fn wishlist(State(a): State<App>) -> R { Ok(Json(json!(steam::wishlist(&a.m.http).await?))) }

#[derive(Deserialize)]
struct Q { q: String }
async fn search(State(a): State<App>, Query(q): Query<Q>) -> R { Ok(Json(json!(steam::search(&a.m.http, &q.q).await?))) }

async fn app(State(a): State<App>, Path(id): Path<u32>) -> R { Ok(Json(json!(steam::details(&a.m.http, id).await?))) }
async fn art(State(a): State<App>, Path(id): Path<u32>) -> R { Ok(Json(json!(steam::art(&a.m.http, id).await))) }

#[derive(Deserialize)]
struct SrcQ { name: String }
async fn sources(State(a): State<App>, Query(q): Query<SrcQ>) -> R {
    let c = cfg(&a);
    let (list, errors) = sources::search(&a.m.http, &c, &q.name).await;
    // Repacks are heavily compressed; SteamRIP and OnlineFix ship the game nearly as-is.
    let est: Vec<u64> = list.iter().map(|s| match s.provider.as_str() {
        "SteamRIP" | "OnlineFix" => s.size_bytes + s.size_bytes / 7,
        _ => storage::estimate_installed(s.size_bytes),
    }).collect();
    Ok(Json(json!({ "sources": list, "installed_estimate": est, "errors": errors })))
}

async fn list_jobs(State(a): State<App>) -> R { Ok(Json(json!(a.m.jobs()))) }

#[derive(Deserialize)]
struct NewJob { appid: u32, name: String, source: sources::Source, #[serde(default)] local_file: Option<std::path::PathBuf> }
async fn new_job(State(a): State<App>, Json(j): Json<NewJob>) -> R {
    tracing::info!("new job: {} ({}) from {}{}", j.name, j.appid, j.source.provider,
        j.local_file.as_ref().map(|f| format!(", local file {}", f.display())).unwrap_or_default());
    let files = match &j.local_file {
        Some(f) => {
            // Only files in the browser's download folder may be handed to a job.
            let dir = config::browser_downloads_dir();
            let ok = f.parent().is_some_and(|p| p == dir) && f.is_file();
            if !ok { return Err(anyhow::anyhow!("{} is not a finished download in {}", f.display(), dir.display()).into()); }
            browser::with_siblings(f)
        }
        None => vec![],
    };
    Ok(Json(json!(a.m.enqueue(j.appid, j.name, j.source, files)?)))
}

#[derive(Deserialize)]
struct Since { #[serde(default)] since: u64 }
async fn browser_downloads(Query(q): Query<Since>) -> R { Ok(Json(json!(browser::list(&config::browser_downloads_dir(), q.since)))) }

#[derive(Deserialize, Default)]
struct Act { #[serde(default)] shortcut_id: Option<u32>, #[serde(default)] exe: Option<std::path::PathBuf> }
async fn job_action(State(a): State<App>, Path((id, act)): Path<(u64, String)>, body: Option<Json<Act>>) -> R {
    let b = body.map(|b| b.0).unwrap_or_default();
    match act.as_str() {
        "pause" => a.m.pause(id),
        "resume" => a.m.resume(id),
        "cancel" => a.m.cancel(id).await,
        "remove" => a.m.remove(id).await?,
        "setup-launched" => a.m.setup_launched(id, b.shortcut_id.ok_or_else(|| anyhow::anyhow!("shortcut_id required"))?),
        "setup-done" => return Ok(Json(json!(a.m.setup_done(id)?))),
        "steam-added" => a.m.steam_added(id, b.shortcut_id.ok_or_else(|| anyhow::anyhow!("shortcut_id required"))?, b.exe)?,
        _ => return Err(anyhow::anyhow!("unknown action {act}").into()),
    }
    Ok(Json(json!({ "ok": true })))
}

async fn clear_jobs(State(a): State<App>) -> R { Ok(Json(json!({ "cleared": a.m.clear_finished().await }))) }

async fn library(State(a): State<App>) -> R { Ok(Json(json!(a.m.library()))) }
async fn uninstall(State(a): State<App>, Path(id): Path<u32>) -> R { Ok(Json(json!(a.m.uninstall(id)?))) }
async fn installer_files(State(a): State<App>, Path(id): Path<u32>) -> R {
    Ok(Json(match a.m.installer_files(id) { Some((d, n)) => json!({ "dir": d, "size": n }), None => json!(null) }))
}
async fn delete_installer_files(State(a): State<App>, Path(id): Path<u32>) -> R { Ok(Json(json!({ "freed": a.m.delete_installer_files(id)? }))) }

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "mercuryd=info".into())).init();
    let http = reqwest::Client::builder().user_agent("Mercury/0.1").connect_timeout(std::time::Duration::from_secs(15)).build()?;
    let cfg = Arc::new(Mutex::new(config::Config::load()));
    let m = jobs::Manager::new(http, cfg);
    tokio::spawn(m.clone().run());
    let app = Router::new()
        .route("/status", get(status))
        .route("/config", get(get_config).put(put_config))
        .route("/rd/check", get(rd_check))
        .route("/steam/wishlist", get(wishlist))
        .route("/steam/search", get(search))
        .route("/steam/app/{id}", get(app))
        .route("/steam/art/{id}", get(art))
        .route("/sources", get(sources))
        .route("/jobs", get(list_jobs).post(new_job))
        .route("/browser/downloads", get(browser_downloads))
        .route("/jobs/clear", post(clear_jobs))
        .route("/jobs/{id}/{act}", post(job_action))
        .route("/library", get(library))
        .route("/library/{appid}/uninstall", post(uninstall))
        .route("/library/{appid}/installer-files", get(installer_files).delete(delete_installer_files))
        .with_state(App { m });
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", PORT)).await?;
    tracing::info!("mercuryd {} on 127.0.0.1:{PORT}", env!("CARGO_PKG_VERSION"));
    axum::serve(listener, app).await?;
    Ok(())
}
