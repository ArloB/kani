// @ts-check
// Download progress overlay — fixed bottom-right panel showing active downloads.

import { h, render } from 'preact';
import { useState, useEffect } from 'preact/hooks';
import htm from 'htm';
import { getState, subscribe, updateState } from '../state.js';
import { iconX } from '../icons.js';
import { Icon } from './icon.js';
import { t } from '../i18n.js';
const html = htm.bind(h);

/** @typedef {import('../state.js').ChapterProgress} ChapterProgress */

function _statusLabel(status) {
  switch (status) {
    case 'in_progress':      return t('download.status.in_progress');
    case 'completed':
    case 'completed_hidden': return t('download.status.done');
    case 'failed':           return t('download.status.failed');
    case 'cancelled':        return t('download.status.cancelled');
    case 'deleted':          return t('download.status.deleted');
    default:                 return status;
  }
}

/** @param {{ entry: ChapterProgress }} props */
function DownloadItem({ entry }) {
  const pct = entry.totalPages > 0
    ? Math.round((entry.completedPages / entry.totalPages) * 100)
    : 0;

  const isTerminal = entry.status !== 'in_progress';

  const barColor =
    entry.status === 'in_progress' ? 'bg-accent' :
    entry.status === 'completed'   ? 'bg-success' :
    entry.status === 'failed'      ? 'bg-danger'  : 'bg-surface-3';

  function dismiss() {
    updateState('chaptersProgress', (map) => {
      const next = new Map(map);
      next.delete(entry.id);
      return next;
    });
  }

  return html`
    <div class="flex flex-col gap-1.5 px-4 py-3">
      <div class="flex items-center gap-2">
        <span class="flex-1 text-sm text-text truncate" title=${entry.name}>${entry.name}</span>
        <span class="text-xs text-text-muted shrink-0">${_statusLabel(entry.status)}</span>
        ${isTerminal && html`
          <button class="btn-icon shrink-0" aria-label=${t('common.dismiss')} onClick=${dismiss}><${Icon} svg=${iconX} /></button>
        `}
      </div>
      <div class="h-1 rounded-full bg-surface-2 overflow-hidden">
        <div class=${'h-full rounded-full transition-[width] duration-300 ' + barColor} style=${{ width: pct + '%' }}></div>{/* justified: animates only width */}
      </div>
    </div>
  `;
}

function DownloadOverlay() {
  const [entries, setEntries] = useState(/** @type {ChapterProgress[]} */ ([]));

  useEffect(() => {
    function sync() {
      /** @type {Map<number, ChapterProgress>} */
      const map = getState('chaptersProgress');
      const visible = [...map.values()].filter(
        e => e.status !== 'deleted' && e.status !== 'completed_hidden'
      );
      // Sort: in_progress first, then by id
      visible.sort((a, b) => {
        if (a.status === 'in_progress' && b.status !== 'in_progress') return -1;
        if (b.status === 'in_progress' && a.status !== 'in_progress') return 1;
        return a.id - b.id;
      });
      setEntries(visible);
    }
    sync();
    return subscribe('chaptersProgress', sync);
  }, []);

  if (entries.length === 0) return null;

  return html`
    <div class="fixed bottom-4 left-4 right-4 sm:left-auto sm:w-72 bg-surface border border-border rounded-xl shadow-lg z-50 overflow-hidden">
      <div class="flex items-center justify-between px-4 py-2.5 border-b border-border-subtle">
        <span class="text-sm font-semibold text-text">${t('download.overlay.title')}</span>
        <span class="text-xs text-text-muted">${t('download.overlay.active', { count: entries.filter(e => e.status === 'in_progress').length })}</span>
      </div>
      <div class="flex flex-col divide-y divide-border-subtle max-h-64 overflow-y-auto">
        ${entries.map(e => html`<${DownloadItem} key=${e.id} entry=${e} />`)}
      </div>
    </div>
  `;
}

/**
 * Mount the download overlay into a container element.
 * @param {HTMLElement} container
 */
export function mountOverlay(container) {
  render(html`<${DownloadOverlay} />`, container);
}
