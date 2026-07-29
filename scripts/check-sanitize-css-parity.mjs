#!/usr/bin/env node
// Checks static/js/sanitize-css.js against the same fixtures the Rust
// sanitiser is pinned to, in kani-app/src/service/ui_ext.rs
// (`the_client_mirror_fixtures_produce_exactly_these_outputs`).
//
// The client mirror exists so the theme editor can preview what the server will
// store. If the two implementations drift, the editor lies about what is saved —
// it shows CSS surviving that the server strips, or vice versa. Both sides are
// therefore pinned to these exact strings; change one and this fails until the
// other is updated to match.
//
// Run: node scripts/check-sanitize-css-parity.mjs

import { sanitizeCss } from '../static/js/sanitize-css.js';

/** @type {[string, string, string, string][]} name, input, expected css, expected stripped */
const CASES = [
  [
    'import',
    '@import url(evil.css); .btn { color: red }',
    '[data-kani-theme] .btn {\n  color: red;\n}',
    'at-rule @import',
  ],
  [
    'url_decl',
    '.a { color: red; background: url(x.png); font-size: 12px }',
    '[data-kani-theme] .a {\n  color: red;\n  font-size: 12px;\n}',
    'declaration `background`',
  ],
  ['scope', '.btn { color: red }', '[data-kani-theme] .btn {\n  color: red;\n}', ''],
  ['root', ':root { --x: 1 }', ':root[data-kani-theme] {\n  --x: 1;\n}', ''],
  [
    'media',
    '@media (min-width: 40rem) { .a { color: red } }',
    '@media (min-width: 40rem) { .a { color: red } }',
    '',
  ],
  ['fontface', '@font-face { src: local(x) }', '', 'at-rule @font-face'],
  [
    'comment',
    '/* } body { background: url(x) */ .a { color: red }',
    '[data-kani-theme] .a {\n  color: red;\n}',
    '',
  ],
  ['unbalanced', '.a { color: red', '', 'unparseable input'],
];

let failures = 0;
for (const [name, input, wantCss, wantStripped] of CASES) {
  const got = sanitizeCss(input);
  if (got.css !== wantCss) {
    failures += 1;
    console.error(
      `✘ ${name}: css mismatch\n  want: ${JSON.stringify(wantCss)}\n  got:  ${JSON.stringify(got.css)}`,
    );
  }
  const gotStripped = got.stripped.join('|');
  if (gotStripped !== wantStripped) {
    failures += 1;
    console.error(
      `✘ ${name}: stripped mismatch\n  want: ${JSON.stringify(wantStripped)}\n  got:  ${JSON.stringify(gotStripped)}`,
    );
  }
}

// Scoping is deliberately NOT idempotent: sanitising already-scoped CSS scopes
// it a second time, producing a descendant selector that matches nothing. Stored
// CSS is sanitised server-side, so `applyCustomCss` in static/js/theme.js must
// only re-sanitise text the user is still typing (`{ raw: true }`). This shipped
// once and disabled every custom rule silently, so it is pinned here: if someone
// makes scoping idempotent, this fails and the `raw` flag should go with it.
{
  const once = sanitizeCss('.btn { color: red }').css;
  const twice = sanitizeCss(once).css;
  if (once === twice) {
    failures += 1;
    console.error(
      '✘ scoping became idempotent — applyCustomCss can now drop its `raw` option ' +
        '(and this check should be deleted along with it)',
    );
  } else if (!twice.includes('[data-kani-theme] [data-kani-theme]')) {
    failures += 1;
    console.error(
      `✘ double-scoping changed shape; re-check applyCustomCss.\n  got: ${JSON.stringify(twice)}`,
    );
  }
}

if (failures > 0) {
  console.error(
    `\n${failures} sanitiser parity failure(s). The client mirror and the Rust ` +
      `sanitiser must agree — update both, or the theme editor will misreport ` +
      `what gets saved.`,
  );
  process.exit(1);
}

console.log(`Sanitiser parity OK (${CASES.length} fixtures match the Rust implementation).`);
