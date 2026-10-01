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
pub enum State { Queued, Resolving, Caching, Downloading, Paused, Extracting, NeedsSetup, Installing, Ready, Done, Failed, Cancelled }

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
    #[serde(default)] pub dir: Option<PathBuf>,
    #[serde(default)] pub setup_exe: Option<PathBuf>,
    #[serde(default)] pub exe: Option<PathBuf>,
    #[serde(default)] pub candidates: Vec<PathBuf>,
    #[serde(default)] pub shortcut_id: Option<u32>,
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
}

#[derive(Default, Serialize, Deserialize)]
struct Saved { jobs: Vec<Job>, library: Vec<Entry>, next_id: u64 }

struct Live { prog: Arc<download::Progress>, stop: Arc<AtomicBool> }

pub struct Manager {
    saved: Mutex<Saved>,
    live: Mutex<HashMap<u64, Live>>,
    wake: tokio::sync::Notify,
    pub http: reqwest::Client,
    pub cfg: Arc<Mutex<Config>>,
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
        s.jobs.iter().filter(|j| j.state.active() || now() - j.created < 86400).cloned().map(|mut j| {
            if let Some(l) = live.get(&j.id) {
                j.done = l.prog.done.load(Ordering::Relaxed);
                j.total = j.total.max(l.prog.total.load(Ordering::Relaxed));
                j.speed = l.prog.speed.load(Ordering::Relaxed);
            }
            j
        }).collect()
    }

    pub fn library(&self) -> Vec<Entry> { self.saved.lock().unwrap().library.clone() }

    pub fn enqueue(&self, appid: u32, name: String, source: Source) -> Result<Job> {
        let mut s = self.saved.lock().unwrap();
        if s.jobs.iter().any(|j| j.appid == appid && j.state.active()) {
            bail!("{name} is already in the queue");
        }
        s.next_id += 1;
        let job = Job { id: s.next_id, appid, name, source, state: State::Queued, error: None, torrent_id: None, links: vec![], dir: None,
            setup_exe: None, exe: None, candidates: vec![], shortcut_id: None, cache_progress: 0.0, done: 0, total: 0, speed: 0, created: now() };
        s.jobs.push(job.clone());
        self.save(&s);
        drop(s);
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
    }

    /// The plugin launched the repack's setup.exe through a Steam shortcut.
    pub fn setup_launched(&self, id: u64, shortcut_id: u32) {
        self.update(id, |j| if j.state == State::NeedsSetup { j.state = State::Installing; j.shortcut_id = Some(shortcut_id) });
    }

    /// The user finished the installer. Look for the game inside the shortcut's Proton prefix.
    pub fn setup_done(&self, id: u64) -> Result<Job> {
        let job = self.get(id).ok_or_else(|| anyhow!("no such job"))?;
        let sid = job.shortcut_id.ok_or_else(|| anyhow!("installer was not launched"))?;
        let drive_c = home().join(format!(".local/share/Steam/steamapps/compatdata/{sid}/pfx/drive_c"));
        let mut cands = vec![];
        for sub in ["Games", "Program Files", "Program Files (x86)", ""] {
            let base = if sub.is_empty() { drive_c.clone() } else { drive_c.join(sub) };
            if !base.is_dir() { continue; }
            for c in install::find_game_exe(&base, &job.name) {
                let p = c.path.to_string_lossy().to_lowercase();
                if p.contains("/windows/") || p.contains("/users/") || p.contains("/programdata/") || p.contains("common files") || p.contains("internet explorer") || p.contains("windows nt") { continue; }
                if c.score > -500 { cands.push(c.path); }
            }
            if !cands.is_empty() { break; }
        }
        if cands.is_empty() { bail!("No game found in the installer's folder yet. Finish the installer, then try again."); }
        self.update(id, |j| { j.exe = Some(cands[0].clone()); j.candidates = cands.into_iter().take(8).collect(); j.state = State::Ready });
        Ok(self.get(id).unwrap())
    }

    /// The plugin created (or repointed) the Steam shortcut. The game is installed.
    pub fn steam_added(&self, id: u64, shortcut_id: u32, exe: Option<PathBuf>) -> Result<()> {
        let job = self.get(id).ok_or_else(|| anyhow!("no such job"))?;
        let exe = exe.or(job.exe.clone()).ok_or_else(|| anyhow!("no exe"))?;
        let repack = job.setup_exe.is_some();
        let dir = if repack { exe.parent().map(|p| p.to_path_buf()).unwrap_or_default() } else { job.dir.clone().unwrap_or_default() };
        let entry = Entry { appid: job.appid, name: job.name.clone(), size: storage::dir_size(&dir), dir, exe: exe.clone(), shortcut_id,
            provider: job.source.provider.clone(), version: job.source.version.clone(), installed: now() };
        {
            let mut s = self.saved.lock().unwrap();
            s.library.retain(|e| e.appid != job.appid);
            s.library.push(entry);
            if let Some(j) = s.jobs.iter_mut().find(|j| j.id == id) { j.state = State::Done; j.shortcut_id = Some(shortcut_id); j.exe = Some(exe); }
            self.save(&s);
        }
        // A repack's own files are not needed once the installer has run.
        if repack { if let Some(d) = job.dir { std::thread::spawn(move || { let _ = std::fs::remove_dir_all(d); }); } }
        Ok(())
    }

    pub fn uninstall(&self, appid: u32) -> Result<Entry> {
        let mut s = self.saved.lock().unwrap();
        let i = s.library.iter().position(|e| e.appid == appid).ok_or_else(|| anyhow!("not installed"))?;
        let e = s.library.remove(i);
        self.save(&s);
        let (dir, sid) = (e.dir.clone(), e.shortcut_id);
        std::thread::spawn(move || {
            let _ = std::fs::remove_dir_all(&dir);
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

    pub async fn run(self: Arc<Self>) {
        loop {
            let next = self.saved.lock().unwrap().jobs.iter().find(|j| j.state == State::Queued).map(|j| j.id);
            let Some(id) = next else { self.wake.notified().await; continue };
            let prog = Arc::new(download::Progress::default());
            let stop = Arc::new(AtomicBool::new(false));
            self.live.lock().unwrap().insert(id, Live { prog: prog.clone(), stop: stop.clone() });
            let res = self.process(id, &prog, &stop).await;
            let (done, total) = (prog.done.load(Ordering::Relaxed), prog.total.load(Ordering::Relaxed));
            self.live.lock().unwrap().remove(&id);
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

    async fn process(&self, id: u64, prog: &Arc<download::Progress>, stop: &Arc<AtomicBool>) -> Result<()> {
        let cfg = self.cfg.lock().unwrap().clone();
        let rd = Rd::new(self.http.clone(), &cfg.rd_key)?;
        let mut job = self.get(id).ok_or_else(|| anyhow!("job vanished"))?;

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
            } else if let Some(url) = &job.source.url {
                vec![url.clone()]
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

        let dl_dir = cfg.downloads_dir.join(id.to_string());
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

        self.update(id, |j| j.state = State::Extracting);
        let dir = cfg.games_dir.join(slug(&job.name));
        if dir.exists() { tokio::fs::remove_dir_all(&dir).await?; }
        extract::extract_all(&dl_dir, &dir).await?;
        let _ = tokio::fs::remove_dir_all(&dl_dir).await;
        if let Some(t) = &job.torrent_id { let _ = rd.delete(t).await; }

        if let Some(setup) = install::find_setup(&dir) {
            self.update(id, |j| { j.dir = Some(dir.clone()); j.setup_exe = Some(setup); j.state = State::NeedsSetup });
            return Ok(());
        }
        let cands = install::find_game_exe(&dir, &job.name);
        let exe = cands.first().filter(|c| c.score > -500).map(|c| c.path.clone()).ok_or_else(|| anyhow!("No game .exe found after extracting"))?;
        self.update(id, |j| { j.dir = Some(dir.clone()); j.exe = Some(exe); j.candidates = cands.into_iter().take(8).map(|c| c.path).collect(); j.state = State::Ready });
        Ok(())
    }
}
