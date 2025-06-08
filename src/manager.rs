use crate::{
    engine::{self, Bandwidth, Outcome, Progress},
    model::{Download, Settings, State, Status, validate_destination, validate_url},
    storage,
    updater::{self, UpdateStatus},
};
use anyhow::{Context, Result, bail, ensure};
use fs2::FileExt;
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU8, Ordering},
        mpsc::{self, SyncSender},
    },
    thread,
};

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub state: State,
    pub ready: bool,
    pub fatal: Option<String>,
    pub notice: Option<String>,
    pub update: UpdateStatus,
    pub stopping: bool,
    pub stopped: bool,
    pub last_add_result: Option<(u64, Option<String>)>,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            state: State::default(),
            ready: false,
            fatal: None,
            notice: None,
            update: UpdateStatus::Idle,
            stopping: false,
            stopped: false,
            last_add_result: None,
        }
    }
}

#[derive(Debug)]
pub enum Command {
    Add {
        url: String,
        directory: PathBuf,
        filename: String,
    },
    Resume(u64),
    Pause(u64),
    Cancel(u64),
    Restart(u64),
    MoveUp(u64),
    RemoveCompleted(u64),
    PauseAll,
    ResumeAll,
    Settings(Settings),
    CheckUpdates,
    InstallUpdate,
    Shutdown,
}

enum Event {
    Command(Command),
    Progress(u64, Progress),
    Finished(u64, std::result::Result<Outcome, String>, Option<Progress>),
    Update(UpdateStatus),
}

pub struct Manager {
    tx: SyncSender<Event>,
    snapshot: Arc<Mutex<Snapshot>>,
}

impl Manager {
    pub fn start(directory: PathBuf, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let (tx, rx) = mpsc::sync_channel(64);
        let snapshot = Arc::new(Mutex::new(Snapshot::default()));
        let shared = snapshot.clone();
        let sender = tx.clone();
        thread::spawn(move || {
            if let Err(error) = supervise(directory, sender, rx, shared.clone(), wake.clone()) {
                let mut snapshot = shared.lock().unwrap();
                snapshot.fatal = Some(format!("{error:#}"));
                snapshot.ready = false;
                snapshot.stopped = true;
                drop(snapshot);
                wake();
            }
        });
        Self { tx, snapshot }
    }

    pub fn snapshot(&self) -> Snapshot {
        self.snapshot.lock().unwrap().clone()
    }

    pub fn send(&self, command: Command) -> Result<()> {
        self.tx
            .try_send(Event::Command(command))
            .context("Background command queue is busy or stopped; try again")
    }
}

fn publish(snapshot: &Snapshot, shared: &Mutex<Snapshot>, wake: &dyn Fn()) {
    *shared.lock().unwrap() = snapshot.clone();
    wake();
}

fn persist(directory: &std::path::Path, snapshot: &mut Snapshot) -> bool {
    match storage::atomic_json(&directory.join("state.json"), &snapshot.state) {
        Ok(()) => true,
        Err(error) => {
            snapshot.notice = Some(format!(
                "State was NOT saved: {error:#}. Check disk space/permissions before closing."
            ));
            false
        }
    }
}

