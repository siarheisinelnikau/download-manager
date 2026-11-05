import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';

import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

const BRIDGE_NAME = 'io.github.siarheisinelnikau.DownloadManager.Gnome';
const OBJECT_PATH = '/io/github/siarheisinelnikau/DownloadManager/Tray';
const INTERFACE = 'io.github.siarheisinelnikau.DownloadManager.Tray1';
const INDICATOR_ID = 'tray-icon tray app download-manager';
const BRIDGE_PATH = '/io/github/siarheisinelnikau/DownloadManager/Gnome';
const BRIDGE_INTERFACE = `<node><interface name="${BRIDGE_NAME}">
    <method name="GetAnchor">
        <arg name="client" type="s" direction="in"/>
        <arg name="x" type="i" direction="out"/>
        <arg name="y" type="i" direction="out"/>
    </method>
</interface></node>`;

export default class DownloadManagerTray extends Extension {
    enable() {
        this._icons = new Map();
        this._pressedIcon = null;
        this._scanSource = 0;
        this._cancellable = new Gio.Cancellable();
        this._stageCapture = global.stage.connect('captured-event',
            (_stage, event) => this._handleEvent(event));
        this._boxes = [Main.panel._leftBox, Main.panel._centerBox, Main.panel._rightBox]
            .map(box => [box, box.connect('child-added', () => this._queueScan())]);
        this._bridge = Gio.DBusExportedObject.wrapJSObject(BRIDGE_INTERFACE, this);
        this._bridge.export(Gio.DBus.session, BRIDGE_PATH);
        this._owner = 0;
        this._hostWatch = Gio.bus_watch_name(
            Gio.BusType.SESSION, 'org.kde.StatusNotifierWatcher',
            Gio.BusNameWatcherFlags.NONE,
            () => {
                this._owner = Gio.bus_own_name(
                    Gio.BusType.SESSION, BRIDGE_NAME, Gio.BusNameOwnerFlags.NONE,
                    null, null, null);
                this._queueScan();
            },
            () => this._releaseBridgeName());
        this._queueScan();
    }

    GetAnchorAsync([client], invocation) {
        for (const icon of this._icons.keys()) {
            const indicator = icon._indicator;
            if (icon.visible && indicator.id === INDICATOR_ID &&
                (indicator.busName === client || indicator._proxy?.g_name_owner === client)) {
                const rect = icon.get_transformed_extents();
                invocation.return_value(new GLib.Variant('(ii)', [
                    Math.round(rect.get_x() + rect.get_width() / 2),
                    Math.round(rect.get_y() + rect.get_height() / 2),
                ]));
                return;
            }
        }
        invocation.return_dbus_error(
            BRIDGE_NAME + '.NotReady', 'The DownloadManager indicator is unavailable');
    }

    _queueScan() {
        if (this._scanSource)
            return;
        this._scanSource = GLib.idle_add(GLib.PRIORITY_DEFAULT_IDLE, () => {
            this._scanSource = 0;
            for (const icon of Object.values(Main.panel.statusArea))
                this._watchIcon(icon);
            return GLib.SOURCE_REMOVE;
        });
    }

    _watchIcon(icon) {
        if (!icon?._indicator?.busName || !icon._indicator._proxy || this._icons.has(icon))
            return;
        const destroy = icon.connect('destroy', () => {
            if (this._pressedIcon === icon)
                this._pressedIcon = null;
            this._icons.delete(icon);
        });
        this._icons.set(icon, destroy);
    }

    _iconAt(x, y) {
        const target = global.stage.get_actor_at_pos(Clutter.PickMode.REACTIVE, x, y);
        if (!target)
            return null;
        for (const icon of this._icons.keys()) {
            if (icon.visible && icon._indicator.id === INDICATOR_ID &&
                (target === icon || icon.contains(target))) {
                const rect = icon.get_transformed_extents();
                if (x >= rect.get_x() && x < rect.get_x() + rect.get_width() &&
                    y >= rect.get_y() && y < rect.get_y() + rect.get_height())
                    return icon;
            }
        }
        return null;
    }

    _handleEvent(event) {
        const type = event.type();
        if (type !== Clutter.EventType.BUTTON_PRESS &&
            type !== Clutter.EventType.BUTTON_RELEASE)
            return Clutter.EVENT_PROPAGATE;
        if (event.get_button() !== Clutter.BUTTON_PRIMARY)
            return Clutter.EVENT_PROPAGATE;

        const [x, y] = event.get_coords();
        const icon = this._iconAt(x, y);
        if (type === Clutter.EventType.BUTTON_PRESS) {
            this._pressedIcon = icon;
            if (!icon)
                return Clutter.EVENT_PROPAGATE;
            icon.menu.close();
        } else {
            const pressed = this._pressedIcon;
            this._pressedIcon = null;
            if (!pressed)
                return Clutter.EVENT_PROPAGATE;
            if (pressed === icon)
                this._openCompactView(icon._indicator.busName, x, y);
        }
        return Clutter.EVENT_STOP;
    }

    _openCompactView(busName, x, y) {
        Gio.DBus.session.call(
            busName, OBJECT_PATH, INTERFACE, 'QuickDownloads',
            new GLib.Variant('(ii)', [Math.round(x), Math.round(y)]),
            null, Gio.DBusCallFlags.NONE, 1500, this._cancellable,
            (connection, result) => {
                try {
                    connection.call_finish(result);
                } catch (error) {
                    if (!error.matches(Gio.IOErrorEnum, Gio.IOErrorEnum.CANCELLED))
                        console.error(`DownloadManager tray activation failed: ${error.message}`);
                }
            });
    }

    disable() {
        this._cancellable.cancel();
        global.stage.disconnect(this._stageCapture);
        this._stageCapture = 0;
        this._pressedIcon = null;
        if (this._scanSource)
            GLib.Source.remove(this._scanSource);
        this._scanSource = 0;
        for (const [box, signal] of this._boxes)
            box.disconnect(signal);
        this._boxes = [];
        for (const [icon, signal] of this._icons)
            icon.disconnect(signal);
        this._icons.clear();
        this._bridge.unexport();
        this._bridge = null;
        Gio.bus_unwatch_name(this._hostWatch);
        this._hostWatch = 0;
        this._releaseBridgeName();
        this._cancellable = null;
    }

    _releaseBridgeName() {
        if (this._owner)
            Gio.bus_unown_name(this._owner);
        this._owner = 0;
    }
}
