#!/usr/bin/env bash
# Runs inside the isolated D-Bus session created by gnome-smoke.sh.
set -euo pipefail

real_before="$(stat -c %Y "$SMOKE_REAL_DCONF" 2>/dev/null || echo none)"
gsettings set org.gnome.shell disable-user-extensions false
gsettings set org.gnome.shell welcome-dialog-last-shown-version '999'
gsettings set org.gnome.desktop.interface enable-hot-corners false
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

echo "1. first toggle (launch + show)"; dropshade Toggle; sleep 5; inspect; shot 1-shown
echo "1b. type a command"; type_text "echo typed-once"; sleep 2; shot 1b-typed
echo "2. second toggle (hide)";         dropshade Toggle; sleep 2; inspect; shot 2-hidden
echo "3. third toggle (show again)";    dropshade Toggle; sleep 2; inspect; shot 3-shown-again
echo "4. new tab (Ctrl+Shift+T) and type"; chord "Control_L Shift_L t"; sleep 2
type_text "echo second-tab"; sleep 2; shot 4-two-tabs
echo "5. rename (Ctrl+Shift+R)"; chord "Control_L Shift_L r"; sleep 1
type_text "build"; sleep 2; shot 5-renamed
echo "6. previous tab (Ctrl+Page_Up)"; chord "Control_L Page_Up"; sleep 2; shot 6-first-tab
echo "7. close confirmation"; chord "Control_L Page_Down"; sleep 1
type_text "sleep 30"; sleep 1; chord "Control_L Shift_L w"; sleep 2; shot 7-confirm-close
echo "8. escape keeps the tab"; chord "Escape"; sleep 1; shot 8-kept
echo "8b. left-click the first tab"; click 100 50 1; sleep 1; shot 8b-clicked-first
echo "9. right-click the second tab"; click 360 50 3; sleep 1; shot 9-menu
echo "10. move it left from the menu"; click 400 99 1; sleep 1; shot 10-moved
config_file="$XDG_CONFIG_HOME/dropshade/config.toml"
echo "11. live reload"
printf '[appearance]\ntheme = "dawn"\ntab_bar = "bottom"\n[font]\nsize = 17\n' > "$config_file"
sleep 2; shot 11-reloaded
echo "12. a broken file keeps the settings and explains why"
printf '[appearance]\ntheme = "dawn"\ntab_bar = "bottom"\nbogus = 1\n' > "$config_file"
sleep 2; shot 12-broken
echo "screenshots: $SMOKE_DEV/out"
