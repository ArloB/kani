// @ts-check
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { croppedWidth, croppedHeight, hasCrop, cropStyles, cropSourceRect } from './crop.js';

test('croppedWidth / croppedHeight', () => {
  assert.equal(croppedWidth(1000), 1000);
  assert.equal(croppedWidth(1000, 10, 10), 800);   // trim 20%
  assert.equal(croppedHeight(1500, 0, 20), 1200);  // trim 20% bottom
  assert.equal(croppedWidth(1000, 50, 50), 1);     // clamped to 1, not 0
});

test('hasCrop', () => {
  assert.equal(hasCrop(), false);
  assert.equal(hasCrop({}), false);
  assert.equal(hasCrop({ cropTop: 0, cropLeft: 0 }), false);
  assert.equal(hasCrop({ cropRight: 5 }), true);
});

test('cropStyles: none → null', () => {
  assert.equal(cropStyles({}), null);
  assert.equal(cropStyles({ cropTop: 0 }), null);
});

test('cropStyles: inset + ratio-scaled vertical margins', () => {
  const s = cropStyles({ cropTop: 10, cropBottom: 5, cropLeft: 4, cropRight: 3 }, 1.5);
  assert.equal(s.clipPath, 'inset(10% 3% 5% 4%)');
  assert.equal(s.marginTop, '-15%');    // 10 * 1.5
  assert.equal(s.marginBottom, '-7.5%'); // 5 * 1.5
  assert.equal(s.marginLeft, '-4%');
  assert.equal(s.marginRight, '-3%');
});

test('cropStyles: default ratio when unknown', () => {
  const s = cropStyles({ cropTop: 10 }, 0);
  assert.equal(s.marginTop, '-15%'); // falls back to 1.5
});

test('cropSourceRect', () => {
  const r = cropSourceRect(1000, 1500, { cropTop: 10, cropBottom: 10, cropLeft: 5, cropRight: 5 });
  assert.equal(r.sx, 5);   // raw percentage preserved (inherited behaviour)
  assert.equal(r.sy, 10);
  assert.equal(r.sw, 900); // 1000 * (1 - 0.10)
  assert.equal(r.sh, 1200); // 1500 * (1 - 0.20)
});