fn supervise(
    directory: PathBuf,
    tx: SyncSender<Event>,
    rx: mpsc::Receiver<Event>,
    shared: Arc<Mutex<Snapshot>>,
    wake: Arc<dyn Fn() + Send + Sync>,
) -> Result<()> {
    fs::create_dir_all(&directory).context("Cannot create application data folder")?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(directory.join("instance.lock"))?;
    lock.try_lock_exclusive()
        .context("Another DownloadManager instance is using this data folder. Close it first.")?;
    let state = storage::load_state(&directory)?;
    let mut snapshot = Snapshot {
        state,
        ready: true,
        ..Default::default()
    };
    let error_report = directory.join("updates/last-update-error.txt");
    if error_report.exists() {
        use std::io::Read;
        let mut report = String::new();
        if let Ok(file) = std::fs::File::open(&error_report) {
            let _ = file.take(8 * 1024).read_to_string(&mut report);
            snapshot.notice = Some(format!("Previous update helper reported: {report}"));
        }
    }
    let bandwidth = Arc::new(Bandwidth::default());
    bandwidth.set_limit(snapshot.state.settings.bandwidth_kib);
    let mut active: HashMap<u64, Arc<AtomicU8>> = HashMap::new();
    let mut update_busy = false;
    let mut install_pending = None;
    if snapshot.state.settings.startup_update_check {
        update_busy = true;
        snapshot.update = UpdateStatus::Checking;
        spawn_check(tx.clone());
    }
    publish(&snapshot, &shared, &*wake);
    while let Ok(event) = rx.recv() {
        let mut dirty = false;
        let mut completed_receipt = None;
        match event {
            Event::Command(Command::Shutdown) => {
                snapshot.stopping = true;
                for control in active.values() {
                    control.store(1, Ordering::Relaxed);
                }
                for job in &mut snapshot.state.downloads {
                    if job.status == Status::Queued {
                        job.status = Status::Paused;
                    }
                    if job.status == Status::Downloading {
                        job.status = Status::Pausing;
                    }
                }
                dirty = true;
            }
            Event::Command(command) if !snapshot.stopping => {
                snapshot.notice = None;
                match command {
                    Command::CheckUpdates if !update_busy => {
                        update_busy = true;
                        snapshot.update = UpdateStatus::Checking;
                        spawn_check(tx.clone());
                    }
                    Command::InstallUpdate if !update_busy => {
                        if let UpdateStatus::Available(release) = snapshot.update.clone() {
                            update_busy = true;
                            snapshot.update = UpdateStatus::Downloading {
                                bytes: 0,
                                total: release.size,
                            };
                            let tx = tx.clone();
                            let dir = directory.clone();
                            thread::spawn(move || {
                                let result = updater::download(&release, &dir, |bytes, total| {
                                    let _ = tx.try_send(Event::Update(UpdateStatus::Downloading {
                                        bytes,
                                        total,
                                    }));
                                });
                                let status = match result {
                                    Ok(staged) => UpdateStatus::Staged(staged),
                                    Err(e) => UpdateStatus::Error(format!("{e:#}")),
                                };
                                let _ = tx.send(Event::Update(status));
                            });
                        }
                    }
                    Command::CheckUpdates | Command::InstallUpdate => {}
                    other => {
                        let adding = matches!(other, Command::Add { .. });
                        let result = apply(other, &mut snapshot.state, &active, &bandwidth);
                        if adding {
                            let sequence = snapshot
                                .last_add_result
                                .as_ref()
                                .map_or(1, |(sequence, _)| sequence + 1);
                            snapshot.last_add_result = Some((
                                sequence,
                                result.as_ref().err().map(|error| format!("{error:#}")),
                            ));
                        }
                        match result {
                            Ok(()) => dirty = true,
                            Err(error) => snapshot.notice = Some(format!("{error:#}")),
                        }
                    }
                }
            }
            Event::Progress(id, progress) => {
                if let Some(job) = snapshot.state.downloads.iter_mut().find(|j| j.id == id) {
                    apply_progress(job, progress);
                }
            }
            Event::Finished(id, result, progress) => {
                active.remove(&id);
                if let Some(job) = snapshot.state.downloads.iter_mut().find(|j| j.id == id) {
                    if let Some(progress) = progress {
                        apply_progress(job, progress);
                    }
                    job.speed = 0.0;
                    match result {
                        Ok(Outcome::Completed) => {
                            job.status = Status::Completed;
                            completed_receipt = Some(job.meta_path());
                        }
                        Ok(Outcome::Paused) => job.status = Status::Paused,
                        Ok(Outcome::Canceled) => job.status = Status::Canceled,
                        Err(error) => {
                            job.status = Status::Failed;
                            job.error = Some(error);
                        }
                    }
                    dirty = true;
                }
            }
            Event::Update(status) => {
                update_busy = matches!(
                    status,
                    UpdateStatus::Checking | UpdateStatus::Downloading { .. }
                );
                if let UpdateStatus::Staged(staged) = &status {
                    install_pending = Some(staged.clone());
                    snapshot.stopping = true;
                    for control in active.values() {
                        control.store(1, Ordering::Relaxed);
                    }
                    for job in &mut snapshot.state.downloads {
                        if job.status == Status::Queued {
                            job.status = Status::Paused;
                        }
                    }
                    dirty = true;
                }
                snapshot.update = status;
            }
            _ => {}
        }
        let saved = !dirty || persist(&directory, &mut snapshot);
        if saved && let Some(path) = completed_receipt {
            let _ = fs::remove_file(path);
        }
        if !saved {
            // Avoid creating further side effects when durable queue state is unavailable.
            for control in active.values() {
                control.store(1, Ordering::Relaxed);
            }
            for job in &mut snapshot.state.downloads {
                if job.status == Status::Queued {
                    job.status = Status::Paused;
                }
            }
        }
        if !snapshot.stopping && saved {
            for job in &mut snapshot.state.downloads {
                if active.len() >= snapshot.state.settings.concurrency {
                    break;
                }
                if job.status != Status::Queued {
                    continue;
                }
                job.status = Status::Downloading;
                job.error = None;
                let control = Arc::new(AtomicU8::new(0));
                active.insert(job.id, control.clone());
                let job = job.clone();
                let tx = tx.clone();
                let bandwidth = bandwidth.clone();
                thread::spawn(move || {
                    let mut latest = None;
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        engine::transfer(&job, control, bandwidth, |progress| {
                            latest = Some(progress.clone());
                            let _ = tx.try_send(Event::Progress(job.id, progress));
                        })
                    }));
                    let result = match result { Ok(result) => result.map_err(|e| format!("{e:#}")), Err(_) => Err("Download worker failed unexpectedly. Partial checkpoint preserved; retry after restarting.".into()) };
                    let _ = tx.send(Event::Finished(job.id, result, latest));
                });
            }
        }
        if snapshot.stopping && active.is_empty() {
            if !persist(&directory, &mut snapshot) {
                snapshot.stopping = false;
                snapshot.update = UpdateStatus::Error("Installation/exit deferred because queue state could not be saved. Fix storage and close again.".into());
                install_pending = None;
            } else if let Some(staged) = install_pending.take() {
                match updater::launch_installer(&staged) {
                    Ok(()) => snapshot.stopped = true,
                    Err(error) => {
                        snapshot.stopping = false;
                        snapshot.update = UpdateStatus::Error(format!("{error:#}"));
                    }
                }
            } else {
                snapshot.stopped = true;
            }
        }
        publish(&snapshot, &shared, &*wake);
        if snapshot.stopped {
            break;
        }
    }
    Ok(())
}

