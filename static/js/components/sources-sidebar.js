// @ts-check
// SourcesSidebar — desktop sources sidebar with search, star toggles, and add source.
// AddSourceModal is co-located here since it is only used with this sidebar.

import { h } from 'preact';
import { useState, useEffect, useRef } from 'preact/hooks';
import htm from 'htm';
import * as api from '../api.js';
import { Modal } from './modal.js';
import { SidebarNav } from './sidebar-nav.js';
import { iconStarFilled, iconStarOutline } from '../icons.js';
import { navigate } from '../router.js';
const html = htm.bind(h);

// ── Pending source cleanup ─────────────────────────────────────────────────

/** @type {number|null} */
let _pendingSourceId = null;

/**
 * Returns and clears the pending source ID (created but not yet fully installed).
 * Call this in your page's destroy() to clean up orphaned records.
 * @returns {number|null}
 */
export function consumePendingSourceId() {
  const id = _pendingSourceId;
  _pendingSourceId = null;
  return id;
}

// ── AddSourceModal ─────────────────────────────────────────────────────────

/**
 * Modal for adding a new source from a WASM URL or file upload.
 * @param {{
 *   open: boolean,
 *   onClose: () => void,
 *   onCreated: () => void,
 * }} props
 */
export function AddSourceModal({ open, onClose, onCreated }) {
  const [mode, setMode]         = useState(/** @type {'url'|'file'} */ ('url'));
  const [wasmUrl, setWasmUrl]   = useState('');
  const [wasmFile, setWasmFile] = useState(/** @type {File|null} */ (null));
  const [loading, setLoading]   = useState(false);
  const [error, setError]       = useState(/** @type {string|null} */ (null));

  // Reset state when modal opens
  useEffect(() => {
    if (open) {
      setMode('url');
      setWasmUrl('');
      setWasmFile(null);
      setLoading(false);
      setError(null);
      _pendingSourceId = null;
    }
  }, [open]);

  // Block close while install is in-flight to prevent orphaned source records
  const handleClose = () => { if (!loading) onClose(); };

  async function handleSubmit() {
    if (mode === 'url') {
      if (!wasmUrl.trim()) { setError('URL is required.'); return; }
      if (!wasmUrl.trim().startsWith('https://')) { setError('URL must start with https://.'); return; }
    } else {
      if (!wasmFile) { setError('Please select a .wasm file.'); return; }
    }

    setLoading(true);
    setError(null);

    const placeholderName = mode === 'url'
      ? (new URL(wasmUrl.trim()).pathname.split('/').pop()?.replace(/\.wasm$/i, '') || 'extension')
      : (wasmFile?.name.replace(/\.wasm$/i, '') ?? 'extension');

    let sourceId;
    try {
      const result = await api.createSource(placeholderName);
      sourceId = result.id;
      _pendingSourceId = sourceId;
    } catch (e) {
      setError(/** @type {any} */ (e)?.message ?? 'Failed to create source.');
      setLoading(false);
      return;
    }

    try {
      if (mode === 'url') {
        await api.fetchWasm(sourceId, wasmUrl.trim());
      } else {
        await api.uploadWasm(sourceId, /** @type {File} */ (wasmFile));
      }
    } catch (e) {
      api.deleteSource(sourceId).catch(() => {});
      _pendingSourceId = null;
      setError(/** @type {any} */ (e)?.message ?? 'Failed to install extension.');
      setLoading(false);
      return;
    }

    _pendingSourceId = null;
    setLoading(false);
    onClose();
    onCreated();
  }

  const footer = html`
    <button class="btn-ghost" disabled=${loading} onClick=${handleClose}>Cancel</button>
    <button
      class="btn-primary"
      disabled=${loading || (mode === 'url' ? !wasmUrl.trim() : !wasmFile)}
      onClick=${handleSubmit}
    >${loading ? 'Installing…' : 'Add source'}</button>
  `;

  return html`
    <${Modal} open=${open} onClose=${handleClose} title="Add source" footer=${footer}>
      <div class="flex flex-col gap-4">

        <div class="flex items-center gap-1 p-1 bg-surface-2 rounded-lg w-fit">
          <button
            type="button"
            class=${'px-3 py-1.5 text-sm rounded-md transition-colors ' + (mode === 'url' ? 'bg-surface shadow text-text' : 'text-muted hover:text-text')}
            disabled=${loading}
            onClick=${() => setMode('url')}
          >From URL</button>
          <button
            type="button"
            class=${'px-3 py-1.5 text-sm rounded-md transition-colors ' + (mode === 'file' ? 'bg-surface shadow text-text' : 'text-muted hover:text-text')}
            disabled=${loading}
            onClick=${() => setMode('file')}
          >Upload file</button>
        </div>

        ${mode === 'url' && html`
          <div class="flex flex-col gap-1.5">
            <label class="text-sm font-medium text-text" for="add-source-url">Extension URL</label>
            <input
              id="add-source-url"
              type="url"
              class="input"
              placeholder="https://example.com/extension.wasm"
              value=${wasmUrl}
              disabled=${loading}
              onInput=${(/** @type {any} */ e) => setWasmUrl(e.target.value)}
              onKeyDown=${(/** @type {KeyboardEvent} */ e) => { if (e.key === 'Enter') handleSubmit(); }}
            />
          </div>
        `}

        ${mode === 'file' && html`
          <div class="flex flex-col gap-1.5">
            <label class="text-sm font-medium text-text" for="add-source-file">Extension file</label>
            <input
              id="add-source-file"
              type="file"
              class="input"
              accept=".wasm"
              disabled=${loading}
              onChange=${(/** @type {any} */ e) => setWasmFile(e.target.files?.[0] ?? null)}
            />
          </div>
        `}

        ${error && html`<p class="text-sm text-danger">${error}</p>`}

      </div>
    </${Modal}>
  `;
}

