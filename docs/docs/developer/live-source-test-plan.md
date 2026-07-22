# Live-Source Test Plan

Every test below needs a server that **misbehaves on cue** — a listing that changes
between scans, a body that stops short of its `Content-Length`, an index that rotates its
signing key. None of it is expressible against a real source, which is why all of it is
currently unverified.

The harness is `kani_shared_test::origin::TestOrigin` (see
[test-conventions.md](./test-conventions.md) for the general rules). Tests live in
`kani-app/tests/live_source_tests.rs` unless noted otherwise.

```rust
let origin = TestOrigin::start().await;
origin.set("/img/a.jpg", Response::image(jpeg_page(1600, 2400, false, 85)));
origin.script("/chapters/m1", vec![listing_a, listing_b]); // changes between calls
origin.ignore_range(true);
origin.hits("/chapters/m1");
```

Loopback IP literals bypass `ValidatingResolver`, so the **production** `SmartClient`
reaches the origin — these drive the real request path, not a stub.

Status legend: **BUG** = confirmed defect, write the test red then fix. **GAP** = behaviour
believed correct but unexercised.

---

## Harness additions needed first

Several groups below are blocked on these. Build them before starting.

| ID | Addition | Unblocks |
|---|---|---|
| H1 | Route on **request headers**, not just path (e.g. serve challenge until `Cookie: cf_clearance=…` appears) | Group C |
| H2 | `manga_details` endpoint + cover routes in `wire_source` | Group B |
| H3 | Echo route returning the received method, path, query and headers as JSON | A1, A10, I1–I4 |
| H4 | A second `TestOrigin` fixture helper (cross-host tests) | A2, A7, C6, E3 |
| H5 | `Body::Slow { bytes, per_chunk_delay }` — trickle rather than stall outright | D2, D3, K2 |

---

## Which backend runs which test

There are two source backends and they share less than the architecture suggests.
`YamlSource::eval_endpoint_once` builds a `Blueprint` and calls the **same**
`html_eval`/`json_eval` the WASM host runs for `extract::html` — so extraction is one
implementation. `YamlSource::make_host_state()` also builds the same
`kani_core::wasm::HostState`, so the 32-request IO budget and `AllowedHost` are shared too.

Everything *wrapping* the Blueprint is written twice:

| Concern | Interpreted YAML | WASM guest |
|---|---|---|
| Request construction | `make_request` + `build_url_with_args` | guest builds `HttpRequest` |
| **Filters** | **discarded** (A1) | guest handles them |
| Preferences | `$pref:` in the eval env | `prefs` host import |
| Pagination, `has_next_page`, `total_pages` | `ValidatedHnp` / `PaginationCfg` | guest logic |
| Composite id encode/decode | `resolve_composite_ids` | guest |
| Result unpacking | `unpack_*`, swallows every mismatch | guest constructs the structs |
| Error kind | collapsed to `ExtensionError::parse` (A15) | real `ExtensionErrorKind` |
| Instance lifecycle | `acquire()` semaphore | lease / pool / drain |

**Every `SourceBackend` integration test in the repo today is `SourceBackend::Yaml`.** No
test anywhere uses `SourceBackend::Wasm`, and `wasm_abi.rs` / `wasm_example.rs` make no HTTP
calls at all — they feed fixture HTML across the ABI in-process. So the backend with *all*
the coverage is the second one, while every shipped extension (MangaDex, WeebCentral,
MangaPill, Comix, Cubari) is WASM.

Classification:

| Tests | Backend | Why |
|---|---|---|
| A2, A4, A5, A6, A9, A12, A13, A14, B1–B4, C, D5–D6, E, G, H, I3–I4, J, K, L, M, N | **shared** — run on either | Downstream of `ChapterList`/`Chapter`, or inside `SmartClient`/`HostState`, both of which are common |
| A1, A7, A8, A10, A11, A15, F1–F10, I1–I2 | **yaml-only** | Live in `make_request` / `build_url_with_args` / `unpack_*` / the `$pref:` env — code the WASM path does not execute |
| Group O (below) | **wasm-only** | The guest-side equivalents, plus lifecycle machinery YAML has no analogue for |

