//! The install pipeline. One job runs at a time; the rest wait in order.
//!
//! queued → resolving → caching → downloading → extracting → ready → done
//!                                                     ↘ needs_setup → installing → ready → done
//! Steam writes (shortcut, art, Proton) happen in the plugin, which reports back the shortcut ID.
use crate::{config::{Config, data_dir, home}, download, extract, install, rd::Rd, sources::Source, storage};
use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf, sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}}, time::Duration};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State { Queued, Resolving, Caching, Downloading, Paused, Extracting, NeedsSetup, Installing, Ready, Review, Done, Failed, Cancelled }

impl State {
    pub fn active(self) -> bool { !matches!(self, State::Done | State::Failed | State::Cancelled) }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Link { pub url: String, pub filename: String, pub size: u64 }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Job {
    pub id: u64,
    pub appid: u32,
    pub name: String,
    pub source: Source,
    pub state: State,
    #[serde(default)] pub error: Option<String>,
    #[serde(default)] pub torrent_id: Option<String>,
    #[serde(default)] pub links: Vec<Link>,
    /// Files the user downloaded in the browser. When set, resolving and downloading are skipped.
    #[serde(default)] pub local_files: Vec<PathBuf>,
    #[serde(default)] pub dir: Option<PathBuf>,
    #[serde(default)] pub setup_exe: Option<PathBuf>,
    #[serde(default)] pub exe: Option<PathBuf>,
    #[serde(default)] pub candidates: Vec<PathBuf>,
    #[serde(default)] pub shortcut_id: Option<u32>,
    /// When the installer was last started (Unix seconds). Only files created after it can be the game.
    #[serde(default)] pub setup_started: Option<u64>,
    /// Set when this job replaces the files of an installed game (keeping its shortcut and Proton prefix).
    #[serde(default)] pub update_of: Option<u32>,
    /// Game folder installed elsewhere (microSD, drop folder). Skips downloading and extracting.
    #[serde(default)] pub import_dir: Option<PathBuf>,
    /// Leave `import_dir` where it is (a microSD card) instead of moving it into the games folder.
    #[serde(default)] pub keep_in_place: bool,
    #[serde(default)] pub cache_progress: f64,
    #[serde(default)] pub done: u64,
    #[serde(default)] pub total: u64,
    #[serde(default)] pub speed: u64,
    pub created: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub appid: u32,
    pub name: String,
    pub dir: PathBuf,
    pub exe: PathBuf,
    pub shortcut_id: u32,
    pub provider: String,
    pub size: u64,
    #[serde(default)] pub version: Option<String>,
    pub installed: u64,
    /// A repack's setup files, kept until the user deletes them.
    #[serde(default)] pub installer_dir: Option<PathBuf>,
    /// Release name of the source it came from, for update checks.
    #[serde(default)] pub source_name: Option<String>,
    /// When the source last changed the release it was installed from (SteamRIP's uploadDate), for update checks.
    #[serde(default)] pub source_updated: Option<String>,
    /// Set after Mercury moved the game: the plugin must point the Steam shortcut at the new exe, then clear it.
    #[serde(default)] pub needs_repoint: bool,
    /// Destination while a move is running.
    #[serde(default)] pub moving_to: Option<PathBuf>,
    /// Launch options the Steam shortcut should have (OnlineFix DLL overrides it lacks); the plugin sets them, then clears this.
    #[serde(default)] pub launch_options_fix: Option<String>,
}

#[derive(Default, Serialize, Deserialize)]
struct Saved { jobs: Vec<Job>, library: Vec<Entry>, next_id: u64 }

struct Live { prog: Arc<download::Progress>, stop: Arc<AtomicBool>, extract: Arc<extract::Permille> }

pub struct Manager {
    saved: Mutex<Saved>,
    live: Mutex<HashMap<u64, Live>>,
    wake: tokio::sync::Notify,
    pub http: reqwest::Client,
    pub cfg: Arc<Mutex<Config>>,
}

/// Move every file under `from` to the same place under `to`, replacing what is there. Returns the file count.
fn overlay(from: &std::path::Path, to: &std::path::Path) -> Result<usize> {
    let mut n = 0;
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)?.flatten() {
        let (src, dst) = (e.path(), to.join(e.file_name()));
        if src.is_dir() {
            if dst.exists() && !dst.is_dir() { std::fs::remove_file(&dst)?; }
            n += overlay(&src, &dst)?;
        } else {
            if dst.is_dir() { std::fs::remove_dir_all(&dst)?; }
            if std::fs::rename(&src, &dst).is_err() { std::fs::copy(&src, &dst)?; }
            n += 1;
        }
    }
    Ok(n)
}

fn now() -> u64 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) }
fn state_path() -> PathBuf { data_dir().join("state.json") }

