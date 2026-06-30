// @ts-check
// Session-scoped state atoms: auth/permissions and server identity.

import { getPermissions } from './api.js';

const _state = {
  /** @type {Set<string>} */
  permissions: new Set(),
  bootId: '',
};

/** @type {Map<string, Set<Function>>} */
const _listeners = new Map();

/** @param {string} key */
function _notify(key) {
  const set = _listeners.get(key);
  if (!set) return;
  const value = _state[key];
  for (const fn of set) {
    try { fn(value); } catch (e) { console.error('Session state listener error:', e); }
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
 */
export function setState(key, value) {
  _state[key] = value;
  _notify(key);
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

/**
 * @param {string} permission
 * @returns {boolean}
 */
export function hasPermission(permission) {
  return _state.permissions.has(permission);
}

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
