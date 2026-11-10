#!/usr/bin/env bash
# Toggles the drop-down over D-Bus; run with DBUS_SESSION_BUS_ADDRESS of the nested shell.
set -euo pipefail
method="${1:-Toggle}"
gdbus call --session \
    --dest io.github.samuelba.Dropshade.Shell \
    --object-path /io/github/samuelba/Dropshade/Shell \
    --method "io.github.samuelba.Dropshade.Shell.$method"
