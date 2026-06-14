# Install with Docker

## Image

```text
ghcr.io/arlob/kani:latest        # latest stable release
ghcr.io/arlob/kani:<version>     # pinned release, e.g. 0.1.0
```

Images are published for `linux/amd64` and `linux/arm64`.

## Docker Compose (recommended)

```yaml
services:
  kani:
    image: ghcr.io/arlob/kani:latest
    container_name: kani
    restart: unless-stopped
    ports:
      - "8242:8242"
    volumes:
      - ./data:/data
      - ./library:/library
    environment:
      KANI_SECRET_KEY: "change-me-to-a-random-string"
      RUST_LOG: "kani=info"
```

Start:

```bash
docker compose up -d
docker compose logs -f kani
```

## Volumes

| Mount | Purpose |
|-------|---------|
| `/data` | SQLite database and installed WASM extension files |
| `/library` | Downloaded chapter images and CBZ archives |

Both directories must be writable by the container user (UID 1000 by default).

## Ports

| Port | Protocol | Purpose |
|------|----------|---------|
| `8242` | TCP | HTTP server (REST API + frontend) |

Bind to a specific interface by setting the `KANI_BIND` environment variable:

```yaml
environment:
  KANI_BIND: "127.0.0.1:8242"   # localhost only (useful behind a reverse proxy)
```

## Environment variables

| Variable | Default | Description |
|----------|---------|-------------|
| `KANI_BIND` | `0.0.0.0:8242` | Listen address and port |
| `KANI_SECRET_KEY` | — | **Required.** 32+ char random string used to sign session cookies |
| `KANI_SECURE_COOKIES` | `false` | Set `true` when serving over HTTPS |
| `KANI_CORS_ORIGIN` | — | Allowed CORS origin (e.g. `https://kani.example.com`) |
| `KANI_OIDC_ISSUER` | — | OIDC provider URL to enable single sign-on |
| `KANI_OIDC_CLIENT_ID` | — | OIDC client ID |
| `KANI_OIDC_CLIENT_SECRET` | — | OIDC client secret |
| `RUST_LOG` | `kani=info` | Tracing filter (e.g. `kani=debug,tower_http=warn`) |

## Health check

The container exposes `GET /health` which returns `200 OK` with the body `ok`.

```yaml
healthcheck:
  test: ["CMD", "curl", "-f", "http://localhost:8242/health"]
  interval: 30s
  timeout: 5s
  retries: 3
```

## Upgrade

```bash
docker compose pull
docker compose up -d
```

Kani applies SQLite migrations automatically on startup. A backup before upgrading is recommended — see [Backup & restore](../admin/backup-restore.md).

## Exit codes

| Code | Meaning |
|------|---------|
| `0` | Clean shutdown |
| `42` | Restart requested (e.g. after a settings change that requires restart) |

The `restart: unless-stopped` policy handles code 42 correctly.
