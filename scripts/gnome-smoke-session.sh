#!/usr/bin/env bash
# Runs inside the isolated D-Bus session created by gnome-smoke.sh.
set -euo pipefail

real_before="$(stat -c %Y "$SMOKE_REAL_DCONF" 2>/dev/null || echo none)"
gsettings set org.gnome.shell disable-user-extensions false
gsettings set org.gnome.shell welcome-dialog-last-shown-version '999'
gsettings set org.gnome.shell enabled-extensions "['$SMOKE_UUID', '$SMOKE_DEVTOOLS']"
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

call() { gdbus call --session "$@"; }
dropshade() {
    call --dest io.github.samuelba.Dropshade.Shell --object-path /io/github/samuelba/Dropshade/Shell \
        --method "io.github.samuelba.Dropshade.Shell.$1" >/dev/null
}
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

for _ in $(seq 1 60); do
    evaljs 'global.context.unsafe_mode' 2>/dev/null | grep -q true && break
    sleep 0.5
done

type_text() {
    # The virtual keyboard lives on `global` so it outlives this Eval while Mutter processes the
    # queued events; keys are sent from a timer, one every 20 ms.
    evaljs "const Clutter = imports.gi.Clutter, GLib = imports.gi.GLib;
        global._dropshadeKeyboard ??= Clutter.get_default_backend().get_default_seat()
            .create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
        const keyboard = global._dropshadeKeyboard;
        const keys = [...'$1'].map(c => Clutter.unicode_to_keysym(c.codePointAt(0)));
        keys.push(Clutter.KEY_Return);
        GLib.timeout_add(GLib.PRIORITY_DEFAULT, 20, () => {
            const keyval = keys.shift();
            if (keyval === undefined)
                return GLib.SOURCE_REMOVE;
            keyboard.notify_keyval(GLib.get_monotonic_time(), keyval, Clutter.KeyState.PRESSED);
            keyboard.notify_keyval(GLib.get_monotonic_time(), keyval, Clutter.KeyState.RELEASED);
            return GLib.SOURCE_CONTINUE;
        });
        'typing'"
}

# The headless backend has no keyboard; create the virtual one before the app connects so the
# Wayland seat advertises a keyboard from the start.
evaljs "const Clutter = imports.gi.Clutter;
    global._dropshadeKeyboard = Clutter.get_default_backend().get_default_seat()
        .create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE); 'keyboard'" >/dev/null
sleep 1

echo "1. first toggle (launch + show)"; dropshade Toggle; sleep 5; inspect; shot 1-shown
echo "1b. type a command"; type_text "echo typed-once"; sleep 2; shot 1b-typed
echo "2. second toggle (hide)";         dropshade Toggle; sleep 2; inspect; shot 2-hidden
echo "3. third toggle (show again)";    dropshade Toggle; sleep 2; inspect; shot 3-shown-again
echo "screenshots: $SMOKE_DEV/out"