Group F is worth calling out: it is entirely about `unpack_*` swallowing malformed data, so
it is YAML-only. "What does a WASM guest do with the same garbage" is a different question
with a different answer, and it is O5.

---

## Group A — Confirmed bugs (write the test red, then fix)

These are defects, not gaps. Verified by reading the code this session.

| ID | Test | Asserts | Origin behaviour | Risk |
|---|---|---|---|---|
| A1 | `a_yaml_source_actually_applies_selected_filters` | The query string the origin receives contains the mapped filter param | Echo route (H3); call `search_manga` with an `ActiveFilter` | SILENT-WRONG |
| A2 | `a_rotated_maintainer_key_does_not_poison_the_index_cache` | After a rejected refresh, `install_from_repo` still uses the **old** index | `script("/index.json", [signed_by_A, signed_by_B])` + `.sig` routes | TRUST BYPASS |
| A3 | `a_disabled_yaml_source_can_be_re_enabled` | `toggle_source_enabled(id, true)` then `search_manga` hits the origin | Any YAML source; disable, re-enable | CRASH-HANG |
| A4 | `a_degenerate_target_listing_does_not_delete_downloads` | CBZ files survive; no rows deleted | Target `chapter_list` returns `{"rows":[]}` | DATA-LOSS |
| A5 | `a_listing_whose_numbers_are_unparseable_does_not_orphan_everything` | Same, for `number: "twelve"` → all `0.0` | Listing with string chapter numbers | DATA-LOSS |
| A6 | `a_scramble_transform_on_a_tiny_image_does_not_panic` | Returns an error, does not unwind the download worker | 2×2 PNG for a page declaring `lcg-tile-5x5:1` | CRASH-HANG |
| A7 | `an_option_set_route_cannot_escape_the_sources_allowed_host` | Origin B is **not** hit | `options_fetched_by.route` absolute into origin B, `base_url` = origin A | SILENT-WRONG |
| A8 | `a_relative_option_set_route_is_joined_to_the_base_url` | Origin A receives `/genres`; dropdown is populated | `route: "/genres"` | SILENT-WRONG |
| A9 | `a_chapter_exceeding_the_io_budget_is_not_marked_complete` | Either the full page set, or a failed download — never a short CBZ marked complete | 60-page listing + 60 `for_each` detail routes | DATA-LOSS |
| A10 | `a_source_supplied_id_cannot_rewrite_the_request_path` | Origin sees the id percent-encoded, not `../admin` | Echo route; manga id `../admin`, `x?y=1`, `a b` | SILENT-WRONG |
| A11 | `an_unresolved_route_placeholder_is_an_error_not_a_literal` | No request is sent containing a literal `$page$` | Route referencing an arg never supplied | SILENT-WRONG |
| A12 | `the_rule_preview_matches_what_auto_download_actually_queues` | `preview_download_rules` count == chapters `filter_chapters_by_rules` returns | Set scanlator prefs + rules; compare both paths | SILENT-WRONG |
| A13 | `a_webhook_cannot_be_pointed_at_a_private_address` | Configuring `http://169.254.169.254/…` or `http://127.0.0.1:8242/…` is refused, or the request is blocked at send | Bind an origin on loopback and register it as a webhook target | **SSRF** |
| A14 | `a_webhook_does_not_follow_a_redirect_to_a_private_address` | A public URL that `302`s inward is not followed | Origin A returns `302` → origin B on loopback (H4) | **SSRF** |
| A15 | `a_yaml_source_reports_a_usable_error_kind` | A `429` from a YAML source classifies as `RateLimited`, not `ParseError` | Listing route returns `429` + `Retry-After` | SILENT-WRONG |

