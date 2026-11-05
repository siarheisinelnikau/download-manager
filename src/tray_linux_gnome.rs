use super::{Command, Update};
use anyhow::{Context, Result};
use gtk::{gio, glib, prelude::*};
use std::sync::Arc;

const BRIDGE_NAME: &str = "io.github.siarheisinelnikau.DownloadManager.Gnome";
const OBJECT_PATH: &str = "/io/github/siarheisinelnikau/DownloadManager/Tray";
const INTERFACE: &str = "io.github.siarheisinelnikau.DownloadManager.Tray1";

pub(super) struct Integration {
    connection: gio::DBusConnection,
    registration: Option<gio::RegistrationId>,
    unwatch: Option<Box<dyn FnOnce()>>,
}

impl Integration {
    pub(super) fn new(
        send: Arc<dyn Fn(Command) + Send + Sync>,
        updates: glib::Sender<Update>,
    ) -> Result<Self> {
        let connection = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE)?;
        let node = gio::DBusNodeInfo::for_xml(&format!(
            "<node><interface name='{INTERFACE}'>
                <method name='QuickDownloads'>
                    <arg name='x' type='i' direction='in'/>
                    <arg name='y' type='i' direction='in'/>
                </method>
            </interface></node>"
        ))?;
        let interface = node
            .lookup_interface(INTERFACE)
            .context("Missing GNOME tray interface")?;
        let registration = connection.register_object(
            OBJECT_PATH,
            &interface,
            move |_, _, _, _, method, parameters, invocation| {
                if method == "QuickDownloads"
                    && let Some((x, y)) = parameters.get::<(i32, i32)>()
                {
                    send(Command::Popup {
                        x: f64::from(x),
                        y: f64::from(y),
                    });
                    invocation.return_value(None);
                } else {
                    invocation.return_dbus_error(
                        "org.freedesktop.DBus.Error.InvalidArgs",
                        "Expected QuickDownloads with two screen coordinates",
                    );
                }
            },
            |_, _, _, _, _| ().to_variant(),
            |_, _, _, _, _, _| false,
        )?;
        let appeared = updates.clone();
        let watcher = gio::bus_watch_name_on_connection(
            &connection,
            BRIDGE_NAME,
            gio::BusNameWatcherFlags::NONE,
            move |_, _, _| {
                let _ = appeared.send(Update::GnomeBridge(true));
            },
            move |_, _| {
                let _ = updates.send(Update::GnomeBridge(false));
            },
        );
        Ok(Self {
            connection,
            registration: Some(registration),
            // gio 0.18 returns an opaque watcher type with a conflicting re-export.
            unwatch: Some(Box::new(move || gio::bus_unwatch_name(watcher))),
        })
    }
}

impl Drop for Integration {
    fn drop(&mut self) {
        if let Some(unwatch) = self.unwatch.take() {
            unwatch();
        }
        if let Some(registration) = self.registration.take() {
            let _ = self.connection.unregister_object(registration);
        }
    }
}

pub(super) fn anchor() -> Option<Command> {
    let connection = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).ok()?;
    let client = connection.unique_name()?;
    let reply = connection
        .call_sync(
            Some(BRIDGE_NAME),
            "/io/github/siarheisinelnikau/DownloadManager/Gnome",
            BRIDGE_NAME,
            "GetAnchor",
            Some(&(client.as_str(),).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            500,
            gio::Cancellable::NONE,
        )
        .ok()?;
    let (x, y) = reply.get::<(i32, i32)>()?;
    Some(Command::Popup {
        x: f64::from(x),
        y: f64::from(y),
    })
}
