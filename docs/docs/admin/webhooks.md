# Webhooks

!!! note "TODO"
    This page is a stub. Full content coming soon.

## Overview

Kani can POST a JSON payload to a URL of your choice when events occur (e.g. a new chapter is downloaded).

## Configuring a webhook

Navigate to **Settings → Webhooks → Add webhook**.

| Field | Description |
|-------|-------------|
| URL | The endpoint Kani will POST to |
| Events | Which event types trigger the webhook |
| Secret | Optional HMAC secret for payload verification |

## Event types

<!-- TODO: list event types and payload schemas -->

## Payload format

```json
{
  "event": "chapter.downloaded",
  "timestamp": "2026-01-01T00:00:00Z",
  "data": { ... }
}
```

## Verifying payloads

If a secret is configured, Kani sets an `X-Kani-Signature` header containing an HMAC-SHA256 hex digest of the raw body.

See also: [API — Webhooks](../api/webhooks.md).
