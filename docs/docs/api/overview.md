# API Overview

Kani exposes a REST API under `/rest/`. An interactive Swagger UI is available at `/api-docs` when the server is running.

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
| `POST /rest/auth/login` | Create a session |
| `GET /rest/auth/oidc/callback` | OIDC callback |

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

Open `/api-docs` in your browser while Kani is running for the full Swagger UI with live request testing.
