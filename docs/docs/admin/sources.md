# Sources

!!! note "TODO"
    This page is a stub. Full content coming soon.

## What is a source?

A source is a WASM extension that teaches Kani how to browse, search, and download manga from a
specific website. Sources are sandboxed — they run inside the WASM runtime and cannot access the
host filesystem.

## Installing a source

1. Navigate to **Settings → Sources → Browse**.
2. Find the source and click its card.
3. Click **Install**.

## Updating sources

Kani checks for source updates periodically. To update manually: **Settings → Sources → [source name] → Update**.

## Extension permissions

| Permission | What it allows |
|------------|---------------|
| `unrestricted_http` | Make HTTP requests to any URL, not just the declared base domain |

Review extension permissions before installing sources from untrusted repositories.

## Source repositories

<!-- TODO: document the official extension repository URL and how to add third-party repos -->

## Writing a source

See [Extension authoring](../extension-authoring/yaml-schema.md).
