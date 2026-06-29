// @ts-check
// Global keyboard shortcut manager.
// Scoped registrations (e.g. 'reader') are unregistered when the page destroys.
// F1 opens a cheatsheet of currently active shortcuts (or triggers a registered override).

/**
 * @typedef {{ key: string, description: string }} ShortcutEntry
 * @typedef {{ key: string | string[], description: string, handler: () => void }} ShortcutDef
 */

/** @type {Map<string, ShortcutEntry[]>} — scope → display entries */
const _registry = new Map();
/** @type {Map<string, (e: KeyboardEvent) => void>} — scope → handler fn */
const _handlers = new Map();
/** Optional override for the F1 help shortcut (e.g. opens a page-specific settings modal). */
let _f1Override = /** @type {(() => void)|null} */ (null);

let _globalListenerAttached = false;

function _onKeyDown(/** @type {KeyboardEvent} */ e) {
  const tag = /** @type {HTMLElement} */ (e.target)?.tagName;
  if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return;
  if (/** @type {HTMLElement} */ (e.target)?.isContentEditable) return;
  if (e.metaKey || e.ctrlKey || e.altKey) return;

  if (e.key === 'F1') {
    e.preventDefault();
    // If an override is registered (e.g. the reader's settings modal), call it instead.
    if (_f1Override) { _f1Override(); return; }
    _showCheatsheet();
    return;
  }

  for (const handler of _handlers.values()) {
    handler(e);
  }
}

function _attachGlobal() {
  if (!_globalListenerAttached) {
    document.addEventListener('keydown', _onKeyDown);
    _globalListenerAttached = true;
  }
}

/**
 * Registers a set of shortcuts under a named scope.
 * Returns an unregister function — call it when the page/component destroys.
 *
 * @param {string} scope
 * @param {ShortcutDef[]} shortcuts
 * @returns {() => void} unregister
 */
export function registerShortcuts(scope, shortcuts) {
  _attachGlobal();

  const entries = /** @type {ShortcutEntry[]} */ ([]);
  for (const s of shortcuts) {
    const keys = Array.isArray(s.key) ? s.key : [s.key];
    const primary = keys[0];
    entries.push({ key: primary, description: s.description });
  }
  _registry.set(scope, entries);

  _handlers.set(scope, (e) => {
    for (const s of shortcuts) {
      const keys = Array.isArray(s.key) ? s.key : [s.key];
      if (keys.includes(e.key)) {
        e.preventDefault();
        s.handler();
        return;
      }
    }
  });

  return () => {
    _registry.delete(scope);
    _handlers.delete(scope);
  };
}

/**
 * Returns the registered shortcut entries for a scope (or all if omitted).
 * @param {string} [scope]
 * @returns {ShortcutEntry[]}
 */
/**
 * Register a function to call instead of the default F1 cheatsheet.
 * Pass null to restore default behaviour.
 * @param {(() => void)|null} fn
 */
export function setF1Override(fn) { _f1Override = fn; }

export function getShortcuts(scope) {
  if (scope) return _registry.get(scope) ?? [];
  const all = [];
  for (const entries of _registry.values()) all.push(...entries);
  return all;
}

export function showCheatsheet() { _showCheatsheet(); }

function _showCheatsheet() {
  if (document.getElementById('shortcut-cheatsheet')) return;

  const backdrop = document.createElement('div');
  backdrop.id = 'shortcut-cheatsheet';
  backdrop.className = 'fixed inset-0 z-top flex items-center justify-center bg-black/60 backdrop-blur-sm p-4';

  const modal = document.createElement('div');
  modal.className = 'bg-surface border border-border rounded-xl shadow-lg max-w-sm w-full p-5 flex flex-col gap-4';
  modal.setAttribute('role', 'dialog');
  modal.setAttribute('aria-modal', 'true');
  modal.setAttribute('aria-label', 'Keyboard shortcuts');

  const header = document.createElement('div');
  header.className = 'flex items-center justify-between';
  header.innerHTML = `<h2 class="text-base font-semibold text-text">Keyboard shortcuts</h2>
    <button type="button" class="btn-icon" aria-label="Close">✕</button>`;
  modal.appendChild(header);

  const body = document.createElement('div');
  body.className = 'flex flex-col gap-3';

  for (const [scope, entries] of _registry) {
    if (entries.length === 0) continue;
    const section = document.createElement('div');
    section.className = 'flex flex-col gap-1';
    const scopeLabel = document.createElement('p');
    scopeLabel.className = 'text-xs font-medium uppercase tracking-wider text-text-muted mb-1';
    scopeLabel.textContent = scope.charAt(0).toUpperCase() + scope.slice(1);
    section.appendChild(scopeLabel);
    for (const entry of entries) {
      const row = document.createElement('div');
      row.className = 'flex items-center justify-between gap-4';
      row.innerHTML = `
        <span class="text-sm text-text">${entry.description}</span>
        <kbd class="text-xs bg-surface-2 border border-border rounded px-1.5 py-0.5 font-mono shrink-0">${entry.key}</kbd>
      `;
      section.appendChild(row);
    }
    body.appendChild(section);
  }

  if (body.children.length === 0) {
    body.innerHTML = '<p class="text-sm text-text-muted">No shortcuts active.</p>';
  }

  modal.appendChild(body);
  backdrop.appendChild(modal);
  document.body.appendChild(backdrop);

  const _close = () => backdrop.remove();
  header.querySelector('button')?.addEventListener('click', _close);
  backdrop.addEventListener('click', (e) => { if (e.target === backdrop) _close(); });
  const _escClose = (/** @type {KeyboardEvent} */ e) => {
    if (e.key === 'Escape') { _close(); document.removeEventListener('keydown', _escClose); }
  };
  document.addEventListener('keydown', _escClose);
}
