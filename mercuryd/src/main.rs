// No console window on Windows; the Mercury app captures the log.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod browser;
mod cache;
#[cfg(windows)]
mod assist;
mod config;
mod download;
mod extract;
mod import;
mod install;
mod jobs;
mod rd;
mod shortcuts;
mod battery;
mod sources;
mod sgdb;
mod steam;
#[cfg(windows)]
mod steamwin;
mod storage;
mod warmup;

use axum::{Json, Router, extract::{Path, Query, State}, http::StatusCode, response::{IntoResponse, Response}, routing::{get, post}};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

pub const PORT: u16 = 47800;

/// "2026-10-02T14:05:00" from Unix seconds (UTC), comparable as a string with the feeds' ISO dates.
fn iso_from_unix(t: u64) -> String {
    let days = (t / 86400) as i64;
    let secs = t % 86400;
    // Civil-from-days (Howard Hinnant's algorithm).
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}", secs / 3600, secs % 3600 / 60, secs % 60)
}

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

fn steam_status() -> Value {
    #[cfg(windows)]
    return steamwin::status();
    #[cfg(not(windows))]
    json!({ "found": false, "flag": false })
}

fn warmup_installed() -> bool {
    #[cfg(windows)]
    return warmup::db_path().is_some();
    #[cfg(not(windows))]
    false
}

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
        "installer_launch_options": if cfg!(unix) { install::installer_fex_config().ok().map(|p| format!("PROTON_LOG=1 FEX_APP_CONFIG={} %command%", p.display())) } else { None },
        "storage": { "total": s.total, "free": s.free, "mercury": mercury },
        "platform": std::env::consts::OS,
        "warmup": warmup_installed(),
        "launcher": c.launcher,
        "steam": steam_status(),
    })))
}

async fn get_config(State(a): State<App>) -> R { Ok(Json(cfg(&a).public())) }