> **A12 detail:** `filter_chapters_by_rules` (`rules.rs:130-185`) applies a second stage —
> blocked scanlators and per-chapter-number priority selection — that
> `preview_download_rules` skips entirely. The preview therefore overstates the result
> whenever scanlator preferences exist.

> **A15 detail:** every failure path in `yaml_source.rs` wraps into
> `ExtensionError::parse` (`:255, :264, :280, :365, :375`), so `ExtensionErrorKind` is
> effectively constant for interpreted sources. `ParseError` does still retry, so this is
> not fatal — but `RateLimited` (which honours `Retry-After`) and `NotFound` (which
> correctly refuses to retry) can never be produced by a YAML source. A rate-limited YAML
> source hammers on generic backoff; a 404 burns every attempt. Stage 0's work to preserve
> the kind through the download pipeline is inert on this backend.

> **A7 sharpening:** `AllowedHost` *is* enforced for YAML, via the shared `HostState`.
> `fetch_option_set` bypasses it by calling `client.get()` directly rather than going
> through the evaluator — so A7 is an escape from a guard that exists, not a missing guard.

> **A13/A14 detail:** `WebhookService` holds a bare `rquest::Client::new()`
> (`webhooks.rs:22,30`) — no `ValidatingResolver`, no timeout, and rquest's default
> redirect following. `validate_url` (`:466`) checks only that the string starts with
> `http://` or `https://`. The URL is user-configured *by design*, and Kani then POSTs a
> signed JSON body to it. This is a broader exposure than the extension-side IP-literal
> hole, because there is no resolver guard on this client at all.

---

## Group B — Extension-driven workflows

The branch `new_ids != []` on a rescan is unreachable today because every fixture serves a
static listing. `origin.script()` opens all of it.

### B1 · New chapters

| ID | Test | Asserts |
|---|---|---|
| B1.1 | `a_grown_listing_emits_new_chapters_once_with_the_right_count` | `AppEvent::NewChapters { count: 1 }`, not 3 |
| B1.2 | `a_rescan_with_no_change_emits_nothing` | No event, no webhook |
| B1.3 | `a_new_chapter_fires_the_configured_webhook` | Webhook payload names the manga and chapter |
| B1.4 | `a_chapter_that_disappears_from_the_listing_is_not_deleted` | Row survives; no data loss on a flaky source |
| B1.5 | `a_relisted_chapter_updates_its_metadata_not_its_identity` | Same row id; title/scanlator refreshed |

### B2 · Auto-download chain (`service/mod.rs:1204-1240`)

| ID | Test | Asserts |
|---|---|---|
| B2.1 | `a_new_chapter_is_enqueued_when_auto_download_is_on` | Job submitted for exactly the new chapter |
| B2.2 | `a_new_chapter_is_not_enqueued_when_auto_download_is_off` | No job |
| B2.3 | `category_membership_enables_auto_download` | The `auto_download \|\| category_manga_ids` branch |
| B2.4 | `auto_download_respects_download_rules` | Filtered-out chapters are not queued |
| B2.5 | `rules_that_exclude_everything_are_surfaced_not_silent` | Currently only a `tracing::info!` — decide and assert a signal |
| B2.6 | `auto_download_skips_a_manga_with_auto_scan_off` | Not scanned at all |

### B3 · Download rules — `rules.rs` has **zero tests of any kind**

Ten rule kinds; `build_chapter_predicate` uses include/exclude semantics (OR within
includes, AND within excludes) on axes 0–1, and plain AND on axes 2+.

