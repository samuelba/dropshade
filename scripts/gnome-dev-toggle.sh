#!/usr/bin/env bash
# Toggles the drop-down over D-Bus; run with DBUS_SESSION_BUS_ADDRESS of the nested shell.
set -euo pipefail
method="${1:-Toggle}"
gdbus call --session \
    --dest dev.samuelba.Dropshade.Shell \
    --object-path /dev/samuelba/Dropshade/Shell \
    --method "dev.samuelba.Dropshade.Shell.$method"
