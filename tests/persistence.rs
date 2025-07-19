use download_manager::{
    model::{Download, Settings, State, Status, Theme, validate_filename, validate_url},
    storage,
};
mod support;

#[test]
fn state_survives_atomic_replacement_and_restarts_paused() {
    let dir = support::fixture_dir();
    let mut state = State {
        next_id: 3,
        settings: Settings {
            startup_update_check: false,
            concurrency: 3,
            bandwidth_kib: 125,
            close_to_tray: false,
            theme: Theme::Light,
            ..Default::default()
        },
        ..Default::default()
    };
    for (id, status) in [(1, Status::Downloading), (2, Status::Queued)] {
        state.downloads.push(Download {
            id,
            url: "https://example.test/file".into(),
            destination: dir.path().join(format!("{id}.bin")),
            status,
            bytes: 100,
            total: Some(200),
            error: None,
            speed: 0.0,
        });
    }
    storage::atomic_json(&dir.path().join("state.json"), &state).unwrap();
    state.settings.bandwidth_kib = 250;
    storage::atomic_json(&dir.path().join("state.json"), &state).unwrap();
    let restored = storage::load_state(dir.path()).unwrap();
    assert_eq!(restored.settings.bandwidth_kib, 250);
    assert_eq!(restored.settings.concurrency, 3);
    assert!(!restored.settings.startup_update_check);
    assert!(!restored.settings.close_to_tray);
    assert_eq!(restored.settings.theme, Theme::Light);
    assert!(
        restored
            .downloads
            .iter()
            .all(|j| j.status == Status::Paused && j.bytes == 100)
    );
}

#[test]
fn existing_settings_without_a_theme_use_dark_and_keep_other_preferences() {
    let dir = support::fixture_dir();
    let mut value = serde_json::to_value(State::default()).unwrap();
    value["settings"].as_object_mut().unwrap().remove("theme");
    value["settings"]["bandwidth_kib"] = 512.into();
    value["settings"]["startup_update_check"] = false.into();
    storage::atomic_json(&dir.path().join("state.json"), &value).unwrap();
    let state = storage::load_state(dir.path()).unwrap();
    assert_eq!(state.settings.theme, Theme::Dark);
    assert_eq!(state.settings.bandwidth_kib, 512);
    assert!(!state.settings.startup_update_check);
}

#[test]
fn corrupt_or_future_state_is_never_replaced_with_empty_history() {
    let dir = support::fixture_dir();
    let path = dir.path().join("state.json");
    for value in [
        b"broken JSON".as_slice(),
        br#"{"schema":9,"next_id":1,"settings":{},"downloads":[]}"#,
    ] {
        std::fs::write(&path, value).unwrap();
        assert!(storage::load_state(dir.path()).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), value);
    }
}

#[test]
fn filenames_and_urls_cannot_escape_or_target_windows_devices() {
    for name in [
        "../outside",
        "CON.txt",
        "LPT1",
        "file:stream",
        "bad.",
        "bad ",
        ".download-manager-4.part",
        ".DOWNLOAD-MANAGER-4.part",
        "",
        "NUL",
        "a\\b",
    ] {
        assert!(validate_filename(name).is_err(), "{name}");
    }
    assert!(validate_filename("archive-2026.zip").is_ok());
    for url in [
        "file:///etc/passwd",
        "ftp://example.test/a",
        "https://user:secret@example.test/",
    ] {
        assert!(validate_url(url).is_err());
    }
    assert_eq!(
        validate_url("https://example.test/file#section").unwrap(),
        "https://example.test/file"
    );
}
