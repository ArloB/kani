# Local Development

## Prerequisites

- Rust toolchain (stable + `wasm32-unknown-unknown` target)
- Node.js 20+ (for esbuild and the i18n check script)
- `wasm-opt` and `wasm-tools` on `PATH`
- Docker (optional, for integration testing against a real DB)

## First-time setup

```bash
cargo run -p kani-cli -- setup
```

This fetches JS vendor files, downloads the Tailwind CLI binary, and configures git hooks.

## Running the server

```bash
cargo run -p kani-web
```

Open [http://localhost:8242](http://localhost:8242).

## CSS (Tailwind)

```bash
cargo run -p kani-cli -- css --watch
```

## Building extensions

```bash
cargo run -p kani-cli -- build kani-weebcentral
cargo run -p kani-cli -- build --all
```

Output: `wasm_sources/<name>.wasm`.

## Tests

```bash
cargo test                                      # all non-extension crates
cargo test -p kani-web --test system_api_tests  # specific integration test
```

Extension crates target `wasm32-unknown-unknown` and are excluded from `default-members` — do not use `--workspace`.

## SQLx offline mode

All SQL queries are pre-validated. After changing a query:

```bash
cargo sqlx prepare --workspace -- --all-targets
```

Commit the updated `.sqlx/` directory. Build without a live database:

```bash
SQLX_OFFLINE=true cargo build
```

## Linting

```bash
cargo clippy --workspace -- -D warnings
```

`unwrap_used` is denied workspace-wide — use `?`, `unwrap_or`, `expect`, or `match`.

## i18n check

```bash
node scripts/check-i18n-keys.js
```

Fails if any `t("key")` call in `static/js/**/*.js` references a key not defined in `_catalog` (`static/js/i18n.js`).
