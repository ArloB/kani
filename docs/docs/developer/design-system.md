# Frontend design system

The single source of truth for how Kani's UI looks and behaves. Tokens live in
the `@theme` block of `static/css/app.css`; this page codifies the rules around
them. CI enforces the token rule (`node scripts/audit-tokens.mjs --check --max 0`), the
i18n key rule (`scripts/check-i18n-keys.js`), and the untranslated-literal rule
(`scripts/check-untranslated-strings.js`).

## Design identity: ink and stamp

Kani's visual identity is **ink and stamp** (adopted 2026-07; applied across the
app by the frontend overhaul — treat it as binding, not aspirational). Manga is
black ink on paper; Kani (蟹) owns a vermilion red. The UI is the paper: calm,
near-monochrome ink surfaces in all three built-in themes (light "paper", dark
showcase, black OLED), and the accent red is the hanko stamp: **rare, small, and
always meaningful**.

Rules that follow from it:

- Red appears only on unread/new indicators, reading progress, and the single
  primary action per view. It is never decoration — not on ghost buttons, not
  on log-level badges, not on multiple header actions at once.
- Primary and destructive are distinct semantics. Destructive actions do not
  reuse the accent fill; they get their own treatment (outline + danger text,
  confirmation weight). A screen must never present "save" and "delete
  everything" as visually identical buttons.
- Hierarchy comes from typography before borders: headings use a
  characterful display face (sparingly — page titles and group headers, not
  row labels), and the type scale carries the difference between an eyebrow,
  a card title, and a row label. Boxes and dividers are the fallback, not the
  first tool.
- The signature is restraint. When a screen feels flat, the fix is removing
  competing emphasis, not adding more accent.

### The trap this keeps falling into

Every accent regression found so far has been the same shape: **a primary
repeated per row.** An install button on every extension, a Save on every
preference row, a Link on every tracker, "Find & Import" on every pending
import. Each is defensible alone; together they turn a list into a wall of red
and leave the view with no primary at all.

