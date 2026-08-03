# Architecture

Kani is a manga/anime server built around a WASM plugin system. Content sources ("extensions") are
compiled to WASM Components; the host runtime loads and sandboxes them, then exposes a DOM/JSON
extraction API so extensions can scrape external websites without owning the networking code.

## Crate layout

| Crate | Role |
|-------|------|
| `kani-core` | WASM runtime, host ABI, extraction engine, CBZ/ComicInfo, downloader, V8 subprocess |
| `kani-shared` | Shared types and traits for both host and guest |
| `kani-app` | Business logic: library, chapters, downloads, trackers, categories, scanlators, OPDS, email, webhooks, backup/restore, import, dedup, audit, export, encryption, preferences, settings, stats |
| `kani-web` | Axum HTTP server, REST API, RBAC auth, frontend serving |
| `kani-cli` | Extension tooling: YAML schema, DSL parser, codegen, build orchestration, CSS/icon/setup |
| `kani-extensions/*` | Individual extension WASM modules |

## WIT interface

Extensions are WASM Components built against a WIT world (`kani-extension`, defined in `kani-core/wit/kani.wit`).

**Host provides (imports into guest):** `http`, `html`, `json`, `utility`, `prefs`, `extraction`

**Guest exports:** `manga-provider`

All cross-boundary values are opaque integer handles allocated on the host:

| Handle type | Meaning |
|-------------|---------|
| `doc-handle` | Parsed HTML document |
| `list-handle` | Element list from a selector query |
| `json-handle` | JSON value tree |

Handles are freed explicitly. RAII wrappers in `kani-shared/src/host_abi.rs` (`HtmlDocument`, `JsonHandle`) do this automatically.

## Declarative extraction

The primary performance primitive. Instead of the guest driving DOM traversal via hundreds of FFI
calls, it serialises a `Blueprint` and sends it across the boundary in a single call. The host
evaluates it natively.

| Symbol | File | Role |
|--------|------|------|
| `Expr` | `kani-shared/src/ast.rs` | DSL AST, ~40 variants |
| `Blueprint` | `kani-shared/src/ast.rs` | Container selector + field definitions + bindings + scalars |
| `BlueprintBuilder` | `kani-shared/src/ast.rs` | Fluent builder; all methods `#[inline]`, zero-cost |
| HTML evaluator | `kani-core/src/evaluator/html_eval.rs` | Evaluates against a parsed document |
| JSON evaluator | `kani-core/src/evaluator/json_eval.rs` | Evaluates against a JSON value tree |

Serialisation: `postcard`. Output: `{ "rows": [...], "scalars": {...} }`.

Preferences are injected as `$pref:key` — use `Expr::pref("key")` in DSL.

## Extension pattern

Every extension implements two traits:

1. `MangaExtension` (`kani-shared/src/lib.rs`) — the actual logic.
2. `Guest` (WIT binding) — thin delegation via a `OnceLock<T>` singleton.

Key helpers from `kani_shared::host_abi::extract`:

- `extract::html(doc_handle, &blueprint)` — evaluate a Blueprint against HTML.
- `extract::json(handle, &blueprint)` — evaluate a Blueprint against JSON.

Attach HTTP requests via `.request(HttpRequest::get(...))` on the Blueprint to avoid a separate `send_html()` call.

Return type: `ExtensionResult<T>` = `Result<T, ExtensionError>`.

## HTTP layer

`kani-web` is an Axum 0.8 server. Routing:

- REST API under `/rest/` — per-domain modules in `kani-web/src/rest/`, each exposing
  `pub fn router() -> Router<AppState>`, merged in `rest::routes()`.
- Static assets and SPA fallback — served from the `static/` directory embedded at compile time.
- Swagger UI — mounted at `/api-docs` in debug builds only; release builds omit it.

Auth is a global `auth_guard` middleware layer with an `is_public_path()` allow-list
(`kani-web/src/auth.rs`). Role-based access: permissions are `resource:action` strings
(e.g. `library:view`, `source:install`, `admin:manage`).

## AppService

`AppService` (`kani-app/src/service/mod.rs`) is the central application handle passed through the Axum state. It holds:

- `SqlitePool`
- `WasmRuntime`
- `SourceManager` map
- `DownloaderManager`
- SSE broadcast channel
- `TrackerRegistry`
- `EmailService`
- optional `CredentialCipher`
- `WebhookService`
- `settings: RwLock<Settings>` — in-memory cache of the singleton settings row

`kani-web`'s `AppState` derefs to `AppService`, so handlers call `state.get_settings().await` directly.

## Frontend

Vanilla JS SPA (`static/js/`). Build: esbuild bundles into `static/js/dist/`. Always edit source files, never `dist/`.

Key infrastructure files:

| File | Role |
|------|------|
| `api.js` | HTTP client, all REST calls |
| `router.js` | SPA routing, page lifecycle |
| `state.js` | Shared state management |
| `sse.js` | Server-Sent Events (download progress) |
| `i18n.js` | `t("key")` translation helper |
| `components/` | Reusable UI components |
| `pages/` | Page modules exported as `init(container, params)` |

## Database

SQLite via `sqlx`. Schema managed by migrations in `migrations/`. Offline query metadata in
`.sqlx/` (committed). Build without a live DB: `SQLX_OFFLINE=true cargo build`.
