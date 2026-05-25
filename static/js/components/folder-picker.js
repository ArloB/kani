// @ts-check
// Folder browser modal — lets users pick or create a directory on the server.

import { h } from 'preact';
import { useState, useEffect, useCallback } from 'preact/hooks';
import htm from 'htm';
import { Modal } from './modal.js';
import * as api from '../api.js';

const html = htm.bind(h);

/**
 * @param {{
 *   open: boolean,
 *   onClose: () => void,
 *   onSelect: (path: string) => void,
 *   initialPath?: string,
 * }} props
 */
export function FolderPicker({ open, onClose, onSelect, initialPath = '/' }) {
  const [currentPath, setCurrentPath] = useState(initialPath);
  const [dirs, setDirs] = useState(/** @type {string[]} */ ([]));
  const [segments, setSegments] = useState(/** @type {string[]} */ ([]));
  const [drives, setDrives] = useState(/** @type {string[]} */ ([]));
  const [selectedPath, setSelectedPath] = useState(initialPath);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(/** @type {string|null} */ (null));
  const [newFolderName, setNewFolderName] = useState('');
  const [creating, setCreating] = useState(false);
  const [createError, setCreateError] = useState(/** @type {string|null} */ (null));

  const load = useCallback(/** @param {string} path */ async (path) => {
    setLoading(true);
    setError(null);
    try {
      const result = await api.fsBrowse(path);
      setCurrentPath(result.path);
      setSelectedPath(result.path);
      setDirs(result.dirs ?? []);
      setSegments(result.segments ?? []);
      setDrives(result.drives ?? []);
    } catch (/** @type {any} */ e) {
      setError(e?.message ?? 'Could not read directory');
    } finally {
      setLoading(false);
    }
  }, []);

  // Load initial path when opened
  useEffect(() => {
    if (open) load(initialPath);
  }, [open, initialPath, load]);

  /** Build path from segments up to index i (inclusive). */
  function buildPath(segs, i) {
    const parts = segs.slice(0, i + 1);
    if (parts.length === 0) return '/';
    // On Windows the first segment is a drive like "C:\" — already complete
    if (parts[0].endsWith(':\\') || parts[0].endsWith(':/')) {
      const [drive, ...rest] = parts;
      if (rest.length === 0) return drive;
      return drive + rest.join('/');
    }
    // Unix: rejoin with '/'
    return parts.map((p, j) => (j === 0 && p === '/') ? '' : p).join('/') || '/';
  }

  async function handleCreate() {
    if (!newFolderName.trim()) return;
    setCreating(true);
    setCreateError(null);
    try {
      const result = await api.fsMkdir(currentPath, newFolderName.trim());
      setNewFolderName('');
      await load(result.path);
    } catch (/** @type {any} */ e) {
      setCreateError(e?.message ?? 'Could not create folder');
    } finally {
      setCreating(false);
    }
  }

  const footer = html`
    <button type="button" class="btn-ghost btn-sm" onClick=${onClose}>Cancel</button>
    <button type="button" class="btn-primary btn-sm" onClick=${() => onSelect(selectedPath)}>
      Select
    </button>
  `;

  return html`
    <${Modal} open=${open} onClose=${onClose} title="Browse for folder" wide=${true} footer=${footer}>
      ${loading && html`<p class="text-sm text-text-muted py-2">Loading…</p>`}
      ${error && html`<p class="text-sm text-danger py-2">${error}</p>`}

      ${/* Drives (Windows only, shown at root level) */ drives.length > 0 && html`
        <div class="mb-3">
          <p class="text-xs text-text-muted mb-1 uppercase tracking-wide">Drives</p>
          <div class="flex flex-wrap gap-2">
            ${drives.map(d => html`
              <button
                type="button"
                class="btn-ghost btn-sm font-mono"
                onClick=${() => load(d)}
              >${d}</button>
            `)}
          </div>
        </div>
      `}

      ${/* Breadcrumb */ segments.length > 0 && html`
        <div class="flex items-center flex-wrap gap-1 text-sm mb-3 font-mono">
          ${segments.map((seg, i) => html`
            ${i > 0 && html`<span class="text-text-muted">/</span>`}
            <button
              type="button"
              class="text-primary hover:underline"
              onClick=${() => load(buildPath(segments, i))}
            >${seg}</button>
          `)}
        </div>
      `}

      ${/* Directory list */ !loading && html`
        <div class="border border-border rounded-lg overflow-hidden mb-3">
          ${dirs.length === 0 && html`
            <p class="text-sm text-text-muted px-3 py-2">No subdirectories</p>
          `}
          ${dirs.map(dir => html`
            <button
              type="button"
              key=${dir}
              class=${'w-full text-left flex items-center gap-2 px-3 py-2 text-sm hover:bg-surface-2 ' + (selectedPath === currentPath + '/' + dir || selectedPath === currentPath + dir ? 'bg-surface-2 font-medium' : '')}
              onClick=${() => {
                const next = currentPath.endsWith('/') || currentPath.endsWith('\\')
                  ? currentPath + dir
                  : currentPath + '/' + dir;
                setSelectedPath(next);
                load(next);
              }}
            >
              <span class="text-text-muted">📁</span>
              <span>${dir}</span>
            </button>
          `)}
        </div>
      `}

      <div class="flex items-center gap-2 mt-2">
        <input
          type="text"
          class="input text-sm flex-1"
          placeholder="New folder name"
          value=${newFolderName}
          onInput=${(/** @type {any} */ e) => { setNewFolderName(e.target.value); setCreateError(null); }}
          onKeyDown=${(/** @type {any} */ e) => { if (e.key === 'Enter') handleCreate(); }}
          disabled=${creating}
        />
        <button
          type="button"
          class="btn-ghost btn-sm"
          onClick=${handleCreate}
          disabled=${creating || !newFolderName.trim()}
        >${creating ? 'Creating…' : '+ New folder'}</button>
      </div>
      ${createError && html`<p class="text-xs text-danger mt-1">${createError}</p>`}

      <div class="mt-4 pt-3 border-t border-border-subtle">
        <p class="text-xs text-text-muted">Selected:</p>
        <p class="text-sm font-mono text-text break-all">${selectedPath}</p>
      </div>
    </${Modal}>
  `;
}
