# Phase 2 — Unified, SSRF-hardened redirect handling in `SmartClient`

Status: **planned** (2026-07-27). Supersedes the "Phase 1 quick fix" for the
redirect-loop hang; folds in the SSRF findings from Groups C/E.

## Why

`SmartClient` has two request entry points that handle redirects **differently and
inconsistently**:

- **`send_request`** (source extraction — the hot path) has *no* manual redirect
  handling. It relies entirely on the client's `redirect(Policy::limited(10))` to
  auto-follow.
- **`safe_get`** (images, covers, repo index/artifacts) follows redirects *manually*
  on the assumption the client won't — to apply a host-aware header policy (keep
  `Range`/`Referer`, drop credentials on host change).

Both share one `rquest::Client`. For `new()` clients (`Policy::limited(10)`), rquest
auto-follows **before** `safe_get`'s manual loop ever sees a 3xx, so:

1. `safe_get`'s `MAX_REDIRECTS` and credential-drop are **dead code** on `new()` clients.
2. A redirect **loop** makes rquest error after 10 hops; `safe_get`/`send_request` treat
   that as a retryable network error and re-run with 5/10/20 s backoff → **~35 s + ~30
   requests** (measured: `hits=20`, outer-timeout). This was the reported "hang".
3. **SSRF-on-redirect is open.** `ValidatingResolver` only guards DNS *names* (rquest's
   `dns_resolver` hook); IP *literals* are dialled directly, and the builder exposes no
   connector hook. Neither path re-validates a redirect **target**, so a source can hand
   back a benign public URL that `302`s to `http://169.254.169.254/` and we dial it.
   `send_request` (auto-follow) has *zero* per-hop SSRF; `safe_get` checks the scheme but
   not `is_forbidden_url_host` on the next hop.

`new_proxy()` already uses `Policy::none()`, so its manual `safe_get` loop is correct —
which is the shape we want everywhere.

## Goal

**One `Policy::none()` client; one shared redirect-follow path used by both
`send_request` and `safe_get`; SSRF validated at every hop; one place for the header
policy.** This fixes the hang, closes the `send_request` redirect-SSRF, and removes the
divergence that caused the bug.

## Design

### 1. Single no-follow client
Both `new()` and `new_proxy()` build the client with `redirect(Policy::none())`.
`MAX_REDIRECTS` becomes the single authority.

### 2. A shared hop classifier
Extract one pure-ish helper both loops call after receiving a response:

```rust
enum Hop {
    Terminal,                                   // not a redirect — hand back to the caller
    Follow { url: String, headers: HeaderMap }, // next request the caller should execute
}

fn classify_redirect(
    resp_status, resp_url, location_header,
    initial_host, current_headers,
    redirect_count, max_redirects,
    egress: &EgressPolicy,
) -> Result<Hop, Error>
```

It: rejects a missing/invalid `Location`; joins relative/protocol-relative targets;
rejects non-`http(s)` schemes; **runs the target through `egress` (below)**; enforces
`max_redirects` as **terminal, not retryable**; and computes the next hop's headers —
keep `Range`/`Referer`, drop credentials (`Authorization`, `Cookie`, and the solver
`cf_clearance`) when `host_of(next) != initial_host`.

`send_request` and `safe_get` keep their own outer loops (challenge/retry/circuit /
cond-cache) but delegate the redirect decision here. No behaviour is duplicated.

### 3. `EgressPolicy` — unify the two SSRF guards
Today SSRF is split: `ValidatingResolver` (DNS names) + `is_forbidden_url_host` (IP
literals), applied inconsistently. Fold them into one policy the whole client shares:

```rust
struct EgressPolicy { allow_private: bool } // false in prod, true for tests

impl EgressPolicy {
    fn check_url(&self, url: &str) -> Result<(), Error>; // is_forbidden_url_host unless allow_private
}
```

- Validate **every** dialled URL through `check_url`: the initial URL *and* every
  redirect target (the classifier calls it). The resolver still guards DNS-name rebinding
  at connect time; `check_url` closes the IP-literal hole the resolver can't see.