fn apply_progress(job: &mut Download, progress: Progress) {
    job.bytes = progress.bytes;
    job.total = progress.total;
    job.speed = progress.speed;
}

fn apply(
    command: Command,
    state: &mut State,
    active: &HashMap<u64, Arc<AtomicU8>>,
    bandwidth: &Bandwidth,
) -> Result<()> {
    match command {
        Command::Add {
            url,
            directory,
            filename,
        } => {
            ensure!(
                state.downloads.len() < 10_000,
                "Queue history limit reached (10,000 items)"
            );
            let url = validate_url(&url)?;
            let destination = validate_destination(&directory, &filename)?;
            ensure!(
                !destination.exists(),
                "Destination already exists; choose another filename"
            );
            ensure!(
                !state
                    .downloads
                    .iter()
                    .any(|j| j.destination.to_string_lossy().to_lowercase()
                        == destination.to_string_lossy().to_lowercase()),
                "This destination is already in the queue/history; choose another filename"
            );
            let id = state.next_id;
            state.next_id = id.checked_add(1).context("Queue ID limit reached")?;
            state.downloads.push(Download {
                id,
                url,
                destination,
                status: Status::Queued,
                bytes: 0,
                total: None,
                error: None,
                speed: 0.0,
            });
            if serde_json::to_vec_pretty(state)?.len() > 15 * 1024 * 1024 {
                state.downloads.pop();
                state.next_id = id;
                bail!(
                    "Queue storage limit reached; remove completed history entries before adding more downloads"
                );
            }
        }
        Command::Settings(mut settings) => {
            settings.concurrency = settings.concurrency.clamp(1, 4);
            settings.bandwidth_kib = settings.bandwidth_kib.min(1_048_576);
            bandwidth.set_limit(settings.bandwidth_kib);
            state.settings = settings;
        }
        Command::PauseAll => {
            for job in &mut state.downloads {
                if let Some(control) = active.get(&job.id) {
                    control.store(1, Ordering::Relaxed);
                    job.status = Status::Pausing;
                } else if job.status == Status::Queued {
                    job.status = Status::Paused;
                }
            }
        }
        Command::ResumeAll => {
            for job in &mut state.downloads {
                if !active.contains_key(&job.id) && matches!(job.status, Status::Paused) {
                    job.status = Status::Queued;
                }
            }
        }
        Command::MoveUp(id) => {
            let pos = state
                .downloads
                .iter()
                .position(|j| j.id == id)
                .context("Download no longer exists")?;
            if pos > 0 {
                state.downloads.swap(pos, pos - 1);
            }
        }
        Command::RemoveCompleted(id) => {
            let pos = state
                .downloads
                .iter()
                .position(|j| j.id == id)
                .context("Download no longer exists")?;
            ensure!(
                state.downloads[pos].status == Status::Completed && !active.contains_key(&id),
                "Only completed history entries can be removed"
            );
            state.downloads.remove(pos);
        }
        command => {
            let id = match command {
                Command::Resume(id)
                | Command::Pause(id)
                | Command::Cancel(id)
                | Command::Restart(id) => id,
                _ => bail!("Unsupported command"),
            };
            let job = state
                .downloads
                .iter_mut()
                .find(|j| j.id == id)
                .context("Download no longer exists")?;
            match command {
                Command::Pause(_) | Command::Cancel(_) => {
                    ensure!(
                        job.status != Status::Completed,
                        "Completed files are preserved"
                    );
                    let cancel = matches!(command, Command::Cancel(_));
                    if let Some(control) = active.get(&id) {
                        control.store(if cancel { 2 } else { 1 }, Ordering::Relaxed);
                        job.status = Status::Pausing;
                    } else {
                        job.status = if cancel {
                            Status::Canceled
                        } else {
                            Status::Paused
                        };
                    }
                }
                Command::Resume(_) => {
                    ensure!(
                        !active.contains_key(&id) && job.status != Status::Completed,
                        "Download is already active or complete"
                    );
                    job.status = Status::Queued;
                    job.error = None;
                }
                Command::Restart(_) => {
                    ensure!(
                        !active.contains_key(&id) && job.status != Status::Completed,
                        "Pause this download before restarting"
                    );
                    engine::restart(job)?;
                    job.bytes = 0;
                    job.total = None;
                    job.error = None;
                    job.status = Status::Queued;
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn spawn_check(tx: SyncSender<Event>) {
    thread::spawn(move || {
        let status = match updater::check() {
            Ok(status) => status,
            Err(error) => UpdateStatus::Error(format!("{error:#}")),
        };
        let _ = tx.send(Event::Update(status));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pause_all_signals_workers_and_holds_waiting_jobs() {
        let mut state = State::default();
        for (id, status) in [(1, Status::Downloading), (2, Status::Queued)] {
            state.downloads.push(Download {
                id,
                url: "https://example.test/file".into(),
                destination: PathBuf::from(format!("{id}.bin")),
                status,
                bytes: 0,
                total: None,
                error: None,
                speed: 0.0,
            });
        }
        let control = Arc::new(AtomicU8::new(0));
        let active = HashMap::from([(1, control.clone())]);
        apply(
            Command::PauseAll,
            &mut state,
            &active,
            &Bandwidth::default(),
        )
        .unwrap();
        assert_eq!(control.load(Ordering::Relaxed), 1);
        assert_eq!(state.downloads[0].status, Status::Pausing);
        assert_eq!(state.downloads[1].status, Status::Paused);
        apply(
            Command::ResumeAll,
            &mut state,
            &active,
            &Bandwidth::default(),
        )
        .unwrap();
        assert_eq!(state.downloads[0].status, Status::Pausing);
        assert_eq!(state.downloads[1].status, Status::Queued);
    }
}
