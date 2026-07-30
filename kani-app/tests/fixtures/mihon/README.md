# Mihon / Suwayomi backup fixtures

Both files derive from one real `.tachibk` backup exported by Suwayomi
(`org.suwayomi.tachidesk`, 2026-05-07), produced by
`scripts/anonymise-tachibk.py`.

That script walks the protobuf wire format directly instead of decoding through
`kani-app/proto/tachiyomi_backup.proto`, so blocks Kani does not model survive
byte-identical — the real backup carries top-level fields `101` (source list),
`9000` (WebUI preferences) and `9001` (server settings) that our schema has no
message for. A fixture re-encoded through our own `.proto` would have dropped
them, and the import tests would then only prove that Kani can read what Kani
writes.

## What was changed

- Titles, series and chapter urls, authors, artists, descriptions, cover urls,
  chapter names, scanlator names, category names and source display names are
  replaced with synthetic values of the same shape.
- Values in the `9001` server-settings block that looked like a host, url or
  account name are redacted — the donor backup carried a LAN FlareSolverr
  address and a login name there.
- Genres, timestamps, source ids, status codes, field order and every wire type
  are untouched.
- Truncated to the first 5 series (6 chapters each; 2 for the hostile variant).

## suwayomi-anonymised.tachibk

The general-purpose fixture. Five series across three source ids
(`2499283573021220255`, `2131019126180322627`, `2292947733994124621`), one
category, chapters carrying real read / last-page-read state.

**Two fields are synthetic additions** (`--augment-first`): the donor backup had
no tracking entries and no series assigned to a category, so the first series
gains `categories: [0]` and one `BackupTracking` (`syncId: 2` = AniList). Every
other byte derives from the donor. Tests that depend on those two paths are
therefore testing our schema understanding, not observed Mihon output — treat a
future real backup with tracking as an upgrade worth taking.

## hostile-titles.tachibk

The same five series with every title replaced by a path-traversal, NUL,
RTL-override or astral-plane case (`--hostile-titles`). Used to prove a title
from a foreign file cannot escape the library directory or corrupt a row.
