// @ts-check
// RestartTray — warn-tray showing pending restart fields, with a Restart button.
// Replaces the old _renderRestartBanner in settings.js.
//
// Storage keys:
//   kani_restart_needed  — '1' when a restart is required
//   kani_restart_boot_id — boot_id at the time the flag was set
//   kani_pending_fields  — JSON array of field names that require restart

import { h, render } from 'preact';
import { useState, useEffect } from 'preact/hooks';
import htm from 'htm';
import { getLocal, setLocal, getJsonSafe } from '../utils.js';
import { iconWarning } from '../icons.js';
const html = htm.bind(h);

const KEY_NEEDED  = 'kani_restart_needed';
const KEY_BOOT    = 'kani_restart_boot_id';
const KEY_FIELDS  = 'kani_pending_fields';

/** @returns {string[]} */
function readPendingFields() {
  const raw = getLocal(KEY_FIELDS);
  if (!raw) return [];
  // Back-compat: treat comma-strings from older clients as arrays
  const parsed = getJsonSafe(raw);
  if (Array.isArray(parsed)) return parsed;
  return raw ? raw.split(',').filter(Boolean) : [];
}

/** @param {string[]} fields */
export function addPendingFields(fields) {
  const existing = new Set(readPendingFields());
  for (const f of fields) existing.add(f);
  setLocal(KEY_NEEDED, '1');
  setLocal(KEY_FIELDS, JSON.stringify([...existing]));
}

export function clearPendingFields() {
  setLocal(KEY_NEEDED, '');
  setLocal(KEY_FIELDS, JSON.stringify([]));
}

/**
 * @param {{
 *   currentBootId?: string | null,
 *   onRestart?: () => void,
 * }} props
 */
function RestartTray({ currentBootId, onRestart }) {
  const [fields, setFields] = useState(/** @type {string[]} */ ([]));
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    function check() {
      const needed  = getLocal(KEY_NEEDED) === '1';
      const bootId  = getLocal(KEY_BOOT);
      const pending = readPendingFields();
      // If boot_id changed since flag was set, the server already restarted
      if (needed && currentBootId && bootId && bootId !== currentBootId) {
        clearPendingFields();
        setVisible(false);
        return;
      }
      setFields(pending);
      setVisible(needed && pending.length > 0);
    }
    check();
    window.addEventListener('storage', check);
    return () => window.removeEventListener('storage', check);
  }, [currentBootId]);

  if (!visible) return null;

  return html`
    <div class="warn-tray mb-4" role="status">
      <span class="shrink-0 icon-sm" aria-hidden="true">${html([iconWarning])}</span>
      <div class="flex-1">
        <strong>Restart required</strong>
        <div class="flex flex-wrap gap-1 mt-1">
          ${fields.map(f => html`<span class="dirty-chip">${f}</span>`)}
        </div>
      </div>
      <button type="button" class="btn-primary btn-sm shrink-0" onClick=${onRestart}>
        Restart now
      </button>
    </div>
  `;
}

/**
 * Mount RestartTray into a DOM element.
 * @param {HTMLElement} el
 * @param {{ currentBootId?: string | null, onRestart?: () => void }} props
 * @returns {{ update: (p: object) => void, unmount: () => void }}
 */
export function mountRestartTray(el, props) {
  let currentProps = props;
  function doRender() {
    render(html`<${RestartTray} ...${currentProps} />`, el);
  }
  doRender();
  return {
    update(p) { currentProps = { ...currentProps, ...p }; doRender(); },
    unmount() { render(null, el); },
  };
}
