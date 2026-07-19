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
