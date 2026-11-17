#!/bin/sh
# Installs or updates Dropshade for the current user, without sudo:
#
#   curl -fsSL https://samuelba.dev/dropshade/install.sh | sh
#
# Running it again updates to the latest release. Options (after `sh -s --` when piped):
#   --gnome-extension  also install the GNOME Shell extension when not on GNOME right now
#   --force            reinstall even when this version is already installed
#   --uninstall        remove Dropshade; the configuration in ~/.config/dropshade stays
# Environment: DROPSHADE_VERSION pins a release (e.g. 0.11.0). DROPSHADE_GNOME_EXTENSION=1,
# DROPSHADE_FORCE=1 and DROPSHADE_UNINSTALL=1 do the same as the options.
#
# Files: ~/.local/bin/dropshade, the desktop entry, icons and metainfo under ~/.local/share,
# and the GNOME Shell extension under ~/.local/share/gnome-shell/extensions.
set -eu

repo="https://github.com/samuelba/dropshade"
app_id="dev.samuelba.Dropshade"
uuid="dropshade@samuelba.dev"
bin_dir="${XDG_BIN_HOME:-$HOME/.local/bin}"
data_dir="${XDG_DATA_HOME:-$HOME/.local/share}"
config_dir="${XDG_CONFIG_HOME:-$HOME/.config}"
binary="$bin_dir/dropshade"
marker="$data_dir/dropshade/install-method"
extension_dir="$data_dir/gnome-shell/extensions/$uuid"
icon_files="scalable/apps/$app_id.svg scalable/apps/$app_id-tray.svg symbolic/apps/$app_id-symbolic.svg"

gnome_extension="${DROPSHADE_GNOME_EXTENSION:-0}"
force="${DROPSHADE_FORCE:-0}"
uninstall="${DROPSHADE_UNINSTALL:-0}"
for arg in "$@"; do
    case "$arg" in
        --gnome-extension) gnome_extension=1 ;;
        --force) force=1 ;;
        --uninstall) uninstall=1 ;;
        -h | --help)
            echo "Usage: install.sh [--gnome-extension] [--force] [--uninstall]"
            echo "Set DROPSHADE_VERSION to install a specific release."
            exit 0
            ;;
        *)
            echo "dropshade install: unknown option $arg" >&2
            exit 2
            ;;
    esac
done

say() { printf '%s\n' "$*"; }
die() {
    printf 'dropshade install: %s\n' "$*" >&2
    exit 1
}
has() { command -v "$1" >/dev/null 2>&1; }

fetch() { # URL FILE
    if has curl; then
        curl -fsSL --proto '=https' --tlsv1.2 -o "$2" "$1"
    elif has wget; then
        wget -q -O "$2" "$1"
    else
        die "needs curl or wget"
    fi
}

