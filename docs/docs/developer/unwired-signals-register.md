# Unwired Signals Register

Things that are computed, stored, validated or exposed but never reach a
consumer. This project has a recurring failure of exactly this shape — a
mechanism that looks complete in isolation and ships a feature that does
nothing — so the register is kept rather than the findings being closed and
forgotten.

Swept 2026-07-22 across `kani-app`, `kani-web`, `kani-core`, `static/js` and
`migrations/`. Statuses are accurate as of commit `ddda1a8`.

| # | Signal | Consequence when unwired | Status |
|---|---|---|---|
| 1 | `user_sessions.revoked_at` / `is_session_valid` | **Security.** "Sign out this device" returned 204, hid the row, and the cookie kept working forever | **FIXED** — enforced in `session_touch_middleware`; renamed `is_session_revoked` |
| 2 | `Settings.concurrent_manga_downloads` | Rendered, validated, persisted, backed up; consumed by nothing | **REMOVED** — duplicated `per_source_download_concurrency` × `max_concurrent_jobs` |
| 3 | `Settings.chapter_queue_size` | As above, and shipped a tooltip describing deferral behaviour that did not exist | **REMOVED** |
| 4 | `chapters.file_verified_at` | Written by every scrub and download, read by none, so every scrub re-hashed the whole library | **FIXED** — `integrity_revalidate_after_days` (default 30); skips hashing only, never the existence check |
| 5 | `chapters.quality_long_edge` / `quality_bytes_per_mp` | Written per download, never read; comparator re-parsed `manifest_json` every scan, and the columns could not answer the colour/encoder axes anyway | **FIXED** — completed with `quality_encoder`/`quality_colour` into a real fast path, manifest fallback retained |
| 6 | `sources.streaming_chapters` | Gate nothing set: capability always false, its UI blurb never rendered, trigger endpoint always refused — and did no work even if opened | **FIXED** — capability is universal (host-side polling), column dropped, redundant stub endpoint removed |
| 7 | `AppEvent::UpgradesFound` | Broadcast on every scan, handled by nobody; upgrades were invisible until you navigated there | **FIXED** — notifications-panel count linking to `/upgrades` |
| 8 | `AppEvent::CircuitOpen` | A tripped breaker was silent; requests just began failing | **FIXED** — toast naming the host |
| 9 | `notify_new_chapters` | Read only via a Map populated for manga opened this session, defaulting to notify; toggle silently stopped working after a reload | **FIXED** — `GET /rest/me/notify-prefs`, loaded on SSE connect |
| 10 | YAML `filter_mapping` / `filter_format` | Interpreted sources rendered the filter panel, accepted a selection and sent an unfiltered request | **FIXED** — `kani_yaml::apply_filters`, shared semantics |
| 11 | `source_circuit_breakers.opened_at` | Never written, never read | **REMOVED** |
| 12 | `user_manga_tracking.reading_layout` | Zero references in Rust or JS; the per-manga layout override was never implemented | **REMOVED** |
| 13 | `repos::unblock_repo` | No callers; `delete_blocked_repo` is the wired path | **REMOVED** |
| 14 | SSE type `scan_complete` | Handled in `manga-details.js`, never emitted — not an `AppEvent` variant | **REMOVED** |
| 15 | `AppEvent::SourceUpdating` | Emitted by `repos.rs`, unhandled in JS — no in-progress indicator while a source updates | **OPEN** |
| 16 | `AppEvent::ImportStarted` | Emitted, unhandled — import UI stays blank until the first item, so a slow import looks hung | **OPEN** |
| 17 | `Settings.auto_download_category_id` (singular) | Superseded by the plural; still SELECTed and shipped to every client, read by nothing | **OPEN** — removal is an API shape change |
| 18 | `JobContext.sse_tx` | Populated for every job, used by none (`#[allow(dead_code)]`) | **OPEN** |
| 19 | `FilterMappingEntry::{SortPair,TupleSplit}.kind` | Deserialised from extension YAML, validated by serde, discarded | **OPEN** |
| 20 | `pause_job` / `resume_job` | Permanent `422` stubs with OpenAPI docs | **FIXED** — implemented for queued jobs, running jobs refuse with a reason, and wired to buttons on the jobs page |
| 21 | `progress::get_noted_chapter_ids` | Chapter notes exist; "which chapters have notes" is never surfaced | **OPEN** — wiring it is a UI feature (note indicator), not a deletion |
| 22 | 23 `api.js` exports with zero callers | Each is a shipped backend capability with no way to reach it | **OPEN** — see below |
| 23 | 6 REST routes with no frontend caller | `/admin/db/{analyze,stats,vacuum}`, `/admin/recurring/{kind}/run`, `/chapters/{id}/cbz`, `/scan/toggle_auto` | **OPEN** |

**Corrected during the sweep:** `create_api_token` was reported as having zero
callers. It in fact had *test* callers only — which is not a production call
site, so it was still dead by the standard that matters. It has been removed and
those tests now call `create_token(.., TokenKind::Opds, None)`, the same entry
point the REST handler uses. Worth keeping the distinction in mind: "no
production callers" and "no callers" are different claims, and only the first
one decides whether something is dead.

### #22 — the uncalled `api.js` exports

`getMe` · `adminTriggerPasswordReset` · `getSourceMetadata` · `getPages` ·
`getRepo` · `getManga` · `scanAllLibrary` · `getRefreshStatus` ·
`syncAllTrackers` · `downloadBackup` · `previewBackup` · `restoreBackup` ·
`resolvePendingImport` · `getNotedChapterIds` · `getReadingPace` · `stepUpTotp` ·
`regenerateBackupCodes` · `purgeAdminLogs` ·
`getMangaDownloadStatus` · `assignChapterVolume` · `getCollectionManga` ·
`updateSavedSearch` · `getMangaUpgrades`

These are not one problem. `pauseJob`/`resumeJob` have since been implemented
and wired to the jobs page, so 23 remain. Some are **missing UI for a working
backend**
(`restoreBackup`, `previewBackup`, `regenerateBackupCodes`, `stepUpTotp`) and
deleting them would discard a shipped capability. Each needs a wire-or-remove
decision; they should not be batch-processed.

## How to avoid adding to this list

Trace a new value the whole way — computed → stored → selected → serialised →
rendered — and name the file at each hop before calling it done. Two specific
traps this codebase has hit repeatedly:

- **Hand-built JSON projections.** `/rest/manga/{id}/details` builds its
  response with `json!({...})` rather than serialising the model, so a field can
  exist on the struct, be returned by `/rest/manga/{id}`, and still be invisible
  to the page that renders the control.
- **State atoms are not pixels.** Adding an SSE handler that updates a store is
  the same defect one layer further out unless something reads the store.
