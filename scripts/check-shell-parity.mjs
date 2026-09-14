#!/usr/bin/env node
// @ts-check
/**
 * Verifies the dev and production frontend shells expose the same mount points.
 *
 * Components portal into elements looked up by id, and every such lookup is
 * guarded (`if (root) render(...)`), so a shell missing one produces no error —
 * the feature is simply inert. Exits with status 1 when the two shells disagree.
 */

import { readFileSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const DEV_SHELL = 'static/index.html';
const PROD_SHELL = 'static/index.prod.html';

/**
 * Ids only the dev shell is expected to carry, mapped to the reason.
 * @type {Record<string, string>}
 */
const DEV_ONLY = {};

/**
 * @param {string} html
 * @returns {Set<string>}
 */
export function elementIds(html) {
  const withoutComments = html.replace(/<!--[\s\S]*?-->/g, '');
  const ids = new Set();
  for (const [, id] of withoutComments.matchAll(/\sid=["']([^"']+)["']/g)) {
    ids.add(id);
  }
  return ids;
}

/**
 * @param {Set<string>} dev
 * @param {Set<string>} prod
 * @returns {{ missingFromProd: string[], missingFromDev: string[] }}
 */
export function compareShells(dev, prod) {
  return {
    missingFromProd: [...dev].filter(id => !prod.has(id) && !(id in DEV_ONLY)).sort(),
    missingFromDev: [...prod].filter(id => !dev.has(id)).sort(),
  };
}

function main() {
  const dev = elementIds(readFileSync(join(ROOT, DEV_SHELL), 'utf8'));
  const prod = elementIds(readFileSync(join(ROOT, PROD_SHELL), 'utf8'));
  const { missingFromProd, missingFromDev } = compareShells(dev, prod);

  if (missingFromProd.length === 0 && missingFromDev.length === 0) {
    console.log(`Frontend shells agree. ${dev.size} mount point(s) in both.`);
    return 0;
  }

  for (const id of missingFromProd) {
    console.error(`${PROD_SHELL} is missing id="${id}" (present in ${DEV_SHELL})`);
  }
  for (const id of missingFromDev) {
    console.error(`${DEV_SHELL} is missing id="${id}" (present in ${PROD_SHELL})`);
  }
  console.error(
    '\nA lookup against a missing mount point fails silently, so whatever portals\n'
    + 'into it stops working with nothing in the console. Add the element to the\n'
    + 'shell that lacks it, or record it in DEV_ONLY with a reason.',
  );
  return 1;
}

if (!process.env.NODE_TEST_CONTEXT && process.argv[1] === fileURLToPath(import.meta.url)) {
  process.exit(main());
}