| ID | Test | Asserts |
|---|---|---|
| B3.1 | `no_rules_passes_everything` | Identity case |
| B3.2 | `a_single_language_include_admits_only_that_language` | Axis 0 include |
| B3.3 | `two_language_includes_are_a_union` | OR within includes |
| B3.4 | `a_language_exclude_removes_that_language` | Axis 0 exclude |
| B3.5 | `include_and_exclude_on_one_axis_both_apply` | Interaction |
| B3.6 | `title_contains_and_title_excludes_compose` | Axis 1 |
| B3.7 | `chapter_number_min_and_max_bound_a_range` | Axis 2, inclusive/exclusive edges |
| B3.8 | `exclude_fractional_drops_point_five_chapters` | Axis 3; keeps 167, drops 167.5 |
| B3.9 | `max_age_days_uses_uploaded_at` | Axis 4; boundary at exactly N days |
| B3.10 | `published_after_is_an_absolute_cutoff` | Axis 4 |
| B3.11 | `rules_on_different_axes_are_conjunctive` | Cross-axis |
| B3.12 | `a_chapter_with_null_language_or_date_is_handled` | Missing-field behaviour, both directions |
| B3.13 | `an_unparseable_rule_row_is_skipped_not_fatal` | `DownloadRule::try_from` returns `Err` |
| B3.14 | `blocked_scanlators_are_removed_after_the_rules` | Second stage |
| B3.15 | `priority_selects_one_release_per_chapter_number` | Second stage |

B3.1–B3.13 are pure and belong in `#[cfg(test)] mod tests` inside `rules.rs`. B3.14–B3.15
and B2.4 need the service.

### B4 · Metadata refresh — `RefreshFields` × `clear_overrides`

| ID | Test | Asserts | Risk |
|---|---|---|---|
| B4.1 | `a_refresh_does_not_overwrite_an_uploaded_cover` | `cover_overridden` protects the user's file | **DATA-LOSS** |
| B4.2 | `clear_overrides_with_cover_restores_the_source_cover` | The deliberate opposite of B4.1 | |
| B4.3 | `a_changed_cover_url_re_downloads_and_updates_the_hash` | `cover_hash` changes | |
| B4.4 | `an_unchanged_cover_url_does_not_re_download` | Hit count unchanged | PERF |
| B4.5 | `refreshing_the_title_leaves_the_description_alone` | `fields` selectivity | |
| B4.6 | `a_local_name_survives_a_refresh_that_renames_upstream` | Override wins | |
| B4.7 | `clear_overrides_drops_local_name_only_when_title_is_selected` | The `opts.clear_overrides && opts.fields.title` pair | |
| B4.8 | `people_and_tags_are_replaced_not_appended` | Re-sync semantics | |
| B4.9 | `a_refresh_that_fails_midway_leaves_the_row_unchanged` | Transaction boundary | DATA-LOSS |
| B4.10 | `fetch_chapters_false_skips_the_listing_entirely` | Hit count is zero | |

---

## Group C — Request-path fallbacks

Mode-switching fallback: give up on one strategy, try a different one. **None** of this is
covered today; no test anywhere sets a `solver_url`.

| ID | Test | Origin behaviour | Status |
|---|---|---|---|
| C1 | `a_challenge_page_triggers_the_solver_and_replays` | Site serves `Just a moment...`, then real HTML; fake solver returns the FlareSolverr envelope | GAP |
| C2 | `the_solved_cookie_is_attached_to_the_replay` | Site keeps serving the challenge until it sees `cf_clearance` (needs H1) | GAP |
| C3 | `a_solver_error_status_surfaces_as_a_useful_error` | Solver returns `{"status":"error","message":…}` | GAP |
| C4 | `a_solver_that_is_unreachable_does_not_hang_the_request` | Solver route `Body::Stall` | GAP |
| C5 | `stored_credentials_are_re_solved_after_a_403` | Site: 200 → 403 → 200; assert solver hit twice | GAP |
| C6 | `expired_credentials_are_dropped_before_reuse` | Manipulate `CREDENTIAL_TTL_SECS` | GAP |
| C7 | `a_304_yields_the_same_result_as_a_200` | `script(["200 index", "304"])`; assert repo state identical | GAP |
| C8 | `a_304_with_no_cached_index_is_an_error_not_an_empty_index` | `304` on first ever fetch | GAP |
| C9 | `the_circuit_opens_after_repeated_real_failures` | N+1 × `502`; assert request N+2 never reaches the socket | GAP |
| C10 | `the_circuit_recovers_after_the_cooldown` | Then `200` | GAP |
| C11 | `a_connection_reset_mid_request_is_retried` | `Body::Reset` twice then `200` | GAP |
| C12 | `a_relative_redirect_resolves_against_the_current_url` | `Location: ../other/page` | GAP |
| C13 | `a_protocol_relative_redirect_is_handled_or_refused` | `Location: //host2/x` (H4) | GAP |
| C14 | `a_scramble_seed_header_drives_the_descramble` | `x-scramble-seed: 12345` | GAP |
| C15 | `a_missing_or_malformed_scramble_seed_stores_the_raw_image` | Header absent, then `abc` | GAP |

