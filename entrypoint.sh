#!/bin/sh

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
