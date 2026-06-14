# Storage

!!! note "TODO"
    This page is a stub. Full content coming soon.

## Volumes

| Path | Contents | Size estimate |
|------|----------|--------------|
| `/data` | SQLite DB + WASM extension files | Small (< 1 GB) |
| `/library` | Downloaded chapter images + CBZ archives | Large (scales with library size) |

## Library path

The library path is configured during the first-run wizard and controls where Kani writes
downloaded chapter files. It maps to the `/library` volume in the default Docker Compose setup.

To change it after setup: **Settings → General → Library path**.

## Database

Kani uses a single SQLite file at `/data/kani.db`. WAL mode is enabled for concurrent reads.

## Storage backends

<!-- TODO: document any planned S3/object-storage backend -->

## Disk space management

<!-- TODO: storage stats page, purge options -->
