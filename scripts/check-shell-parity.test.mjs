// @ts-check
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { elementIds, compareShells } from './check-shell-parity.mjs';

test('elementIds: collects ids and ignores commented-out markup', () => {
  const html = `
    <div id="modal-root"></div>
    <!-- <div id="ghost-root"></div> -->
    <div id='popover-root'></div>
    <main class="shell" id="app"></main>`;
  assert.deepEqual([...elementIds(html)].sort(), ['app', 'modal-root', 'popover-root']);
});

test('elementIds: ignores attributes that merely end in id', () => {
  assert.deepEqual([...elementIds('<div aria-labelledby="x" data-id="y" id="real"></div>')], ['real']);
});

test('compareShells: agreeing shells report nothing', () => {
  const a = new Set(['app', 'modal-root']);
  const b = new Set(['modal-root', 'app']);
  assert.deepEqual(compareShells(a, b), { missingFromProd: [], missingFromDev: [] });
});

test('compareShells: names the mount point each shell lacks', () => {
  const dev = new Set(['app', 'popover-root']);
  const prod = new Set(['app', 'toast-root']);
  assert.deepEqual(compareShells(dev, prod), {
    missingFromProd: ['popover-root'],
    missingFromDev: ['toast-root'],
  });
});
