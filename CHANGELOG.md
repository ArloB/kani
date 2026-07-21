# Changelog

All notable changes to Kani are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Kani uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.9.0] - 2026-07-21

Pre-1.0 stabilisation release. The runtime and feature set are mature; this
series exists to prove the release, observability and data-safety machinery
before 1.0 makes its compatibility promises.

### Added

- Structured JSON logging via `KANI_LOG_FORMAT=json`, and an `x-request-id` trace ID on every
  response (echoed back when supplied) that also appears in log lines and error toasts.
- Slow SQL statement logging, tunable with `KANI_SLOW_QUERY_THRESHOLD_MS`.
- Prometheus metrics at `/metrics`, requiring an API token scoped to `metrics:read`.
- Diagnostics admin page: version, uptime, database and disk usage, jobs, extension load state,
  browser runtime, circuit breakers and proxy bandwidth.
- Downloadable support bundle with redacted settings, schema and recent logs.
- Daily update check with a dismissible banner, toggleable in Settings → Advanced.
- Opt-in error reporting, gated on both `KANI_GLITCHTIP_DSN` and a setting that defaults to off.
- `kani-cli rollback <backup.zip>` verifies a backup archive can be restored onto this build.
- `/healthz` and `/readyz` aliases for the existing health probes.

### Fixed

- Download retries: `ExtensionErrorKind` is preserved through the download pipeline, so transient
  source failures are retried with backoff instead of being treated as permanent.