---

## Group D — Source lifecycle

| ID | Test | Origin behaviour | Status |
|---|---|---|---|
| D1 | `an_in_flight_call_completes_against_the_old_backend_after_hot_swap` | `Body::Stall` the listing, then `hot_swap` | GAP |
| D2 | `a_deleted_source_does_not_hang_an_in_flight_request` | `Body::Slow`, then `delete_source` (YAML gets no drain at all) | GAP |
| D3 | `drain_timeout_does_not_swap_while_leases_are_live` | Park a call past the 30 s drain timeout | GAP |
| D4 | `concurrent_installs_of_the_same_extension_serialise` | Two concurrent `update_source_from_repo` | GAP |
| D5 | `disable_then_call_reports_disabled_not_not_found` | `require_source_active` re-checks `enabled` | GAP |
| D6 | `uninstall_removes_the_backend_and_the_artifact` | | GAP |

> D1/D3 relate to a suspected memory-ordering issue: `lease_instance`/`drain`
> (`wasm_source.rs:87-97`, `:159`) use `AcqRel`/`Release` where an increment-then-check
> pair needs `SeqCst`. Worth an independent review — a test may not reproduce it reliably.

---

## Group E — Repo trust and artifacts

| ID | Test | Origin behaviour | Status |
|---|---|---|---|
| E1 | `an_artifact_larger_than_the_cap_is_rejected` | `Body::Truncated { announced: 50MB }` | GAP |
| E2 | `an_artifact_whose_hash_does_not_match_is_rejected` | Serve different bytes than the index claims | GAP |
| E3 | `an_index_entry_url_cannot_point_at_another_host` | Index on A, `url` absolute into B (H4) | GAP |
| E4 | `a_wasm_download_larger_than_MAX_WASM_BYTES_is_rejected` | `fetch_wasm` | GAP |
| E5 | `a_redirect_chain_during_install_is_bounded` | 6 hops | GAP |
| E6 | `an_index_with_no_signature_is_refused` | `.sig` returns 404 | GAP |
| E7 | `a_repo_that_starts_failing_does_not_lose_its_cached_index` | `200` then `500` | GAP |

---

## Group F — Malformed and hostile source data

`unpack_*` swallows every structural mismatch: `rows` becomes empty, `number` becomes
`0.0`, non-string ids are dropped. Nothing distinguishes "the source changed shape" from
"the source has no chapters" — which is what makes A4/A5 data-loss.

| ID | Test | Origin serves |
|---|---|---|
| F1 | `a_listing_whose_rows_is_an_object_is_an_error_not_an_empty_list` | `{"rows": {"a": 1}}` |
| F2 | `a_chapter_with_a_numeric_id_is_reported_not_silently_dropped` | `{"id": 12}` |
| F3 | `a_chapter_number_that_is_a_string_does_not_become_zero` | `"number": "twelve"` |
| F4 | `an_enormous_title_is_truncated_or_refused` | 5 MB string field |
| F5 | `an_astral_plane_id_round_trips` | Emoji / CJK extension B ids |
| F6 | `a_fifty_thousand_row_listing_hits_MAX_LIST_SIZE_predictably` | Assert error vs truncation |
| F7 | `a_null_where_a_string_is_required_is_an_error` | `"title": null` on a required field |
| F8 | `a_duplicate_chapter_id_in_one_listing_is_deduplicated` | Same id twice |
| F9 | `a_listing_that_is_not_json_at_all_is_an_error` | `text/html` from a JSON endpoint |
| F10 | `a_negative_or_nan_chapter_number_is_rejected` | `-1`, `NaN` |

