// @ts-check
/**
 * Finds values that are written but cannot be read back.
 *
 * The 2026-07 sweep looked for *uncalled* code, which is a different defect:
 * `is_orphaned` had a writer, a serialiser, a client field and a rendered
 * badge, so every caller-count was healthy — and the feature was still dead,
 * because every SELECT filtered the written value out. A grep for dead code
 * cannot see that. This looks at the chain instead:
 *
 *     written -> selected -> serialised -> rendered
 *
 * and reports the hop where a column falls off. It is a lead generator, not a
 * verdict: read the code before believing any row.
 *
 * Known blind spots, each confirmed by a false positive on the first run:
 *
 *  - `SELECT *` hides every column, so a column read that way looks unselected.
 *    `manga.local_description` flagged for exactly this reason.
 *  - Coalescing done in Rust rather than SQL is invisible here. That same
 *    column is applied as `local_description.or(description)` in
 *    `kani-web/src/rest/manga.rs`, so it was never broken.
 *  - `settings.*` loads as one struct query, so every settings column reports
 *    zero selects. Treat that whole table as noise.
 *  - A column that is legitimately only ever a filter (`deleted_at`,
 *    `token_hash`) is correct usage, not a defect.
 *
 * The signal worth chasing is a column written with a meaningful value whose
 * only reads exclude that value — which is what `chapters.is_orphaned` was
 * before commit 2e63b9c.
 *
 * Usage: node scripts/audit-signal-chain.mjs [--all]
 */

import { readFileSync, readdirSync, statSync } from 'fs';
import { join, extname } from 'path';

const ROOT = process.env.KANI_ROOT ?? process.cwd();
const SHOW_ALL = process.argv.includes('--all');

function walk(dir, out = [], skip = ['target', 'node_modules', 'dist', 'vendor', '.git', 'site']) {
  for (const entry of readdirSync(dir)) {
    if (skip.includes(entry)) continue;
    const p = join(dir, entry);
    const st = statSync(p);
    if (st.isDirectory()) walk(p, out, skip);
    else out.push(p);
  }
  return out;
}

const files = walk(ROOT);
const rustSrc = files.filter((f) => extname(f) === '.rs' && !f.includes('/tests/') && !f.includes('/migrations/'));
const jsSrc = files.filter((f) => extname(f) === '.js' && f.includes('/static/js/'));
const migrations = files.filter((f) => f.startsWith(join(ROOT, 'migrations')) && f.endsWith('.sql'));

const rustText = rustSrc.map((f) => readFileSync(f, 'utf8')).join('\n');
const jsText = jsSrc.map((f) => readFileSync(f, 'utf8')).join('\n');
const sqlText = rustText;

/** Columns per table, from the migrations. */
function schema() {
  const tables = new Map();
  for (const f of migrations) {
    const sql = readFileSync(f, 'utf8');
    for (const m of sql.matchAll(/CREATE TABLE (?:IF NOT EXISTS )?[`"]?(\w+)[`"]?\s*\(([\s\S]*?)\n\s*\);/gi)) {
      const table = m[1];
      const cols = tables.get(table) ?? new Set();
      for (const line of m[2].split('\n')) {
        const c = line.trim().match(/^[`"]?(\w+)[`"]?\s+(INTEGER|TEXT|REAL|BLOB|BOOLEAN|NUMERIC|TIMESTAMP)/i);
        if (c && !/^(PRIMARY|FOREIGN|UNIQUE|CHECK|CONSTRAINT)$/i.test(c[1])) cols.add(c[1]);
      }
      tables.set(table, cols);
    }
    for (const m of sql.matchAll(/ALTER TABLE [`"]?(\w+)[`"]?\s+ADD COLUMN [`"]?(\w+)[`"]?/gi)) {
      const cols = tables.get(m[1]) ?? new Set();
      cols.add(m[2]);
      tables.set(m[1], cols);
    }
  }
  return tables;
}

const NOISE = new Set(['id', 'created_at', 'updated_at', 'user_id', 'manga_id', 'chapter_id', 'source_id']);

const rows = [];
for (const [table, cols] of schema()) {
  for (const col of cols) {
    if (NOISE.has(col)) continue;
    const word = new RegExp(`\\b${col}\\b`, 'g');

    const written = (sqlText.match(new RegExp(`(INSERT INTO[\\s\\S]{0,400}?\\b${col}\\b|SET[^;"]{0,200}\\b${col}\\b\\s*=)`, 'g')) ?? []).length;
    const mentions = (rustText.match(word) ?? []).length;
    // A SELECT that only ever appears inside a filter is not a read.
    const selected = (sqlText.match(new RegExp(`SELECT[\\s\\S]{0,600}?\\b${col}\\b`, 'g')) ?? []).length;
    const filtered = (sqlText.match(new RegExp(`(WHERE|AND|OR)[^;"']{0,80}\\b${col}\\b\\s*(=|IS|!=|<>)`, 'g')) ?? []).length;
    const inJs = (jsText.match(word) ?? []).length;

    rows.push({ table, col, written, mentions, selected, filtered, inJs });
  }
}

const suspects = rows.filter((r) => {
  if (r.written === 0 && r.mentions === 0) return false;         // schema-only, never used at all
  if (r.inJs > 0 && r.selected > r.filtered) return false;        // reaches the client and is really read
  return (
    (r.written > 0 && r.selected === 0) ||                        // written, never selected
    (r.written > 0 && r.filtered > 0 && r.selected <= r.filtered) // only ever used as a filter
  );
});

const shown = SHOW_ALL ? rows : suspects;
shown.sort((a, b) => (a.table + a.col).localeCompare(b.table + b.col));

console.log(`Scanned ${rows.length} columns across ${schema().size} tables\n`);
console.log('table.column'.padEnd(46), 'writes  selects  filters  js   verdict');
for (const r of shown) {
  let verdict = 'check';
  if (r.written > 0 && r.selected === 0) verdict = 'written, never selected';
  else if (r.filtered > 0 && r.selected <= r.filtered) verdict = 'only ever a filter';
  if (r.inJs === 0 && verdict !== 'check') verdict += '; absent from the UI';
  console.log(
    `${r.table}.${r.col}`.padEnd(46),
    String(r.written).padStart(5),
    String(r.selected).padStart(8),
    String(r.filtered).padStart(8),
    String(r.inJs).padStart(4),
    ' ' + verdict,
  );
}
console.log(`\n${shown.length} lead(s). Each is a lead, not a verdict — read the code.`);
