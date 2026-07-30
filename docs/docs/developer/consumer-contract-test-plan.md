# Consumer Contract Test Plan (Groups P–T)

A companion to `live-source-test-plan.md`. That plan covers behaviour against a
hostile *upstream*; this one covers the opposite edge — the artefacts Kani hands
to an external *consumer*.

**The gap class.** Kani produces something another program must read (an Atom
feed, a `ComicInfo.xml`, a Prometheus exposition, an email, an SSE stream) and
the tests assert on it with `text.contains(...)` — or never read it back at all.
A document can satisfy every substring assertion and still be unusable: malformed
XML, an advertised link that 404s, an exposition no scraper accepts. The inverse
case counts too: where Kani *is* the client of a foreign format, tested only
against inputs Kani itself constructed.

**Found by:** auditing after the OPDS client-test gap (2026-07-28). `/opds` had
thorough per-endpoint tests and zero tests that parsed a feed or followed a link.

**The precedent to copy.** Two suites already do this correctly and are the model
for the rest:
- `kani-app/tests/wasm_conformance_tests.rs` — drives the real compiled
  `kani-fixture-source` against the host ABI, the client test for the WIT surface.
- `kani-web/tests/opds_client_tests.rs` — parses feeds with a real XML reader and
  navigates by the links they advertise.

**Already covered, do not redo:** backup export→import (`backup_import_tests.rs`),
portable archive export/verify (`archive_tests.rs`), support-bundle zip
(`tracker_reauth_tests.rs::tracker_tokens_are_never_written_to_the_support_bundle`).

**Rule for every test below:** parse with a real parser and fail on malformed
input. A test that only greps the output reproduces the gap it is meant to close.

---

## Group P — `ComicInfo.xml` inside the CBZ

