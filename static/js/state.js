// @ts-check
// Observable global state store. Module-scoped atoms with subscribe/unsubscribe.
// No external dependencies.

import { getPermissions } from './api.js';

/**
 * @typedef {{ id: number, name: string, mangaId: number, mangaTitle: string,
 *             totalPages: number, completedPages: number,
 *             number?: number | null, downloadedAt?: string | null,
 *             status: 'in_progress'|'completed'|'completed_hidden'|'failed'|'cancelled'|'deleted' }} ChapterProgress
 * @typedef {{ mangaId: number, mangaName: string, count: number, chapterNames: string[] }} ScanNotification
 * @typedef {{ type: 'idle' }
 *          | { type: 'running', completed: number, total: number }
 *          | { type: 'done',    total: number,     failed: number }} RefreshState
 */

/** @type {Map<string, Set<Function>>} */
const _listeners = new Map();

/** @type {Record<string, any>} */
const _state = {
  /** @type {Set<string>} */
  permissions: new Set(),

  /** @type {Map<number, ChapterProgress>} */
  chaptersProgress: new Map(),

  /** @type {ScanNotification[]} */
  scanNotifications: [],

  /** @type {RefreshState} */
  refreshState: { type: 'idle' },

  /** Incremented when a full library refresh completes. Pages subscribe to re-fetch. */
  libraryInvalidation: 0,

  /** Incremented when a source is enabled/disabled. Sidebar subscribes to re-fetch. */
  sourcesInvalidation: 0,

  /** Server boot_id — compared on SSE reconnect to detect server restarts. */
  bootId: '',

  /** Chapter ids currently being requested for download (prevents double-submit). */
  inFlightChapters: /** @type {Set<number>} */ (new Set()),

  /** Manga ids whose SSE scan event just fired — briefly non-empty while scan sweeps. */
  scanningMangaIds: /** @type {Set<number>} */ (new Set()),

  /**
   * Populated when a scan run completes (via SSE 'completed' event).
   * @type {{ total: number, failed: number, newChapters: number, perManga: Map<number, number> } | null}
   */
  scanResult: null,

  /** Per-manga browser notification opt-out. True = notify (default). */
  mangaNotifyPrefs: /** @type {Map<number, boolean>} */ (new Map()),
};

/**
 * Returns the current value of a state atom.
 * @param {string} key
 */
export function getState(key) {
  return _state[key];
}

/**
 * Replaces a state atom and notifies subscribers.
 * @param {string} key
 * @param {any} value
 */
export function setState(key, value) {
  _state[key] = value;
  _notify(key);
}

/**
 * Applies an updater function to a state atom and notifies subscribers.
 * @param {string} key
 * @param {(current: any) => any} fn
 */
export function updateState(key, fn) {
  _state[key] = fn(_state[key]);
  _notify(key);
}

/**
 * Subscribes to changes on `key`. Returns an unsubscribe function.
 * @param {string} key
 * @param {(value: any) => void} listener
 * @returns {() => void} unsubscribe
 */
export function subscribe(key, listener) {
  let set = _listeners.get(key);
  if (!set) {
    set = new Set();
    _listeners.set(key, set);
  }
  set.add(listener);
  return () => _listeners.get(key)?.delete(listener);
}

/** @param {string} key */
function _notify(key) {
  const set = _listeners.get(key);
  if (!set) return;
  const value = _state[key];
  for (const fn of set) {
    try { fn(value); } catch (e) { console.error('State listener error:', e); }
  }
}

/**
 * Returns true if the current user has the given permission string.
 * @param {string} permission — e.g. 'library:view'
 * @returns {boolean}
 */
export function hasPermission(permission) {
  /** @type {Set<string>} */
  const perms = _state.permissions;
  return perms.has(permission);
}

/**
 * Fetches permissions from the server and populates the permissions state atom.
 * Redirects to /login on 401 (handled by api.js).
 */
export async function initPermissions() {
  try {
    const list = await getPermissions();
    
    if (Array.isArray(list)) {
      setState('permissions', new Set(list));
    } else if (list && typeof list === 'object') {
      const perms = [];
      for (const [resource, actions] of Object.entries(list)) {
        if (Array.isArray(actions)) {
          for (const action of actions) perms.push(`${resource}:${action}`);
        } else {
          perms.push(resource);
        }
      }
      setState('permissions', new Set(perms));
    }
  } catch (e) {
    /** @type {any} */
    const err = e;
    if (err.status !== 401) console.error('Failed to load permissions:', err);
  }
}
