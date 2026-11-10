#!/usr/bin/env bash
# Runs inside the isolated D-Bus session created by gnome-smoke.sh.
set -euo pipefail

real_before="$(stat -c %Y "$SMOKE_REAL_DCONF" 2>/dev/null || echo none)"
gsettings set org.gnome.shell disable-user-extensions false
gsettings set org.gnome.shell welcome-dialog-last-shown-version '999'
gsettings set org.gnome.shell enabled-extensions "['$SMOKE_UUID', '$SMOKE_DEVTOOLS']"
gsettings --schemadir "$SMOKE_ROOT/gnome-extension/schemas" \
    set org.gnome.shell.extensions.dropshade command "['$SMOKE_ROOT/target/debug/dropshade']"
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

echo "1. first toggle (launch + show)"; dropshade Toggle; sleep 5; inspect; shot 1-shown
echo "2. second toggle (hide)";         dropshade Toggle; sleep 2; inspect; shot 2-hidden
echo "3. third toggle (show again)";    dropshade Toggle; sleep 2; inspect; shot 3-shown-again
echo "screenshots: $SMOKE_DEV/out"
