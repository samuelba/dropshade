#!/usr/bin/env bash
# Headless smoke test for the GNOME extension (or, with --fallback, for the app without it). Runs a throwaway GNOME Shell whose D-Bus session,
# dconf database, runtime dir and extension dir all live under target/gnome-smoke, so the real
# session's settings are never touched. A dev-only helper extension enables unsafe mode inside
# that shell so the test can inspect windows (Eval) and take screenshots.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dev="$root/target/gnome-smoke"
uuid="dropshade@samuelba.github.io"
devtools="devtools@dropshade.local"

rm -rf -- "$dev"
mkdir -p "$dev/config" "$dev/state" "$dev/cache" "$dev/out" \
    "$dev/data/gnome-shell/extensions/$devtools"
install -d -m 0700 "$dev/runtime"
ln -sfn "$root/gnome-extension" "$dev/data/gnome-shell/extensions/$uuid"
cat > "$dev/data/gnome-shell/extensions/$devtools/metadata.json" <<JSON
{"uuid": "$devtools", "name": "Dropshade devtools", "description": "Test-only: unsafe mode.", "shell-version": ["50"]}
JSON
cat > "$dev/data/gnome-shell/extensions/$devtools/extension.js" <<'JS'
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
export default class DevtoolsExtension extends Extension {
    enable() { global.context.unsafe_mode = true; }
    disable() { global.context.unsafe_mode = false; }
}
JS

cargo build --manifest-path "$root/Cargo.toml" -p dropshade
glib-compile-schemas --strict "$root/gnome-extension/schemas"

# Exported before dbus-run-session so the bus and every service it activates inherit them.
export XDG_CONFIG_HOME="$dev/config" XDG_DATA_HOME="$dev/data" XDG_STATE_HOME="$dev/state"
export XDG_CACHE_HOME="$dev/cache" XDG_RUNTIME_DIR="$dev/runtime"
export SMOKE_DEV="$dev" SMOKE_ROOT="$root" SMOKE_UUID="$uuid" SMOKE_DEVTOOLS="$devtools"
export SMOKE_REAL_DCONF="$HOME/.config/dconf/user"

session="gnome-smoke-session.sh"
if [[ "${1:-}" == "--fallback" ]]; then
    session="gnome-smoke-fallback-session.sh"
fi
exec dbus-run-session -- bash "$root/scripts/$session"
