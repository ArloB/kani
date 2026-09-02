# Changelog

All notable changes to Kani are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Kani uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [1.0.0-rc.2] - 2026-09-03

### Fixed

- The container failed to boot against a freshly created bind mount for `/data` or `/library`:
  Docker creates the host directory owned by root, and the `kani` user (a fixed UID/GID 1000,
  with no `PUID`/`PGID` remapping) couldn't write to it. The container now starts as root,
  chowns both mount roots, and drops to the unprivileged `kani` user via `setpriv` before
  running Kani itself.

## [1.0.0-rc.1] - 2026-09-02

Release candidate. Soaking as the daily driver before tagging `v1.0.0`.

### Removed

- Kani no longer runs its own browser. Browser capture happens in the configured solver, so the
  `KANI_BROWSER_ENABLED`, `CHROMIUM_PATH`, and `BROWSER_IDLE_TIMEOUT_MS` environment variables are
  gone, along with the `kani_browser_reuses_total`, `kani_browser_recovery_launches_total`,
  `kani_browser_challenges_total`, and `kani_browser_page_close_timeouts_total` metrics. Dashboards
  referencing those series need updating; `kani_browser_solver_*` remains and now covers every
  capture.

### Changed

- The migration history is consolidated into a single baseline. An existing database that has
  applied every prior migration is adopted automatically at startup and nothing else changes. One
  whose history is incomplete — an upgrade interrupted part-way, or a migration recorded as failed
  — now stops with an error naming the offending version instead of proceeding against a schema it
  cannot verify. Restore a backup taken before the interrupted upgrade and start again.
- `kani-cli rollback` is renamed `kani-cli backup-verify`. Its behaviour is unchanged: it checks
  whether a backup archive can be restored onto this build and performs no restore itself. The
  name is freed for a command that actually rolls back, which needs the deferred `kani-cli` async
  restructure.

## [0.9.0] - 2026-07-21

Pre-1.0 stabilisation release focused on release processes, observability, and data safety.

### Added

- Structured JSON logging via `KANI_LOG_FORMAT=json`, and an `x-request-id` trace ID on every
  response (echoed back when supplied) that also appears in log lines and error toasts.
- Slow SQL statement logging, tunable with `KANI_SLOW_QUERY_THRESHOLD_MS`.
- Prometheus metrics at `/metrics`, requiring an API token scoped to `metrics:read`.
- Diagnostics admin page: version, uptime, database and disk usage, jobs, extension load state,
  browser runtime, circuit breakers and proxy bandwidth.
- Downloadable support bundle with redacted settings, schema and recent logs.
- Daily update check with a dismissible banner, toggleable in Settings → Advanced.
- `kani-cli rollback <backup.zip>` verifies a backup archive can be restored onto this build.
- `/healthz` and `/readyz` aliases for the existing health probes.

### Fixed

- Download retries: `ExtensionErrorKind` is preserved through the download pipeline, so transient
  source failures are retried with backoff instead of being treated as permanent.
