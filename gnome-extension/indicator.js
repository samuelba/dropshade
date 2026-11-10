import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GObject from 'gi://GObject';
import St from 'gi://St';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';

/**
 * The top bar icon shown while Dropshade runs: a click toggles the terminal, a right click opens a
 * menu with the settings and the about dialog.
 */
const IndicatorButton = GObject.registerClass(
class DropshadeIndicatorButton extends PanelMenu.Button {
    _init(iconFile, {onToggle, onSettings, onAbout}) {
        super._init(0.5, 'Dropshade');
        this._onToggle = onToggle;

        this.add_child(new St.Icon({
            gicon: new Gio.FileIcon({file: iconFile}),
            style_class: 'system-status-icon',
        }));
        this.menu.addAction('Settings…', onSettings);
        this.menu.addAction('About Dropshade', onAbout);

        // The stock gesture opens the menu on any button; only the secondary one should.
        this.remove_action(this._clickGesture);
        this._clickGesture = new Clutter.ClickGesture();
        this._clickGesture.set_recognize_on_press(true);
        this._clickGesture.connect('recognize', gesture => this._onClick(gesture.get_button()));
        this.add_action(this._clickGesture);
    }

    _onClick(button) {
        if (button === Clutter.BUTTON_SECONDARY) {
            this.menu.toggle();
        } else {
            this.menu.close();
            this._onToggle();
        }
    }
});

export class Indicator {
    constructor(uuid, extensionDir, callbacks) {
        const iconFile = extensionDir.get_child('icons').get_child('dropshade-symbolic.svg');
        this._button = new IndicatorButton(iconFile, callbacks);
        this._running = false;
        Main.panel.addToStatusArea(uuid, this._button);
        this._sessionId = Main.sessionMode.connect('updated', () => this._sync());
        this._sync();
    }

    set running(running) {
        this._running = running;
        this._sync();
    }

    destroy() {
        Main.sessionMode.disconnect(this._sessionId);
        this._button.destroy();
        this._button = null;
    }

    _sync() {
        this._button.visible = this._running && !Main.sessionMode.isLocked;
    }
}
