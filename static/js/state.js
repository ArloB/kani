// @ts-check
// Backward-compatible state hub. Delegates to session.js (auth/identity atoms),
// cache.js (SSE-fed server-cache atoms), or manages UI-local atoms directly.
// Prefer importing from the specific module in new code.

import {
  getState as _getSession,
  setState as _setSession,
  updateState as _updateSession,
  subscribe as _subSession,
} from './session.js';
import {
  getState as _getCache,
  setState as _setCache,
  updateState as _updateCache,
  subscribe as _subCache,
} from './cache.js';

export { hasPermission, initPermissions } from './session.js';

const _SESSION_KEYS = new Set(['permissions', 'bootId']);
const _CACHE_KEYS = new Set([
  'chaptersProgress',
  'scanNotifications',
  'refreshState',
  'libraryInvalidation',
  'sourcesInvalidation',
  'scanResult',
  'scanningMangaIds',
]);

// UI-local atoms (not cross-tab, not server-cache)
/** @type {Map<string, Set<Function>>} */
const _uiListeners = new Map();

/** @type {Record<string, any>} */
const _ui = {
  /** @type {Set<number>} */
  inFlightChapters: new Set(),

  /** @type {Map<number, boolean>} */
  mangaNotifyPrefs: new Map(),

  /**
   * Incremented per source when a source preference is changed.
   * @type {Map<number, number>}
   */
  sourcePreferenceVersion: new Map(),
};

/** @param {string} key */
function _uiNotify(key) {
  const set = _uiListeners.get(key);
  if (!set) return;
  const value = _ui[key];
  for (const fn of set) {
    try { fn(value); } catch (e) { console.error('State listener error:', e); }
  }
}

/**
 * @param {string} key
 * @returns {any}
 */
export function getState(key) {
  if (_SESSION_KEYS.has(key)) return _getSession(key);
  if (_CACHE_KEYS.has(key)) return _getCache(key);
  return _ui[key];
}

/**
 * @param {string} key
 * @param {any} value
 * @param {{ broadcast?: boolean }} [opts]
 */
export function setState(key, value, { broadcast = true } = {}) {
  if (_SESSION_KEYS.has(key)) { _setSession(key, value); return; }
  if (_CACHE_KEYS.has(key)) { _setCache(key, value, { broadcast }); return; }
  _ui[key] = value;
  _uiNotify(key);
}

/**
 * @param {string} key
 * @param {(current: any) => any} fn
 */
export function updateState(key, fn) {
  if (_SESSION_KEYS.has(key)) { _updateSession(key, fn); return; }
  if (_CACHE_KEYS.has(key)) { _updateCache(key, fn); return; }
  _ui[key] = fn(_ui[key]);
  _uiNotify(key);
}

/**
 * @param {string} key
 * @param {(value: any) => void} listener
 * @returns {() => void}
 */
export function subscribe(key, listener) {
  if (_SESSION_KEYS.has(key)) return _subSession(key, listener);
  if (_CACHE_KEYS.has(key)) return _subCache(key, listener);
  let set = _uiListeners.get(key);
  if (!set) { set = new Set(); _uiListeners.set(key, set); }
  set.add(listener);
  return () => _uiListeners.get(key)?.delete(listener);
}