- **Test-egress flag (required).** Adding `is_forbidden_url_host` to the fetch path
  blocks loopback — which would break *every* live-origin test (all of Groups C/D/E/K/O
  and the conformance suite run against `127.0.0.1`). `allow_private` is the escape
  hatch, set only in tests via a `with_egress`/`allow_private_egress_for_test` seam.
  **Precedent: the webhook `allow_private_egress_for_test`, commit 371f5e8** — reuse the
  same shape so there is one obvious "tests may reach loopback" switch.

### 4. Redirect × challenge ordering
Fold redirect-following into the existing loops so the interaction is correct: a
`403`/`503` **after** a redirect must solve for the **final** host (the classifier having
already moved `current_url`), and a `Just a moment…` body after a redirect renders the
final URL. Today `send_request` gets this by accident (rquest lands on the final URL
before it inspects status); the unified loop must preserve it deliberately.

### 5. Kill the amplification
A redirect-limit hit or an `EgressPolicy` rejection is a **terminal** error — never fed
to the retry/backoff arm. (Independently, treat rquest's own `is_redirect()` errors as
terminal in case any auto-follow remains.)

## Additional `SmartClient` improvements to fold in

Ranked; do the ones that carry their weight.

1. **Whole-call deadline (high value).** `request_timeout` (added in Timings) bounds a
   single attempt, but a call can still spend minutes across redirects + retries +
   solves. Add a per-call wall-clock deadline (e.g. 90 s) threaded through both loops, so
   worst-case latency is bounded regardless of how the hops compose. This alone would
   have capped the 35 s symptom.
2. **De-duplicate the retry/backoff arm (high value, low risk).** `send_request` and
   `safe_get` each hand-roll compute_delay + jitter + `record_failure` + sleep. Extract
   one `retry_after_failure(&mut state) -> ControlFlow` helper. Same class of drift that
   produced the redirect bug; collapsing it prevents the next one.
3. **De-duplicate the challenge/solver handling (medium).** Both paths implement the
   403/503→solve→replay flow; `safe_get` also has the `Just a moment…` HTML path. One
   shared `solve_and_replay` keeps them consistent (and is where the "solve for the final
   host after a redirect" logic lives once).
4. **Observability on SSRF rejections and redirect caps (medium, security-relevant).**
   A blocked egress hop is a signal, not noise — emit a counter/event (like
   `circuit_event_tx`) so an operator can see a source trying to reach internal hosts.
5. **`try_clone()`-None is explicitly non-retryable (low, correctness).** A streaming
   body can't be cloned for a retry; today the retry arm silently requires `is_some()`.
   Make it an explicit terminal branch with a clear error rather than falling through.
6. **Credential scope on sibling subdomains (design decision).** Credentials are keyed on
   `base_domain`, but the drop-on-host-change uses exact host. Decide and document whether
   a redirect to a sibling subdomain keeps the solved cookie (base-domain scope) or drops
   it (exact-host scope). Pick the stricter default unless a real source needs otherwise.

Out of scope: a custom hyper connector for connect-time IP filtering (rquest doesn't
expose it) — the per-hop `EgressPolicy.check_url` is the practical equivalent.

## Test matrix

Drive with `TestOrigin` (+ the egress test-flag so loopback is reachable):

- relative / protocol-relative / absolute redirect resolution (already have C12/C13 at
  the `safe_get` unit level — extend to `send_request`);
- redirect **loop** trips `MAX_REDIRECTS` **fast** (terminal, no backoff) — the
  regression for the reported hang, on **both** entry points;
- redirect **off-host drops credentials**, keeps `Range`/`Referer` — assert via echo
  `last_request().header(...)`;
- redirect **to a forbidden IP literal is refused** at the hop (with `allow_private`
  off) — the new SSRF guard, on **both** entry points;
- `403` **after** a redirect solves for the **final** host (solver hit with the final
  URL);
- whole-call deadline fires on a pathological redirect+stall composition;
- existing C/D/E/K/O suites stay green **with** the egress test-flag wired.

## Risks / sequencing

- Touching `send_request` (the extraction hot path) is the main risk — every source
  request flows through it. Land behind the full matrix above; watch the conformance
  suite.
- The `EgressPolicy` change is load-bearing for the whole test fleet — wire the
  `allow_private` test seam **first**, in the same PR, or CI goes red everywhere.
- Ship as its own PR, independent of Group O / the evaluator work.
