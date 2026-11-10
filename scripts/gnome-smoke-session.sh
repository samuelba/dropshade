#!/usr/bin/env bash
# Runs inside the isolated D-Bus session created by gnome-smoke.sh.
set -euo pipefail

source "$SMOKE_ROOT/scripts/gnome-smoke-lib.sh"
start_isolated_shell "['$SMOKE_UUID', '$SMOKE_DEVTOOLS']"

dropshade() {
    call --dest io.github.samuelba.Dropshade.Shell --object-path /io/github/samuelba/Dropshade/Shell \
        --method "io.github.samuelba.Dropshade.Shell.$1" >/dev/null
}

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
dropshade_cli="$SMOKE_ROOT/target/debug/dropshade"
echo "13. window settings from the config, applied by the extension"
printf '[window]\nheight = 0.3\nwidth = 0.6\nhotkey = "<Super>t"\n' > "$config_file"
sleep 2
"$dropshade_cli" hide; sleep 1; "$dropshade_cli" show; sleep 2; inspect; shot 13-geometry
echo "    mirrored hotkey: $(gsettings --schemadir "$SMOKE_ROOT/gnome-extension/schemas" get org.gnome.shell.extensions.dropshade toggle-shortcut)"
echo "14. new tab from the command line"
"$dropshade_cli" new-tab --cwd /tmp; sleep 2; type_text "pwd"; sleep 1; shot 14-new-tab
app_pid() {
    call --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus \
        --method org.freedesktop.DBus.GetConnectionUnixProcessID io.github.samuelba.Dropshade |
        sed -E 's/.*uint32 ([0-9]+).*/\1/'
}
echo "15. tabs survive a logout (SIGTERM) and come back on the next start"
sleep 4
pid="$(app_pid)"; kill -TERM "$pid"; sleep 2
echo "    app $pid still running: $(kill -0 "$pid" 2>/dev/null && echo yes || echo no)"
sed 's/^/    | /' "$XDG_STATE_HOME/dropshade/session.toml"
"$dropshade_cli" show; sleep 5; inspect; shot 15-restored
echo "16. layouts"
"$dropshade_cli" layout save work; sleep 1
echo "    layouts: $("$dropshade_cli" layout list | tr '\n' ' ')"
"$dropshade_cli" layout load work; sleep 3; shot 16-layout-loaded
echo "17. a hand-written layout with a startup command"
printf '[[tab]]\nname = "scripted"\ncwd = "/tmp"\ncommand = "echo from-layout"\n' \
    > "$XDG_CONFIG_HOME/dropshade/layouts/scripted.toml"
"$dropshade_cli" layout load scripted; sleep 3; shot 17-scripted
echo "screenshots: $SMOKE_DEV/out"