---

## Group G — Cache coherence

| ID | Test | Asserts |
|---|---|---|
| G1 | `the_reader_and_the_downloader_agree_on_the_page_set` | Page-list cache (90 s) vs the downloader's direct call |
| G2 | `a_cache_namespace_declared_in_yaml_is_honoured` | TTL from the `cache` block |
| G3 | `invalidating_a_manga_clears_its_chapter_list_cache` | Hit count rises after invalidation |
| G4 | `a_failed_fetch_is_not_cached_as_a_success` | `500` then `200`; second call must hit the origin |
| G5 | `option_set_failures_are_negatively_cached` | `500,500,200` across three `get_filter_list` calls |

---

## Group H — Budgets and limits

| ID | Test | Asserts |
|---|---|---|
| H6 | `the_io_budget_is_enforced_per_call_not_per_source` | 32-request cap scope |
| H7 | `MAX_STRING_LENGTH_is_enforced_on_a_live_response` | 2 MB field |
| H8 | `a_chapter_list_page_ceiling_is_reported_not_silently_complete` | Ties to A4 — truncation must not read as "done" |
| H9 | `a_slow_body_hits_the_request_timeout_not_a_hang` | `Body::Slow` past the timeout |

---

## Group I — Preferences and filters

| ID | Test | Asserts |
|---|---|---|
| I1 | `a_preference_change_reaches_the_next_request` | `$pref:` in a route/header; assert two different requests (H3) |
| I2 | `a_preference_change_propagates_without_a_restart` | WASM (next lease) and YAML (next eval) |
| I3 | `a_fetched_option_set_populates_the_filter_panel` | Live fetch → dropdown values |
| I4 | `a_sort_option_maps_into_the_request` | `filter_format` / `sort_pair` |
| I5 | `a_broken_option_set_is_reported_not_silently_empty` | `get_fetched_option_sets` errors are swallowed to `"[]"` today |

---

## Group J — Migration

| ID | Test | Asserts |
|---|---|---|
| J1 | `migration_matches_chapters_by_number_across_sources` | Happy path, two origins |
| J2 | `keep_orphaned_downloads_true_preserves_files` | The safe branch |
| J3 | `a_partial_target_listing_does_not_orphan_the_remainder` | Ties to H8 |
| J4 | `read_progress_survives_a_migration` | Progress remap |
| J5 | `a_failed_migration_rolls_back_completely` | File deletion currently happens **before** `tx.commit()` |

---

## Group K — Remaining surfaces

| ID | Test | Asserts |
|---|---|---|
| K1 | `the_image_proxy_retries_a_429_then_succeeds` | `proxy_tests.rs` is entirely synthetic today |
| K2 | `the_image_proxy_times_out_rather_than_hanging` | `Body::Stall` upstream |
| K3 | `concurrent_proxy_requests_for_one_url_coalesce` | One upstream hit |
| K4 | `a_range_request_through_the_proxy_is_capped_at_50MB` | |
| K5 | `an_upstream_that_ignores_range_still_serves_the_reader` | |
| K6 | `a_cover_larger_than_the_cap_is_rejected` | 20 MB image |
| K7 | `a_cover_served_as_html_is_rejected` | Content-Type gate |
| K8 | `a_failed_cover_is_retried_by_the_sweep` | `503` then `200` |
| K9 | `opds_reflects_what_the_source_actually_returned` | |
| K10 | `a_manifest_backfill_after_a_live_download_survives_a_rename` | Currently only covered with a hand-written CBZ |

