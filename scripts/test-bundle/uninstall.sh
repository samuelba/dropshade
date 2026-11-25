#!/bin/sh
# Removes Dropshade with its own uninstaller, as a user would. The configuration stays; prune.sh
# removes the rest.
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
exec sh "$here/installer.sh" --uninstall "$@"