async fn put_config(State(a): State<App>, Json(mut v): Json<Value>) -> R {
    let mut c = cfg(&a);
    // An empty key from the UI means "unchanged"; the UI never sees the stored key.
    if v["rd_key"].as_str().is_none_or(|k| k.is_empty()) { v["rd_key"] = json!(c.rd_key); }
    if v["sgdb_key"].as_str().is_none_or(|k| k.is_empty()) { v["sgdb_key"] = json!(c.sgdb_key); }
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
async fn art(State(a): State<App>, Path(id): Path<u32>) -> R { Ok(Json(json!(steam::art(&a.m.http, id, &cfg(&a).sgdb_key).await))) }

#[derive(Deserialize)]
struct SrcQ { name: String, #[serde(default)] refresh: bool }
async fn sources(State(a): State<App>, Query(q): Query<SrcQ>) -> R {
    let c = cfg(&a);
    let (list, errors) = sources::search(&a.m.http, &c, &q.name, q.refresh).await;
    // Repacks are heavily compressed; SteamRIP and OnlineFix ship the game nearly as-is.
    let est: Vec<u64> = list.iter().map(|s| match s.provider.as_str() {
        "SteamRIP" | "OnlineFix" => s.size_bytes + s.size_bytes / 7,
        _ => storage::estimate_installed(s.size_bytes),
    }).collect();
    Ok(Json(json!({ "sources": list, "installed_estimate": est, "errors": errors })))
}

#[derive(Deserialize)]
struct Magnets { magnets: Vec<String> }
async fn rd_cached(State(a): State<App>, Json(b): Json<Magnets>) -> R {
    let rd = rd::Rd::new(a.m.http.clone(), &cfg(&a).rd_key)?;
    // Each probe briefly adds the torrent to the user's Real-Debrid account, so keep the batch small.
    let magnets: Vec<String> = b.magnets.into_iter().take(6).collect();
    Ok(Json(json!(cache::check(&rd, magnets).await)))
}

async fn list_jobs(State(a): State<App>) -> R { Ok(Json(json!(a.m.jobs()))) }

#[derive(Deserialize)]
struct NewJob { appid: u32, name: String, source: sources::Source, #[serde(default)] local_file: Option<std::path::PathBuf>, #[serde(default)] update: bool }
async fn new_job(State(a): State<App>, Json(j): Json<NewJob>) -> R {
    tracing::info!("new {}: {} ({}) from {}{}", if j.update { "update" } else { "job" }, j.name, j.appid, j.source.provider,
        j.local_file.as_ref().map(|f| format!(", local file {}", f.display())).unwrap_or_default());
    let files = match &j.local_file {
        Some(f) => {
            // Only files in ~/Downloads or another import location may be handed to a job.
            let in_downloads = f.parent().is_some_and(|p| p == config::browser_downloads_dir());
            if !(f.is_file() && (in_downloads || import::allowed(f))) {
                return Err(anyhow::anyhow!("{} is not a finished download or importable file", f.display()).into());
            }
            browser::with_siblings(f)
        }
        None => vec![],
    };
    Ok(Json(json!(a.m.enqueue(j.appid, j.name, j.source, files, j.update)?)))
}

#[derive(Deserialize)]
struct Since { #[serde(default)] since: u64 }
async fn browser_downloads(Query(q): Query<Since>) -> R { Ok(Json(json!(browser::list(&config::browser_downloads_dir(), q.since)))) }

#[derive(Deserialize, Default)]
struct Act {
    #[serde(default)] shortcut_id: Option<u32>, #[serde(default)] exe: Option<std::path::PathBuf>,
    #[serde(default)] name: Option<String>, #[serde(default)] art: std::collections::HashMap<String, String>,
}
async fn job_action(State(a): State<App>, Path((id, act)): Path<(u64, String)>, body: Option<Json<Act>>) -> R {
    let b = body.map(|b| b.0).unwrap_or_default();
    match act.as_str() {
        "pause" => a.m.pause(id),
        "resume" => a.m.resume(id),
        "cancel" => a.m.cancel(id).await,
        "remove" => a.m.remove(id).await?,
        "setup-launched" => a.m.setup_launched(id, b.shortcut_id.ok_or_else(|| anyhow::anyhow!("shortcut_id required"))?),
        "setup-done" => return Ok(Json(json!(a.m.setup_done(id)?))),
        "shortcut-created" => a.m.shortcut_created(id, b.shortcut_id.ok_or_else(|| anyhow::anyhow!("shortcut_id required"))?),
        #[cfg(windows)]
        "run-setup" => a.m.clone().run_setup(id, false).await?,
        #[cfg(windows)]
        "finish-setup" => a.m.clone().finish_setup(id).await?,
        #[cfg(windows)]
        "run-setup-admin" => a.m.clone().run_setup(id, true).await?,
        #[cfg(windows)]
        "confirm" => a.m.clone().confirm_review(id, b.name, b.art).await?,
        "steam-added" => a.m.steam_added(id, b.shortcut_id.ok_or_else(|| anyhow::anyhow!("shortcut_id required"))?, b.exe, b.name)?,
        _ => return Err(anyhow::anyhow!("unknown action {act}").into()),
    }
    Ok(Json(json!({ "ok": true })))
}

async fn clear_jobs(State(a): State<App>) -> R { Ok(Json(json!({ "cleared": a.m.clear_finished().await }))) }

/// Newer release of an installed game among its current sources, if any. Cached for six hours per game.
async fn update_check(State(a): State<App>, Path(appid): Path<u32>) -> R {
    use std::collections::HashMap;
    use std::sync::Mutex as M;
    static SEEN: M<Option<HashMap<u32, (std::time::Instant, Value)>>> = M::new(None);
    if let Some((t, v)) = SEEN.lock().unwrap().get_or_insert_with(HashMap::new).get(&appid) {
        if t.elapsed() < std::time::Duration::from_secs(6 * 3600) { return Ok(Json(v.clone())); }
    }
    let e = a.m.library().into_iter().find(|e| e.appid == appid).ok_or_else(|| anyhow::anyhow!("not installed"))?;
    let current = e.version.clone().or_else(|| e.source_name.as_deref().and_then(sources::version_in));
    let (list, _) = sources::search(&a.m.http, &cfg(&a), &e.name, false).await;
    let newest = current.as_ref().and_then(|cur| {
        list.iter().filter(|s| s.supported)
            .filter_map(|s| s.version.clone().or_else(|| sources::version_in(&s.name)).map(|v| (v, s)))
            .filter(|(v, _)| sources::is_newer(v, cur))
            .max_by(|(a, _), (b, _)| sources::version_key(a).cmp(&sources::version_key(b)))
            .map(|(v, s)| json!({ "version": v, "source": s }))
    });
    // SteamRIP often has no comparable version; a release it changed after this install counts as an update.
    let newest = newest.or_else(|| {
        if e.provider != "SteamRIP" { return None; }
        let since = e.source_updated.clone().unwrap_or_else(|| iso_from_unix(e.installed));
        list.iter().filter(|s| s.provider == "SteamRIP" && s.supported_or_browser())
            .filter(|s| s.updated.as_deref().is_some_and(|u| u > since.as_str()))
            .max_by(|a, b| a.updated.cmp(&b.updated))
            .map(|s| json!({ "version": format!("updated {}", &s.updated.as_deref().unwrap_or("")[..10.min(s.updated.as_deref().unwrap_or("").len())]), "source": s }))
    });
    let v = json!({ "current": current, "provider": e.provider, "newer": newest });
    SEEN.lock().unwrap().get_or_insert_with(HashMap::new).insert(appid, (std::time::Instant::now(), v.clone()));
    Ok(Json(v))
}

async fn import_candidates() -> R {
    let _ = std::fs::create_dir_all(import::drop_folder());
    let (c, cards) = tokio::task::spawn_blocking(|| (import::candidates(), import::unmounted_cards())).await?;
    Ok(Json(json!({ "drop_folder": import::drop_folder(), "candidates": c, "unmounted_cards": cards })))
}

#[derive(Deserialize)]
struct MountBody { device: String }
async fn mount_card(Json(b): Json<MountBody>) -> R {
    let msg = tokio::task::spawn_blocking(move || import::mount(&b.device)).await??;
    tracing::info!("mounted card: {msg}");
    Ok(Json(json!({ "message": msg })))
}

#[derive(Deserialize)]
struct ImportBody { path: std::path::PathBuf, appid: u32, name: String, #[serde(default)] keep_in_place: bool }
async fn import_game(State(a): State<App>, Json(b): Json<ImportBody>) -> R {
    if !import::allowed(&b.path) { return Err(anyhow::anyhow!("{} is not in an import location", b.path.display()).into()); }
    tracing::info!("import: {} ({}) from {} keep_in_place={}", b.name, b.appid, b.path.display(), b.keep_in_place);
    if b.path.is_dir() {
        Ok(Json(json!(a.m.enqueue_import(b.appid, b.name, b.path, b.keep_in_place)?)))
    } else {
        let src = sources::Source { name: b.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), provider: "Imported".into(),
            size: String::new(), size_bytes: 0, magnet: None, url: None, version: None, urls: vec![], supported: true, repack: false, declared: false, updated: None };
        Ok(Json(json!(a.m.enqueue(b.appid, b.name, src, browser::with_siblings(&b.path), false)?)))
    }
}

async fn library(State(a): State<App>) -> R { Ok(Json(json!(a.m.library()))) }

/// Places a game can live: internal storage plus any mounted microSD card.
fn locations(c: &config::Config) -> Vec<(String, std::path::PathBuf)> {
    let internal = config::home().join("Games/Mercury");
    let mut out = vec![("Internal storage".to_string(), internal.clone())];
    if c.games_dir != internal && !c.games_dir.starts_with("/run/media") { out.push(("Custom".into(), c.games_dir.clone())); }
    for (label, root) in import::roots() {
        if label == "microSD" { out.push((format!("microSD ({})", root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()), root.join("Mercury"))); }
    }
    out
}

async fn list_locations(State(a): State<App>) -> R {
    let c = cfg(&a);
    let v: Vec<Value> = locations(&c).into_iter().map(|(label, path)| {
        let st = storage::of(&path);
        json!({ "label": label, "path": path, "free": st.free, "total": st.total, "default": path == c.games_dir })
    }).collect();
    Ok(Json(json!(v)))
}

#[derive(Deserialize)]
struct MoveBody { to: std::path::PathBuf }
async fn move_game(State(a): State<App>, Path(appid): Path<u32>, Json(b): Json<MoveBody>) -> R {
    let c = cfg(&a);
    if !locations(&c).iter().any(|(_, p)| p == &b.to) { return Err(anyhow::anyhow!("{} is not an install location", b.to.display()).into()); }
    std::fs::create_dir_all(&b.to)?;
    Ok(Json(json!(a.m.start_move(appid, b.to)?)))
}

#[derive(Deserialize)]
struct ExeBody { exe: std::path::PathBuf }
/// Launch options for a game's shortcut: the configured ones, plus DLL overrides if it carries OnlineFix.
async fn launch_options(State(a): State<App>, Json(b): Json<ExeBody>) -> R {
    Ok(Json(json!({ "launch_options": install::launch_options(&cfg(&a).launch_options, &b.exe) })))
}
async fn launch_options_set(State(a): State<App>, Path(appid): Path<u32>) -> R { a.m.launch_options_set(appid); Ok(Json(json!({ "ok": true }))) }

async fn repointed(State(a): State<App>, Path(appid): Path<u32>) -> R { a.m.repointed(appid); Ok(Json(json!({ "ok": true }))) }

/// The user's non-Steam shortcuts that Mercury does not manage yet (candidates for adopting).
async fn steam_shortcuts(State(a): State<App>) -> R {
    let lib = a.m.library();
    let jobs = a.m.jobs();
    let browser = cfg(&a).browser_shortcut_id;
    let list: Vec<shortcuts::Shortcut> = shortcuts::list().into_iter().filter(|s| {
        !lib.iter().any(|e| e.shortcut_id == s.appid) && !jobs.iter().any(|j| j.shortcut_id == Some(s.appid))
            && Some(s.appid) != browser && s.exe.to_lowercase().ends_with(".exe")
    }).collect();
    Ok(Json(json!(list)))
}

#[derive(Deserialize)]
struct AdoptBody { appid: u32, name: String, dir: std::path::PathBuf, exe: std::path::PathBuf, shortcut_id: u32, #[serde(default)] provider: Option<String>, #[serde(default)] version: Option<String> }
async fn adopt(State(a): State<App>, Json(b): Json<AdoptBody>) -> R {
    if !b.exe.starts_with(&b.dir) { return Err(anyhow::anyhow!("exe must be inside dir").into()); }
    let e = jobs::Entry { appid: b.appid, name: b.name, dir: b.dir, exe: b.exe, shortcut_id: b.shortcut_id,
        provider: b.provider.unwrap_or_else(|| "Added by hand".into()), size: 0, version: b.version, installed: 0, installer_dir: None, source_name: None,
        source_updated: None, needs_repoint: false, moving_to: None, launch_options_fix: None };
    tracing::info!("adopt: {} ({}) shortcut {}", e.name, e.appid, e.shortcut_id);
    Ok(Json(json!(a.m.adopt(e)?)))
}

#[derive(Deserialize)]
struct ShortcutBody { shortcut_id: u32 }
async fn set_shortcut(State(a): State<App>, Path(appid): Path<u32>, Json(b): Json<ShortcutBody>) -> R {
    Ok(Json(json!(a.m.set_shortcut(appid, b.shortcut_id)?)))
}
async fn uninstall(State(a): State<App>, Path(id): Path<u32>) -> R { Ok(Json(json!(a.m.uninstall(id)?))) }
async fn installer_files(State(a): State<App>, Path(id): Path<u32>) -> R {
    Ok(Json(match a.m.installer_files(id) { Some((d, n)) => json!({ "dir": d, "size": n }), None => json!(null) }))
}
/// Windows: start an installed game directly (its folder as the working directory).
async fn play(State(a): State<App>, Path(id): Path<u32>) -> R {
    let e = a.m.library().into_iter().find(|e| e.appid == id).ok_or_else(|| anyhow::anyhow!("not installed"))?;
    std::process::Command::new(&e.exe).current_dir(e.exe.parent().unwrap_or(&e.dir)).spawn()
        .map_err(|err| anyhow::anyhow!("could not start {}: {err}", e.exe.display()))?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct OpenUrl { url: String }
/// Open a download page in the default browser (for hosts Real-Debrid cannot fetch).
async fn open_url(Json(o): Json<OpenUrl>) -> R {
    let ok = o.url.starts_with("https://") && o.url.chars().all(|c| c.is_ascii_alphanumeric() || "-._~:/?#[]@!&()*+,;=%".contains(c));
    if !ok { return Err(anyhow::anyhow!("not a plain https link").into()); }
    #[cfg(windows)]
    std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", &o.url]).spawn()?;
    #[cfg(not(windows))]
    std::process::Command::new("xdg-open").arg(&o.url).spawn()?;
    Ok(Json(json!({ "ok": true })))
}

async fn sgdb_options(State(a): State<App>, Path((appid, slot)): Path<(u32, u8)>) -> R {
    let key = cfg(&a).sgdb_key;
    Ok(Json(json!({ "options": sgdb::options(&a.m.http, &key, appid, slot).await? })))
}

#[derive(Deserialize)]
struct ResolveReq { appid: u32, #[serde(default)] choices: std::collections::HashMap<String, String> }
/// Store art with the chosen SteamGridDB images swapped in, ready for SetCustomArtworkForApp.
async fn art_resolve(State(a): State<App>, Json(r): Json<ResolveReq>) -> R {
    Ok(Json(json!({ "assets": sgdb::resolve(&a.m.http, r.appid, &r.choices).await? })))
}

/// Windows: make sure Steam runs with its debug port open and that Mercury has its own Steam shortcut (with art).
async fn launcher_setup(State(a): State<App>) -> R {
    #[cfg(windows)]
    {
        let http = a.m.http.clone();
        steamwin::ensure_ready(&http).await?;
        let known = cfg(&a).steam_self_id;
        if let Some(id) = known { if steamwin::shortcut_exists(&http, id).await { return Ok(Json(json!({ "ok": true, "shortcut": id, "added": false }))); } }
        let dir = std::env::current_exe()?.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        let exe = dir.join("Mercury.exe");
        let mut art = vec![];
        for (t, f) in [(0u8, "cover.jpg"), (1, "hero.jpg")] {
            if let Ok(b) = std::fs::read(dir.join("art").join(f)) {
                art.push((t, "jpg".to_string(), base64::Engine::encode(&base64::engine::general_purpose::STANDARD, b)));
            }
        }
        let id = steamwin::add_shortcut(&http, "Mercury", &exe, &art).await?;
        a.m.set_steam_self(id);
        Ok(Json(json!({ "ok": true, "shortcut": id, "added": true })))
    }
    #[cfg(not(windows))]
    { let _ = a; Err(anyhow::anyhow!("only available on Windows").into()) }
}

/// Windows: sleep, restart, shut down or sign out. The command runs 1.5 s after this reply goes out, with no
/// shutdown.exe countdown (a non-zero /t makes Windows pop up a "you are about to be signed out" warning).
async fn power(Path(action): Path<String>) -> R {
    #[cfg(windows)]
    {
        let mut c = match action.as_str() {
            "sleep" => { let mut c = std::process::Command::new("rundll32.exe"); c.args(["powrprof.dll,SetSuspendState", "0,1,0"]); c }
            "restart" => { let mut c = std::process::Command::new("shutdown.exe"); c.args(["/r", "/t", "0"]); c }
            "shutdown" => { let mut c = std::process::Command::new("shutdown.exe"); c.args(["/s", "/t", "0"]); c }
            "signout" => { let mut c = std::process::Command::new("shutdown.exe"); c.arg("/l"); c }
            _ => return Err(anyhow::anyhow!("unknown power action").into()),
        };
        std::os::windows::process::CommandExt::creation_flags(&mut c, 0x0800_0000);
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(1500));
            if let Err(e) = c.spawn() { tracing::warn!("power {action} failed: {e}"); }
        });
        Ok(Json(json!({ "ok": true })))
    }
    #[cfg(not(windows))]
    { Err(anyhow::anyhow!("power controls are only available on Windows ({action})").into()) }
}

async fn delete_installer_files(State(a): State<App>, Path(id): Path<u32>) -> R { Ok(Json(json!({ "freed": a.m.delete_installer_files(id)? }))) }

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "mercuryd=info".into());
    // Windows: always keep a log file. Started by Mercury the engine's output is already piped to mercuryd.log; started
    // any other way (a scheduled task, the installer) it would otherwise have nowhere to write.
    #[cfg(windows)]
    {
        let _ = std::fs::create_dir_all(config::data_dir());
        match std::fs::OpenOptions::new().create(true).append(true).open(config::data_dir().join("engine.log")) {
            Ok(f) => tracing_subscriber::fmt().with_env_filter(filter).with_ansi(false).with_writer(Mutex::new(f)).init(),
            Err(_) => tracing_subscriber::fmt().with_env_filter(filter).init(),
        }
    }
    #[cfg(not(windows))]
    tracing_subscriber::fmt().with_env_filter(filter).init();
    #[cfg(windows)]
    {
        let args: Vec<String> = std::env::args().collect();
        if let Some(i) = args.iter().position(|a| a == "--assist") {
            let dir = std::path::PathBuf::from(args.get(i + 1).cloned().unwrap_or_default());
            let parent = args.get(i + 2).and_then(|p| p.parse().ok()).unwrap_or(0);
            assist::helper_main(dir, parent);
            return Ok(());
        }
    }
    let http = reqwest::Client::builder().user_agent("Mercury/0.1").connect_timeout(std::time::Duration::from_secs(15)).build()?;
    let cfg = Arc::new(Mutex::new(config::Config::load()));
    let m = jobs::Manager::new(http, cfg);
    m.start();
    let app = Router::new()
        .route("/status", get(status))
        .route("/battery", get(|| async { Json(battery::read()) }))
        .route("/config", get(get_config).put(put_config))
        .route("/rd/check", get(rd_check))
        .route("/steam/wishlist", get(wishlist))
        .route("/steam/search", get(search))
        .route("/steam/app/{id}", get(app))
        .route("/steam/art/{id}", get(art))
        .route("/sources", get(sources))
        .route("/sources/cached", post(rd_cached))
        .route("/jobs", get(list_jobs).post(new_job))
        .route("/browser/downloads", get(browser_downloads))
        .route("/jobs/clear", post(clear_jobs))
        .route("/jobs/{id}/{act}", post(job_action))
        .route("/library", get(library))
        .route("/library/adopt", post(adopt))
        .route("/library/{appid}/move", post(move_game))
        .route("/library/{appid}/repointed", post(repointed))
        .route("/library/{appid}/launch-options-set", post(launch_options_set))
        .route("/launch-options", post(launch_options))
        .route("/locations", get(list_locations))
        .route("/steam/shortcuts", get(steam_shortcuts))
        .route("/import", get(import_candidates).post(import_game))
        .route("/import/mount", post(mount_card))
        .route("/library/{appid}/uninstall", post(uninstall))
        .route("/library/{appid}/update", get(update_check))
        .route("/library/{appid}/shortcut", post(set_shortcut))
        .route("/library/{appid}/installer-files", get(installer_files).delete(delete_installer_files))
        .route("/library/{appid}/play", post(play))
        .route("/open", post(open_url))
        .route("/power/{action}", post(power))
        .route("/launcher/setup", post(launcher_setup))
        .route("/sgdb/{appid}/{slot}", get(sgdb_options))
        .route("/art/resolve", post(art_resolve))
        .with_state(App { m })
        // The Windows app's window (tauri.localhost) calls the engine from a different origin.
        .layer(tower_http::cors::CorsLayer::new()
            .allow_origin(["http://tauri.localhost".parse::<axum::http::HeaderValue>()?, "tauri://localhost".parse()?, "http://localhost:1420".parse()?])
            .allow_methods(tower_http::cors::Any).allow_headers(tower_http::cors::Any));
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", PORT)).await?;
    tracing::info!("mercuryd {} on 127.0.0.1:{PORT}", env!("CARGO_PKG_VERSION"));
    axum::serve(listener, app).await?;
    Ok(())
}
