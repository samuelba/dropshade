#!/bin/sh
# Installs this test build with Dropshade's own installer, from this folder instead of a release.
# Takes the installer's options, e.g. --gnome-extension.
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
exec sh "$here/installer.sh" --from "$here" --force "$@"
