use super::{Command, MenuState, NativeTray, menu_state};
use anyhow::{Context, Result, ensure};
use download_manager::manager::Snapshot;
use gtk::glib::{self, ControlFlow, MainContext, Priority};
use std::{
    sync::{Arc, mpsc},
    thread,
};

pub struct Tray {
    updates: glib::Sender<Option<MenuState>>,
    last_state: Option<MenuState>,
    worker: Option<thread::JoinHandle<()>>,
}

impl Tray {
    pub fn new(send: Arc<dyn Fn(Command) + Send + Sync>) -> Result<Self> {
        let watcher = std::process::Command::new("gdbus")
            .args([
                "call",
                "--session",
                "--timeout",
                "2",
                "--dest",
                "org.kde.StatusNotifierWatcher",
                "--object-path",
                "/StatusNotifierWatcher",
                "--method",
                "org.freedesktop.DBus.Properties.Get",
                "org.kde.StatusNotifierWatcher",
                "IsStatusNotifierHostRegistered",
            ])
            .output()
            .context("Cannot query the desktop AppIndicator host")?;
        ensure!(
            watcher.status.success() && watcher.stdout == b"(<true>,)\n",
            "The desktop AppIndicator host is unavailable"
        );
        let (ready, init) = mpsc::sync_channel(1);
        #[allow(deprecated)]
        let (updates, receiver) = MainContext::channel(Priority::DEFAULT);
        let worker = thread::Builder::new()
            .name("appindicator".into())
            .spawn(move || {
                let tray = gtk::init()
                    .context("Cannot start GTK for the tray")
                    .and_then(|()| NativeTray::new(send));
                let mut tray = match tray {
                    Ok(tray) => tray,
                    Err(error) => {
                        let _ = ready.send(Err(error));
                        return;
                    }
                };
                let _source = receiver.attach(None, move |state| {
                    if let Some((language, enabled)) = state {
                        tray.update_menu(language, enabled);
                        ControlFlow::Continue
                    } else {
                        gtk::main_quit();
                        ControlFlow::Break
                    }
                });
                if ready.send(Ok(())).is_ok() {
                    gtk::main();
                }
            })?;
        init.recv()
            .context("AppIndicator initialization failed")??;
        Ok(Self {
            updates,
            last_state: None,
            worker: Some(worker),
        })
    }

    pub fn update(&mut self, snapshot: &Snapshot) {
        let state = menu_state(snapshot);
        if self.last_state != Some(state) && self.updates.send(Some(state)).is_ok() {
            self.last_state = Some(state);
        }
    }

    pub fn anchor(&self) -> Option<(f64, f64)> {
        None
    }

    pub fn native_window(&self) -> Option<usize> {
        None
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        let _ = self.updates.send(None);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
