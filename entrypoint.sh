#!/bin/sh
# Wrapper that restarts kani-web when it exits with code 42 (restart signal).
# Exit code 0 = clean stop; any other code = error (let Docker/systemd handle it).
#
# Deliberately no `set -e`. It aborted the script the instant kani-web returned
# non-zero — including the 42 that means "restart me" — so the exit code was
# never tested and this loop never ran. An admin-triggered restart killed the
# container instead of restarting the process.

while true; do
    if /app/kani-web "$@"; then
        EXIT_CODE=0
    else
        EXIT_CODE=$?
    fi

    if [ "$EXIT_CODE" -eq 42 ]; then
        echo "[entrypoint] Restart requested (exit code 42), restarting in 1s..."
        sleep 1
    else
        exit "$EXIT_CODE"
    fi
done
