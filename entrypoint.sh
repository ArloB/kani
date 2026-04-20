#!/bin/sh
# Wrapper that restarts kani-web when it exits with code 42 (restart signal).
# Exit code 0 = clean stop; any other code = error (let Docker/systemd handle it).
set -e

while true; do
    /app/kani-web "$@"
    EXIT_CODE=$?
    if [ "$EXIT_CODE" -eq 42 ]; then
        echo "[entrypoint] Restart requested (exit code 42), restarting in 1s..."
        sleep 1
    else
        exit "$EXIT_CODE"
    fi
done
