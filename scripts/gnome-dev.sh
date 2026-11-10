#!/usr/bin/env bash
# Runs a nested GNOME Shell with the Dropshade extension, isolated from the real session:
# its own D-Bus session, dconf database and extension directory under target/gnome-dev.
# The XDG variables are exported before dbus-run-session on purpose: services the bus activates
# (dconf-service in particular) inherit the bus environment, not the environment of the caller.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dev="$root/target/gnome-dev"
uuid="dropshade@samuelba.dev"
schemas="$root/gnome-extension/schemas"

if ! rpm -q mutter-devkit >/dev/null 2>&1 && ! command -v mutter-devkit >/dev/null 2>&1; then
    echo "The nested shell needs the devkit viewer: sudo dnf install mutter-devkit" >&2
    exit 1
fi

cargo build --manifest-path "$root/Cargo.toml" -p dropshade
glib-compile-schemas --strict "$schemas"

mkdir -p "$dev/data/gnome-shell/extensions" "$dev/config" "$dev/state" "$dev/cache"
ln -sfn "$root/gnome-extension" "$dev/data/gnome-shell/extensions/$uuid"

export XDG_DATA_HOME="$dev/data"
export XDG_CONFIG_HOME="$dev/config"
export XDG_STATE_HOME="$dev/state"
export XDG_CACHE_HOME="$dev/cache"
export DROPSHADE_DEV_REAL_DCONF="$HOME/.config/dconf/user"
export DROPSHADE_DEV_ROOT="$root" DROPSHADE_DEV_UUID="$uuid" DROPSHADE_DEV_SCHEMAS="$schemas"
export DROPSHADE_DEV_BUS_FILE="$dev/bus-address"

echo "Nested shell starting. Press F12 inside it, or from another terminal:"
echo "  DBUS_SESSION_BUS_ADDRESS=\$(cat $DROPSHADE_DEV_BUS_FILE) $root/scripts/gnome-dev-toggle.sh"

exec dbus-run-session -- bash -c '
    set -euo pipefail
    echo "$DBUS_SESSION_BUS_ADDRESS" > "$DROPSHADE_DEV_BUS_FILE"
    real_before="$(stat -c %Y "$DROPSHADE_DEV_REAL_DCONF" 2>/dev/null || echo none)"
    gsettings set org.gnome.shell disable-user-extensions false
    gsettings set org.gnome.shell enabled-extensions "[\"$DROPSHADE_DEV_UUID\"]"
    gsettings --schemadir "$DROPSHADE_DEV_SCHEMAS" set org.gnome.shell.extensions.dropshade \
        command "[\"$DROPSHADE_DEV_ROOT/target/debug/dropshade\"]"
    real_after="$(stat -c %Y "$DROPSHADE_DEV_REAL_DCONF" 2>/dev/null || echo none)"
    if [[ ! -f "$XDG_CONFIG_HOME/dconf/user" || "$real_before" != "$real_after" ]]; then
        echo "ABORT: settings did not stay inside the isolated session" >&2
        exit 1
    fi
    exec gnome-shell --devkit --wayland
'