---

---

## Group L — Trackers (AniList, MyAnimeList)

Both clients are bare `rquest::Client::new()` (`anilist.rs:21`, `mal.rs:19`) — no shared
timeout policy, no retry, no rate-limit handling. Token refresh is **proactive only**,
triggered by `expires_at` (`trackers/mod.rs:399`); nothing reacts to a 401.

| ID | Test | Origin behaviour | Status |
|---|---|---|---|
| L1 | `an_expired_token_is_refreshed_before_the_call` | Token endpoint returns a new pair; assert the API call carries the new bearer | GAP |
| L2 | `a_revoked_token_is_recovered_from_reactively` | API returns `401` while `expires_at` is still in the future | **likely BUG** |
| L3 | `a_failed_refresh_marks_the_link_as_needing_reauth` | Refresh endpoint returns `400 invalid_grant`; today the `?` propagates and sync just fails forever | **likely BUG** |
| L4 | `a_tracker_rate_limit_is_respected` | `429` + `Retry-After`; assert the wait | GAP |
| L5 | `a_tracker_that_stalls_does_not_hang_the_sync_job` | `Body::Stall` (no timeout is configured on these clients) | GAP |
| L6 | `a_malformed_tracker_response_does_not_corrupt_progress` | `{"data": null}`, wrong types | GAP |
| L7 | `a_partial_sync_failure_does_not_abort_the_remaining_entries` | One entry `500`, the rest `200` | GAP |
| L8 | `the_oauth_code_exchange_surfaces_a_provider_error` | `{"error":"invalid_grant"}` | GAP |
| L9 | `tracker_tokens_are_never_written_to_logs_or_the_support_bundle` | Assert redaction | GAP |

## Group M — Webhooks

| ID | Test | Origin behaviour | Status |
|---|---|---|---|
| M1 | `a_webhook_delivery_records_its_status` | `200`, then `500`; assert `webhook_deliveries` rows | GAP |
| M2 | `a_failing_webhook_does_not_block_the_triggering_action` | `Body::Stall`; the scan/download must still complete | GAP |
| M3 | `a_webhook_times_out` | No timeout is set on this client today | **likely BUG** |
| M4 | `the_hmac_signature_matches_the_delivered_body` | Verify `X-Kani-Signature` server-side | GAP |
| M5 | `a_webhook_is_not_retried_into_a_duplicate_delivery` | Assert delivery-count semantics | GAP |
| M6 | `an_oversized_webhook_response_is_not_buffered` | 100 MB response body | GAP |

## Group N — Other external services

| ID | Test | Origin behaviour | Status |
|---|---|---|---|
| N1 | `a_breach_check_failure_does_not_block_registration` | HIBP route `500` — documented as advisory, never tested | GAP |
| N2 | `a_stalled_breach_check_does_not_hang_registration` | `Body::Stall`; currently inherits the 35 s client timeout | GAP |
| N3 | `a_breached_password_is_rejected` | Serve a range response containing the suffix | GAP |
| N4 | `a_hostile_breach_response_is_bounded` | 50 MB body from the range endpoint | GAP |
| N5 | `an_update_check_failure_is_silent_and_harmless` | GitHub route `500`, then malformed JSON | GAP |
| N6 | `an_update_check_does_not_run_more_often_than_configured` | Hit count across ticks | GAP |
| N7 | `a_metadata_provider_enrichment_preserves_local_overrides` | Provider returns a title; assert `local_name` wins | GAP |
| N8 | `a_metadata_provider_failure_leaves_the_manga_unchanged` | `500` mid-enrichment | GAP |

> N1–N4 note: `check_password_breached` (`password_policy.rs:113`) is deliberately
> fail-open — the module doc says "advisory — skipped on network failure". The *policy* is
> fine; what is untested is whether a slow or hostile HIBP actually degrades cleanly rather
> than stalling or memory-spiking a registration request.

---

## Group O — The WASM path against a live origin

