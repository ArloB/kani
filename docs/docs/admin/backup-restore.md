# Backup & Restore

!!! note "TODO"
    This page is a stub. Full content coming soon.

## What to back up

- `/data/kani.db` — SQLite database (library metadata, settings, user accounts)
- `/data/*.wasm` — installed extension files
- `/library/` — downloaded chapter images (large; can be re-downloaded if lost)

## Manual backup

```bash
docker compose exec kani sqlite3 /data/kani.db ".backup /data/kani-backup-$(date +%F).db"
```

## Restore

<!-- TODO: restore procedure from backup file -->

## Automated backups

<!-- TODO: example cron / systemd timer for scheduled backups -->

## Export

Kani includes a built-in export feature under **Settings → Admin → Export** that produces a
portable archive of your library metadata.
