# Crate Map

## Workspace members

```text
kani/
├── kani-core/          # WASM runtime + host ABI
├── kani-shared/        # Types shared by host and guest
├── kani-app/           # Business logic (DB-backed services)
├── kani-web/           # Axum HTTP server + REST API
├── kani-cli/           # Developer tooling
└── kani-extensions/
    ├── kani-example/   # Minimal extension template
    ├── kani-test-abi/  # ABI contract tests
    └── kani-*/         # Community extensions
```

Extension crates (`kani-extensions/*`) target `wasm32-unknown-unknown` and are excluded from
`default-members` — run `cargo test` (not `cargo test --workspace`) to avoid native compilation
failures.

## kani-core

WASM runtime, content extraction, and asset pipeline.

| Module | Purpose |
|--------|---------|
| `runtime/` | Wasmtime component instantiation, host function registration |
| `evaluator/html_eval.rs` | Blueprint evaluation against HTML (`scraper` crate) |
| `evaluator/json_eval.rs` | Blueprint evaluation against JSON (`serde_json`) |
| `downloader/` | HTTP download queue, retry logic, progress events |
| `cbz/` | CBZ read/write and ComicInfo.xml (de)serialisation |
| `wit/kani.wit` | WIT interface definition |

## kani-shared

Zero-dependency crate importable by both host Rust and WASM guest code.

| File | Purpose |
|------|---------|
| `ast.rs` | `Expr`, `Blueprint`, `BlueprintBuilder` |
| `host_abi.rs` | RAII handle wrappers, `extract::html` / `extract::json` helpers |
| `lib.rs` | `MangaExtension` trait, `ExtensionResult`, `ExtensionError` |
| `types.rs` | Shared DTOs (`AppSettings`, `MangaDto`, `ChapterDto`, …) |

## kani-app

All business logic. No HTTP knowledge — pure service layer over SQLite.

| Module | Purpose |
|--------|---------|
| `service/mod.rs` | `AppService` — the central handle |
| `service/library.rs` | Library CRUD, cover management |
| `service/chapters.rs` | Chapter list, read progress |
| `service/downloads.rs` | Download scheduling and state |
| `service/settings.rs` | Settings singleton read/write |
| `service/sources.rs` | Extension install, update, list |
| `service/trackers.rs` | AniList / MAL sync |
| `service/backup.rs` | Backup export and restore import |
| `service/webhooks.rs` | Outbound webhook delivery |
| `models.rs` | `sqlx` row structs |
| `ids.rs` | Typed ID newtypes (`MangaId`, `ChapterId`, `UserId`, `SourceId`) |

## kani-web

Axum HTTP layer. Delegates all logic to `AppService`.

| Module | Purpose |
|--------|---------|
| `rest/` | Per-domain route modules |
| `rest/system.rs` | `GET /rest/system/info`, `POST /rest/system/first-run-complete` |
| `rest/library.rs` | Library listing and management |
| `rest/manga.rs` | Manga detail and metadata |
| `rest/chapters.rs` | Chapter list and read progress |
| `rest/auth.rs` | Login, logout, OIDC callback |
| `rest/admin.rs` | Admin-only endpoints |
| `auth.rs` | `auth_guard` middleware, `is_public_path()` allow-list |
| `permissions.rs` | Permission constants and guards |
| `openapi.rs` | `ApiDoc` struct (utoipa) |
| `app.rs` | `build_app()` — testable router factory |
| `main.rs` | Binary entry point |

## kani-cli

Extension authoring toolchain and dev utilities.

| Module | Purpose |
|--------|---------|
| `yaml/` | YAML extension schema, validator, and Rust codegen |
| `dsl/` | DSL parser (converts YAML to `Expr` AST) |
| `build.rs` | `cargo run -p kani-cli -- build` — WASM compile + opt + component |
| `css.rs` | Tailwind CSS pipeline |
| `setup.rs` | First-time dev environment setup |
| `repl.rs` | Interactive REPL for extension development |

## Dependency graph (simplified)

```text
kani-web ──► kani-app ──► kani-core ──► kani-shared
kani-cli                              ──► kani-shared
kani-extensions/* ────────────────────► kani-shared (WASM guest)
```

`kani-shared` is the only crate imported by both the host (`kani-core`, `kani-app`, `kani-web`)
and the WASM guest (`kani-extensions/*`). It must remain `no_std`-compatible and free of
host-only dependencies.
