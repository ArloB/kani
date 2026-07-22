# Unifying the two YAML execution paths

## The problem, stated precisely

There are three execution paths for a content source:

1. YAML → `kani-cli generate` → Rust → WASM component
2. YAML → `ValidatedExtension` → interpreted host-side by `YamlSource`
3. Hand-written Rust → WASM component

Path 3 is legitimately different: arbitrary code needs a sandbox and a real runtime.

Paths 1 and 2 take **the same input file** and produce **different behaviour**.
`kani-cli/src/codegen/request.rs:11-53` consumes `filter_mapping` and `filter_format` to
emit filter-applying code. `kani-app/src/source/yaml_source.rs:390` takes `_filters` and
discards it. So a `.yaml` built with `kani-cli build` filters correctly, and the identical
file installed as an interpreted source silently ignores every filter the user selects.

That is not a capability tier. It is one specification with two implementations, one
unfinished.

## What is already shared, and why it has no bugs

`YamlSource::eval_endpoint_once` builds a `Blueprint` and calls the same
`html_eval::extract_html` / `json_eval::extract_json` that the WASM host runs for
`extract::html`. `YamlSource::make_host_state()` constructs the same
`kani_core::wasm::HostState`, so the 32-request IO budget and `AllowedHost` apply to both.

Neither of those shared components has produced a divergence bug. Every divergence bug
found sits in the band *around* them.

## What is duplicated

| Concern | Interpreted | Codegen | Divergence found |
|---|---|---|---|
| Request construction | `yaml_source.rs::make_request` + `kani_yaml::build_url_with_args` | `codegen/request.rs` | A10, A11 (no encoding, literal placeholders) |
| Filter mapping | **absent** | `codegen/request.rs:53` | **A1** |
| Pagination offsets | `PaginationCfg` handling | generated | not yet measured |
| Composite id encode/decode | `resolve_composite_ids` | generated | not yet measured |
| `has_next_page` / `total_pages` | `ValidatedHnp` / `ValidatedTotalPages` | generated | not yet measured |
| Result unpacking | `unpack_*`, swallows every mismatch | guest builds the structs | Group F |
| Error kinds | collapsed to `ExtensionError::parse` | real `ExtensionErrorKind` | **A15** |

"Not yet measured" is the honest state: nobody has compared these. That is the first
problem to solve.

---

## Phase 1 — Measure the divergence (do this first)

A conformance suite. Take a set of YAML fixtures, run each through **both** paths against
the same `TestOrigin`, assert identical observable behaviour.

```rust
for fixture in FIXTURES {
    let interpreted = drive(YamlSource::from(fixture), &origin).await;
    let compiled     = drive(build_wasm(fixture), &origin).await;
    assert_eq!(interpreted.requests_seen, compiled.requests_seen);
    assert_eq!(interpreted.result, compiled.result);
    assert_eq!(interpreted.error_kind, compiled.error_kind);
}
```

Three assertions, in order of value:

1. **Requests on the wire** — method, path, query, headers. Catches A1, A10, A11,
   pagination and composite-id divergence in one shot. Needs the echo route (harness
   addition H3).
2. **Parsed result** — the `MangaList` / `ChapterList` / `Chapter` returned.
3. **Error kind** — catches A15 and anything like it.

Fixtures should cover: a filtered search, a paginated listing, composite ids, `then` /
`for_each` chained fetches, an HTML source and a JSON source, and one malformed response
per unpack path.

**Why first:** the refactor below is substantial, and right now nobody knows how far the
engines diverge — filters and Group F were found by reading, not measurement. This
quantifies the problem, protects the refactor, and needs `kani-fixture-source`, which
Group O of the [live-source test plan](./live-source-test-plan.md) already requires. Same
investment, two payoffs.

**Exit:** a table of every measured divergence. Some become bugs to fix in place; the rest
justify Phase 2.

---

## Phase 2 — Extract the envelope

Do not delete an engine. They have genuinely different properties worth keeping:

- **Interpreted:** no build step, hot-swappable, distributes as signed plain text, needs no
  wasm toolchain.
- **Compiled:** sandboxed, shares a runtime with hand-written extensions.

Unify the *interpretation of the spec*, which is the only part that should never differ.

### 2a — A guest-safe spec type

`ValidatedEndpoint` lives in `kani-yaml` and is not `wasm32`-compatible as it stands. Add a
leaner `EndpointSpec` in `kani-shared` carrying only what request-building and unpacking
need, with `ValidatedEndpoint` lowering into it. This is real work, not a move.

### 2b — Shared implementation

In `kani-shared`, compiled for both `wasm32` and native:

```rust
pub fn build_request(spec: &EndpointSpec, args: &Args, filters: &[ActiveFilter],
                     prefs: &Prefs) -> RequestDef;
pub fn unpack_chapter_list(value: &Value, spec: &EndpointSpec) -> Result<ChapterList>;
pub fn unpack_manga_list(value: &Value, spec: &EndpointSpec) -> Result<MangaList>;
pub fn unpack_chapter(value: &Value, spec: &EndpointSpec) -> Result<Chapter>;
```

Note the `Result`: today `unpack_*` returns a value and swallows malformed input. The
shared version must be able to say "this response does not match the spec", which is what
makes Group F and the migration data-loss path (A4/A5) fixable rather than merely tested.

### 2c — Invert codegen

`kani-cli` stops emitting request-building logic and instead emits a `const EndpointSpec`
plus a call to the shared function. Codegen becomes "generate data", not "generate an
implementation" — substantially less generated code, and divergence becomes structurally
impossible for anything it covers.

### 2d — Unify error kinds

`yaml_source.rs` wraps every failure into `ExtensionError::parse` (`:255, :264, :280, :365,
:375`), so an interpreted source can never report `RateLimited` or `NotFound`. It has the
real error in hand and discards it. Map `HostState`/HTTP errors to genuine
`ExtensionErrorKind`s. This is a straight defect fix and does not depend on 2a–2c.

---

## What stays divergent, deliberately

- **Instance lifecycle.** WASM pools and leases instances and needs `drain`; the
  interpreter has a plain semaphore. No shared abstraction is warranted.
- **Sandboxing.** The whole point of the two tiers.
- **Browser payload.** Each path reaches V8 differently; the payload contract is the shared
  part and already is.

---

## Risks

- **`kani-shared` must stay `wasm32`-clean.** The existing trap is that ungating a serde
  type is caught only by `kani-cli build`, never by `cargo check` — moving more code in
  raises that exposure. Mitigate by adding a `--dev` extension build to CI if it is not
  already gating.
- **Codegen output churn.** 2c changes every generated extension. The snapshot tests in
  `kani-cli/tests/` will all move; that is expected, but review the diffs rather than
  blanket-accepting them.
- **Phase 2 without Phase 1 is a rewrite with no safety net.** The conformance suite is
  what makes the extraction verifiable.

## Timing

This is Stage 4-adjacent. If the YAML schema is going to be frozen as a stable interface,
two engines disagreeing about what it means is a poor thing to freeze — the conformance
suite should land before that decision, even if the extraction does not.