The rule: a control that appears once per row is *never* `btn-primary`. If a row
action is the important one, it earns `btn-secondary`. The accent fill is
reserved for the one action the whole view is about — and inside a dialog, for
its single confirm. A trigger that merely opens a form is not that action (the
form's confirm is).

Closing is not a primary action either: dialog "Close" buttons are `btn-ghost`.

## Design tokens

All colour, radius, shadow, z-index, and motion values come from the `@theme`
block in `static/css/app.css`. Never hard-code these in JS **or** CSS — the
runtime theme system (`static/js/theme.js`, built-in `dark` / `light` /
`black` plus named custom themes) rewrites the custom properties, so any
literal silently breaks custom themes.

| Group | Tokens | Notes |
|-------|--------|-------|
| Surfaces | `--color-bg`, `--color-surface`, `--color-surface-2/3`, `--color-surface-alt` | Elevation ladder: bg → surface → surface-2 → surface-3 |
| Borders | `--color-border`, `--color-border-subtle` | `subtle` for internal dividers, `border` for control outlines |
| Text | `--color-text`, `--color-text-muted`, `--color-text-faint` | Strictly ordered by prominence: text > muted > faint. `faint` is for tertiary chrome only (section labels, separators), never body copy |
| Accent | `--color-accent`, `--color-accent-hover`, `--color-accent-dim`, `--color-on-accent`, `--color-brand-2` | `on-accent` for any text/icon sitting on an accent or status fill. `brand-2` is the sidebar-mark gradient endpoint |
| Status | `--color-success`, `--color-warn`, `--color-danger`, `--color-danger-dim` | Tint backgrounds with `color-mix(in srgb, var(--color-X) 15%, transparent)` — never a separate literal. `btn-danger` is an outline treatment, never an accent-style fill |
| Type scale | `--text-2xs` … `--text-3xl` | Use the scale, not px values |
| Fonts | `--font-body`, `--font-mono`, `--font-display` | Display face (Zen Kaku Gothic New) applies to `h1`/`h2` via base styles and the `font-display` utility — page and group titles only, never row labels or body copy |
| Radii | `--radius-sm/md/lg/xl/full` | |
| Shadows | `--shadow-sm/md/lg/card/popover/focus-ring` | |
| Motion | `--motion-fast/base/slow` + easings | Every animation gates on `prefers-reduced-motion` (a global catch-all also exists) |
| Z-stack | `--z-sidebar/header/modal/modal-stack/toast/popover/top` + reader/banner layers | Never invent a raw z-index |
| Layout | `--header-h`, `--header-h-mobile`, `--sidebar-w`, `--container-page/narrow` | |
| Icon sizes | `--icon-xs` … `--icon-3xl` via `.icon-*` helper classes | |
| Charts | `--chart-1` … `--chart-11` | Per-theme overrides exist for light |

Permitted literal exceptions (mark JS ones with `// audit-ignore`):

- Reader / cover-viewer pure-black backdrop.
- `theme.js` token *source* values.
- Text over image scrims (e.g. manga-card title gradient) — genuinely
  theme-independent.

## Icons

One icon set: **Heroicons v2 outline, `stroke-width="1.5"`, `viewBox="0 0 24 24"`**,
exported as strings from `static/js/icons.js` with explicit
`width="18" height="18"` and `aria-hidden="true"`.

- Never paste an inline `<svg>` into a page or component. If a glyph is
  missing, add it to `icons.js` (Heroicons outline style) and import it.
- The only non-Heroicons entry is `iconDragHandle` (six-dot, fill-based);
  reuse it for every drag grip.
- Sizing: the default 18px is right inside `.btn-icon`/nav items. To resize,
  wrap in a container with an `.icon-*` class — the rule is
  `.icon-sm svg { … }` (descendant selector), so the class goes on the
  **wrapper**, not on the `<svg>` itself. An `<svg>` with no effective
  width/height stretches to fill its flex parent — this was the historical
  "giant icon in a button" bug.
- Progress rings use `viewBox="0 0 32 32"` deliberately; that is not a drift.

## Components (use before building)

Ready-made components in `static/js/components/` cover nearly every pattern:
empty/error states, list items, toasts, modals (`modal.js` is the *only*
dialog implementation — `showConfirm`/`showAlert`), pagination, filters,
master-detail, skeletons, sortable lists (`sortable-list.js` owns drag
handles), breadcrumbs, tabs, menus, combobox, chip groups. Settings pages use
the `_shared.js` builders. Extract any pattern used twice into a component.

Auth-style pages (centred card, error alert, labelled fields) compose
`components/auth-card.js` (`AuthCard`/`AuthError`/`AuthSuccess`/`AuthField`)
— don't rebuild that shell.

New components are Preact/htm; the vanilla-DOM escape hatches are limited to
the established legacy and performance-sensitive surfaces. Never mix both
styles inside one component's render path.

**Grep `static/js/components/` before writing a "new" widget.** Several
components exist in *both* a Preact and a vanilla flavour from the same file
(`Tabs`/`renderTabs`, `Pagination`/`renderPagination`, `EmptyState`/
`createEmptyState`, `Callout`/`createCallout`) — so "there's no Preact one" is
usually wrong. The overhaul found three pages that had hand-rolled a duplicate
of a component already sitting next to them.

Added by the overhaul, and worth knowing before you build:

- `form/` — `Select` (token-styled popover; sizes to its widest option, not to
  the trigger), `NumberInput` (steppers only for small bounded ranges),
  `DateInput` (labelled), `Callout` (info/warn/danger).
- `modal.js` gained a `sheet` variant — bottom-anchored at every width. Use it
  for mobile action sheets (the nav "More" sheet) rather than a second dialog.
- `EmptyState` gained `compact` — for a card sub-list or table body, where the
  full `py-16` treatment would dwarf its own panel.
- `chip-group.js` exposes `selected()`; don't mirror its state in the page.
- `.data-table` (app.css) is the one table look — mark numeric columns `.num`.
- `.prose-kani` (app.css) renders server-sanitised markdown (changelog, manga
  descriptions). The client never parses markdown; `render_description()` in
  `kani-web/src/utils.rs` does it, through ammonia.

**Drill-in flows use modals, not in-place view swaps.** When a row expands
into a detail editor (e.g. a source's MultiValueList preference), open a
`Modal` around the detail view on every breakpoint rather than replacing the
list inline on desktop and modal-ing only on mobile. One flow, both form
factors.

**Feature tabs that may be empty** are rendered greyed out (`disabled` on the
`Tabs` component) until data proves they apply, rather than appearing and
disappearing — e.g. the source-details Preferences tab.

## Responsive rules

Breakpoints: `sm` 480 / `md` 768 / `lg` 1024 / `xl` 1400 (Tailwind screens,
overridden in `@theme`).

- **Shell**: fixed sidebar (234px, 200px on md–lg tablets, hidden < md);
  mobile gets the bottom tab nav — `#page-content` reserves
  `4rem + env(safe-area-inset-bottom)` under md.
- **Viewport units**: use `dvh`/`svh`, never bare `vh`, for anything sized to
  the viewport (mobile URL-bar collapse). Pattern: `max-height: 90vh;` fallback
  line followed by `max-height: 90dvh;` in CSS; plain `dvh` is fine in inline
  styles.
- **Tables**: every `<table>` is wrapped in a `div.overflow-x-auto`. The page
  body never scrolls horizontally.
- **Touch targets**: interactive controls reach ≥ 40px on touch via
  `@media (hover: none)` bumps (`.btn-sm`, `.btn-xs`, `.dl-btn`, `.tile-btn`
  already do this). Hover-only affordances (card menu reveal, row nav arrows)
  must have an always-visible touch equivalent.
- **Hover**: wrap all hover styles in `@media (hover: hover)`.
- **Modals**: bottom-sheet on mobile (`items-end rounded-t-2xl`), centred card
  from `sm:` up — this comes free from `components/modal.js`; don't hand-roll.
- **Grids**: `.manga-grid` = 2 columns under 480px, then
  `auto-fill minmax(140px, 1fr)` (180px for `--large`).

## Client-side state

Three focused modules, each with the same `getState/setState/updateState/subscribe`
shape — pick the one that owns the key you need, import directly from it:

- `static/js/session.js` — auth/identity: `permissions`, `bootId`, `user`
  (populated from `getCurrentUser()` at boot and after a server restart),
  plus `hasPermission`/`initPermissions`.
- `static/js/cache.js` — SSE-fed server state: `chaptersProgress`,
  `scanNotifications`, `refreshState`, `libraryInvalidation`,
  `sourcesInvalidation`, `scanResult`, `scanningMangaIds`. A few keys
  cross-tab broadcast (see `_BROADCAST_KEYS`).
- `static/js/ui-state.js` — UI-local, not server-derived, not cross-tab:
  `inFlightChapters`, `mangaNotifyPrefs`, `sourcePreferenceVersion`.

There is no longer a generic barrel (`state.js` was split and deleted). If a
file needs keys from more than one module, import each under an alias
(`subscribe as subscribeCache`, `subscribe as subscribeUiState`, etc.) rather
than introducing a new re-export layer — `source-details.js` and `sse.js`
are worked examples of the three-module case.

## Settings search

The settings page searches individual settings, not just section names, and
the command palette (F-key/⌘K) surfaces the same individual-setting hits
deep-linked into their section. Both consume one shared index:
`static/js/settings-search-index.js` exports `SECTION_SEARCH_PREFIXES` (i18n
key prefixes per section) and `buildSettingsSearchIndex()` — **when adding a
settings section (or a new key prefix to an existing one), register its
prefix there** or its settings won't be findable from either surface. A
palette hit navigates to `/settings?section=X&q=<text>`; `settings/index.js`
reads the `q` param on load and re-runs its own search so the matching row
gets the `.search-hit` highlight and scrolls into view.

Settings unsaved-changes prompts go through the shared `_confirmDiscard()`
helper in `settings/index.js`, not a raw `showConfirm()` call — keeps the
title/labels consistent across the page-leave and section-switch guards.

The active section's nav entry shows a live `.dirty-dot` while it has
unsaved changes, polled from `isDirty()` every 400ms (`_startDirtyPoll` in
`settings/index.js`) rather than event-driven — a listener on inputs would
miss programmatic dirty→clean transitions, like a Reset button that clears
state without firing an input event.

## Density

Compact mode scales the **vertical rhythm of repeated content only** — rows,
list items, table cells — via the `--density` factor (`1` comfortable, `0.55`
compact) and `--chapter-row-h` for the virtualised chapter list. Consumers:
`.li-row`, `.kv`, `.data-table` cells, `[data-settings-row]`, the updates
timeline rows. Nothing else may read `--density`: control heights, tap
targets, cover art and nav chrome must be identical in both modes.

Never reintroduce a global `--spacing` override for density: every Tailwind
padding, gap, width and height utility derives from `--spacing`, so overriding
it silently shrinks the bottom nav and cover thumbnails while leaving
component-CSS rows untouched. If a new row surface should be density-aware,
move its vertical padding out of a `py-*` utility into a class that multiplies
by `var(--density)`.

## Dates crossing the API boundary

Two separate bugs shipped as "Invalid Date" / "Created unknown" in the UI. Both
are easy to reintroduce, and neither is visible from the frontend:

1. **`time`'s default serde emits `OffsetDateTime` as a JSON *array* of
   components**, not a string — `new Date(...)` cannot parse it. Any
   `OffsetDateTime` that reaches the client needs
   `#[serde(with = "time::serde::rfc3339")]` (or `::option`).
2. **The column may simply never be selected.** `list_users` had no `created_at`
   in its query at all, so the field was `undefined` rather than malformed.

If a date renders wrong, check *both*: is it selected, and is it RFC3339?
SQLite text timestamps should be emitted as RFC3339 from SQL
(`strftime('%Y-%m-%dT%H:%M:%SZ', col)`) rather than passed through ambiguous.

## Accessibility floor

- Visible focus: global `:focus-visible` outline; components may substitute
  `box-shadow: var(--shadow-focus-ring)`. Hidden inputs (toggle, star) forward
  focus rings to their visual element.
- Icon-only buttons always carry `aria-label` (translated).
- Modals: `role="dialog"`, `aria-modal`, labelled title, focus trap, focus
  restore, Escape to close.
- `prefers-reduced-motion: reduce` collapses all animation.

## Copy

- Every user-visible string goes through `t("key")` (`static/js/i18n.js`),
  catalog in `static/locales/en.js`. No inline literals — including default
  parameter values in components (the historical `'Notice'` / `'OK'` trap).
- Buttons name the action ("Save changes", not "Submit"); the same verb
  follows through to the toast ("Published" after "Publish").
- Errors say what went wrong and what to do next; empty states invite the
  first action. Follow the async-feedback contract below.
- CI runs `scripts/check-untranslated-strings.js` on every push, scanning
  `html\`...\`` text nodes and `.textContent`/`.innerHTML =` string literals.
  A genuinely non-translatable technical string (a version prefix `v${...}`,
  an HTTP header name, a CLI command) should be pulled into a local `const`
  and interpolated via `${...}` — that removes it from the lint's scan
  surface without embedding a `// i18n-ignore` comment inside literal HTML
  output (which would render as visible text). `// i18n-ignore` only works
  on a genuine single-line JS statement, never inside a multi-line template
  literal.

## Async feedback contract (summary)

| Situation | Feedback |
|-----------|----------|
| Instant, synchronous | Inline update only |
| Async, result visible in UI | `withBusy`/`useBusy` disable + inline error |
| Async, result not visible | Success toast; `showApiError(e)` on failure |
| Destructive | `showConfirm` first; trigger disabled in flight |
