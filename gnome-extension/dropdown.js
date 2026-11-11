import Clutter from 'gi://Clutter';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';

/**
 * Lists or unlists the window in Alt+Tab, the dash and the window list. Mutter before GNOME 47
 * cannot do this for Wayland clients, so there the window stays listed.
 */
function setListed(window, listed) {
    if (typeof window.hide_from_window_list !== 'function')
        return;
    if (listed)
        window.show_in_window_list();
    else
        window.hide_from_window_list();
}

/**
 * Places the Dropshade window at the top of a monitor, keeps it above other windows and out of
 * the window list, and slides it in and out.
 */
export class Dropdown {
    constructor(prefs) {
        this._prefs = prefs;
        this._window = null;
        this._shown = false;
        this._signals = [];
        this._focusId = global.display.connect('notify::focus-window', () => this._onFocusChanged());
    }

    get attached() {
        return this._window !== null;
    }

    attach(window) {
        this.detach();
        this._window = window;
        setListed(window, false);
        this._connect(window, 'unmanaged', () => this._forget());

        Main.wm.skipNextEffect(window.get_compositor_private());
        // Mutter places new windows when they are first shown; placing earlier gets overridden.
        // Restacking also waits: before then the window is not in the stack yet, and Mutter
        // (seen on GNOME 46) logs a critical for it.
        this._connectOnce(window, 'shown', () => {
            window.make_above();
            window.stick();
            this.show();
        });
    }

    /**
     * Follows whether the drop-down is shown or on its way in, not the focus: after a click into
     * another window the shortcut hides it rather than sliding it in again.
     */
    toggle() {
        if (this._isShown())
            this.hide();
        else
            this.show();
    }

    show() {
        const window = this._window;
        if (!window)
            return;
        const actor = window.get_compositor_private();
        const rect = this._targetRect();

        if (window.minimized) {
            Main.wm.skipNextEffect(actor);
            window.unminimize();
        }
        window.move_resize_frame(false, rect.x, rect.y, rect.width, rect.height);
        Main.activateWindow(window, global.get_current_time());
        if (this._isShown())
            return;
        this._shown = true;
        this._slide(actor, -rect.height, 0, () => {});
    }

    hide() {
        const window = this._window;
        if (!window || window.minimized || !this._shown)
            return;
        this._shown = false;
        const actor = window.get_compositor_private();
        const height = window.get_frame_rect().height;
        this._slide(actor, 0, -height, () => {
            Main.wm.skipNextEffect(actor);
            // Mutter refuses to minimize windows that are hidden from the window list, so list
            // the window just for the minimize; nothing repaints in between.
            setListed(window, true);
            window.minimize();
            setListed(window, false);
            actor.translation_y = 0;
        });
    }

    /** Hands the window back as an ordinary window, e.g. when the extension is disabled. */
    detach() {
        const window = this._window;
        if (!window)
            return;
        const actor = window.get_compositor_private();
        actor?.remove_all_transitions();
        if (actor)
            actor.translation_y = 0;
        setListed(window, true);
        window.unmake_above();
        window.unstick();
        this._forget();
    }

    destroy() {
        global.display.disconnect(this._focusId);
        this.detach();
    }

    _onFocusChanged() {
        const window = this._window;
        if (!window || window.minimized || !this._prefs.hideOnUnfocus)
            return;
        const focused = global.display.focus_window;
        if (focused && focused !== window)
            this.hide();
    }

    _isShown() {
        const window = this._window;
        return window !== null && !window.minimized && this._shown;
    }

    _monitorIndex() {
        const display = global.display;
        switch (this._prefs.monitor) {
        case 'primary':
            return display.get_primary_monitor();
        case 'pointer':
            return display.get_current_monitor();
        default: {
            const manager = global.backend.get_monitor_manager();
            const index = manager.get_monitor_for_connector(this._prefs.monitor);
            return index >= 0 ? index : display.get_current_monitor();
        }
        }
    }

    _targetRect() {
        const area = Main.layoutManager.getWorkAreaForMonitor(this._monitorIndex());
        const width = Math.round(area.width * this._prefs.width);
        const height = Math.round(area.height * this._prefs.height);
        return {
            x: area.x + horizontalOffset(this._prefs.align, area.width - width),
            y: area.y,
            width,
            height,
        };
    }

    _slide(actor, from, to, onComplete) {
        actor.remove_all_transitions();
        actor.translation_y = from;
        actor.ease({
            translation_y: to,
            duration: this._prefs.animationDuration,
            mode: Clutter.AnimationMode.EASE_OUT_CUBIC,
            onComplete,
        });
    }

    _connect(object, signal, handler) {
        this._signals.push([object, object.connect(signal, handler)]);
    }

    _connectOnce(object, signal, handler) {
        const id = object.connect(signal, (...args) => {
            object.disconnect(id);
            this._signals = this._signals.filter(([, signalId]) => signalId !== id);
            handler(...args);
        });
        this._signals.push([object, id]);
    }

    _forget() {
        for (const [object, id] of this._signals)
            object.disconnect(id);
        this._signals = [];
        this._window = null;
        this._shown = false;
    }
}

/** How far right of the work area's left edge a window leaves `spare` pixels for `align`. */
function horizontalOffset(align, spare) {
    switch (align) {
    case 'left':
        return 0;
    case 'right':
        return spare;
    default:
        return Math.round(spare / 2);
    }
}
