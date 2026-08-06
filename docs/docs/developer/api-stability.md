# API Stability

Kani publishes two compatibility tiers. A tier is a promise about change, not a statement about
quality: an unstable endpoint may be entirely reliable and still move without notice.

## What each tier promises

**Stable.** Within a major version, a stable operation may gain optional request fields and
optional response fields. It may not remove or repurpose an existing field, change a status code,
change a default, or move to another path. Breaking any of those requires a major version.

**Unstable.** No compatibility promise. The operation may change shape, move, or disappear in any
release, including a patch. Build against it only if you are prepared to track releases.

## REST API

Every operation in the OpenAPI document carries an `x-stability` extension whose value is
`stable` or `unstable`, so a generated client can read the tier directly rather than inferring it
from the path.

Unstable prefixes, and why:

| Prefix | Reason |
| --- | --- |
| `/rest/admin` | Server administration. Shape follows the admin UI's needs. |
| `/rest/ui` | Theme storage, introduced shortly before 1.0 and not yet exercised by third parties. |
| `/rest/jobs` | Background-job framework internals. |
| `/rest/server` | Process control, including restart. |
| `/rest/image_proxy` | Reader transport detail. |
| `/rest/boot_id`, `/rest/features`, `/rest/refresh` | Single-page-app plumbing for Kani's own frontend. |
| `/rest/trash` | Retention behaviour still settling. |

Everything else under `/rest` is stable. The default is deliberate: a new route is stable unless
someone lists it, so the reviewable mistake is forgetting to mark an internal route rather than
silently shipping an unannounced promise.

The single source of truth is `UNSTABLE_PREFIXES` in `kani-web/src/openapi.rs`.
`kani-web/tests/openapi_coverage_tests.rs` fails the build if an operation carries no tier, if an
administrative or internal route is marked stable, or if the tier is absent from the serialised
document.

## OPDS

`/opds` is **not covered by either tier**. It is absent from the OpenAPI document, and the README
describes the feed as experimental. Its conformance is pinned by `opds_client_tests.rs`, but no
compatibility promise is published for 1.0.

## Command-line interface

`kani-cli` splits the same way. The extension-authoring pipeline is stable:

```text
kani-cli new        kani-cli validate
kani-cli generate   kani-cli build
```

Every other subcommand is repository plumbing or a diagnostic, and its `--help` text is marked
`[unstable]`. That includes `setup`, `css` and `icons`, which exist to build this repository rather
than to be a supported interface — which is what makes replacing them a later judgement call rather
than a breaking change.

The list is `STABLE_COMMANDS` in `kani-cli/src/commands/mod.rs`, checked against the help clap
renders by `kani-cli/tests/command_stability_tests.rs`.

## Changing a tier

Promoting unstable to stable is additive and may happen in a minor release. Demoting stable to
unstable is a breaking change and requires a major version. Record either in the changelog.
