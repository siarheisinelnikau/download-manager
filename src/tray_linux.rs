use super::{Command, MenuState, NativeTray, menu_state};
use anyhow::{Context, Result, ensure};
use download_manager::manager::Snapshot;
use gtk::{
    gdk, gdk_pixbuf,
    glib::{self, ControlFlow, MainContext, MainLoop, Priority, translate::*},
    prelude::*,
};
use std::{
    sync::{Arc, mpsc},
    thread,
    time::Duration,
};

#[path = "tray_linux_gnome.rs"]
mod gnome;
#[path = "tray_linux_shape.rs"]
mod shape;

pub(super) struct Icon {
    xembed: Option<glib::Object>,
    indicator: Option<tray_icon::TrayIcon>,
    menu: tray_icon::menu::Menu,
    send: Arc<dyn Fn(Command) + Send + Sync>,
    gnome_bridge: bool,
}

impl Icon {
    pub(super) fn new(
        menu: tray_icon::menu::Menu,
        send: Arc<dyn Fn(Command) + Send + Sync>,
    ) -> Result<Self> {
        let xembed = Self::try_xembed(&menu, Arc::clone(&send));
        let indicator = if xembed.is_none() {
            Some(Self::app_indicator(menu.clone(), 32)?)
        } else {
            None
        };
        Ok(Self {
            xembed,
            indicator,
            menu,
            send,
            gnome_bridge: false,
        })
    }

    fn try_xembed(
        menu: &tray_icon::menu::Menu,
        send: Arc<dyn Fn(Command) + Send + Sync>,
    ) -> Option<glib::Object> {
        let x11 = gdk::Display::default()
            .is_some_and(|display| display.type_().name() == "GdkX11Display");
        if x11 {
            Self::xembed(menu, send, 32)
        } else {
            None
        }
    }

    fn set_gnome_bridge(&mut self, enabled: bool) {
        if self.gnome_bridge == enabled {
            return;
        }
        self.gnome_bridge = enabled;
        if let Err(error) = self.switch_gnome_bridge(enabled) {
            eprintln!("Cannot switch the GNOME tray integration: {error:#}");
        }
    }

    fn switch_gnome_bridge(&mut self, enabled: bool) -> Result<()> {
        if enabled {
            if let Some(indicator) = &mut self.indicator {
                indicator.set_visible(true)?;
            } else {
                self.indicator = Some(Self::app_indicator(self.menu.clone(), 32)?);
            }
            self.xembed = None;
        } else if self.xembed.is_none()
            && let Some(icon) = Self::try_xembed(&self.menu, Arc::clone(&self.send))
        {
            if let Some(indicator) = &mut self.indicator {
                indicator.set_visible(false)?;
            }
            self.xembed = Some(icon);
        }
        Ok(())
    }

    fn xembed(
        menu: &tray_icon::menu::Menu,
        send: Arc<dyn Fn(Command) + Send + Sync>,
        side: u32,
    ) -> Option<glib::Object> {
        use tray_icon::menu::ContextMenu;

        let pixbuf = gdk_pixbuf::Pixbuf::from_bytes(
            &glib::Bytes::from_owned(crate::icons::rgba(side)),
            gdk_pixbuf::Colorspace::Rgb,
            true,
            8,
            side as i32,
            side as i32,
            (side * 4) as i32,
        );
        // GTK returns one owned reference. The object and its signal handlers
        // stay on the GTK thread and are dropped before that thread exits.
        let icon: glib::Object = unsafe {
            from_glib_full(
                gtk::ffi::gtk_status_icon_new_from_pixbuf(pixbuf.to_glib_none().0).cast(),
            )
        };
        icon.set_property("title", "DownloadManager");
        if !wait_for_embedding(&icon) {
            icon.set_property("visible", false);
            return None;
        }
        shape::follow_icon(&icon);
        icon.connect_local("activate", false, move |_| {
            send(quick_downloads_command());
            None
        });
        let gtk_menu = menu.gtk_context_menu();
        gtk_menu.set_anchor_hints(
            gdk::AnchorHints::FLIP_Y | gdk::AnchorHints::SLIDE_X | gdk::AnchorHints::SLIDE_Y,
        );
        icon.connect_local("popup-menu", false, move |args| {
            if let Ok(icon) = args[0].get::<glib::Object>()
                && let Some((root, rect)) = icon_geometry(&icon)
            {
                gtk_menu.popup_at_rect(
                    &root,
                    &rect,
                    gdk::Gravity::SouthEast,
                    gdk::Gravity::NorthEast,
                    None,
                );
            } else {
                gtk_menu.popup_at_pointer(None);
            }
            None
        });
        Some(icon)
    }

