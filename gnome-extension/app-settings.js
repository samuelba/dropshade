import Gio from 'gi://Gio';

const BUS_NAME = 'dev.samuelba.Dropshade';
const OBJECT_PATH = '/dev/samuelba/Dropshade';
const INTERFACE = `
<node>
  <interface name="${BUS_NAME}">
    <property name="Hotkey" type="s" access="read"/>
    <property name="Width" type="d" access="read"/>
    <property name="Height" type="d" access="read"/>
    <property name="Align" type="s" access="read"/>
    <property name="VerticalAlign" type="s" access="read"/>
    <property name="Monitor" type="s" access="read"/>
    <property name="Animation" type="s" access="read"/>
    <property name="AnimationDuration" type="u" access="read"/>
    <property name="HideOnUnfocus" type="b" access="read"/>
    <method name="OpenSettings"/>
    <method name="OpenAbout"/>
  </interface>
</node>`;

const DEFAULTS = {
    hotkey: 'F12',
    width: 1.0,
    height: 0.5,
    align: 'center',
    verticalAlign: 'top',
    monitor: 'pointer',
    animation: 'slide-top',
    animationDuration: 180,
    hideOnUnfocus: false,
};

/**
 * The window settings from Dropshade's config file, read over D-Bus while the app runs. The last
 * known values (or defaults) stay in effect while it is not running. The hotkey is mirrored into
 * the extension's GSettings, where GNOME Shell reads keybindings from, so it works before the
 * first launch too.
 */
export class AppSettings {
    constructor(gsettings) {
        this._gsettings = gsettings;
        this._values = {...DEFAULTS};
        this._proxy = null;
        this._changedId = 0;
        const info = Gio.DBusNodeInfo.new_for_xml(INTERFACE).interfaces[0];
        this._watchId = Gio.bus_watch_name(
            Gio.BusType.SESSION, BUS_NAME, Gio.BusNameWatcherFlags.NONE,
            () => this._connect(info),
            () => this._disconnect());
    }

    get width() {
        return this._values.width;
    }

    get height() {
        return this._values.height;
    }

    get align() {
        return this._values.align;
    }

    get verticalAlign() {
        return this._values.verticalAlign;
    }

    get monitor() {
        return this._values.monitor;
    }

    get animation() {
        return this._values.animation;
    }

    get animationDuration() {
        return this._values.animationDuration;
    }

    get hideOnUnfocus() {
        return this._values.hideOnUnfocus;
    }

    /** Asks the running app to show its settings panel. */
    openSettings() {
        this._callApp('OpenSettings', 'open the settings');
    }

    /** Asks the running app to show its About dialog. */
    openAbout() {
        this._callApp('OpenAbout', 'open the about dialog');
    }

    destroy() {
        Gio.bus_unwatch_name(this._watchId);
        this._disconnect();
    }

    _callApp(method, action) {
        if (!this._proxy) {
            console.error(`Dropshade: cannot ${action}, the app is not running`);
            return;
        }
        this._proxy.call(method, null, Gio.DBusCallFlags.NONE, -1, null, (proxy, result) => {
            try {
                proxy.call_finish(result);
            } catch (error) {
                console.error(`Dropshade: cannot ${action}: ${error.message}`);
            }
        });
    }

    _connect(info) {
        Gio.DBusProxy.new_for_bus(
            Gio.BusType.SESSION, Gio.DBusProxyFlags.NONE, info,
            BUS_NAME, OBJECT_PATH, BUS_NAME, null,
            (_source, result) => {
                try {
                    this._proxy = Gio.DBusProxy.new_for_bus_finish(result);
                } catch (error) {
                    console.error(`Dropshade: cannot read the app's settings: ${error.message}`);
                    return;
                }
                this._changedId = this._proxy.connect('g-properties-changed', () => this._refresh());
                this._refresh();
            });
    }

    _refresh() {
        const read = (name, fallback) => this._proxy?.get_cached_property(name)?.deepUnpack() ?? fallback;
        this._values = {
            hotkey: read('Hotkey', this._values.hotkey),
            width: read('Width', this._values.width),
            height: read('Height', this._values.height),
            align: read('Align', this._values.align),
            verticalAlign: read('VerticalAlign', this._values.verticalAlign),
            monitor: read('Monitor', this._values.monitor),
            animation: read('Animation', this._values.animation),
            animationDuration: read('AnimationDuration', this._values.animationDuration),
            hideOnUnfocus: read('HideOnUnfocus', this._values.hideOnUnfocus),
        };
        const current = this._gsettings.get_strv('toggle-shortcut');
        if (this._values.hotkey && (current.length !== 1 || current[0] !== this._values.hotkey))
            this._gsettings.set_strv('toggle-shortcut', [this._values.hotkey]);
    }

    _disconnect() {
        if (this._proxy && this._changedId)
            this._proxy.disconnect(this._changedId);
        this._proxy = null;
        this._changedId = 0;
    }
}