Consumers: Komga, Kavita, Calibre, Jellyfin, ComicRack. This is the
data-portability promise in plan 15 §1 ("a library managed by 1.0 is
exportable"), so a malformed sidecar breaks the promise directly.

Current state: `kani-core/src/comic_info.rs` has in-file tests, but they are
`xml.contains("Berserk &amp; Co")`-style substring checks (lines ~176-184). **No
test opens a CBZ that Kani produced and reads the `ComicInfo.xml` back out.**

| ID | Test | Asserts |
|---|---|---|
| P1 | `a_written_cbz_contains_parseable_comicinfo` | Build a chapter through the real download/CBZ path, open the archive, parse the entry with a strict XML reader — malformed is a failure, not a warning |
| P2 | `comicinfo_metadata_round_trips_through_the_archive` | Title, series, number, volume, writer/penciller, summary, page count read back equal to what was stored |
| P3 | `xml_hostile_metadata_still_parses` | Title/summary containing `& < > " '` and a `]]>` sequence; parse must succeed and the values round-trip |
| P4 | `comicinfo_element_names_match_the_schema` | Element names are exactly the ComicRack names consumers match on (`Series`, `Number`, `Volume`, `Writer`, `PageCount`, …) — a rename silently drops metadata for every consumer |
| P5 | `a_chapter_with_no_optional_metadata_omits_rather_than_emits_empty` | Absent fields are omitted, not emitted as empty elements (some readers treat `<Writer></Writer>` as a writer named "") |
| P6 | `page_count_matches_the_images_actually_in_the_archive` | `PageCount` equals the real image entry count — the classic drift between sidecar and payload |

Location: `kani-core/tests/comic_info_archive_tests.rs` (needs the real CBZ
writer, so an integration test rather than in-file).

---

## Group Q — Tachiyomi / Mihon import

The inverse case: Kani is the client of a foreign gzipped-protobuf backup.
`kani-app/src/service/import/tachiyomi.rs` is **659 lines with zero
`#[cfg(test)]`** and no fixtures. It runs on a user's first interaction with
Kani, so a parsing bug corrupts their library before they have any reason to
distrust it. Highest untested blast radius in the repo.

**Prerequisite — a real fixture.** A synthetic backup built from Kani's own
understanding of the format tests only that understanding. Source a small,
anonymised real Mihon backup (a handful of series, one tracked, one with
categories) and commit it under `kani-app/tests/fixtures/mihon/`.

| ID | Test | Asserts |
|---|---|---|
| Q1 | `a_real_mihon_backup_imports_its_series` | Series count, titles and source ids match the fixture |
| Q2 | `categories_survive_the_import` | Category names and membership preserved |
| Q3 | `read_progress_and_chapter_state_survive` | Read/unread and last-page-read map onto `user_chapter_tracking` |
| Q4 | `an_unknown_source_id_is_reported_not_silently_dropped` | A backup referencing a source Kani lacks is surfaced, not skipped in silence |
| Q5 | `a_truncated_backup_is_rejected_cleanly` | Half a gzip stream errors without partially writing the library |
| Q6 | `a_backup_with_hostile_titles_is_stored_intact` | Path-traversal (`../`), NUL, RTL-override and astral characters in titles neither escape the library dir nor corrupt rows |
| Q7 | `importing_twice_is_idempotent` | A second import of the same backup does not duplicate series or chapters |

Location: `kani-app/tests/mihon_import_tests.rs`.

**DONE (2026-07-30).** 11 tests in `kani-app/tests/mihon_import_tests.rs`, driven
by two fixtures derived from a real Suwayomi export — see
`kani-app/tests/fixtures/mihon/README.md` for provenance and for the two fields
that are synthetic. `scripts/anonymise-tachibk.py` produced them by rewriting the
protobuf **wire format** rather than re-encoding through
`kani-app/proto/tachiyomi_backup.proto`, so the top-level `101`/`9000`/`9001`
blocks Kani has no message for survive byte-identical; the tests therefore prove
Kani tolerates a real file's unknown fields, not merely its own output. The donor
also revealed that Suwayomi source ids are its own hashes, unrelated to Mihon's.

Every expectation is read out of the fixture at test time (the tests decode it
with `prost` themselves), so no assertion hardcodes a value that could silently
drift from the file.

Against the table: Q1 ✓ (plus title/description/cover/status/genre/author
mapping), Q2 ✓, Q3 ✓ (pre-seeded chapter rows so the progress-matching path runs
without a live source), Q4 ✓, Q5 ✓ (truncated gzip → error, and `manga`,
`categories`, `pending_imports` all still empty), Q6 ✓, Q7 ✓. Four beyond the
table: the preview writes nothing, tracker links map (`syncId 2` → AniList,
status 1 → Kani 0), progress that cannot be applied yet is *warned* about rather
than dropped in silence, and a series resembling one already in the library is
parked with `possible_duplicate_of` set instead of imported.

**No defects found** — the importer was correct on every path tested, which is
worth recording given it had 659 lines and zero tests.

**Not covered, for want of donor data:** `viewer`/`viewer_flags` (reading
direction) and `favorite` are absent from this backup, so the reading-direction
mapping remains untested. A future donor backup with tracking and a viewer flag
set would upgrade three of these tests from schema-understanding to observed
output.

---

## Group R — Prometheus `/metrics` exposition

The consumer is a scraper. Current coverage
(`observability_api_tests.rs::registered_kani_metrics_are_present_in_the_exposition`)
is `text.contains(metric)` plus `text.contains("# TYPE")` — that is the entire
format check. Malformed exposition breaks monitoring silently, which is the
worst way for observability to fail.

| ID | Test | Asserts |
|---|---|---|
| R1 | `the_metrics_exposition_parses_as_prometheus_text_format` | Parse the whole body with a real exposition parser; malformed is a failure |
| R2 | `every_metric_has_a_matching_help_and_type_line` | No sample without a preceding `# HELP`/`# TYPE`, no duplicate declarations |
| R3 | `label_values_are_escaped` | Drive a label from a source/extension name containing `"`, `\` and a newline; the exposition must still parse and the value round-trip |
| R4 | `metric_names_are_valid_identifiers` | Names match `[a-zA-Z_:][a-zA-Z0-9_:]*` — a name derived from user data would break the scrape |
| R5 | `counters_do_not_go_backwards_across_two_scrapes` | Scrape twice and compare; a decreasing counter silently corrupts every rate() |

Location: extend `kani-web/tests/observability_api_tests.rs`.

---

## Group S — Email

`email.rs` (171) + `email_templates.rs` (98) + `email_verification.rs` (119) —
**388 lines, zero tests.** Consumers are mail clients and spam filters; a
malformed header or body means mail silently rejected or spam-filed, with no
local signal that anything went wrong.

| ID | Test | Asserts |
|---|---|---|
| S1 | `a_rendered_email_is_valid_mime` | Parse the built message with a MIME parser: headers well-formed, encoding declared, body decodable |
| S2 | `a_subject_with_non_ascii_is_encoded_word_wrapped` | A non-ASCII subject is RFC 2047 encoded rather than emitted raw |
| S3 | `header_injection_via_a_display_name_is_impossible` | A name containing `\r\n` cannot inject a header (`Bcc:`) — this is a security property, not just a format one |
| S4 | `the_html_and_text_parts_carry_the_same_link` | Multipart alternative parts agree, so a text-only client gets a working link |
| S5 | `a_verification_link_survives_url_encoding` | The token round-trips out of the rendered body byte-identical |

Location: `kani-app/tests/email_tests.rs`. S1/S2 need a MIME parser dev-dep.

**DONE (2026-07-30).** Landed as in-file `#[cfg(test)]` modules rather than an
integration test, because the subjects are pure functions: `service/email.rs`
(message construction) and `service/email_templates.rs` (rendering). The test
seam is a new `build_message(from, to, subject, html)` extracted out of
`SmtpEmailTransport::send`, so a message can be built and inspected without an
SMTP transport. `mail-parser` is the dev-dep; every header assertion runs against
its parse, not a substring of ours.

Coverage against the table: S1 ✓, S2 ✓, S3 ✓ (split into display-name/recipient
and subject vectors — CRLF in an address is *rejected*, CRLF in a subject is
absorbed into an RFC 2047 encoded word, and the check is header-set equality
against a clean message, with `the_header_name_scan_sees_an_extra_header` proving
the scanner would notice an injected header). S4 was **re-scoped**: the messages
are single-part `text/html`, so there are no alternative parts to compare —
the equivalent guarantee is that the action link survives without the styled
button (`an_action_link_is_reachable_without_rendering_the_button`). S5 ✓
(`a_verification_link_survives_transfer_encoding`, asserted on the *decoded* body).

**Two real defects fixed:**
- **No `Message-ID` header.** lettre only emits one if you ask; `build_message`
  never did, so every Kani email shipped without it (SpamAssassin `MISSING_MID`,
  and clients thread/dedupe on it). Now generated as `<uuid@from-domain>`.
- **The username was interpolated raw into HTML.** Escaped now (`escape_html`),
  along with the action URL, so a display name cannot inject markup or break out
  of the `href` attribute. Impact was low — these mails go to the account's own
  address — but `&` in a legitimate username also rendered as a broken entity.

**Known gap, deliberately not closed here:** there is still no `text/plain`
alternative part. Adding one is a product change (every template needs a text
rendering), not a test.

---

## Group T — SSE event contract (frontend ↔ backend)

The backend emits `AppEvent` variants as snake_case type strings; the frontend
subscribes by string literal in `static/js/sse.js` / `hooks/use-sse.js`. Nothing
checks the two agree. A renamed variant compiles, ships, and silently stops
updating the UI — the exact "built but never wired" failure the repo's testing
rules call out.

**Structural note.** There is no JS test harness in the repo, so T1/T2 are the
cheap Rust-side half and T3 is the part that needs a decision.

| ID | Test | Asserts |
|---|---|---|
| T1 | `every_app_event_serialises_to_a_snake_case_type_tag` | Enumerate `AppEvent`, assert each type string matches the convention (no `camelCase`/`kebab` drift) |
| T2 | `the_emitted_event_names_match_the_frontend_subscriptions` | Parse the `useSSE('…')` / `case '…'` literals out of `static/js/` and assert set-equality with the Rust variants — a build-time contract check, no JS runtime needed |
| T3 | `a_subscriber_receives_a_well_formed_event_payload` | Drive a real action through `AppService`, read the SSE stream, parse each frame as JSON and assert the documented fields exist |

T2 is the high-value one: it is a static cross-language check that would have
caught every past instance of this drift, and it needs no JS harness.

Location: `kani-app/tests/sse_contract_tests.rs` (T1/T3) and a small script or
test for T2.

**DONE (2026-07-30)** — all three, in `kani-web/tests/sse_contract_tests.rs`
(not kani-app: the frame set is only complete at the route, which merges
`AppEvent`, `DownloadProgressEvent` and the hand-rolled `state_snapshot` frame
built in `rest/sse.rs`).

- T1 `every_app_event_serialises_to_a_snake_case_type_tag` — every variant is
  sampled and its wire tag pinned by an exhaustive `expected_app_event_tag`
  match, so adding a variant fails to compile until its tag is declared.
- T2 `the_emitted_event_names_match_the_frontend_subscriptions` — both
  directions. Emitted-but-unhandled is allowed only via a declared prefix
  handler (`job_`, checked to still exist in `static/js`); subscribed-but-never-
  emitted is allowed only via the declared `NON_SSE_TYPE_DISCRIMINANTS` list.
  The scanner reads `useSSE('…')` and `.type === '…'`, and deliberately skips
  `job_type === '…'` and `typeof x === 'string'` — pinned by
  `the_subscription_scanner_reads_handlers_and_not_lookalikes`. Two anti-vacuity
  tests back it: `the_frontend_scan_reaches_the_real_sources` and
  `the_contract_check_rejects_a_renamed_event`.
- T3 `a_subscriber_receives_a_well_formed_event_payload` — opens `/rest/events`
  as an authed client, asserts the `text/event-stream` content type, parses the
  first frame as `state_snapshot` with its documented fields, then drives
  `invalidate_library()` and asserts the live frame that follows.

---

## Suggested order

1. ~~**P** (ComicInfo)~~ — done (ea8d992).
2. ~~**R** (metrics)~~ — done (ea8d992).
3. ~~**T** (SSE contract)~~ — done 2026-07-30, all of T1–T3.
4. ~~**S** (email)~~ — done 2026-07-30; two real defects fixed, S4 re-scoped.
5. ~~**Q** (Mihon import)~~ — done 2026-07-30 from a real Suwayomi export.

**All five groups are complete.** What remains is upgrade work, not gaps: a donor
backup carrying tracking / viewer flags (Q), and a `text/plain` alternative part
(S) if the email templates ever grow one.
