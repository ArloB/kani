// @ts-check
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { pagesPerMinute, minutesRemaining, adaptivePreloadCount, preloadThreshold } from './preload.js';

test('pagesPerMinute', () => {
  assert.equal(pagesPerMinute([]), null);
  assert.equal(pagesPerMinute([1000]), null);
  // 3 samples spanning 60s → 2 advances / 1 min = 2 ppm.
  assert.equal(pagesPerMinute([0, 30000, 60000]), 2);
  // Zero interval → null.
  assert.equal(pagesPerMinute([5000, 5000]), null);
});

test('minutesRemaining', () => {
  assert.equal(minutesRemaining(null, 10), null);
  assert.equal(minutesRemaining(2, 0), 0);
  assert.equal(minutesRemaining(2, -3), 0);
  assert.equal(minutesRemaining(2, 10), 5); // 10 pages / 2 ppm
});

test('adaptivePreloadCount: falls back to max without data', () => {
  assert.equal(adaptivePreloadCount({ max: 4, fetchMsLog: [], ppm: 2 }), 4);
  assert.equal(adaptivePreloadCount({ max: 4, fetchMsLog: [100, 100], ppm: 2 }), 4); // <3 samples
  assert.equal(adaptivePreloadCount({ max: 4, fetchMsLog: [100, 100, 100], ppm: null }), 4);
});

test('adaptivePreloadCount: computes clamped count', () => {
  // ppm 2 → msPerPage 30000; avgFetch 100ms → floor(300) clamped to max 4.
  assert.equal(adaptivePreloadCount({ max: 4, fetchMsLog: [100, 100, 100], ppm: 2 }), 4);
  // Slow fetch (20000ms): floor(30000/20000)=1.
  assert.equal(adaptivePreloadCount({ max: 4, fetchMsLog: [20000, 20000, 20000], ppm: 2 }), 1);
  // Very slow fetch → still min 1.
  assert.equal(adaptivePreloadCount({ max: 4, fetchMsLog: [90000, 90000, 90000], ppm: 2 }), 1);
});

test('preloadThreshold', () => {
  assert.equal(preloadThreshold('paged', 20), 17);
  assert.equal(preloadThreshold('scroll', 20), 16);           // floor(20 * 0.8)
  assert.equal(preloadThreshold('continuous-paged', 20), 16);
});