Nothing here has ever run. Needs the fixture below.

| ID | Test | Asserts | Status |
|---|---|---|---|
| O1 | `a_wasm_source_applies_selected_filters` | The WASM counterpart to A1 — the guest's filter handling reaches the wire | GAP |
| O2 | `a_wasm_source_builds_the_request_it_declared` | Method, path, query, headers as the guest intended (echo route, H3) | GAP |
| O3 | `a_preference_change_reaches_a_running_wasm_instance` | `set_preference`, then the next lease sends the new value | GAP |
| O4 | `a_guest_error_kind_survives_to_the_download_classifier` | A `429` upstream produces `RateLimited`, not `Unknown` — the contrast case for A15 | GAP |
| O5 | `a_guest_handling_malformed_upstream_data_fails_loudly` | The WASM counterpart to Group F: same garbage, assert the guest errors rather than returning an empty list | GAP |
| O6 | `a_handle_is_not_leaked_when_a_live_fetch_fails` | `wasm_abi.rs` proves this for in-process calls only; assert it across a real `500`/reset | GAP |
| O7 | `an_in_flight_wasm_call_completes_across_a_hot_swap` | The real D1 — needs a genuinely parked HTTP call, not a synthetic lease | GAP |
| O8 | `the_io_budget_is_charged_identically_on_both_backends` | `HostState` is shared, so A9's result must match here | GAP |
| O9 | `a_wasm_source_honours_its_declared_rate_limit` | Confirms the fix on the backend real extensions use | GAP |
| O10 | `composite_id_encoding_round_trips_through_a_live_call` | Guest-side encode/decode vs YAML's `resolve_composite_ids` | GAP |

### Fixture: `kani-fixture-source`

`kani-test-abi` returns canned data and never fetches, so it cannot drive Group O. Needs a
small WASM extension that makes real HTTP calls:

- `base_url` supplied at install time so a test can point it at `TestOrigin`
- `search_manga` / `get_chapter_list` / `get_pages` that fetch and `extract::json`
- filters declared in `get_filter_list` and genuinely mapped into the request (so O1 can
  fail if that mapping breaks)
- one preference read via the `prefs` import and sent as a header (O3)
- an endpoint that returns each `ExtensionErrorKind` on demand, keyed by `manga_id` — the
  same trick `kani-test-abi` already uses for its `error-paths` mode (O4)
- excluded from `--all` like `kani-test-abi`, built with `kani-cli build --dev`

Once it exists, O1–O10 are ordinary integration tests and every "shared" test in the plan
can optionally be parameterised over both backends.

---

## Suggested order

1. **A13/A14 first** — webhook SSRF is the widest exposure here and the target URL is
   user-supplied by design.
2. **The rest of Group A** — twelve more confirmed defects, three of them data-loss or
   trust bypass. Test red, then fix.
3. **B3** — zero coverage on logic that decides what a user's library downloads, and most
   of it is pure (no harness needed).
4. **B4.1–B4.3** — the uploaded-cover guard is the only untested path here whose failure is
   unrecoverable.
5. **B1/B2** — unlocks a whole branch CI has never entered.
6. **Group C** — the largest untested subsystem in the request path.
7. **L and M** — trackers and webhooks are the largest non-extension surface and
   contain three suspected bugs of their own.
8. **The `kani-fixture-source` extension, then Group O** — the WASM path is what every
   shipped extension is, and it currently has no live coverage at all.
9. **F, G, H** — cheap once the harness additions exist.
10. **D, E, J, K, N** — valuable, but each needs more setup.

## Deliberately excluded

- Anything requiring a real browser/V8 subprocess (`via: browser_payload` end-to-end).
  The V8 process has its own harness; testing it through `TestOrigin` would test the
  subprocess, not the extension system.
- The SSRF IP-literal hole. Closing it breaks every test here, because loopback access is
  what lets them use the production client. It needs a deliberate test-only allowlist
  first, then its own test.
