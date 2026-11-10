#!/usr/bin/env bash
# Runs inside the isolated D-Bus session created by `gnome-smoke.sh --fallback`: the Dropshade
# extension is not loaded, so the app shows and hides its own window.
set -euo pipefail

source "$SMOKE_ROOT/scripts/gnome-smoke-lib.sh"
start_isolated_shell "['$SMOKE_DEVTOOLS']"

dropshade_cli="$SMOKE_ROOT/target/debug/dropshade"
socket="$(find "$XDG_RUNTIME_DIR" -maxdepth 1 -name 'wayland-*' -type s | head -1)"
export WAYLAND_DISPLAY="${socket##*/}"

echo "f1. start without the extension"
"$dropshade_cli" &
app_pid=$!
trap 'kill "$app_pid" 2>/dev/null; kill "$shell_pid" 2>/dev/null; wait 2>/dev/null' EXIT
sleep 5; inspect; click 700 400 1; sleep 1; type_text "echo kept-alive"; sleep 2; shot f1-started
echo "f2. hide closes the window"; "$dropshade_cli" hide; sleep 2; inspect
echo "f3. show opens it again with the tabs intact"; "$dropshade_cli" show; sleep 3; inspect; shot f3-shown-again
echo "f4. a second launch hands over instead of starting another instance"
"$dropshade_cli"; sleep 2; echo "    instances: $(pgrep -fc "^$dropshade_cli$" || true)"
echo "f5. toggle hides the open window"; "$dropshade_cli" toggle; sleep 2; inspect
echo "screenshots: $SMOKE_DEV/out"
