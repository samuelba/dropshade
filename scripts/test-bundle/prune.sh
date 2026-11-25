#!/bin/sh
# Uninstalls Dropshade and removes everything it leaves behind, so the next test starts from a
# clean account: its configuration, saved session and scrollback, cache and state, the extension
# and its settings, and the autostart entry.
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
uuid="dropshade@samuelba.dev"
app_id="dev.samuelba.Dropshade"
bin_dir="${XDG_BIN_HOME:-$HOME/.local/bin}"
data_dir="${XDG_DATA_HOME:-$HOME/.local/share}"
config_dir="${XDG_CONFIG_HOME:-$HOME/.config}"
cache_dir="${XDG_CACHE_HOME:-$HOME/.cache}"
state_dir="${XDG_STATE_HOME:-$HOME/.local/state}"

say() { printf '%s\n' "$*"; }
has() { command -v "$1" >/dev/null 2>&1; }

# Removes ITEM from the string list SCHEMA KEY, when the schema exists and lists it.
drop_from_list() { # SCHEMA KEY ITEM
    list="$(gsettings get "$1" "$2" 2>/dev/null)" || return 0
    case "$list" in
        *"'$3'"*) ;;
        *) return 0 ;;
    esac
    updated="$(printf '%s\n' "$list" | sed -e "s|'$3', ||g" -e "s|, '$3'||g" -e "s|'$3'||g")"
    gsettings set "$1" "$2" "$updated"
    say "Removed $3 from $1 $2."
}

if [ -e "$bin_dir/dropshade" ] || [ -e "$data_dir/dropshade/install-method" ]; then
    sh "$here/installer.sh" --uninstall
fi

for path in "$data_dir/gnome-shell/extensions/$uuid" "$config_dir/dropshade" \
    "$cache_dir/dropshade" "$state_dir/dropshade" "$data_dir/dropshade" \
    "$config_dir/autostart/$app_id.desktop" "${XDG_RUNTIME_DIR:+$XDG_RUNTIME_DIR/dropshade}"; do
    if [ -n "$path" ] && [ -e "$path" ]; then
        rm -rf -- "$path"
        say "Removed $path"
    fi
done

if has gsettings; then
    drop_from_list org.gnome.shell enabled-extensions "$uuid"
    drop_from_list org.gnome.shell disabled-extensions "$uuid"
fi
if has dconf; then
    dconf reset -f /org/gnome/shell/extensions/dropshade/
fi

if grep -qs dropshade "$config_dir/kglobalshortcutsrc"; then
    say "KDE still has a Dropshade shortcut: remove it in System Settings → Keyboard → Shortcuts."
fi
if grep -qs dropshade "$config_dir/cosmic/com.system76.CosmicSettings.Shortcuts/v1/custom"; then
    say "COSMIC still has a Dropshade shortcut: remove it in Settings → Keyboard → Custom Shortcuts."
fi
say "Pruned Dropshade. Log out and back in if GNOME Shell had loaded its extension."
