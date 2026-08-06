// @ts-check
// Session-scoped state atoms: auth/permissions and server identity.
// Signal-backed via the shared store factory; `sigFor` is exported for
// reactive readers.

import { getPermissions } from './api.js';
import { createSignalStore } from './signal-store.js';

const _store = createSignalStore({
  /** @type {Set<string>} */
  permissions: new Set(),
  bootId: '',
  /** @type {{ id: number, username: string, email?: string, roles?: string[], email_verified_at?: string|null } | null} */
  user: null,
});

export const { sigFor, getState, setState, updateState, subscribe } = _store;

/**
 * @param {string} permission
 * @returns {boolean}
 */
export function hasPermission(permission) {
  return getState('permissions').has(permission);
}

/**
 * Last-known permissions, so the shell can still be navigated when the server
 * cannot be reached. These decide what UI to *show*, never what is allowed —
 * every request is authorised server-side — so a stale set can at worst offer a
 * link that 403s, which is what offline reading needs to work at all.
 */
const PERMS_CACHE_KEY = 'kani_permissions';

/** @param {Set<string>} perms */
function _rememberPermissions(perms) {
  try {
    localStorage.setItem(PERMS_CACHE_KEY, JSON.stringify([...perms]));
  } catch { /* storage full or blocked — the live set still works this session */ }
}

/** @returns {Set<string>|null} */
function _recallPermissions() {
  try {
    const raw = localStorage.getItem(PERMS_CACHE_KEY);
    if (!raw) return null;
    const list = JSON.parse(raw);
    return Array.isArray(list) && list.length ? new Set(list) : null;
  } catch {
    return null;
  }
}

export function clearRememberedPermissions() {
  try {
    localStorage.removeItem(PERMS_CACHE_KEY);
  } catch { /* nothing to clear */ }
}

export async function initPermissions() {
  try {
    const list = await getPermissions();
    if (Array.isArray(list)) {
      const perms = new Set(list);
      setState('permissions', perms);
      _rememberPermissions(perms);
    } else if (list && typeof list === 'object') {
      const perms = [];
      for (const [resource, actions] of Object.entries(list)) {
        if (Array.isArray(actions)) {
          for (const action of actions) perms.push(`${resource}:${action}`);
        } else {
          perms.push(resource);
        }
      }
      const set = new Set(perms);
      setState('permissions', set);
      _rememberPermissions(set);
    }
  } catch (e) {
    /** @type {any} */
    const err = e;
    if (err.status === 401) {
      // Signed out: the remembered set belongs to whoever was here before.
      clearRememberedPermissions();
      return;
    }
    console.error('Failed to load permissions:', err);

    // Without this the set stays empty, every hasPermission() is false, and the
    // navigation empties out — which offline made obvious: the shell booted and
    // cached chapters were served, but there was no way to navigate to one.
    const remembered = _recallPermissions();
    if (remembered) {
      setState('permissions', remembered);
      return;
    }

    // Nothing remembered (a first load that failed) — a stripped-down UI with
    // no explanation reads as "the feature is gone", not "reload me".
    const { showToast } = await import('./components/toast.js');
    const { t } = await import('./i18n.js');
    showToast(t('session.permissions_failed'), { type: 'error' });
  }
}