fn slug(s: &str) -> String {
    let s: String = s.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { ' ' }).collect();
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl Manager {
    pub fn new(http: reqwest::Client, cfg: Arc<Mutex<Config>>) -> Arc<Self> {
        let mut saved: Saved = std::fs::read(state_path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        // Games installed before versions were recorded: take source name and version from their finished job.
        let done: Vec<(u32, Source)> = saved.jobs.iter().filter(|j| j.state == State::Done).map(|j| (j.appid, j.source.clone())).collect();
        for e in saved.library.iter_mut().filter(|e| e.source_name.is_none()) {
            if let Some((_, src)) = done.iter().rev().find(|(a, _)| *a == e.appid) {
                e.source_name = Some(src.name.clone());
                if e.version.is_none() { e.version = src.version.clone().or_else(|| crate::sources::version_in(&src.name)); }
            }
        }
        // Installed games whose OnlineFix never loads: their shortcut has no DLL overrides yet.
        let shortcuts = crate::shortcuts::list();
        for e in saved.library.iter_mut().filter(|e| e.launch_options_fix.is_none()) {
            let Some(sc) = shortcuts.iter().find(|s| s.appid == e.shortcut_id) else { continue };
            let want = install::launch_options(&sc.launch_options, &e.exe);
            if want != sc.launch_options { tracing::info!("{} needs OnlineFix DLL overrides", e.name); e.launch_options_fix = Some(want); }
        }
        // Anything interrupted by a restart waits for the user rather than resuming on its own.
        for j in saved.jobs.iter_mut() {
            if matches!(j.state, State::Resolving | State::Caching | State::Downloading | State::Extracting) {
                j.state = State::Paused;
            }
        }
        Arc::new(Self { saved: Mutex::new(saved), live: Mutex::new(HashMap::new()), wake: tokio::sync::Notify::new(), http, cfg })
    }

    fn save(&self, s: &Saved) {
        let _ = std::fs::create_dir_all(data_dir());
        let tmp = state_path().with_extension("tmp");
        if std::fs::write(&tmp, serde_json::to_vec_pretty(s).unwrap_or_default()).is_ok() {
            let _ = std::fs::rename(tmp, state_path());
        }
    }

    fn update(&self, id: u64, f: impl FnOnce(&mut Job)) {
        let mut s = self.saved.lock().unwrap();
        if let Some(j) = s.jobs.iter_mut().find(|j| j.id == id) { f(j); }
        self.save(&s);
    }

    fn get(&self, id: u64) -> Option<Job> { self.saved.lock().unwrap().jobs.iter().find(|j| j.id == id).cloned() }

    pub fn jobs(&self) -> Vec<Job> {
        let s = self.saved.lock().unwrap();
        let live = self.live.lock().unwrap();
        s.jobs.iter().cloned().map(|mut j| {
            if let Some(l) = live.get(&j.id) {
                if j.state == State::Extracting {
                    // Reported as thousandths so the UI shows a real bar while 7z or unrar runs.
                    j.done = l.extract.load(Ordering::Relaxed);
                    j.total = 1000;
                    j.speed = 0;
                } else {
                    j.done = l.prog.done.load(Ordering::Relaxed);
                    j.total = j.total.max(l.prog.total.load(Ordering::Relaxed));
                    j.speed = l.prog.speed.load(Ordering::Relaxed);
                }
            }
            j
        }).collect()
    }

    pub fn library(&self) -> Vec<Entry> { self.saved.lock().unwrap().library.clone() }

    pub fn enqueue(&self, appid: u32, name: String, source: Source, local_files: Vec<PathBuf>, update: bool) -> Result<Job> {
        self.enqueue_with(appid, name, source, local_files, update, |_| {})
    }

    /// `setup` runs before the job is visible to the workers, so they never see it half-configured.
    fn enqueue_with(&self, appid: u32, name: String, source: Source, local_files: Vec<PathBuf>, update: bool, setup: impl FnOnce(&mut Job)) -> Result<Job> {
        let mut s = self.saved.lock().unwrap();
        if s.jobs.iter().any(|j| j.appid == appid && j.state.active()) {
            bail!("{name} is already in the queue");
        }
        let installed = s.library.iter().find(|e| e.appid == appid).cloned();
        if update && installed.is_none() { bail!("{name} is not installed, so there is nothing to update"); }
        if !update && installed.is_some() { bail!("{name} is already installed. Use Update to replace its files."); }
        s.next_id += 1;
        let job = Job { id: s.next_id, appid, name, source, state: State::Queued, error: None, torrent_id: None, links: vec![], local_files: vec![], dir: None,
            setup_exe: None, exe: None, candidates: vec![], shortcut_id: None, setup_started: None, update_of: None, import_dir: None, keep_in_place: false, cache_progress: 0.0, done: 0, total: 0, speed: 0, created: now() };
        let mut job = job;
        job.local_files = local_files;
        if let Some(e) = installed.filter(|_| update) {
            job.update_of = Some(appid);
            // Reuse the game's shortcut: same Steam entry, same Proton prefix, so saves and settings stay.
            job.shortcut_id = Some(e.shortcut_id);
        }
        setup(&mut job);
        s.jobs.push(job.clone());
        self.save(&s);
        drop(s);
        self.wake.notify_waiters();
        self.wake.notify_one();
        Ok(job)
    }

    pub fn pause(&self, id: u64) {
        if let Some(l) = self.live.lock().unwrap().get(&id) { l.stop.store(true, Ordering::Relaxed); }
        self.update(id, |j| if matches!(j.state, State::Queued | State::Resolving | State::Caching | State::Downloading) { j.state = State::Paused });
    }

    pub fn resume(&self, id: u64) {
        self.update(id, |j| if matches!(j.state, State::Paused | State::Failed) { j.state = State::Queued; j.error = None });
        self.wake.notify_one();
    }

    pub async fn cancel(&self, id: u64) {
        if let Some(l) = self.live.lock().unwrap().get(&id) { l.stop.store(true, Ordering::Relaxed); }
        let Some(job) = self.get(id) else { return };
        self.update(id, |j| j.state = State::Cancelled);
        let cfg = self.cfg.lock().unwrap().clone();
        let _ = tokio::fs::remove_dir_all(cfg.downloads_dir.join(id.to_string())).await;
        if let Some(d) = &job.dir { let _ = tokio::fs::remove_dir_all(d).await; }
        if let (Some(t), Ok(rd)) = (&job.torrent_id, Rd::new(self.http.clone(), &cfg.rd_key)) { let _ = rd.delete(t).await; }
        let mut s = self.saved.lock().unwrap();
        s.jobs.retain(|j| j.id != id);
        self.save(&s);
    }

    /// The plugin launched the repack's setup.exe through a Steam shortcut.
    pub fn setup_launched(&self, id: u64, shortcut_id: u32) {
        self.update(id, |j| if j.state == State::NeedsSetup { j.state = State::Installing; j.shortcut_id = Some(shortcut_id); j.setup_started = Some(now()); j.error = None });
    }

    /// The installer closed. Look for the game inside the shortcut's Proton prefix.
    /// If nothing real was installed, go back to `needs_setup` so the user can run it again.
    pub fn setup_done(&self, id: u64) -> Result<Job> {
        let job = self.get(id).ok_or_else(|| anyhow!("no such job"))?;
        let sid = job.shortcut_id.ok_or_else(|| anyhow!("installer was not launched"))?;
        let drive_c = home().join(format!(".local/share/Steam/steamapps/compatdata/{sid}/pfx/drive_c"));
        let mut cands = vec![];
        for sub in ["Games", "Program Files", "Program Files (x86)", ""] {
            let base = if sub.is_empty() { drive_c.clone() } else { drive_c.join(sub) };
            if !base.is_dir() { continue; }
            for c in install::find_game_exe(&base, &job.name) {
                if c.score <= -500 || install::is_wine_dir(&c.path) || install::is_wine_stub(&c.path) { continue; }
                // ctime is set when the file is created and cannot be backdated by an installer (unlike mtime).
                #[cfg(unix)]
                let created = std::fs::metadata(&c.path).map(|m| std::os::unix::fs::MetadataExt::ctime(&m) as u64).unwrap_or(0);
                #[cfg(not(unix))]
                let created = u64::MAX;
                if job.setup_started.is_some_and(|t| created + 5 < t) { continue; }
                cands.push(c.path);
            }
            if !cands.is_empty() { break; }
        }
        if cands.is_empty() {
            let msg = "The installer closed without installing the game. Run it again; if it fails the same way, its Proton log is in ~/steam-<id>.log on the Odin.".to_string();
            self.update(id, |j| { j.state = State::NeedsSetup; j.error = Some(msg.clone()) });
            bail!("{msg}");
        }
        // An update keeps its Steam shortcut, so there is nothing to review.
        let st = if job.update_of.is_some() { State::Ready } else { self.installed_state() };
        self.update(id, |j| { j.exe = Some(cands[0].clone()); j.candidates = cands.into_iter().take(8).collect(); j.state = st; j.error = None });
        Ok(self.get(id).unwrap())
    }

    /// State for a newly installed game: the Odin plugin adds `ready` jobs to Steam on its own; with `review_art` it
    /// waits in `review` until the user confirms the title and artwork. (Windows decides in `add_to_library`.)
    fn installed_state(&self) -> State {
        if cfg!(unix) && self.cfg.lock().unwrap().review_art { State::Review } else { State::Ready }
    }

    /// Add a game folder that is already installed somewhere else.
    pub fn enqueue_import(&self, appid: u32, name: String, dir: PathBuf, keep_in_place: bool) -> Result<Job> {
        let source = Source { name: dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), provider: "Imported".into(),
            size: String::new(), size_bytes: 0, magnet: None, url: None, version: None, urls: vec![], supported: true, repack: false, declared: false, updated: None };
        self.enqueue_with(appid, name, source, vec![], false, |j| { j.import_dir = Some(dir); j.keep_in_place = keep_in_place })
    }

    /// Steam made the shortcut; the plugin is still configuring it. A retry reuses it instead of adding a duplicate.
    pub fn shortcut_created(&self, id: u64, shortcut_id: u32) {
        self.update(id, |j| if matches!(j.state, State::Ready | State::Review) { j.shortcut_id = Some(shortcut_id) });
    }

    /// The plugin created (or repointed) the Steam shortcut. The game is installed.
    /// `name`: the title the user chose on the Review screen.
    pub fn steam_added(&self, id: u64, shortcut_id: u32, exe: Option<PathBuf>, name: Option<String>) -> Result<()> {
        let mut job = self.get(id).ok_or_else(|| anyhow!("no such job"))?;
        if let Some(n) = name.filter(|n| !n.trim().is_empty()) { job.name = n.trim().to_string(); }
        let exe = exe.or(job.exe.clone()).ok_or_else(|| anyhow!("no exe"))?;
        let repack = job.setup_exe.is_some();
        let old = self.saved.lock().unwrap().library.iter().find(|e| e.appid == job.appid).cloned();
        let dir = match (&old, repack) {
            (Some(e), _) if job.update_of.is_some() => e.dir.clone(),
            (_, true) => exe.parent().map(|p| p.to_path_buf()).unwrap_or_default(),
            _ => job.dir.clone().unwrap_or_default(),
        };
        let version = job.source.version.clone().or_else(|| crate::sources::version_in(&job.source.name));
        let entry = Entry { appid: job.appid, name: job.name.clone(), size: storage::dir_size(&dir), dir, exe: exe.clone(), shortcut_id,
            provider: job.source.provider.clone(), version, installed: now(),
            installer_dir: if repack { job.dir.clone() } else { old.and_then(|e| e.installer_dir) },
            source_name: Some(job.source.name.clone()), source_updated: job.source.updated.clone(), needs_repoint: false, moving_to: None, launch_options_fix: None };
        {
            let mut s = self.saved.lock().unwrap();
            s.library.retain(|e| e.appid != job.appid);
            s.library.push(entry);
            if let Some(j) = s.jobs.iter_mut().find(|j| j.id == id) { j.state = State::Done; j.shortcut_id = Some(shortcut_id); j.exe = Some(exe); }
            self.save(&s);
        }
        // A repack's installer files stay until the user deletes them from the game page.
        let _ = repack;
        Ok(())
    }

    /// Windows: run a repack's setup.exe into games_dir/<name>, wait for it to close, then find the game and add it.
    /// By default it runs as the normal user (RunAsInvoker): no admin prompt, and the controller can drive its window
    /// (`assist`). `admin` runs it elevated instead, for installers that fail without it; the controller cannot drive those.
    #[cfg(windows)]
    pub async fn run_setup(self: Arc<Self>, id: u64, admin: bool) -> Result<()> {
        let job = self.get(id).ok_or_else(|| anyhow!("no such job"))?;
        if job.state != State::NeedsSetup { bail!("{} is not waiting for its installer", job.name); }
        let setup = job.setup_exe.clone().ok_or_else(|| anyhow!("no installer"))?;
        let target = self.cfg.lock().unwrap().games_dir.join(slug(&job.name));
        self.update(id, |j| { j.state = State::Installing; j.setup_started = Some(now()); j.error = None });
        let me = self.clone();
        tokio::spawn(async move {
            let res: Result<()> = async {
                let _assist = crate::assist::spawn(setup.parent().map(|p| p.to_path_buf()).unwrap_or_default());
                if admin {
                    // Start-Process -Verb RunAs shows the UAC prompt; -Wait returns when the installer closes.
                    let q = |p: &std::path::Path| p.display().to_string().replace('\'', "''");
                    let script = format!("Start-Process -FilePath '{}' -WorkingDirectory '{}' -ArgumentList '/DIR=\"{}\"' -Verb RunAs -Wait",
                        q(&setup), q(setup.parent().unwrap()), q(&target));
                    let mut c = tokio::process::Command::new("powershell");
                    c.args(["-NoProfile", "-NonInteractive", "-Command", &script]);
                    extract::quiet(&mut c);
                    let out = c.output().await?;
                    if !out.status.success() {
                        let e = String::from_utf8_lossy(&out.stderr);
                        bail!("the installer did not run{}", if e.contains("canceled by the user") { " (the admin prompt was declined)" } else { "" });
                    }
                } else {
                    let mut c = tokio::process::Command::new(&setup);
                    c.current_dir(setup.parent().unwrap()).env("__COMPAT_LAYER", "RunAsInvoker");
                    c.raw_arg(format!("/DIR=\"{}\"", target.display()));
                    let mut child = c.spawn().map_err(|e| anyhow!("the installer did not start: {e}"))?;
                    child.wait().await?;
                }
                let cands = install::find_game_exe(&target, &job.name);
                let exe = cands.first().filter(|c| c.score > -500).map(|c| c.path.clone())
                    .ok_or_else(|| anyhow!("The installer closed without installing the game. Run it again."))?;
                me.update(id, |j| { j.exe = Some(exe); j.candidates = cands.into_iter().take(8).map(|c| c.path).collect(); j.state = State::Ready });
                me.add_to_library(id).await
            }.await;
            if let Err(e) = res {
                tracing::warn!("job {id} installer: {e:#}");
                me.update(id, |j| { j.state = State::NeedsSetup; j.error = Some(format!("{e:#}")) });
            }
        });
        Ok(())
    }

    /// Windows: the installer was closed or finished while Mercury was not watching (it was restarted or closed).
    /// Look for the installed game in the job's folder and finish the job.
    #[cfg(windows)]
    pub async fn finish_setup(self: Arc<Self>, id: u64) -> Result<()> {
        let job = self.get(id).ok_or_else(|| anyhow!("no such job"))?;
        if !matches!(job.state, State::NeedsSetup | State::Installing) { bail!("{} is not at the installer step", job.name); }
        let target = self.cfg.lock().unwrap().games_dir.join(slug(&job.name));
        let cands = install::find_game_exe(&target, &job.name);
        let exe = cands.first().filter(|c| c.score > -500).map(|c| c.path.clone())
            .ok_or_else(|| anyhow!("No installed game found in {} yet. Finish the installer first.", target.display()))?;
        self.update(id, |j| { j.exe = Some(exe); j.candidates = cands.into_iter().take(8).map(|c| c.path).collect(); j.state = State::Ready; j.error = None });
        self.add_to_library(id).await
    }

    /// Windows: record the game in Mercury's library and add it to warmUP.
    #[cfg(windows)]
    async fn add_to_library(&self, id: u64) -> Result<()> {
        let (launcher, review) = { let c = self.cfg.lock().unwrap(); (c.launcher.clone(), c.review_art) };
        if launcher != "warmup" && review {
            // Wait for the user to confirm the title and artwork (the Review screen), then `confirm_review` finishes the job.
            self.update(id, |j| { j.state = State::Review; j.error = None });
            return Ok(());
        }
        self.finalize_add(id, None, Default::default()).await
    }

    /// The user confirmed the Review screen: add the game to Steam with the chosen title and artwork.
    #[cfg(windows)]
    pub async fn confirm_review(self: Arc<Self>, id: u64, name: Option<String>, art: HashMap<String, String>) -> Result<()> {
        if self.get(id).ok_or_else(|| anyhow!("no such job"))?.state != State::Review { bail!("this game is not waiting for review"); }
        self.finalize_add(id, name, art).await
    }

    #[cfg(windows)]
    async fn finalize_add(&self, id: u64, name_override: Option<String>, choices: HashMap<String, String>) -> Result<()> {
        let job = self.get(id).ok_or_else(|| anyhow!("no such job"))?;
        let exe = job.exe.clone().ok_or_else(|| anyhow!("no exe"))?;
        let repack = job.setup_exe.is_some();
        let dir = if repack { self.cfg.lock().unwrap().games_dir.join(slug(&job.name)) } else { job.dir.clone().unwrap_or_default() };
        let app = crate::steam::details(&self.http, job.appid).await.ok();
        let name: String = name_override.map(|n| n.trim().to_string()).filter(|n| !n.is_empty())
            .unwrap_or_else(|| job.name.chars().filter(|c| !matches!(c, '\u{2122}' | '\u{00ae}' | '\u{00a9}')).collect::<String>().trim().to_string());
        let launcher = self.cfg.lock().unwrap().launcher.clone();
        let mut shortcut_id = 0u32;
        let added: Result<()> = if launcher == "warmup" {
            let r = crate::warmup::add(&exe, &dir, &name, app.as_ref(), job.appid);
            if let Err(e) = &r { tracing::warn!("warmUP: could not add {name}: {e:#}"); }
            r.map(|_| ()).map_err(|e| anyhow!("Installed, but not added to warmUP: {e:#}"))
        } else {
            let r: Result<u32> = async {
                // A reinstall replaces the old shortcut instead of leaving two.
                let old = self.saved.lock().unwrap().library.iter().find(|e| e.appid == job.appid).map(|e| e.shortcut_id).unwrap_or(0);
                let _ = crate::steamwin::remove_shortcut(&self.http, old).await;
                let art = crate::sgdb::resolve(&self.http, job.appid, &choices).await?;
                crate::steamwin::add_shortcut(&self.http, &name, &exe, &art).await
            }.await;
            match r {
                Ok(id) => { shortcut_id = id; Ok(()) }
                Err(e) => { tracing::warn!("Steam: could not add {name}: {e:#}"); Err(anyhow!("Installed, but not added to Steam: {e:#}")) }
            }
        };
        let entry = Entry { appid: job.appid, name: name.clone(), size: storage::dir_size(&dir), dir, exe: exe.clone(), shortcut_id,
            provider: job.source.provider.clone(), version: job.source.version.clone(), installed: now(),
            installer_dir: if repack { job.dir.clone() } else { None },
            source_name: Some(job.source.name.clone()), source_updated: job.source.updated.clone(), needs_repoint: false, moving_to: None, launch_options_fix: None };
        let mut s = self.saved.lock().unwrap();
        s.library.retain(|e| e.appid != job.appid);
        s.library.push(entry);
        if let Some(j) = s.jobs.iter_mut().find(|j| j.id == id) {
            j.state = State::Done;
            j.shortcut_id = if shortcut_id != 0 { Some(shortcut_id) } else { None };
            j.error = added.err().map(|e| format!("{e:#}"));
        }
        self.save(&s);
        Ok(())
    }

    pub fn set_steam_self(&self, id: u32) {
        let mut c = self.cfg.lock().unwrap();
        c.steam_self_id = Some(id);
        let _ = c.save();
    }

    /// Installer files of a repack that is already installed, and their size.
    pub fn installer_files(&self, appid: u32) -> Option<(PathBuf, u64)> {
        let s = self.saved.lock().unwrap();
        let d = s.library.iter().find(|e| e.appid == appid)?.installer_dir.clone().filter(|d| d.is_dir())?;
        let n = storage::dir_size(&d);
        Some((d, n))
    }

    pub fn delete_installer_files(&self, appid: u32) -> Result<u64> {
        let (d, n) = self.installer_files(appid).ok_or_else(|| anyhow!("no installer files"))?;
        std::fs::remove_dir_all(&d)?;
        let mut s = self.saved.lock().unwrap();
        if let Some(e) = s.library.iter_mut().find(|e| e.appid == appid) { e.installer_dir = None; }
        self.save(&s);
        Ok(n)
    }

    /// Remove a finished, failed or cancelled job from the list, and delete what it left behind.
    /// An installed game's files are never touched.
    pub async fn remove(&self, id: u64) -> Result<()> {
        let (job, installed_dirs) = {
            let s = self.saved.lock().unwrap();
            let job = s.jobs.iter().find(|j| j.id == id).cloned().ok_or_else(|| anyhow!("no such job"))?;
            if job.state.active() { bail!("{} is still in progress. Cancel it first.", job.name); }
            let dirs: Vec<PathBuf> = s.library.iter().flat_map(|e| [Some(e.dir.clone()), e.installer_dir.clone()]).flatten().collect();
            (job, dirs)
        };
        let cfg = self.cfg.lock().unwrap().clone();
        let _ = tokio::fs::remove_dir_all(cfg.downloads_dir.join(id.to_string())).await;
        if job.state != State::Done {
            if let Some(d) = &job.dir {
                if !installed_dirs.iter().any(|x| x == d) { let _ = tokio::fs::remove_dir_all(d).await; }
            }
        }
        let mut s = self.saved.lock().unwrap();
        s.jobs.retain(|j| j.id != id);
        self.save(&s);
        Ok(())
    }

    /// Clear every job that is not in progress. Returns how many were cleared.
    pub async fn clear_finished(&self) -> usize {
        let ids: Vec<u64> = self.saved.lock().unwrap().jobs.iter().filter(|j| !j.state.active()).map(|j| j.id).collect();
        let mut n = 0;
        for id in ids { if self.remove(id).await.is_ok() { n += 1; } }
        n
    }

    /// Copy the freshly extracted files over the installed game. The new game folder (the one holding its exe)
    /// lands on the old game folder, so a layout change between releases does not leave a second copy.
    async fn apply_update(&self, id: u64, appid: u32, staged: &std::path::Path) -> Result<()> {
        let job = self.get(id).ok_or_else(|| anyhow!("job vanished"))?;
        let entry = self.library().into_iter().find(|e| e.appid == appid).ok_or_else(|| anyhow!("{} is no longer installed", job.name))?;
        let new_exe = install::find_game_exe(staged, &job.name).into_iter().find(|c| c.score > -500)
            .ok_or_else(|| anyhow!("No game .exe in the new files; the installed game was not changed"))?.path;
        let new_root = new_exe.parent().unwrap_or(staged).to_path_buf();
        let old_root = entry.exe.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| entry.dir.clone());
        let moved = tokio::task::spawn_blocking({
            let (from, to) = (new_root.clone(), old_root.clone());
            move || overlay(&from, &to)
        }).await??;
        tracing::info!("update {}: {moved} files replaced in {}", job.name, old_root.display());
        let _ = tokio::fs::remove_dir_all(staged).await;
        let exe = old_root.join(new_exe.strip_prefix(&new_root).unwrap_or(&new_exe));
        self.update(id, |j| { j.dir = Some(entry.dir.clone()); j.exe = Some(exe.clone()); j.candidates = vec![exe.clone()]; j.state = State::Ready });
        Ok(())
    }

    /// Record a game the user already has as a Steam shortcut, without touching the shortcut (its art, launch
    /// options and Proton prefix, which holds the saves, stay as they are).
    pub fn adopt(&self, mut e: Entry) -> Result<Entry> {
        let mut s = self.saved.lock().unwrap();
        if s.library.iter().any(|x| x.appid == e.appid) { bail!("{} is already in Mercury's library", e.name); }
        if !e.exe.is_file() { bail!("{} does not exist", e.exe.display()); }
        e.size = storage::dir_size(&e.dir);
        e.installed = now();
        s.library.push(e.clone());
        self.save(&s);
        Ok(e)
    }

    /// Move an installed game's folder to `root` (internal storage or a microSD card). Runs in the background;
    /// when done the entry points at the new place and `needs_repoint` asks the plugin to update the shortcut.
    pub fn start_move(self: &Arc<Self>, appid: u32, root: PathBuf) -> Result<Entry> {
        let e = {
            let mut s = self.saved.lock().unwrap();
            let e = s.library.iter_mut().find(|e| e.appid == appid).ok_or_else(|| anyhow!("not installed"))?;
            if e.moving_to.is_some() { bail!("{} is already moving", e.name); }
            let to = root.join(e.dir.file_name().map(|n| n.to_owned()).unwrap_or_else(|| slug(&e.name).into()));
            if to == e.dir { bail!("{} is already there", e.name); }
            if to.exists() { bail!("{} already exists", to.display()); }
            e.moving_to = Some(to);
            let out = e.clone();
            self.save(&s);
            out
        };
        let m = self.clone();
        let (from, to) = (e.dir.clone(), e.moving_to.clone().unwrap());
        tokio::task::spawn_blocking(move || {
            let res = crate::import::move_dir(&from, &to);
            let mut s = m.saved.lock().unwrap();
            if let Some(x) = s.library.iter_mut().find(|x| x.appid == appid) {
                x.moving_to = None;
                match res {
                    Ok(()) => {
                        let rel = x.exe.strip_prefix(&x.dir).map(|r| r.to_path_buf()).unwrap_or_default();
                        tracing::info!("moved {} to {}", x.name, to.display());
                        x.exe = to.join(rel);
                        x.dir = to;
                        x.needs_repoint = true;
                    }
                    Err(err) => tracing::warn!("moving {} failed: {err:#}", x.name),
                }
            }
            m.save(&s);
        });
        Ok(e)
    }

    pub fn launch_options_set(&self, appid: u32) {
        let mut s = self.saved.lock().unwrap();
        if let Some(e) = s.library.iter_mut().find(|e| e.appid == appid) { e.launch_options_fix = None; }
        self.save(&s);
    }

    pub fn repointed(&self, appid: u32) {
        let mut s = self.saved.lock().unwrap();
        if let Some(e) = s.library.iter_mut().find(|e| e.appid == appid) { e.needs_repoint = false; }
        self.save(&s);
    }

    /// The game's Steam shortcut was re-created (after the user removed it in Steam).
    pub fn set_shortcut(&self, appid: u32, shortcut_id: u32) -> Result<Entry> {
        let mut s = self.saved.lock().unwrap();
        let e = s.library.iter_mut().find(|e| e.appid == appid).ok_or_else(|| anyhow!("not installed"))?;
        e.shortcut_id = shortcut_id;
        let out = e.clone();
        self.save(&s);
        Ok(out)
    }

    pub fn uninstall(&self, appid: u32) -> Result<Entry> {
        let mut s = self.saved.lock().unwrap();
        let i = s.library.iter().position(|e| e.appid == appid).ok_or_else(|| anyhow!("not installed"))?;
        let e = s.library.remove(i);
        let repack_dirs: Vec<PathBuf> = e.installer_dir.iter().cloned().collect();
        s.jobs.retain(|j| j.appid != appid || j.state.active());
        self.save(&s);
        #[cfg(windows)]
        {
            if let Err(err) = crate::warmup::remove(&e.exe) { tracing::warn!("warmUP: could not remove {}: {err:#}", e.name); }
            let (http, sid, name) = (self.http.clone(), e.shortcut_id, e.name.clone());
            tokio::spawn(async move { if let Err(err) = crate::steamwin::remove_shortcut(&http, sid).await { tracing::warn!("Steam: could not remove {name}: {err:#}"); } });
        }
        let (dir, sid) = (e.dir.clone(), e.shortcut_id);
        std::thread::spawn(move || {
            let _ = std::fs::remove_dir_all(&dir);
            for d in repack_dirs { let _ = std::fs::remove_dir_all(d); }
            let _ = std::fs::remove_dir_all(home().join(format!(".local/share/Steam/steamapps/compatdata/{sid}")));
            // Steam leaves a removed shortcut's custom art behind in every user's grid folder.
            if let Ok(users) = std::fs::read_dir(home().join(".local/share/Steam/userdata")) {
                for u in users.flatten() {
                    if let Ok(files) = std::fs::read_dir(u.path().join("config/grid")) {
                        for f in files.flatten() {
                            let n = f.file_name().to_string_lossy().to_string();
                            if n.starts_with(&sid.to_string()) && n[sid.to_string().len()..].starts_with(['.', '_', 'p']) {
                                let _ = std::fs::remove_file(f.path());
                            }
                        }
                    }
                }
            }
        });
        Ok(e)
    }

    /// Start the workers. Each claims the oldest queued job under the lock, so two never take the same one.
    pub fn start(self: &Arc<Self>) {
        let n = self.cfg.lock().unwrap().parallel_jobs.clamp(1, 4);
        for _ in 0..n { tokio::spawn(self.clone().run()); }
    }

    fn claim(&self) -> Option<u64> {
        let mut s = self.saved.lock().unwrap();
        let j = s.jobs.iter_mut().find(|j| j.state == State::Queued)?;
        j.state = State::Resolving;
        let id = j.id;
        self.save(&s);
        Some(id)
    }

    async fn run(self: Arc<Self>) {
        loop {
            let Some(id) = self.claim() else { self.wake.notified().await; continue };
            let prog = Arc::new(download::Progress::default());
            let stop = Arc::new(AtomicBool::new(false));
            let extract = Arc::new(extract::Permille::new(0));
            self.live.lock().unwrap().insert(id, Live { prog: prog.clone(), stop: stop.clone(), extract: extract.clone() });
            let res = self.process(id, &prog, &stop, &extract).await;
            let (done, total) = (prog.done.load(Ordering::Relaxed), prog.total.load(Ordering::Relaxed));
            self.live.lock().unwrap().remove(&id);
            self.wake.notify_one();
            match res {
                Ok(()) => self.update(id, |j| { j.done = done; j.total = j.total.max(total); j.speed = 0 }),
                Err(e) => {
                    tracing::warn!("job {id}: {e:#}");
                    self.update(id, |j| if j.state.active() && j.state != State::Paused { j.state = State::Failed; j.error = Some(format!("{e:#}")) });
                }
            }
        }
    }

    fn stopped(&self, id: u64, stop: &AtomicBool) -> bool {
        stop.load(Ordering::Relaxed) || self.get(id).is_none_or(|j| !matches!(j.state, State::Queued | State::Resolving | State::Caching | State::Downloading | State::Extracting))
    }

    async fn process(&self, id: u64, prog: &Arc<download::Progress>, stop: &Arc<AtomicBool>, extract: &Arc<extract::Permille>) -> Result<()> {
        let cfg = self.cfg.lock().unwrap().clone();
        let mut job = self.get(id).ok_or_else(|| anyhow!("job vanished"))?;
        let dl_dir = cfg.downloads_dir.join(id.to_string());

        if let Some(src) = job.import_dir.clone() {
            self.update(id, |j| j.state = State::Extracting);
            let dir = if job.keep_in_place { src.clone() } else {
                let to = cfg.games_dir.join(slug(&job.name));
                if to.exists() { bail!("{} already exists; remove it or keep the game where it is", to.display()); }
                tokio::task::spawn_blocking({ let (a, b) = (src.clone(), to.clone()); move || crate::import::move_dir(&a, &b) }).await??;
                to
            };
            if let Some(setup) = install::find_setup(&dir) {
                self.update(id, |j| { j.dir = Some(dir.clone()); j.setup_exe = Some(setup); j.state = State::NeedsSetup });
                return Ok(());
            }
            let cands = install::find_game_exe(&dir, &job.name);
            let exe = cands.first().filter(|c| c.score > -500).map(|c| c.path.clone()).ok_or_else(|| anyhow!("No game .exe found in {}", dir.display()))?;
            let st = self.installed_state();
            self.update(id, |j| { j.dir = Some(dir.clone()); j.exe = Some(exe); j.candidates = cands.into_iter().take(8).map(|c| c.path).collect(); j.state = st });
            return Ok(());
        }
        if !job.local_files.is_empty() {
            // Browser download: take the user's file(s) out of ~/Downloads and go straight to extraction.
            tokio::fs::create_dir_all(&dl_dir).await?;
            for f in &job.local_files {
                if !f.exists() { continue; }
                let to = dl_dir.join(f.file_name().unwrap_or_default());
                if tokio::fs::rename(f, &to).await.is_err() {
                    tokio::fs::copy(f, &to).await?;
                    let _ = tokio::fs::remove_file(f).await;
                }
            }
            return self.finish(id, &cfg, None, &dl_dir, extract).await;
        }
        let rd = Rd::new(self.http.clone(), &cfg.rd_key)?;

        if job.links.is_empty() {
            self.update(id, |j| j.state = State::Resolving);
            let hoster_links = if let Some(magnet) = &job.source.magnet {
                let tid = match &job.torrent_id { Some(t) => t.clone(), None => {
                    let t = rd.add_magnet(magnet).await?;
                    rd.select_all(&t).await?;
                    self.update(id, |j| j.torrent_id = Some(t.clone()));
                    t
                }};
                self.update(id, |j| j.state = State::Caching);
                loop {
                    if self.stopped(id, stop) { return Ok(()); }
                    let info = rd.info(&tid).await?;
                    match info.status.as_str() {
                        "downloaded" => break info.links,
                        "magnet_error" | "error" | "virus" | "dead" => bail!("Real-Debrid could not get this torrent ({})", info.status),
                        "waiting_files_selection" => rd.select_all(&tid).await?,
                        _ => {}
                    }
                    self.update(id, |j| j.cache_progress = info.progress);
                    tokio::time::sleep(Duration::from_secs(3)).await;
                }
            } else if !job.source.urls.is_empty() || job.source.url.is_some() {
                let mut mirrors = job.source.urls.clone();
                if let Some(u) = &job.source.url { if !mirrors.contains(u) { mirrors.insert(0, u.clone()); } }
                let mut last = anyhow!("no mirror");
                let mut ok = None;
                for m in mirrors {
                    match rd.unrestrict(&m).await {
                        Ok(_) => { ok = Some(m); break; }
                        Err(e) => last = e,
                    }
                }
                vec![ok.ok_or_else(|| anyhow!("Real-Debrid cannot download from this source's hosts: {last}"))?]
            } else {
                bail!("source has no magnet or link");
            };
            let mut links = vec![];
            for l in hoster_links {
                let u = rd.unrestrict(&l).await.map_err(|e| anyhow!("{e} (link: {l})"))?;
                links.push(Link { url: u.download, filename: u.filename, size: u.filesize });
            }
            let total: u64 = links.iter().map(|l| l.size).sum();
            let need = total + if job.source.magnet.is_some() && job.source.provider != "OnlineFix" { storage::estimate_installed(total) } else { total + total / 7 };
            let free = storage::of(&cfg.games_dir).free;
            if need > free {
                bail!("Not enough space: needs about {} GB, {} GB free", need >> 30, free >> 30);
            }
            self.update(id, |j| { j.links = links.clone(); j.total = total });
            job = self.get(id).unwrap();
        }

        tokio::fs::create_dir_all(&dl_dir).await?;
        self.update(id, |j| j.state = State::Downloading);
        prog.total.store(job.links.iter().map(|l| l.size).sum(), Ordering::Relaxed);
        let mut on_disk = 0;
        for l in &job.links {
            on_disk += tokio::fs::metadata(dl_dir.join(&l.filename)).await.map(|m| m.len().min(l.size.max(m.len()))).unwrap_or(0);
        }
        prog.done.store(on_disk, Ordering::Relaxed);
        for l in &job.links {
            let dest = dl_dir.join(&l.filename);
            let have = tokio::fs::metadata(&dest).await.map(|m| m.len()).unwrap_or(0);
            if l.size > 0 && have >= l.size { continue; }
            match download::fetch(&self.http, &l.url, &dest, prog, stop).await? {
                download::Outcome::Stopped => return Ok(()),
                download::Outcome::Finished => {}
            }
        }
        if self.stopped(id, stop) { return Ok(()); }
        self.finish(id, &cfg, Some(&rd), &dl_dir, extract).await
    }

    /// Extract what is in `dl_dir`, then decide between a repack installer and a ready game.
    async fn finish(&self, id: u64, cfg: &Config, rd: Option<&Rd>, dl_dir: &std::path::Path, extract: &extract::Permille) -> Result<()> {
        let job = self.get(id).ok_or_else(|| anyhow!("job vanished"))?;
        self.update(id, |j| j.state = State::Extracting);
        // Updates extract next to the game first; the installed copy is untouched until this succeeds.
        let dir = if job.update_of.is_some() { cfg.games_dir.join(".staging").join(id.to_string()) } else { cfg.games_dir.join(slug(&job.name)) };
        if dir.exists() { tokio::fs::remove_dir_all(&dir).await?; }
        extract::extract_all(dl_dir, &dir, Some(extract)).await?;
        let _ = tokio::fs::remove_dir_all(dl_dir).await;
        if let (Some(t), Some(rd)) = (&job.torrent_id, rd) { let _ = rd.delete(t).await; }

        if let Some(appid) = job.update_of {
            if install::find_setup(&dir).is_none() {
                return self.apply_update(id, appid, &dir).await;
            }
        }

        if let Some(setup) = install::find_setup(&dir) {
            // On Windows the installer writes the game to games_dir/<name>, so its own files move aside first.
            #[cfg(windows)]
            let (dir, setup) = {
                let staged = cfg.games_dir.join(".mercury-repacks").join(slug(&job.name));
                if staged.exists() { tokio::fs::remove_dir_all(&staged).await?; }
                tokio::fs::create_dir_all(staged.parent().unwrap()).await?;
                tokio::fs::rename(&dir, &staged).await?;
                let setup = staged.join(setup.file_name().unwrap());
                (staged, setup)
            };
            self.update(id, |j| { j.dir = Some(dir.clone()); j.setup_exe = Some(setup); j.state = State::NeedsSetup });
            return Ok(());
        }
        let cands = install::find_game_exe(&dir, &job.name);
        let exe = cands.first().filter(|c| c.score > -500).map(|c| c.path.clone()).ok_or_else(|| anyhow!("No game .exe found after extracting"))?;
        let st = self.installed_state();
        self.update(id, |j| { j.dir = Some(dir.clone()); j.exe = Some(exe); j.candidates = cands.into_iter().take(8).map(|c| c.path).collect(); j.state = st });
        #[cfg(windows)]
        self.add_to_library(id).await?;
        Ok(())
    }
}
