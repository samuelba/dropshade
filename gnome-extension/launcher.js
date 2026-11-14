import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Meta from 'gi://Meta';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as MessageTray from 'resource:///org/gnome/shell/ui/messageTray.js';

const DEFAULT_COMMAND = ['dropshade'];
const SIGTERM = 15;
const STOP_DELAY_SECONDS = 3;

/**
 * The app a destroyed launcher left running, waiting for the next launcher to take it over.
 * GNOME disables and re-enables every extension enabled after one it turns off; when the screen
 * locks it turns off each extension without the unlock-dialog session mode, so this extension
 * can be disabled and enabled several times in a row. Stopping the app each time would lose its
 * tabs, so it is only stopped when no launcher takes it over within STOP_DELAY_SECONDS.
 */
let parked = null;

/**
 * Starts argv as a Wayland client whose windows Mutter can attribute to it. GNOME 47 replaced
 * WaylandClient.new() and spawnv() with new_subprocess(); GNOME 46 only has the former.
 */
function spawnClient(launcher, argv) {
    if (Meta.WaylandClient.new_subprocess) {
        const client = Meta.WaylandClient.new_subprocess(global.context, launcher, argv);
        return {client, subprocess: client.get_subprocess()};
    }
    const client = Meta.WaylandClient.new(global.context, launcher);
    return {client, subprocess: client.spawnv(global.display, argv)};
}

/**
 * Points the app at the desktop's cursor theme. Mutter before GNOME 48 lacks the cursor-shape
 * protocol, so the app draws its own pointer from XCURSOR_THEME, which GNOME never sets; without
 * it the pointer falls back to the system "default" theme (often DMZ-White) instead of the one
 * the rest of the desktop uses. Values the user set in the environment are kept.
 */
function setCursorTheme(launcher) {
    const iface = new Gio.Settings({schema_id: 'org.gnome.desktop.interface'});
    const theme = iface.get_string('cursor-theme');
    if (theme)
        launcher.setenv('XCURSOR_THEME', theme, false);
    const size = iface.get_int('cursor-size');
    if (size > 0)
        launcher.setenv('XCURSOR_SIZE', `${size}`, false);
}

/**
 * Starts Dropshade as a privileged Wayland client so its window can be identified reliably.
 */
export class AppLauncher {
    constructor(settings, {downloadUrl, onWindow, onExit}) {
        this._settings = settings;
        this._downloadUrl = downloadUrl;
        this._missingNotification = null;
        this._onWindow = onWindow;
        this._onExit = onExit;
        this._client = null;
        this._subprocess = null;
        this._cancellable = new Gio.Cancellable();
        this._windowCreatedId = global.display.connect('window-created', (_display, window) => {
            if (this._client?.owns_window(window))
                this._onWindow(window, this._client);
        });
        if (parked) {
            GLib.source_remove(parked.stopId);
            this._track(parked.client, parked.subprocess);
            parked = null;
        }
    }

    get running() {
        return this._client !== null;
    }

    get client() {
        return this._client;
    }

    /** The app's window if it already has one, e.g. when taken over from an earlier launcher. */
    findWindow() {
        const client = this._client;
        if (!client)
            return null;
        return global.get_window_actors()
            .map(actor => actor.meta_window)
            .find(window => client.owns_window(window)) ?? null;
    }

    launch() {
        if (this._client)
            return;

        const configured = this._settings.get_strv('command');
        const argv = configured.length > 0 ? configured : DEFAULT_COMMAND;
        if (!GLib.find_program_in_path(argv[0])) {
            this._notifyMissing(argv[0]);
            return;
        }
        const launcher = new Gio.SubprocessLauncher({flags: Gio.SubprocessFlags.NONE});
        launcher.setenv('DROPSHADE_SHELL_LAUNCHED', '1', true);
        setCursorTheme(launcher);

        let spawned;
        try {
            spawned = spawnClient(launcher, argv);
        } catch (error) {
            console.error(`Dropshade: could not start ${argv.join(' ')}: ${error.message}`);
            Main.notifyError('Dropshade could not start', `${argv[0]}: ${error.message}`);
            return;
        }
        this._track(spawned.client, spawned.subprocess);
    }

    _track(client, subprocess) {
        this._client = client;
        this._subprocess = subprocess;
        this._watchExit(client, subprocess);
    }

    _notifyMissing(program) {
        if (this._missingNotification)
            return;

        const notification = new MessageTray.Notification({
            source: MessageTray.getSystemSource(),
            title: 'Dropshade is not installed',
            body: `The extension starts the Dropshade app, but “${program}” was not found. ` +
                'Install Dropshade from a release package to use it.',
            urgency: MessageTray.Urgency.HIGH,
        });
        notification.addAction('Download Dropshade', () => {
            try {
                Gio.AppInfo.launch_default_for_uri(this._downloadUrl, null);
            } catch (error) {
                console.error(`Dropshade: could not open ${this._downloadUrl}: ${error.message}`);
            }
        });
        notification.connect('destroy', () => {
            this._missingNotification = null;
        });
        this._missingNotification = notification;
        MessageTray.getSystemSource().addNotification(notification);
    }

    _watchExit(client, subprocess) {
        subprocess.wait_async(this._cancellable, (proc, result) => {
            try {
                proc.wait_finish(result);
            } catch (error) {
                if (error.matches(Gio.IOErrorEnum, Gio.IOErrorEnum.CANCELLED))
                    return;
                console.error(`Dropshade: waiting for the app failed: ${error.message}`);
            }
            if (this._client !== client)
                return;
            this._client = null;
            this._subprocess = null;
            this._onExit();
        });
    }

    /** Parks a running app for the next launcher; see `parked`. */
    destroy() {
        global.display.disconnect(this._windowCreatedId);
        this._cancellable.cancel();
        this._missingNotification?.destroy();
        const client = this._client;
        const subprocess = this._subprocess;
        this._client = null;
        this._subprocess = null;
        if (!subprocess)
            return;

        const stopId = GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT, STOP_DELAY_SECONDS, () => {
            if (parked?.stopId === stopId)
                parked = null;
            subprocess.send_signal(SIGTERM);
            return GLib.SOURCE_REMOVE;
        });
        parked = {client, subprocess, stopId};
    }
}
