use download_manager::{manager::Snapshot, model::Status};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    Popup { x: f64, y: f64 },
    Queue,
    NewDownload,
    Settings,
    PauseAll,
    ResumeAll,
    Quit,
}

pub fn tooltip(snapshot: &Snapshot) -> String {
    let active = snapshot
        .state
        .downloads
        .iter()
        .filter(|d| d.status == Status::Downloading)
        .count();
    let waiting = snapshot
        .state
        .downloads
        .iter()
        .filter(|d| d.status == Status::Queued)
        .count();
    let errors = snapshot
        .state
        .downloads
        .iter()
        .filter(|d| d.status == Status::Failed)
        .count();
    if snapshot.stopping {
        "DownloadManager · Saving downloads before exit".into()
    } else if errors > 0 {
        format!("DownloadManager · {active} active · {waiting} waiting · {errors} need attention")
    } else {
        format!("DownloadManager · {active} active · {waiting} waiting")
    }
}

#[cfg(windows)]
pub struct Tray {
    icon: tray_icon::TrayIcon,
    alive: std::sync::Arc<std::sync::atomic::AtomicBool>,
    last_tooltip: String,
    pause: tray_icon::menu::MenuItem,
    resume: tray_icon::menu::MenuItem,
    new: tray_icon::menu::MenuItem,
    last_enabled: Option<(bool, bool, bool)>,
}

