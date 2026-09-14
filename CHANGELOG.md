# Changelog

All notable changes to Kani are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Kani uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [1.0.0-rc.3] - 2026-09-14

### Added

- Bulk migration. Select titles in the library and choose Migrate: pick a target source, review
  the proposed match for each title (or pick another candidate, search again, or skip it), then
  migrate. Each title becomes its own migration job, and a title that fails reports why without
  stopping the rest. Dates in titles are ignored when matching, but sequel numbers are not.
- Filter the library by source. The Library tab on a source's page is removed; old links to it
  open the library with that source filter applied.
- A cover quality setting under Display (Low, Medium, High; High by default). Library covers were
  small and grainy on high-density screens.
- A tile size setting for the library, source browse and search rails. Grids on larger screens
  fit whole tiles to the window instead of scrolling the page.
- Imported Tachiyomi and Mihon manga whose ids a source cannot address are repaired by a
  background queue. Library cards and the manga page show that a title is waiting, and a title
  that cannot be repaired explains why and offers a migrate action.
- Library search shows which field a result matched on when it was not the title.
- Library cards show the orphaned badge.
- A manga's refresh action has its own row on the manga page instead of sitting in a dialogue.
- Chapter lists show page numbers for sources that return the whole list at once, and for MangaDex
  (extension 0.1.1).
- `kani-cli repl request` prints the request an endpoint would send — route, filter defaults,
  pagination offset and queries — without sending it. `repl record` applies filter defaults, and
  `repl test` and `repl replay` run the real evaluator.
- YAML extension hooks can read and rewrite response bodies, read and set query parameters, call
  shared `scripts.pure` functions, and use byte primitives (base64url, UTF-8 and keyed substitution)
  that run natively.

### Changed

- Browsing sources is faster. Cached covers no longer wait on the per-host throttle, proxied cover
  URLs no longer change every hour (so the browser can keep them), and image requests no longer
  each write to the session table.
- Phones always use infinite scroll for the library, source browse and chapter lists, and the
  Pagination settings are hidden there.
- Global search keeps every source's rail the same height, including sources that found nothing,
  places the scrollbar under the covers, and no longer scrolls the whole page sideways. A source
  that timed out says so and offers a retry instead of reporting no matches.
- The sources page and a source's page share one header.
- Loading more titles in a source list shows placeholder cards inside the grid.
- Page transitions fade faster.
- Scheduled backups default to a `backups` directory next to the database.
- Tachiyomi and Mihon imports match sources by the source list the backup itself carries, as well
  as by declared Mihon ids. Importing chapter progress now overwrites existing progress, and
  progress for a manga whose chapters are not listed yet is applied once they are.
- Duplicate warnings no longer flag a numbered sequel as a duplicate of the original series.
- The bundled solver's port is no longer published to the host in `docker-compose.yml`; only the
  Kani container can reach it.
- Dependencies are updated to their latest patch releases, including wasmtime 46.0.3, axum 0.8.9,
  reqwest 0.13.4 and serde 1.0.229, alongside arc-swap 1.9.2, bytes 1.12.1, regex 1.13.1 and insta
  1.48.0.

### Fixed

- Opening an undownloaded chapter showed "No pages found" once its download finished, until the
  page was refreshed.
- Importing a backup that names a manga already in the trash reported it as imported even though
  it stayed hidden. The import now reports it separately and leaves the trash untouched.
- Uploading an extension or a cover, previewing or importing a Tachiyomi backup, and previewing or
  restoring a Kani backup were rejected by CSRF protection.
- Statistics charts did not load in release builds.
- Onboarding showed the sidebar and bottom navigation, and finishing it through "browse sources"
  did not mark first-run setup complete.
- Some dropdowns, including the library filters and display menus and the jobs filter, did not
  open in production builds.
- A source installed from a repository, or a change to a source's preferences, did not appear
  until the page was reloaded.
- Source health counted configuration reads as source calls, so error counts were misleading.
- Browser-backed sources returned their own page size instead of the number of titles requested.
- Paginated YAML sources showed a single page because their total page count was dropped.
- Chapter selection could be opened by long-pressing a chapter of a manga not in the library.
- Holding a chapter row left a focus ring stuck on the first row.
- In infinite scroll, the library's category tabs collapsed under the filter bar; long category
  names also wrapped instead of scrolling.
- The library grid ran off the side of the screen on phones.
- The continue-reading shelf query was slow on large libraries.
- The migration dialogue always reported zero chapters migrated, and reported failure to a user
  without job administration rights even when the migration succeeded.
- Four-digit page numbers overflowed their pagination buttons.

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
