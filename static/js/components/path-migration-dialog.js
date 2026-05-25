// @ts-check
// Migration dialog — confirm moving files to a new path and show progress.

import { h } from 'preact';
import { useState, useEffect, useRef } from 'preact/hooks';
import htm from 'htm';
import { Modal } from './modal.js';
import * as api from '../api.js';

const html = htm.bind(h);

/**
 * @param {{
 *   open: boolean,
 *   field: 'library_path' | 'wasm_storage_path',
 *   currentPath: string,
 *   newPath: string,
 *   onDone: (movedFiles: boolean) => void,
 *   onCancel: () => void,
 * }} props
 */
export function PathMigrationDialog({ open, field, currentPath, newPath, onDone, onCancel }) {
  // States: 'estimating' | 'confirm' | 'migrating' | 'done' | 'error'
  const [phase, setPhase] = useState('estimating');
  const [estimate, setEstimate] = useState(/** @type {any} */ (null));
  const [estimateError, setEstimateError] = useState(/** @type {string|null} */ (null));
  const [bytesCopied, setBytesCopied] = useState(0);
  const [totalBytes, setTotalBytes] = useState(0);
  const [migError, setMigError] = useState(/** @type {string|null} */ (null));
  const sseListenerRef = useRef(/** @type {((e: Event) => void) | null} */ (null));

  // Fetch estimate when dialog opens
  useEffect(() => {
    if (!open) return;
    setPhase('estimating');
    setEstimate(null);
    setEstimateError(null);
    setBytesCopied(0);
    setTotalBytes(0);
    setMigError(null);

    api.estimatePathMigration(field, newPath).then((est) => {
      setEstimate(est);
      setPhase('confirm');
    }).catch((/** @type {any} */ e) => {
      setEstimateError(e?.message ?? 'Could not estimate migration');
      setPhase('confirm');
    });
  }, [open, field, newPath]);

  // Detach SSE listener on unmount / close
  useEffect(() => {
    return () => {
      if (sseListenerRef.current) {
        window.removeEventListener('kani:sse', sseListenerRef.current);
        sseListenerRef.current = null;
      }
    };
  }, []);

  function detachSseListener() {
    if (sseListenerRef.current) {
      window.removeEventListener('kani:sse', sseListenerRef.current);
      sseListenerRef.current = null;
    }
  }

  async function handleMoveFiles() {
    setPhase('migrating');
    setBytesCopied(0);

    /** @param {Event} ev */
    const handler = (ev) => {
      const data = /** @type {CustomEvent} */ (ev).detail;
      if (!data || data.field !== field) return;
      if (data.type === 'path_migration_started') {
        setTotalBytes(data.total_bytes ?? 0);
      } else if (data.type === 'path_migration_progress') {
        setBytesCopied(data.bytes_copied ?? 0);
        setTotalBytes(data.total_bytes ?? 0);
      } else if (data.type === 'path_migration_completed') {
        detachSseListener();
        setPhase('done');
        setTimeout(() => onDone(true), 800);
      } else if (data.type === 'path_migration_failed') {
        detachSseListener();
        setMigError(data.error ?? 'Migration failed');
        setPhase('error');
      }
    };

    sseListenerRef.current = handler;
    window.addEventListener('kani:sse', handler);

    try {
      await api.startPathMigration(field, newPath);
    } catch (/** @type {any} */ e) {
      detachSseListener();
      setMigError(e?.message ?? 'Failed to start migration');
      setPhase('error');
    }
  }

  function handleChangePathOnly() {
    onDone(false);
  }

  function fmtBytes(bytes) {
    const gib = 1024 * 1024 * 1024;
    const mib = 1024 * 1024;
    if (bytes >= gib) return (bytes / gib).toFixed(1) + ' GB';
    return Math.round(bytes / mib) + ' MB';
  }

  const progressPct = totalBytes > 0 ? Math.round((bytesCopied / totalBytes) * 100) : 0;
  const label = field === 'library_path' ? 'Library' : 'WASM storage';

  let footer = null;
  if (phase === 'confirm') {
    footer = html`
      <button type="button" class="btn-ghost btn-sm" onClick=${onCancel}>Cancel</button>
      <button type="button" class="btn-ghost btn-sm" onClick=${handleChangePathOnly}>
        Change path only
      </button>
      <button
        type="button"
        class="btn-primary btn-sm"
        onClick=${handleMoveFiles}
        disabled=${estimateError != null || (estimate && !estimate.can_migrate)}
      >
        Move files
      </button>
    `;
  }

  return html`
    <${Modal}
      open=${open}
      onClose=${phase === 'migrating' ? undefined : onCancel}
      title=${'Move ' + label + ' files'}
      footer=${footer}
    >
      ${phase === 'estimating' && html`
        <p class="text-sm text-text-muted">Checking available space…</p>
      `}

      ${phase === 'confirm' && html`
        <div class="space-y-4 text-sm">
          <div class="flex flex-col gap-1">
            <span class="text-text-muted">From</span>
            <span class="font-mono text-text break-all">${currentPath}</span>
          </div>
          <div class="flex flex-col gap-1">
            <span class="text-text-muted">To</span>
            <span class="font-mono text-text break-all">${newPath}</span>
          </div>

          ${estimateError && html`
            <p class="text-danger">${estimateError}</p>
          `}

          ${estimate && html`
            <div class="bg-surface-2 rounded-lg p-3 space-y-1">
              <div class="flex justify-between">
                <span class="text-text-muted">Data to copy</span>
                <span class="font-medium">${fmtBytes(estimate.current_bytes)}</span>
              </div>
              <div class="flex justify-between">
                <span class="text-text-muted">Available space</span>
                <span class="font-medium">${fmtBytes(estimate.available_bytes)}</span>
              </div>
            </div>
          `}

          ${estimate && !estimate.can_migrate && html`
            <p class="text-danger text-sm">${estimate.reason ?? 'Migration not possible'}</p>
          `}

          <p class="text-text-muted text-xs">
            <strong>Change path only</strong> updates the setting without moving files —
            existing covers and downloads will be inaccessible until you move them manually.
          </p>
        </div>
      `}

      ${phase === 'migrating' && html`
        <div class="space-y-3 text-sm">
          <p class="text-text">Copying files… do not close this window.</p>
          <progress
            class="w-full h-2"
            value=${bytesCopied}
            max=${totalBytes || 1}
          ></progress>
          <p class="text-text-muted text-xs text-right">
            ${fmtBytes(bytesCopied)} / ${fmtBytes(totalBytes)} (${progressPct}%)
          </p>
        </div>
      `}

      ${phase === 'done' && html`
        <p class="text-success text-sm">Files moved successfully.</p>
      `}

      ${phase === 'error' && html`
        <div class="space-y-3">
          <p class="text-danger text-sm">${migError ?? 'Migration failed'}</p>
          <p class="text-text-muted text-xs">
            Your files have not been moved. The original path is still active.
          </p>
          <button type="button" class="btn-ghost btn-sm" onClick=${onCancel}>Close</button>
        </div>
      `}
    </${Modal}>
  `;
}
