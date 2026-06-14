# Webhooks API

See also: [Admin guide — Webhooks](../admin/webhooks.md).

!!! note "TODO"
    Full webhook payload schemas coming soon.

## Payload envelope

All webhook payloads share the same envelope:

```json
{
  "event": "<event-type>",
  "timestamp": "2026-01-01T00:00:00Z",
  "data": { ... }
}
```

## Signature verification

If a secret is configured for a webhook endpoint, Kani sets:

```http
X-Kani-Signature: sha256=<hex-digest>
```

The digest is HMAC-SHA256 of the raw request body using the configured secret as the key.

Verification example (Python):

```python
import hashlib, hmac

def verify(secret: str, body: bytes, header: str) -> bool:
    expected = "sha256=" + hmac.new(
        secret.encode(), body, hashlib.sha256
    ).hexdigest()
    return hmac.compare_digest(expected, header)
```

## Event types

| Event | Trigger |
|-------|---------|
| `chapter.downloaded` | A chapter download completed successfully |
| `chapter.failed` | A chapter download failed after all retries |
| `library.updated` | Library metadata was refreshed |

<!-- TODO: document full data schemas for each event type -->