// ── SourcesSidebar ─────────────────────────────────────────────────────────

/**
 * Desktop sources sidebar: search input, source list with star toggles, and add source button.
 * Receives sources from the parent; call onCreated to signal a refresh is needed.
 *
 * @param {{
 *   sources: any[],
 *   activeSourceId?: number,
 *   onCreated?: () => void,
 * }} props
 */
export function SourcesSidebar({ sources, activeSourceId, onCreated }) {
  const [query, setQuery]     = useState('');
  // Track optimistic starred state per source id; sync when sources prop changes
  const [starred, setStarred] = useState(() => _buildStarred(sources));

  useEffect(() => {
    setStarred(prev => {
      // Merge: keep local overrides, add any new ids from refreshed sources
      /** @type {Record<number, boolean>} */
      const next = {};
      for (const s of sources) {
        next[s.id] = s.id in prev ? prev[s.id] : (s.favourited ?? false);
      }
      return next;
    });
  }, [sources]);

  /** @param {any[]} srcs @returns {Record<number, boolean>} */
  function _buildStarred(srcs) {
    /** @type {Record<number, boolean>} */
    const m = {};
    for (const s of srcs) m[s.id] = s.favourited ?? false;
    return m;
  }

  async function _toggleStar(/** @type {number} */ id, /** @type {boolean} */ current) {
    const newVal = !current;
    setStarred(prev => ({ ...prev, [id]: newVal }));
    try {
      await api.toggleSourceFavourite(id, newVal);
    } catch {
      setStarred(prev => ({ ...prev, [id]: current }));
    }
  }

  const filtered = query
    ? sources.filter(s => s.name?.toLowerCase().includes(query.toLowerCase()))
    : sources;

  return html`
    <div class="p-3 border-b border-border-subtle">
      <input
        type="search"
        class="input input-sm w-full"
        placeholder="Filter sources…"
        aria-label="Filter sources"
        value=${query}
        onInput=${(/** @type {any} */ e) => setQuery(e.target.value)}
      />
    </div>

    <${SidebarNav}>
      ${filtered.length === 0
        ? html`<p class="text-xs text-text-muted px-3 py-2">No sources found.</p>`
        : filtered.map(src => {
            const isActive  = src.id === activeSourceId;
            const isStarred = starred[src.id] ?? false;
            const initial   = (src.name ?? '?')[0].toUpperCase();
            return html`
              <div
                key=${src.id}
                class=${['list-item group cursor-pointer', isActive ? 'active' : '', !src.enabled ? 'opacity-60' : ''].filter(Boolean).join(' ')}
                style="padding: 9px 12px; gap: 10px;"
                role="link"
                tabIndex=${0}
                aria-current=${isActive ? 'page' : 'false'}
                onClick=${(/** @type {MouseEvent} */ e) => {
                  if (/** @type {HTMLElement} */ (e.target).closest('button')) return;
                  navigate(`/source/${src.id}`);
                }}
                onKeyDown=${(/** @type {KeyboardEvent} */ e) => {
                  if (e.key === 'Enter') navigate(`/source/${src.id}`);
                }}
              >
                <div class="flex items-center gap-3 border-b border-border-subtle last:border-0 w-full">
                    <span
                        class="avatar shrink-0"
                        style="background:var(--color-surface-3);color:var(--color-text-muted)"
                        aria-hidden="true"
                    >${initial}</span>
                    <span class="flex flex-col min-w-0 flex-1">
                        <span class="li-title truncate">${src.name}</span>
                        <span class="li-sub flex items-center gap-1.5">
                            <span>v${src.version}${src.language ? ` · ${src.language}` : ''}</span>
                            ${!src.enabled && html`<span class="text-2xs px-1 py-0.5 rounded bg-warn/20 text-warn font-medium leading-none">Off</span>`}
                        </span>
                    </span>
                    <button
                        type="button"
                        class=${[
                            'shrink-0 p-1 rounded-md transition-colors icon-xs',
                            'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent',
                            isStarred ? 'text-accent' : 'text-text-faint opacity-0 group-hover:opacity-100',
                        ].join(' ')}
                        aria-label=${isStarred ? 'Unfavourite' : 'Favourite'}
                        onClick=${(/** @type {MouseEvent} */ e) => {
                            e.preventDefault();
                            e.stopPropagation();
                            _toggleStar(src.id, isStarred);
                        }}
                        dangerouslySetInnerHTML=${{ __html: isStarred ? iconStarFilled : iconStarOutline }}
                    />
                </div>
              </div>
            `;
          })
      }
    <//>

  `;
}
