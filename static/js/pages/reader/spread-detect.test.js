// @ts-check
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { isWideImage, spreadPairVerdict, lumaVariance, edgeMatchResult } from './spread-detect.js';

test('isWideImage: landscape vs portrait', () => {
  assert.equal(isWideImage({ w: 2000, h: 1400 }), true);   // 1.43
  assert.equal(isWideImage({ w: 800, h: 1200 }), false);   // 0.67
  assert.equal(isWideImage({ w: 1200, h: 1000 }), true);   // 1.2 exactly
  assert.equal(isWideImage(null), false);
  assert.equal(isWideImage(undefined), false);
});

test('isWideImage: server-analysis gating', () => {
  // With server analysis, only flagged doubles count even if landscape.
  assert.equal(isWideImage({ w: 2000, h: 1000 }, { hasServerAnalysis: true, isServerDouble: false }), false);
  assert.equal(isWideImage({ w: 2000, h: 1000 }, { hasServerAnalysis: true, isServerDouble: true }), true);
  // Flagged double but portrait ratio → not wide.
  assert.equal(isWideImage({ w: 800, h: 1200 }, { hasServerAnalysis: true, isServerDouble: true }), false);
});

test('spreadPairVerdict: server path', () => {
  const a = { w: 800, h: 1200 }, b = { w: 800, h: 1200 };
  assert.equal(spreadPairVerdict(a, b, { hasServerAnalysis: true, isServerDoubleA: true }), 'pair');
  assert.equal(spreadPairVerdict(a, b, { hasServerAnalysis: true, isServerDoubleA: false }), 'not-pair');
  // Landscape A rejected even if flagged.
  assert.equal(spreadPairVerdict({ w: 2000, h: 1000 }, b, { hasServerAnalysis: true, isServerDoubleA: true }), 'not-pair');
});

test('spreadPairVerdict: heuristic path — ratio gating + edge-match states', () => {
  const a = { w: 800, h: 1200 }, b = { w: 800, h: 1200 }; // combined ratio 1600/1200 = 1.33 (in range)
  assert.equal(spreadPairVerdict(a, b, { edgeMatch: undefined }), 'needs-edge-check');
  assert.equal(spreadPairVerdict(a, b, { edgeMatch: null }), 'pending');
  assert.equal(spreadPairVerdict(a, b, { edgeMatch: true }), 'pair');
  assert.equal(spreadPairVerdict(a, b, { edgeMatch: false }), 'not-pair');
  // Landscape page rejected.
  assert.equal(spreadPairVerdict({ w: 1200, h: 1000 }, b, { edgeMatch: true }), 'not-pair');
  // Combined ratio too wide (>2.5): two very wide pages.
  assert.equal(spreadPairVerdict({ w: 1500, h: 1000 }, { w: 1500, h: 1000 }, { edgeMatch: true }), 'not-pair');
  // Missing dims.
  assert.equal(spreadPairVerdict(null, b), 'not-pair');
});

test('lumaVariance: flat vs varied', () => {
  const flat = new Uint8ClampedArray([10, 10, 10, 255, 10, 10, 10, 255]);
  assert.equal(lumaVariance(flat), 0);
  const varied = new Uint8ClampedArray([0, 0, 0, 255, 255, 255, 255, 255]);
  assert.ok(lumaVariance(varied) > 1000);
  assert.equal(lumaVariance(new Uint8ClampedArray([])), 0);
});

test('edgeMatchResult: matching mirrored strips', () => {
  // 2x1 strips. pxA right column should mirror pxB left column.
  // stripW=2, sampleH=1. pxA columns [c0, c1], compared mirrored to pxB [c0, c1]:
  // diff sums |pxA[mirror] - pxB[x]|. Build identical-after-mirror content.
  const stripW = 2, sampleH = 1;
  // pxA = [black, white], pxB = [white, black] → after mirror pxA becomes [white, black] == pxB → diff 0.
  const pxA = new Uint8ClampedArray([0, 0, 0, 255,  255, 255, 255, 255]);
  const pxB = new Uint8ClampedArray([255, 255, 255, 255,  0, 0, 0, 255]);
  const r = edgeMatchResult(pxA, pxB, { stripW, sampleH });
  assert.equal(r.flat, false);
  assert.equal(r.avgDiff, 0);
  assert.equal(r.isMatch, true);
});

test('edgeMatchResult: differing strips do not match', () => {
  const stripW = 2, sampleH = 1;
  const pxA = new Uint8ClampedArray([0, 0, 0, 255,  255, 255, 255, 255]);
  const pxB = new Uint8ClampedArray([0, 0, 0, 255,  255, 255, 255, 255]); // not mirror-equal
  const r = edgeMatchResult(pxA, pxB, { stripW, sampleH });
  assert.equal(r.flat, false);
  assert.ok(r.avgDiff > 20);
  assert.equal(r.isMatch, false);
});

test('edgeMatchResult: flat strips are rejected regardless of diff', () => {
  const stripW = 2, sampleH = 1;
  const flatA = new Uint8ClampedArray([10, 10, 10, 255, 10, 10, 10, 255]);
  const flatB = new Uint8ClampedArray([10, 10, 10, 255, 10, 10, 10, 255]);
  const r = edgeMatchResult(flatA, flatB, { stripW, sampleH });
  assert.equal(r.flat, true);
  assert.equal(r.isMatch, false);
  assert.equal(r.avgDiff, null);
});
