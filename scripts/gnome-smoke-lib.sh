#!/usr/bin/env bash
# Shared by the smoke-test sessions: settings for the isolated session (with a guard that the real
# dconf database is untouched), the headless shell, and helpers to inspect windows, take
# screenshots and send keyboard and pointer input. Source it, then call start_isolated_shell.

# Usage: start_isolated_shell "['extension@uuid', ...]"
start_isolated_shell() {
    real_before="$(stat -c %Y "$SMOKE_REAL_DCONF" 2>/dev/null || echo none)"
    gsettings set org.gnome.shell disable-user-extensions false
    gsettings set org.gnome.shell welcome-dialog-last-shown-version '999'
    gsettings set org.gnome.desktop.interface enable-hot-corners false
    gsettings set org.gnome.shell enabled-extensions "$1"
    gsettings --schemadir "$SMOKE_ROOT/gnome-extension/schemas" \
        set org.gnome.shell.extensions.dropshade command "${SMOKE_COMMAND:-['$SMOKE_ROOT/target/debug/dropshade']}"
    real_after="$(stat -c %Y "$SMOKE_REAL_DCONF" 2>/dev/null || echo none)"

    if [[ ! -f "$SMOKE_DEV/config/dconf/user" || "$real_before" != "$real_after" ]]; then
        echo "ABORT: settings did not stay inside the isolated session" >&2
        exit 1
    fi

    gnome-shell --headless --virtual-monitor 1600x900 --wayland >"$SMOKE_DEV/out/shell.log" 2>&1 &
    shell_pid=$!
    trap 'kill "$shell_pid" 2>/dev/null; wait "$shell_pid" 2>/dev/null' EXIT

    for _ in $(seq 1 60); do
        evaljs 'global.context.unsafe_mode' 2>/dev/null | grep -q true && break
        sleep 0.5
    done

    # The headless backend has no keyboard; create the virtual one before the app connects so the
    # Wayland seat advertises a keyboard from the start.
    evaljs "const Clutter = imports.gi.Clutter, GLib = imports.gi.GLib;
        const keyboard = Clutter.get_default_backend().get_default_seat()
            .create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
        global._dropshadeKeyboard = keyboard;
        global._dropshadeSend = steps => GLib.timeout_add(GLib.PRIORITY_DEFAULT, 20, () => {
            const step = steps.shift();
            if (step === undefined)
                return GLib.SOURCE_REMOVE;
            const state = step[1] ? Clutter.KeyState.PRESSED : Clutter.KeyState.RELEASED;
            keyboard.notify_keyval(GLib.get_monotonic_time(), step[0], state);
            return GLib.SOURCE_CONTINUE;
        });
        const pointer = Clutter.get_default_backend().get_default_seat()
            .create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
        global._dropshadeClick = (x, y, button) => {
            const now = GLib.get_monotonic_time();
            pointer.notify_absolute_motion(now, x, y);
            GLib.timeout_add(GLib.PRIORITY_DEFAULT, 60, () => {
                pointer.notify_button(GLib.get_monotonic_time(), button, Clutter.ButtonState.PRESSED);
                pointer.notify_button(GLib.get_monotonic_time() + 1000, button, Clutter.ButtonState.RELEASED);
                return GLib.SOURCE_REMOVE;
            });
        };
        'keyboard'" >/dev/null
    sleep 1
}

call() { gdbus call --session "$@"; }
evaljs() { call --dest org.gnome.Shell --object-path /org/gnome/Shell --method org.gnome.Shell.Eval "$1"; }
shot() {
    call --dest org.gnome.Shell --object-path /org/gnome/Shell/Screenshot \
        --method org.gnome.Shell.Screenshot.Screenshot false false "$SMOKE_DEV/out/$1.png" >/dev/null
}
inspect() {
    evaljs 'JSON.stringify(global.display.list_all_windows().map(w => { const r = w.get_frame_rect();
        return {id: w.get_wm_class(), skipTaskbar: w.skip_taskbar, above: w.is_above(),
            allWorkspaces: w.is_on_all_workspaces(), minimized: w.minimized, focused: w.has_focus(),
            rect: [r.x, r.y, r.width, r.height]}; }))'
}

# Key steps are [keyval, pressed] pairs, sent one every 20 ms by global._dropshadeSend.
send_steps() {
    evaljs "global._dropshadeSend($1); 'sent'" >/dev/null
}
type_text() {
    send_steps "[...'$1'].flatMap(c => {
            const keyval = imports.gi.Clutter.unicode_to_keysym(c.codePointAt(0));
            return [[keyval, true], [keyval, false]];
        }).concat([[imports.gi.Clutter.KEY_Return, true], [imports.gi.Clutter.KEY_Return, false]])"
}
# Clicks at screen coordinates; button 1 = left, 2 = middle, 3 = right.
click() {
    evaljs "global._dropshadeClick($1, $2, $3); 'clicked'" >/dev/null
}
# Presses a chord such as "Control_L Shift_L t": modifiers down in order, released in reverse.
chord() {
    local keys=""
    for key in $1; do keys+="imports.gi.Clutter.KEY_$key, "; done
    send_steps "(keys => [...keys.map(k => [k, true]), ...keys.reverse().map(k => [k, false])])([$keys])"
}