    fn app_indicator(menu: tray_icon::menu::Menu, side: u32) -> Result<tray_icon::TrayIcon> {
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
        let icon = tray_icon::TrayIconBuilder::new()
            .with_id("download-manager")
            .with_temp_dir_path(download_manager::storage::data_directory()?.join("tray-icons"))
            .with_menu(Box::new(menu))
            .with_icon(tray_icon::Icon::from_rgba(
                crate::icons::rgba(side),
                side,
                side,
            )?)
            .build()?;
        Ok(icon)
    }
}

fn icon_geometry(icon: &glib::Object) -> Option<(gdk::Window, gdk::Rectangle)> {
    let mut screen = std::ptr::null_mut();
    let mut rect = gdk::Rectangle::new(0, 0, 0, 0);
    // Only called with the GtkStatusIcon that emitted the popup signal.
    unsafe {
        if gtk::ffi::gtk_status_icon_get_geometry(
            icon.as_ptr().cast(),
            &mut screen,
            rect.to_glib_none_mut().0,
            std::ptr::null_mut(),
        ) == 0
            || screen.is_null()
        {
            return None;
        }
        let screen: gdk::Screen = from_glib_none(screen);
        Some((screen.root_window()?, rect))
    }
}

pub(super) fn quick_downloads_command() -> Command {
    let anchor = gtk::current_event().and_then(|event| {
        let window = event.window()?;
        if window.display().type_().name() != "GdkX11Display" {
            return None;
        }
        let (x, y) = event.root_coords()?;
        let scale = f64::from(window.scale_factor());
        Some(Command::Popup {
            x: x * scale,
            y: y * scale,
        })
    });
    anchor
        .or_else(gnome::anchor)
        .unwrap_or(Command::QuickDownloads)
}

fn wait_for_embedding(icon: &glib::Object) -> bool {
    if icon.property::<bool>("embedded") {
        return true;
    }
    let startup = MainLoop::new(None, false);
    let on_embed = startup.clone();
    let signal = icon.connect_notify_local(Some("embedded"), move |icon, _| {
        if icon.property::<bool>("embedded") {
            on_embed.quit();
        }
    });
    let on_timeout = startup.clone();
    let deadline = glib::timeout_add_local(Duration::from_millis(350), move || {
        on_timeout.quit();
        ControlFlow::Continue
    });
    startup.run();
    deadline.remove();
    icon.disconnect(signal);
    icon.property::<bool>("embedded")
}

enum Update {
    State(MenuState),
    GnomeBridge(bool),
    Stop,
}

pub struct Tray {
    updates: glib::Sender<Update>,
    last_state: Option<MenuState>,
    worker: Option<thread::JoinHandle<()>>,
}

impl Tray {
    pub fn new(send: Arc<dyn Fn(Command) + Send + Sync>) -> Result<Self> {
        let (ready, init) = mpsc::sync_channel(1);
        #[allow(deprecated)]
        let (updates, receiver) = MainContext::channel(Priority::DEFAULT);
        let bridge_updates = updates.clone();
        let worker = thread::Builder::new()
            .name("linux-tray".into())
            .spawn(move || {
                // The app window already uses X11 or XWayland. GtkStatusIcon needs
                // GDK's X11 backend to dock with an XEmbed tray host.
                gdk::set_allowed_backends("x11,*");
                let tray = gtk::init()
                    .context("Cannot start GTK for the system tray")
                    .and_then(|()| NativeTray::new(Arc::clone(&send)));
                let mut tray = match tray {
                    Ok(tray) => tray,
                    Err(error) => {
                        let _ = ready.send(Err(error));
                        return;
                    }
                };
                let _gnome = match gnome::Integration::new(send, bridge_updates) {
                    Ok(integration) => Some(integration),
                    Err(error) => {
                        eprintln!("GNOME tray integration is unavailable: {error:#}");
                        None
                    }
                };
                let _source = receiver.attach(None, move |update| match update {
                    Update::State((language, enabled)) => {
                        tray.update_menu(language, enabled);
                        ControlFlow::Continue
                    }
                    Update::GnomeBridge(enabled) => {
                        tray._icon.set_gnome_bridge(enabled);
                        ControlFlow::Continue
                    }
                    Update::Stop => {
                        gtk::main_quit();
                        ControlFlow::Break
                    }
                });
                if ready.send(Ok(())).is_ok() {
                    gtk::main();
                }
            })?;
        init.recv().context("System tray initialization failed")??;
        Ok(Self {
            updates,
            last_state: None,
            worker: Some(worker),
        })
    }

    pub fn update(&mut self, snapshot: &Snapshot) {
        let state = menu_state(snapshot);
        if self.last_state != Some(state) && self.updates.send(Update::State(state)).is_ok() {
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
        let _ = self.updates.send(Update::Stop);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
