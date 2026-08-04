# Test Conventions

## Test placement

| Kind | Location | Use |
|---|---|---|
| Pure unit | `#[cfg(test)] mod tests` beside the code | New deterministic logic |
| Service integration | `kani-app/tests/<area>_tests.rs` | SQLite-backed service behavior |
| REST contract | `kani-web/tests/<area>_api_tests.rs` | Routing, auth, validation, and serialization |
| CLI/YAML | `kani-cli/tests` and `kani-yaml` tests | Schema, DSL, generation, and commands |
| ABI/runtime | `kani-core/tests` | WIT, WASM host, extraction, and network behavior |
| Browser contract | repository scripts | Permission-gated and responsive UI workflows |

Standalone test files begin with `#![allow(clippy::unwrap_used)]`. Commit accepted Insta snapshots
and never commit `.snap.new` files.

## Minimum behavior coverage

- Give every new pure function a happy path and an edge or error case.
- Test DB-backed service methods through the shared test service.
- Give a new REST endpoint authenticated success, unauthenticated failure, and invalid-input
  coverage, plus permission denial where authentication alone is insufficient.
- Test what a shared-path change displaces: rename, delete, restore, retry, migration, re-download,
  and rollback paths are common hidden consumers.
- Use deterministic local HTTP origins or recorded fixtures instead of making unit tests depend on
  a live content site.

## REST helpers

`kani-web/tests/common/mod.rs` builds an `AppState`, router, administrator, login cookie, JSON
requests, and response decoding for integration tests. Read the current helper signatures before
using them; do not reproduce a second test harness in each file.

OpenAPI route coverage is a separate contract: adding or removing a REST route requires the
generated document and its coverage test to change together.

## Trace a signal to a pixel

For a new value, identify every hop:

```text
compute -> persist -> select -> serialize -> client cache -> render
```

Tests should fail when any required hop is disconnected. Pay special attention to hand-built JSON
objects, queries that omit new columns, settings saved but never read, and SSE handlers that update
state no component consumes.

## Permissions

The server contract test ensures every frontend permission literal parses. The permission-matrix
browser script then checks each gated surface with an account that has and lacks that permission.
Run it against a disposable instance because it creates roles and accounts:

```bash
node scripts/verify-permission-matrix.mjs http://127.0.0.1:8299 admin '<password>'
```

Install Playwright in a scratch environment and raise development rate limits for the sweep.

## Commands

```bash
cargo test
cargo clippy --locked --no-deps -- -D warnings
cargo fmt --all --check
```

Do not use `--workspace`; extension members are WASM-only. Use focused package/test filters while
iterating, then run the default-member suite before handoff.