latest_version() {
    if has curl; then
        url="$(curl -fsSLI --proto '=https' -o /dev/null -w '%{url_effective}' "$repo/releases/latest")"
    elif has wget; then
        url="$(wget -q -S --spider --max-redirect=0 "$repo/releases/latest" 2>&1 |
            sed -n 's/^ *[Ll]ocation: *//p' | tr -d '\r' | tail -n 1)" || true
    else
        die "needs curl or wget"
    fi
    version="${url##*/tag/v}"
    case "$version" in
        "" | */*) die "cannot find the latest release at $repo/releases/latest" ;;
    esac
    printf '%s\n' "$version"
}

installed_version() {
    if [ -f "$marker" ]; then
        sed -n 's/^version=//p' "$marker"
    elif [ -x "$binary" ]; then
        "$binary" --version 2>/dev/null | awk '{ print $2 }'
    fi
}

sha256() {
    if has sha256sum; then
        sha256sum "$1" | awk '{ print $1 }'
    elif has shasum; then
        shasum -a 256 "$1" | awk '{ print $1 }'
    else
        die "needs sha256sum or shasum to check the download"
    fi
}

on_gnome() {
    case ":${XDG_CURRENT_DESKTOP:-}:" in
        *:GNOME:* | *:GNOME-Classic:*) return 0 ;;
    esac
    return 1
}

# The Exec key of a desktop entry. Paths with characters that need escaping are refused.
desktop_exec() {
    case "$1" in
        *[\"\`\$\\]*) die "cannot write a desktop entry for $1 (it contains \", \`, \$ or \\)" ;;
        *[!A-Za-z0-9/._+-]*) printf '"%s"' "$1" ;;
        *) printf '%s' "$1" ;;
    esac
}

do_uninstall() {
    [ -e "$binary" ] || [ -e "$marker" ] || die "Dropshade is not installed in $bin_dir"
    if [ -d "$extension_dir" ]; then
        if has gnome-extensions; then
            gnome-extensions disable "$uuid" 2>/dev/null || true
        fi
        rm -rf -- "$extension_dir"
    fi
    rm -f -- "$binary" "$marker" \
        "$data_dir/applications/$app_id.desktop" \
        "$data_dir/metainfo/$app_id.metainfo.xml" \
        "$config_dir/autostart/$app_id.desktop"
    for icon in $icon_files; do
        rm -f -- "$data_dir/icons/hicolor/$icon"
    done
    rmdir -- "$data_dir/dropshade" 2>/dev/null || true
    say "Removed Dropshade. Your configuration in $config_dir/dropshade stays."
    say "On COSMIC, also remove the Dropshade entry from Settings → Keyboard → Custom shortcuts."
}

[ "$(uname -s)" = Linux ] || die "Dropshade runs on Linux only"

if [ "$uninstall" = 1 ]; then
    do_uninstall
    exit 0
fi

if [ -e /usr/bin/dropshade ]; then
    die "Dropshade is installed as a system package (/usr/bin/dropshade). Update it with your package manager, or remove the package before using this script."
fi

case "$(uname -m)" in
    x86_64 | amd64) arch=x86_64 ;;
    aarch64 | arm64) arch=aarch64 ;;
    *) die "there is no Dropshade build for $(uname -m), only for x86_64 and aarch64" ;;
esac

if [ -n "${DROPSHADE_VERSION:-}" ]; then
    version="${DROPSHADE_VERSION#v}"
else
    version="$(latest_version)"
fi
previous="$(installed_version || true)"

# A GNOME extension installed earlier is kept up to date, even when updating from another desktop.
want_extension=0
if [ "$gnome_extension" = 1 ] || on_gnome || [ -d "$extension_dir" ]; then
    want_extension=1
fi

if [ "$previous" = "$version" ] && [ "$force" != 1 ]; then
    if [ "$gnome_extension" = 1 ] && [ ! -d "$extension_dir" ]; then
        "$binary" gnome-extension install
        exit 0
    fi
    say "Dropshade $version is already installed. Use --force to reinstall it."
    exit 0
fi

tmp="$(mktemp -d)"
trap 'rm -rf -- "$tmp"' EXIT
trap 'exit 130' INT TERM

bundle="dropshade-$version-$arch-linux"
say "Downloading Dropshade $version ($arch)"
fetch "$repo/releases/download/v$version/$bundle.tar.gz" "$tmp/$bundle.tar.gz" ||
    die "cannot download Dropshade $version for $arch. Nothing was installed."
fetch "$repo/releases/download/v$version/SHA256SUMS-$arch" "$tmp/SHA256SUMS" ||
    die "cannot download the checksums. Nothing was installed."
expected="$(awk -v name="$bundle.tar.gz" '$2 == name || $2 == "*" name { print $1 }' "$tmp/SHA256SUMS")"
[ -n "$expected" ] || die "$bundle.tar.gz is missing from SHA256SUMS-$arch. Nothing was installed."
[ "$(sha256 "$tmp/$bundle.tar.gz")" = "$expected" ] ||
    die "the download does not match its checksum. Nothing was installed."
tar -xzf "$tmp/$bundle.tar.gz" -C "$tmp"
src="$tmp/$bundle"
[ -x "$src/dropshade" ] || die "the archive has no dropshade binary. Nothing was installed."

# Written next to the old binary and renamed over it, so a running Dropshade keeps working.
mkdir -p "$bin_dir"
cp "$src/dropshade" "$bin_dir/.dropshade.new"
chmod 755 "$bin_dir/.dropshade.new"
mv -f "$bin_dir/.dropshade.new" "$binary"

exec_path="$(desktop_exec "$binary")"
mkdir -p "$data_dir/applications" "$data_dir/metainfo"
sed "s|^Exec=dropshade|Exec=$exec_path|" "$src/packaging/$app_id.desktop" \
    >"$data_dir/applications/$app_id.desktop"
cp "$src/packaging/$app_id.metainfo.xml" "$data_dir/metainfo/"
for icon in $icon_files; do
    mkdir -p "$data_dir/icons/hicolor/${icon%/*}"
    cp "$src/packaging/icons/hicolor/$icon" "$data_dir/icons/hicolor/$icon"
done

# Older Dropshade versions wrote `Exec=dropshade --hidden` into the autostart entry, and the
# session may not look in ~/.local/bin.
autostart="$config_dir/autostart/$app_id.desktop"
if [ -f "$autostart" ]; then
    sed "s|^Exec=dropshade |Exec=$exec_path |" "$autostart" >"$tmp/autostart" &&
        cat "$tmp/autostart" >"$autostart"
fi

if [ "$want_extension" = 1 ]; then
    "$binary" gnome-extension install
fi

mkdir -p "${marker%/*}"
printf 'method=script\nversion=%s\n' "$version" >"$marker"

if [ -n "$previous" ]; then
    say ""
    if [ "$previous" = "$version" ]; then
        say "Reinstalled Dropshade $version. Restart Dropshade if it is running."
    else
        say "Updated Dropshade $previous → $version. Restart Dropshade to use the new version."
    fi
    if [ "$want_extension" = 1 ]; then
        say "Log out and back in so GNOME Shell loads the updated extension."
    fi
    exit 0
fi

say ""
say "Installed Dropshade $version to $binary."
case ":$PATH:" in
    *":$bin_dir:"*) ;;
    *) say "Add $bin_dir to your PATH to run \`dropshade\` from a shell." ;;
esac
case ":${XDG_CURRENT_DESKTOP:-}:" in
    *:COSMIC:*)
        say "Next: open Dropshade once from the app launcher. It adds F12 to COSMIC's keyboard shortcuts."
        ;;
    *:KDE:*)
        say "Next: open Dropshade once from the app launcher and confirm the shortcut. Then turn on"
        say "Settings → Session → Start when you log in."
        ;;
    *)
        if [ "$want_extension" = 1 ]; then
            say "Next: log out and back in. From then on F12 drops the terminal down."
        else
            say "Next: open Dropshade from the app launcher."
        fi
        ;;
esac
