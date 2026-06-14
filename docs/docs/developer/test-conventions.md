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

## Database path

Integration tests use `sqlite:kani.db`. Run kani-app tests with `--features test-util`:

```bash
cargo test -p kani-app --features test-util
```
