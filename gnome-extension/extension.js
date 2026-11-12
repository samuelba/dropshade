import Meta from 'gi://Meta';
import Shell from 'gi://Shell';

import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

import {AppSettings} from './app-settings.js';
import {ShellService} from './dbus.js';
import {Dropdown} from './dropdown.js';
import {Indicator} from './indicator.js';
import {AppLauncher} from './launcher.js';

const SHORTCUT_KEY = 'toggle-shortcut';

export default class DropshadeExtension extends Extension {
    enable() {
        this._settings = this.getSettings();
        this._appSettings = new AppSettings(this._settings);
        this._dropdown = new Dropdown(this._appSettings);
        this._indicator = new Indicator(this.uuid, this.dir, {
            onToggle: () => this._toggle(),
            onSettings: () => this._openSettings(),
            onAbout: () => this._openAbout(),
        });
        this._launcher = new AppLauncher(this._settings, {
            downloadUrl: `${this.metadata.url}/releases/latest`,
            onWindow: window => {
                this._dropdown.attach(window);
                this._indicator.running = true;
            },
            onExit: () => {
                this._dropdown.detach();
                this._indicator.running = false;
            },
        });
        const window = this._launcher.findWindow();
        if (window) {
            this._dropdown.adopt(window);
            this._indicator.running = true;
        }
        Main.wm.addKeybinding(
            SHORTCUT_KEY,
            this._settings,
            Meta.KeyBindingFlags.IGNORE_AUTOREPEAT,
            Shell.ActionMode.NORMAL | Shell.ActionMode.OVERVIEW,
            () => this._toggle());
        this._service = new ShellService({
            toggle: () => this._toggle(),
            show: () => this._show(),
            hide: () => this._dropdown.hide(),
            toggleMaximize: () => this._dropdown.toggleMaximize(),
            isManaged: () => this._dropdown.attached,
        });
    }

    disable() {
        Main.wm.removeKeybinding(SHORTCUT_KEY);
        this._service.destroy();
        this._launcher.destroy();
        this._indicator.destroy();
        this._dropdown.destroy();
        this._appSettings.destroy();
        this._appSettings = null;
        this._service = null;
        this._launcher = null;
        this._indicator = null;
        this._dropdown = null;
        this._settings = null;
    }

    _toggle() {
        if (Main.overview.visible)
            Main.overview.hide();
        if (this._dropdown.attached)
            this._dropdown.toggle();
        else
            this._launcher.launch();
    }

    _openSettings() {
        if (Main.overview.visible)
            Main.overview.hide();
        this._appSettings.openSettings();
    }

    _openAbout() {
        if (Main.overview.visible)
            Main.overview.hide();
        this._appSettings.openAbout();
    }

    _show() {
        if (this._dropdown.attached)
            this._dropdown.show();
        else
            this._launcher.launch();
    }
}
