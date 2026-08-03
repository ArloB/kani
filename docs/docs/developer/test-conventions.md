# Test Conventions

## Test types

| Kind | Location | When to write |
|------|----------|--------------|
| Unit (pure, no I/O) | `#[cfg(test)] mod tests` in source file | Any new pure function |
| Integration (DB-backed) | `kani-app/tests/<area>_tests.rs` | New service methods touching SQLite |
| REST API | `kani-web/tests/<area>_api_tests.rs` | New HTTP endpoints |
| CLI / codegen | `kani-cli/tests/` | YAML rules, DSL changes, codegen output |

## Rules

- Every new pure function: at least one happy-path test and one edge/error test.
- New service methods taking `SqlitePool` or `&AppService`: integration test using `common::test_service()`.
- New REST endpoints: test triplet — 200 authed, 401 unauthed, 4xx invalid.
- All test files start with `#![allow(clippy::unwrap_used)]`.
- Snapshots (insta): commit `.snap` files; never commit `.snap.new`.

## REST test helpers (`kani-web/tests/common/mod.rs`)

| Helper | Purpose |
|--------|---------|
| `test_state()` | Create an `AppState` backed by an in-memory SQLite DB |
| `build_test_app(state)` | Build the Axum test router |
| `create_admin(app)` | Register an admin user, return credentials |
| `login(app, username, password)` | POST `/rest/auth/login`, return session cookie |
| `get_req(app, path, cookie)` | Authenticated GET, return `Response` |
| `post_json(app, path, body)` | Unauthenticated POST with JSON body |
| `authed_post(app, path, cookie, body)` | Authenticated POST with JSON body |
| `body_json(response)` | Extract `serde_json::Value` from response body |

## Trace a signal to a pixel

The recurring failure in this codebase is a mechanism that is built and never
connected: a probe with no caller, a column never selected into its listing, a
setting validated and persisted but read by nothing. Each looks complete in
isolation and each ships a feature that does nothing.

Follow a new value the whole way — computed → stored → selected → serialised →
rendered — and name the file at each hop before calling it done. Two traps this
project has hit repeatedly:

- **Hand-built JSON projections.** `/rest/manga/{id}/details` builds its
  response with `json!({...})` rather than serialising the model, so a field can
  exist on the struct, be returned by `/rest/manga/{id}`, and still be invisible
  to the page that renders the control.
- **State atoms are not pixels.** Adding an SSE handler that updates a store is
  the same defect one layer further out, unless something reads the store.

If a step is deliberately deferred, say so in the commit — silence reads as
wired.

## Permission-gated UI

The frontend hides whole surfaces behind `hasPermission(...)`, so the number of
possible layouts is combinatorial. Two checks cover it without enumerating
combinations:

- `kani-web/tests/permission_contract_tests.rs` — every `'resource:action'`
  literal the UI gates on parses as a server `Permission`. A typo there hides a
  feature silently and forever, with no error anywhere.
- `scripts/verify-permission-matrix.mjs` — per *permission* rather than per
  combination: for each one, an account that holds it and an account that does
  not, asserting the surfaces it gates appear in the first case and not the
  second. Expectations are read out of `static/js/app.js`,
  `static/js/pages/settings/index.js` and `static/locales/en.js`, so it cannot
  drift from the app.

```bash
node scripts/verify-permission-matrix.mjs http://127.0.0.1:8299 admin '<password>'
```

It needs Playwright and an instance you may create users on — it makes
`permmatrix-*` roles and accounts. Point it at a throwaway instance, and raise
`KANI_API_RATE_PER_SECOND` / `KANI_API_BURST_SIZE` on it: the sweep is heavy
enough to drain the limiter otherwise.

## Database path

Integration tests use `sqlite:kani.db`. Run kani-app tests with `--features test-util`:

```bash
cargo test -p kani-app --features test-util
```
