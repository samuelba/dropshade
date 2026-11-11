import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Meta from 'gi://Meta';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as MessageTray from 'resource:///org/gnome/shell/ui/messageTray.js';

const DEFAULT_COMMAND = ['dropshade'];
const SIGTERM = 15;

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
        this._windowCreatedId = global.display.connect('window-created', (_display, window) => {
            if (this._client?.owns_window(window))
                this._onWindow(window);
        });
    }

    get running() {
        return this._client !== null;
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

        let spawned;
        try {
            spawned = spawnClient(launcher, argv);
        } catch (error) {
            console.error(`Dropshade: could not start ${argv.join(' ')}: ${error.message}`);
            Main.notifyError('Dropshade could not start', `${argv[0]}: ${error.message}`);
            return;
        }
        this._client = spawned.client;
        this._subprocess = spawned.subprocess;
        this._watchExit(spawned.client, spawned.subprocess);
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
        subprocess.wait_async(null, (proc, result) => {
            try {
                proc.wait_finish(result);
            } catch (error) {
                console.error(`Dropshade: waiting for the app failed: ${error.message}`);
            }
            if (this._client !== client)
                return;
            this._client = null;
            this._subprocess = null;
            this._onExit();
        });
    }

    destroy() {
        global.display.disconnect(this._windowCreatedId);
        this._missingNotification?.destroy();
        const subprocess = this._subprocess;
        this._client = null;
        this._subprocess = null;
        subprocess?.send_signal(SIGTERM);
    }
}
