import Gio from 'gi://Gio';
import Meta from 'gi://Meta';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';

const DEFAULT_COMMAND = ['dropshade'];

/**
 * Starts Dropshade as a privileged Wayland client so its window can be identified reliably.
 */
export class AppLauncher {
    constructor(settings, {onWindow, onExit}) {
        this._settings = settings;
        this._onWindow = onWindow;
        this._onExit = onExit;
        this._client = null;
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
        const launcher = new Gio.SubprocessLauncher({flags: Gio.SubprocessFlags.NONE});
        launcher.setenv('DROPSHADE_SHELL_LAUNCHED', '1', true);

        try {
            this._client = Meta.WaylandClient.new_subprocess(global.context, launcher, argv);
        } catch (error) {
            console.error(`Dropshade: could not start ${argv.join(' ')}: ${error.message}`);
            Main.notifyError('Dropshade could not start', `${argv[0]}: ${error.message}`);
            return;
        }
        this._watchExit(this._client);
    }

    _watchExit(client) {
        const subprocess = client.get_subprocess();
        subprocess.wait_async(null, (proc, result) => {
            try {
                proc.wait_finish(result);
            } catch (error) {
                console.error(`Dropshade: waiting for the app failed: ${error.message}`);
            }
            if (this._client !== client)
                return;
            this._client = null;
            this._onExit();
        });
    }

    destroy() {
        global.display.disconnect(this._windowCreatedId);
        this._client = null;
    }
}
