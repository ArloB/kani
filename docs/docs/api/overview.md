# API Overview

Kani exposes a REST API under `/rest/`. Debug builds also serve an interactive
Swagger UI at `/api-docs`; release builds — including the published Docker
image — do not, so treat this page and the OpenAPI document itself as the
reference.

## Base URL

```text
http://localhost:8242/rest
```

## Authentication

All endpoints except the ones listed below require an authenticated session. See [Authentication](auth.md) for details.

### Public endpoints

| Endpoint | Description |
|----------|-------------|
| `GET /rest/system/info` | Server version and first-run state |
| `GET /rest/auth/*` | The whole auth surface: login, registration, password reset, e-mail verification, captcha, and first-run setup |
| `GET /health`, `/healthz`, `/ready`, `/readyz` | Liveness and readiness probes |
| `GET /metrics` | Prometheus metrics |
| `GET /opds/*` | OPDS catalog (authenticates per-handler, and accepts HTTP Basic) |

## Response format

All responses return JSON. Successful responses use the `2xx` range. Errors use the appropriate
`4xx` / `5xx` code with a JSON body:

```json
{
  "error": "Unauthorized",
  "message": "Session required"
}
```

## Pagination

List endpoints accept `page` and `page_size` query parameters and return a paginated envelope:

```json
{
  "items": [...],
  "total": 100,
  "page": 1,
  "page_size": 20
}
```

## Interactive docs

Debug builds serve the full Swagger UI at `/api-docs`, with live request
testing. A release build does not mount it — build from source with
`cargo run -p kani-web` if you want to explore the surface interactively.
