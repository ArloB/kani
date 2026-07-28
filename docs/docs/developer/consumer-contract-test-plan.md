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

---

## Suggested order

1. **P** (ComicInfo) — cheapest, and a stated 1.0 promise.
2. **R** (metrics) — cheapest of all; a parser dev-dep and five focused tests.
3. **T2** (SSE name contract) — small, static, catches a whole failure class.
4. **S** (email) — moderate; S3 is a security property worth having regardless.
5. **Q** (Mihon import) — highest value, but blocked on sourcing a real fixture.
