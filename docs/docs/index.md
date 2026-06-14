# Kani

Kani is a self-hosted manga and comics server. It downloads chapters from online sources, organises
your library, and serves a reading interface to every device on your network.

## Highlights

- **Extension system** — content sources are WASM plugins; install community extensions or write
  your own in Rust or the declarative YAML DSL.
- **Automatic downloads** — schedule chapter downloads; Kani polls for new releases and queues them
  without manual intervention.
- **CBZ / ComicInfo** — imports existing CBZ archives; writes ComicInfo.xml metadata for compatibility with other readers.
- **Tracker sync** — links progress to AniList and MAL so your reading history stays in one place.
- **OPDS catalog** — browse and download from any OPDS-capable reader app.
- **Role-based access** — multiple users with per-user permissions; optional OIDC single sign-on.
- **Webhooks & email** — notify external services or your inbox when new chapters arrive.

## Quick links

- [Quickstart](getting-started/quickstart.md) — up and running with Docker in five minutes.
- [Install with Docker](getting-started/install-docker.md) — full Docker reference.
- [Install from binary](getting-started/install-binary.md) — run the server directly.
- [Architecture](developer/architecture.md) — how the crates fit together.
- [Extension authoring](extension-authoring/yaml-schema.md) — write a source extension.

## Screenshots

*Screenshots coming soon.*

## License

Kani is released under the MIT licence. See
[DISCLAIMER](https://github.com/ArloB/kani/blob/main/DISCLAIMER.md) for content liability notes.
