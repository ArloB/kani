// @ts-check
// Server-cache state atoms: SSE-populated, cross-tab broadcast where noted.

import { broadcastStateChange } from './sync.js';

/**
 * @typedef {{ id: number, name: string, mangaId: number, mangaTitle: string,
 *             totalPages: number, completedPages: number,
 *             number?: number | null, downloadedAt?: string | null,
 *             jobId?: string | null,
 *             status: 'in_progress'|'completed'|'completed_hidden'|'failed'|'cancelled'|'deleted' }} ChapterProgress
 * @typedef {{ mangaId: number, mangaName: string, count: number, chapterNames: string[] }} ScanNotification
 * @typedef {{ type: 'idle' }
 *          | { type: 'running', completed: number, total: number }
 *          | { type: 'done',    total: number,     failed: number }} RefreshState
 */

const _BROADCAST_KEYS = new Set(['chaptersProgress', 'libraryInvalidation', 'sourcesInvalidation']);

/** @type {Record<string, any>} */
const _state = {
  /** @type {Map<number, ChapterProgress>} */
  chaptersProgress: new Map(),

  /** @type {ScanNotification[]} */
  scanNotifications: [],

  /** @type {RefreshState} */
  refreshState: { type: 'idle' },

  libraryInvalidation: 0,

  sourcesInvalidation: 0,

  /** @type {{ total: number, failed: number, newChapters: number, perManga: Map<number, number> } | null} */
  scanResult: null,

  /** @type {Set<number>} */
  scanningMangaIds: new Set(),
};

/** @type {Map<string, Set<Function>>} */
const _listeners = new Map();

/** @param {string} key */
function _notify(key) {
  const set = _listeners.get(key);
  if (!set) return;
  const value = _state[key];
  for (const fn of set) {
    try { fn(value); } catch (e) { console.error('Cache state listener error:', e); }
  }
}

/**
 * @param {string} key
 * @returns {any}
 */
export function getState(key) {
  return _state[key];
}

/**
 * @param {string} key
 * @param {any} value
 * @param {{ broadcast?: boolean }} [opts]
 */
export function setState(key, value, { broadcast = true } = {}) {
  _state[key] = value;
  _notify(key);
  if (broadcast && _BROADCAST_KEYS.has(key)) broadcastStateChange(key, value);
}

/**
 * @param {string} key
 * @param {(current: any) => any} fn
 */
export function updateState(key, fn) {
  _state[key] = fn(_state[key]);
  _notify(key);
}

/**
 * @param {string} key
 * @param {(value: any) => void} listener
 * @returns {() => void}
 */
export function subscribe(key, listener) {
  let set = _listeners.get(key);
  if (!set) { set = new Set(); _listeners.set(key, set); }
  set.add(listener);
  return () => _listeners.get(key)?.delete(listener);
}
