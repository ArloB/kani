# Request Flow

!!! note "TODO"
    This page is a stub. Full content coming soon.

## HTTP request lifecycle

```text
Client
  │
  ▼
nginx / Caddy (TLS termination)
  │
  ▼
Axum (kani-web)
  ├─ Static assets → served directly from embedded static/
  ├─ /health → inline handler
  ├─ /api-docs → Swagger UI (debug builds only)
  └─ /rest/* → auth_guard middleware
                  │
                  ├─ is_public_path() → bypass auth
                  └─ AuthGuard<Permission> → validate session cookie
                       │
                       └─ Route handler
                            │
                            └─ AppService method (kani-app)
                                 │
                                 └─ SqlitePool / WasmRuntime / ...
```

## Extension invocation flow

```text
REST handler (e.g. search manga)
  │
  ▼
AppService::search_manga()
  │
  ▼
SourceManager::get_source(source_id)
  │
  ▼
WasmRuntime::call_search()        ← WASM boundary
  │
  ▼
Extension guest (manga-provider export)
  │
  ├─ http::send() → HTTP request to external site
  ├─ html::parse() → doc-handle
  └─ extraction::extract(blueprint_bytes) → json-handle
       │
       ▼
    Host evaluator (html_eval.rs / json_eval.rs)
       │
       ▼
    JSON result deserialized back in guest
       │
       ▼
Extension returns MangaList via WIT
  │
  ▼
AppService serializes to REST response DTOs
```
