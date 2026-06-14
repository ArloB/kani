#!/usr/bin/env node
// @ts-check
// Scans static/js/**/*.js for t("...") / t('...') calls and verifies that
// every referenced key exists in the base catalog (static/locales/en.js).
// Exits 1 if any key is referenced but not defined.
//
// Usage: node scripts/check-i18n-keys.js

import { readFileSync, readdirSync, statSync } from 'fs';
import { join, extname } from 'path';
import { fileURLToPath, pathToFileURL } from 'url';
import { dirname } from 'path';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const ROOT = join(__dirname, '..');
const JS_DIR = join(ROOT, 'static', 'js');
const LOCALE_FILE = join(ROOT, 'static', 'locales', 'en.js');

function walk(dir) {
  /** @type {string[]} */
  const files = [];
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) {
      files.push(...walk(full));
    } else if (extname(full) === '.js') {
      files.push(full);
    }
  }
  return files;
}

const { default: catalog } = await import(pathToFileURL(LOCALE_FILE).href);
const catalogKeys = new Set(Object.keys(catalog));

const T_RE = /\bt\(\s*['"]([^'"]+)['"]/g;
const allFiles = walk(JS_DIR);

/** @type {Map<string, string[]>} key → [file, ...] */
const missing = new Map();

for (const file of allFiles) {
  const src = readFileSync(file, 'utf8');
  for (const m of src.matchAll(T_RE)) {
    const key = m[1];
    if (!catalogKeys.has(key)) {
      if (!missing.has(key)) missing.set(key, []);
      missing.get(key)?.push(file.replace(ROOT + '/', ''));
    }
  }
}

if (missing.size === 0) {
  console.log('✓ All i18n keys are defined in the catalog.');
  process.exit(0);
} else {
  console.error(`✗ ${missing.size} i18n key(s) referenced but not in catalog:`);
  for (const [key, files] of missing) {
    console.error(`  "${key}"  →  ${[...new Set(files)].join(', ')}`);
  }
  process.exit(1);
}
