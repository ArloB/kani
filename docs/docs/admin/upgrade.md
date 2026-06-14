# Upgrades

!!! note "TODO"
    This page is a stub. Full content coming soon.

## Docker upgrade

```bash
docker compose pull
docker compose up -d
```

Kani applies SQLite migrations automatically on startup.

## Before upgrading

1. Read the [CHANGELOG](https://github.com/ArloB/kani/blob/main/CHANGELOG.md) for breaking changes.
2. Take a database backup (see [Backup & restore](backup-restore.md)).

## Downgrading

Downgrading is not officially supported. If a migration added columns, the older binary will fail
to start against the migrated database.

## Upgrading extensions

Installed extensions can be updated from **Settings → Sources**. Extension updates are independent of the Kani server version.
