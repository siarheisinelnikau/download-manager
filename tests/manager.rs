use download_manager::{
    manager::{Command, Manager},
    model::{Settings, State},
    storage,
};
use std::{
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};
mod support;

fn wait(
    manager: &Manager,
    rx: &mpsc::Receiver<()>,
    predicate: impl Fn(&download_manager::manager::Snapshot) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !predicate(&manager.snapshot()) {
        rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap();
    }
}

#[test]
fn coordinator_persists_settings_rejects_second_instance_and_shuts_down() {
    let dir = support::fixture_dir();
    let path = dir.path().to_path_buf();
    let state = State {
        settings: Settings {
            startup_update_check: false,
            ..Default::default()
        },
        ..Default::default()
    };
    storage::atomic_json(&path.join("state.json"), &state).unwrap();
    let (tx, rx) = mpsc::channel();
    let manager = Manager::start(
        path.clone(),
        Arc::new(move || {
            let _ = tx.send(());
        }),
    );
    wait(&manager, &rx, |s| s.ready);
    assert!(matches!(
        manager.snapshot().update,
        download_manager::updater::UpdateStatus::Idle
    ));
    let (tx2, rx2) = mpsc::channel();
    let other = Manager::start(
        path.clone(),
        Arc::new(move || {
            let _ = tx2.send(());
        }),
    );
    wait(&other, &rx2, |s| s.fatal.is_some());
    assert!(
        other
            .snapshot()
            .fatal
            .unwrap()
            .contains("Another DownloadManager instance")
    );
    let settings = Settings {
        concurrency: 3,
        bandwidth_kib: 50,
        startup_update_check: false,
        ..Default::default()
    };
    manager.send(Command::Settings(settings)).unwrap();
    wait(&manager, &rx, |s| s.state.settings.bandwidth_kib == 50);
    manager.send(Command::Shutdown).unwrap();
    wait(&manager, &rx, |s| s.stopped);
    let state = storage::load_state(&path).unwrap();
    assert_eq!(state.settings.concurrency, 3);
    assert!(!state.settings.startup_update_check);
}