#[cfg(windows)]
impl Tray {
    pub fn new(send: std::sync::Arc<dyn Fn(Command) + Send + Sync>) -> anyhow::Result<Self> {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        use tray_icon::{
            MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent,
            menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
        };
        let alive = Arc::new(AtomicBool::new(true));
        let menu = Menu::new();
        let open = MenuItem::with_id("queue", "Show downloads", true, None);
        let new = MenuItem::with_id("new", "New download", true, None);
        let pause = MenuItem::with_id("pause", "Pause all", true, None);
        let resume = MenuItem::with_id("resume", "Resume paused", true, None);
        let settings = MenuItem::with_id("settings", "Settings", true, None);
        let quit = MenuItem::with_id("quit", "Quit DownloadManager", true, None);
        menu.append_items(&[
            &open,
            &new,
            &PredefinedMenuItem::separator(),
            &pause,
            &resume,
            &settings,
            &PredefinedMenuItem::separator(),
            &quit,
        ])?;
        let icon = TrayIconBuilder::new()
            .with_id("download-manager")
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .with_tooltip("DownloadManager · Click to view downloads")
            .with_icon(tray_icon::Icon::from_rgba(crate::icons::rgba(32), 32, 32)?)
            .build()?;
        let active = alive.clone();
        let callback = send.clone();
        let skip_up = AtomicBool::new(false);
        TrayIconEvent::set_event_handler(Some(move |event| {
            if !active.load(Ordering::Relaxed) {
                return;
            }
            if matches!(
                event,
                TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                }
            ) {
                skip_up.store(true, Ordering::Relaxed);
                callback(Command::Queue);
                return;
            }
            if let TrayIconEvent::Click {
                rect,
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                if skip_up.swap(false, Ordering::Relaxed) {
                    return;
                }
                callback(Command::Popup {
                    x: rect.position.x + rect.size.width as f64 / 2.,
                    y: rect.position.y,
                });
            }
        }));
        let active = alive.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            if !active.load(Ordering::Relaxed) {
                return;
            }
            let command = match event.id.0.as_str() {
                "queue" => Command::Queue,
                "new" => Command::NewDownload,
                "pause" => Command::PauseAll,
                "resume" => Command::ResumeAll,
                "settings" => Command::Settings,
                "quit" => Command::Quit,
                _ => return,
            };
            send(command);
        }));
        Ok(Self {
            icon,
            alive,
            last_tooltip: String::new(),
            pause,
            resume,
            new,
            last_enabled: None,
        })
    }
    pub fn update(&mut self, snapshot: &Snapshot) {
        let enabled = (
            snapshot
                .state
                .downloads
                .iter()
                .any(|d| matches!(d.status, Status::Downloading | Status::Queued)),
            snapshot
                .state
                .downloads
                .iter()
                .any(|d| d.status == Status::Paused),
            snapshot.ready && !snapshot.stopping,
        );
        if self.last_enabled != Some(enabled) {
            self.pause.set_enabled(enabled.0);
            self.resume.set_enabled(enabled.1);
            self.new.set_enabled(enabled.2);
            self.last_enabled = Some(enabled);
        }
        let text = tooltip(snapshot);
        if text != self.last_tooltip && self.icon.set_tooltip(Some(&text)).is_ok() {
            self.last_tooltip = text;
        }
    }
    pub fn anchor(&self) -> Option<(f64, f64)> {
        self.icon
            .rect()
            .map(|r| (r.position.x + r.size.width as f64 / 2., r.position.y))
    }
    pub fn native_window(&self) -> Option<usize> {
        Some(self.icon.window_handle() as usize)
    }
}
#[cfg(windows)]
impl Drop for Tray {
    fn drop(&mut self) {
        self.alive
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(not(windows))]
pub struct Tray;
#[cfg(not(windows))]
impl Tray {
    pub fn new(_: std::sync::Arc<dyn Fn(Command) + Send + Sync>) -> anyhow::Result<Self> {
        anyhow::bail!("System tray is supported on Windows only")
    }
    pub fn update(&mut self, _: &Snapshot) {}
    pub fn anchor(&self) -> Option<(f64, f64)> {
        None
    }
    pub fn native_window(&self) -> Option<usize> {
        None
    }
}

/// Physical-coordinate placement, clamped to a monitor's work area, including left/top taskbars.
pub fn popup_origin(
    anchor: (f64, f64),
    size: (f64, f64),
    work: (f64, f64, f64, f64),
) -> (i32, i32) {
    let x = (anchor.0 - size.0 + 20.).clamp(work.0 + 8., (work.2 - size.0 - 8.).max(work.0 + 8.));
    let above = anchor.1 - size.1 - 10.;
    let y = if above >= work.1 + 8. {
        above
    } else {
        anchor.1 + 28.
    };
    (
        x.round() as i32,
        y.clamp(work.1 + 8., (work.3 - size.1 - 8.).max(work.1 + 8.))
            .round() as i32,
    )
}
#[cfg(windows)]
pub fn work_area(anchor: (f64, f64)) -> Option<(f64, f64, f64, f64)> {
    use windows_sys::Win32::{
        Foundation::POINT,
        Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint},
    };
    let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    let ok = unsafe {
        GetMonitorInfoW(
            MonitorFromPoint(
                POINT {
                    x: anchor.0 as i32,
                    y: anchor.1 as i32,
                },
                MONITOR_DEFAULTTONEAREST,
            ),
            &mut info,
        )
    };
    (ok != 0).then(|| {
        let r = info.rcWork;
        (r.left as f64, r.top as f64, r.right as f64, r.bottom as f64)
    })
}
#[cfg(not(windows))]
pub fn work_area(_: (f64, f64)) -> Option<(f64, f64, f64, f64)> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn popup_fits_bottom_top_and_negative_monitor_work_areas() {
        for (anchor, work) in [
            ((1900., 1040.), (0., 0., 1920., 1040.)),
            ((20., 12.), (0., 40., 1920., 1080.)),
            ((-20., 1000.), (-1920., 0., 0., 1040.)),
        ] {
            let (x, y) = popup_origin(anchor, (380., 440.), work);
            assert!(x as f64 >= work.0 && x as f64 + 380. <= work.2);
            assert!(y as f64 >= work.1 && y as f64 + 440. <= work.3);
        }
    }
    #[test]
    fn tooltip_does_not_change_for_byte_progress() {
        let a = Snapshot::default();
        let mut b = a.clone();
        b.state.next_id += 1;
        assert_eq!(tooltip(&a), tooltip(&b));
    }
}
