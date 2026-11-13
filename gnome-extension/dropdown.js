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
 * Places the Dropshade window on a monitor, keeps it above other windows and out of the window
 * list, and slides it in and out over the top or bottom edge of the monitor, or fades it.
 */
export class Dropdown {
    constructor(prefs) {
        this._prefs = prefs;
        this._window = null;
        this._shown = false;
        this._maximized = false;
        this._signals = [];
        this._clip = null;
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
     * Takes over a window that is already on screen, e.g. after the extension was disabled and
     * enabled again, keeping it shown or hidden as it is.
     */
    adopt(window) {
        this.detach();
        this._window = window;
        setListed(window, false);
        this._connect(window, 'unmanaged', () => this._forget());
        window.make_above();
        window.stick();
        this._shown = !window.minimized;
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
        const index = this._monitorIndex();
        const rect = this._targetRect(index);

        if (window.minimized) {
            Main.wm.skipNextEffect(actor);
            window.unminimize();
        }
        window.move_resize_frame(false, rect.x, rect.y, rect.width, rect.height);
        Main.activateWindow(window, global.get_current_time());
        if (this._isShown())
            return;
        this._shown = true;
        const monitor = Main.layoutManager.monitors[index];
        this._animate(actor, monitor, this._hiddenState(rect, monitor), SHOWN, () => {});
    }

    /** Fills the work area, or returns to the configured size; hiding ends it. */
    toggleMaximize() {
        const window = this._window;
        if (!this._isShown())
            return;
        this._maximized = !this._maximized;
        const rect = this._targetRect(this._monitorIndex());
        window.move_resize_frame(false, rect.x, rect.y, rect.width, rect.height);
    }

    hide() {
        const window = this._window;
        if (!window || window.minimized || !this._shown)
            return;
        this._shown = false;
        this._maximized = false;
        const actor = window.get_compositor_private();
        const monitor = Main.layoutManager.monitors[window.get_monitor()];
        const hidden = this._hiddenState(window.get_frame_rect(), monitor);
        this._animate(actor, monitor, SHOWN, hidden, () => {
            Main.wm.skipNextEffect(actor);
            // Mutter refuses to minimize windows that are hidden from the window list, so list
            // the window just for the minimize; nothing repaints in between.
            setListed(window, true);
            window.minimize();
            setListed(window, false);
            this._unclip();
            resetActor(actor);
        });
    }

    /** Hands the window back as an ordinary window, e.g. when the extension is disabled. */
    detach() {
        const window = this._window;
        if (!window)
            return;
        const actor = window.get_compositor_private();
        actor?.remove_all_transitions();
        this._unclip();
        if (actor)
            resetActor(actor);
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

    _targetRect(index) {
        const area = Main.layoutManager.getWorkAreaForMonitor(index);
        const width = this._maximized ? area.width : Math.round(area.width * this._prefs.width);
        const height = this._maximized ? area.height : Math.round(area.height * this._prefs.height);
        return {
            x: area.x + horizontalOffset(this._prefs.align, area.width - width),
            y: area.y + verticalOffset(this._prefs.verticalAlign, area.height - height),
            width,
            height,
        };
    }

    /** The actor's state when a window at `rect` is hidden: beyond the monitor's edge, or faded. */
    _hiddenState(rect, monitor) {
        switch (this._prefs.animation) {
        case 'slide-bottom':
            return {...SHOWN, translation_y: monitor.y + monitor.height - rect.y};
        case 'fade':
            return {...SHOWN, opacity: 0, scale_x: FADE_SCALE, scale_y: FADE_SCALE};
        case 'none':
            return SHOWN;
        default:
            return {...SHOWN, translation_y: monitor.y - rect.y - rect.height};
        }
    }

    _animate(actor, monitor, from, to, onComplete) {
        actor.remove_all_transitions();
        this._unclip();
        actor.set_pivot_point(0.5, 0.5);
        Object.assign(actor, from);
        // A slide would otherwise show the window on the monitor above or below.
        if (from.translation_y !== to.translation_y)
            this._clipTo(actor, monitor);
        actor.ease({
            ...to,
            duration: this._prefs.animation === 'none' ? 0 : this._prefs.animationDuration,
            mode: Clutter.AnimationMode.EASE_OUT_CUBIC,
            onComplete: () => {
                if (to === SHOWN)
                    this._unclip();
                onComplete();
            },
        });
    }

    /** Keeps the part of `actor` outside `monitor` undrawn while it moves. */
    _clipTo(actor, monitor) {
        const update = () => {
            const [x, y] = actor.get_position();
            const top = monitor.y - y - actor.translation_y;
            actor.set_clip(monitor.x - x, top, monitor.width, monitor.height);
        };
        const ids = ['notify::translation-y', 'notify::x', 'notify::y']
            .map(signal => actor.connect(signal, update));
        this._clip = {actor, ids};
        update();
    }

    _unclip() {
        if (!this._clip)
            return;
        const {actor, ids} = this._clip;
        this._clip = null;
        for (const id of ids)
            actor.disconnect(id);
        actor.remove_clip();
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
        this._maximized = false;
    }
}

/** The actor's state when the window is fully shown. */
const SHOWN = Object.freeze({
    translation_x: 0,
    translation_y: 0,
    opacity: 255,
    scale_x: 1,
    scale_y: 1,
});

/** The scale a fading window grows from and shrinks to. */
const FADE_SCALE = 0.9;

function resetActor(actor) {
    Object.assign(actor, SHOWN);
}

/** How far below the work area's top edge a window leaves `spare` pixels for `verticalAlign`. */
function verticalOffset(verticalAlign, spare) {
    switch (verticalAlign) {
    case 'bottom':
        return spare;
    case 'center':
        return Math.round(spare / 2);
    default:
        return 0;
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
