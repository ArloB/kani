# Quickstart

This guide gets Kani running on your machine using Docker Compose. The full reference is in [Install with Docker](install-docker.md).

## Prerequisites

- Docker 24+ and Docker Compose v2+
- A machine with at least 1 GB of free RAM

## 1 — Create a compose file

Create a directory and a `docker-compose.yml` inside it:

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
```

## 2 — Start the server

```bash
docker compose up -d
```

Wait a few seconds, then open [http://localhost:8242](http://localhost:8242) in your browser.

## 3 — First-run wizard

Kani opens a setup wizard on first launch. It will ask for:

1. **Library path** — where downloaded chapters are stored inside the container (`/library` in the
   default compose file maps to `./library` on the host).
2. **Source install** — optionally install your first content source so you can start browsing immediately.

Complete the wizard and you land on the library screen.

## 4 — Install a source

Navigate to **Settings → Sources** and click **Browse**. Find a source, click its card, and
install it. Once installed it appears under **Sources** in the sidebar.

## 5 — Follow a series

Open a source, search for a title, open the manga detail page, and click **Follow**. Kani will
check for new chapters on its next scheduled scan.

## Next steps

- [Reverse proxy setup](reverse-proxy.md) — put Kani behind nginx or Caddy with HTTPS.
- [Users & roles](../admin/users-roles.md) — create additional accounts.
- [Sources admin guide](../admin/sources.md) — manage and update installed sources.
