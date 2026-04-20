// @ts-check
// Downloads page — active queue and history, split into tabs.

import { h, render } from 'preact';
import { useState, useEffect } from 'preact/hooks';
import htm from 'htm';
import { getState, subscribe } from '../state.js';
import { cancelDownload, getDownloadHistory } from '../api.js';
import { navigate } from '../router.js';
import { iconX, iconCheck, iconWarning } from '../icons.js';
import { formatChapterTitle, formatRelativeTime, getLocalInt, setLocal } from '../utils.js';
import { Icon } from '../components/icon.js';
const html = htm.bind(h);

/** @typedef {import('../state.js').ChapterProgress} ChapterProgress */

const HISTORY_SIZES = [5, 10, 25, 50];
const HISTORY_KEY = 'kani_download_history_size';

/** @param {{ entry: ChapterProgress }} props */
function ActiveRow({ entry }) {
  const pct = entry.totalPages > 0
    ? Math.round((entry.completedPages / entry.totalPages) * 100)
    : 0;

  async function handleCancel() {
    try { await cancelDownload(entry.id); } catch { /* ignore */ }
  }

  return html`
    <div class="flex flex-col gap-2 px-4 py-3 border-b border-border-subtle last:border-b-0">
      <div class="flex items-center gap-3">
        <div class="shrink-0 w-7 h-7 flex items-center justify-center rounded-full text-accent">
          <svg class="w-4 h-4 dl-ring-spin" viewBox="0 0 32 32" aria-hidden="true">
            <circle cx="16" cy="16" r="12" fill="none" stroke="currentColor" stroke-width="2.5" opacity="0.25" />
            <circle cx="16" cy="16" r="12" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"
              stroke-dasharray="56.5 18.9" />
          </svg>
        </div>
        <div class="flex-1 min-w-0">
          ${entry.mangaTitle && html`
            <p class="text-xs text-text-muted truncate">${entry.mangaTitle}</p>
          `}
          <p class="text-sm text-text truncate" title=${entry.name}>${entry.name}</p>
          ${entry.totalPages > 0 && html`
            <p class="text-xs text-text-muted mt-0.5">${entry.completedPages} / ${entry.totalPages} pages</p>
          `}
        </div>
        <button class="btn-ghost btn-sm shrink-0 text-danger" onClick=${handleCancel} aria-label="Cancel">Cancel</button>
      </div>
      <div class="h-1 rounded-full bg-surface-2 overflow-hidden ml-10">
        <div class="h-full rounded-full bg-accent transition-[width] duration-300" style=${{ width: pct + '%' }}></div>
      </div>
    </div>
  `;
}

/** @param {{ entry: ChapterProgress }} props */
function HistoryRow({ entry }) {
  const iconEl =
    entry.status === 'completed' || entry.status === 'completed_hidden'
      ? html`<${Icon} svg=${iconCheck} />`
      : entry.status === 'failed'
        ? html`<${Icon} svg=${iconWarning} />`
        : html`<${Icon} svg=${iconX} />`;

  const iconColor =
    entry.status === 'completed' || entry.status === 'completed_hidden' ? 'text-success' :
    entry.status === 'failed' ? 'text-danger' : 'text-text-muted';
  
  const date = entry.downloadedAt ? new Date(entry.downloadedAt) : null;
  const relTime = entry.downloadedAt ? formatRelativeTime(date) : null;
  const absTime = entry.downloadedAt ? date.toLocaleString() : null;

  return html`
    <div class="flex items-center gap-3 px-4 py-3 border-b border-border-subtle last:border-b-0">
      <div class=${'shrink-0 w-7 h-7 flex items-center justify-center [&_svg]:w-4 [&_svg]:h-4 ' + iconColor}>
        ${iconEl}
      </div>
      <div class="flex-1 min-w-0">
        ${entry.mangaId > 0 && html`
          <a
            href=${'/manga/' + entry.mangaId}
            class="text-xs text-text-muted truncate block hover:text-accent transition-colors"
            onClick=${(/** @type {MouseEvent} */ e) => { e.preventDefault(); navigate('/manga/' + entry.mangaId); }}
          >${entry.mangaTitle || 'Manga'}</a>
        `}
        <a
          href=${'/reader/' + entry.id}
          class="text-sm text-text truncate block hover:text-accent transition-colors"
          onClick=${(/** @type {MouseEvent} */ e) => { e.preventDefault(); navigate('/reader/' + entry.id); }}
        >${entry.name || formatChapterTitle({ chapter_number: entry.number ?? null })}</a>
        ${relTime && html`
          <span class="text-xs text-text-muted mt-0.5 block" title=${absTime}>${relTime}</span>
        `}
      </div>
    </div>
  `;
}

function DownloadsPage() {
  const [activeTab, setActiveTab] = useState(/** @type {'active'|'history'} */ ('active'));
  const [historySize, setHistorySize] = useState(() => getLocalInt(HISTORY_KEY, 10));
  const [active, setActive] = useState(/** @type {ChapterProgress[]} */ ([]));
  // History: seeded from API (persistent), merged with real-time SSE completions
  const [history, setHistory] = useState(/** @type {ChapterProgress[]} */ ([]));

  // Sync active downloads from chaptersProgress state
  useEffect(() => {
    function syncActive() {
      /** @type {Map<number, ChapterProgress>} */
      const map = getState('chaptersProgress');
      const entries = [...map.values()].filter(e => e.status === 'in_progress');
      entries.sort((a, b) => b.id - a.id);
      setActive(entries);
    }
    syncActive();
    return subscribe('chaptersProgress', syncActive);
  }, []);

  // Seed history from API, then merge new completions from SSE
  useEffect(() => {
    /** @type {Set<number>} */
    const seenIds = new Set();

    getDownloadHistory(50).then(items => {
      if (!Array.isArray(items)) return;
      const mapped = items.map(item => ({
        id: item.id,
        name: item.name,
        mangaId: item.mangaId,
        mangaTitle: item.mangaTitle,
        totalPages: 0,
        completedPages: 0,
        downloadedAt: item.downloadedAt ?? null,
        status: /** @type {ChapterProgress['status']} */ ('completed'),
      }));
      for (const e of mapped) seenIds.add(e.id);
      setHistory(mapped);
    }).catch(() => {});

    // Append new completions from SSE-driven chaptersProgress state
    return subscribe('chaptersProgress', () => {
      /** @type {Map<number, ChapterProgress>} */
      const map = getState('chaptersProgress');
      const newEntries = /** @type {ChapterProgress[]} */ ([]);
      for (const e of map.values()) {
        if ((e.status === 'completed' || e.status === 'completed_hidden' || e.status === 'failed' || e.status === 'cancelled') && !seenIds.has(e.id)) {
          seenIds.add(e.id);
          newEntries.push(e);
        }
      }
      if (newEntries.length > 0) {
        newEntries.sort((a, b) => b.id - a.id);
        setHistory(prev => [...newEntries, ...prev]);
      }
    });
  }, []);

  function changeHistorySize(/** @type {number} */ n) {
    setHistorySize(n);
    setLocal(HISTORY_KEY, String(n));
  }

  /** @param {'active'|'history'} id */
  function TabBtn({ id, label, count }) {
    const isActive = activeTab === id;
    return html`
      <button
        type="button"
        role="tab"
        aria-selected=${isActive}
        class=${'px-4 py-2 text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent rounded-t-md'
          + (isActive ? ' text-accent border-b-2 border-accent' : ' text-text-muted hover:text-text')}
        onClick=${() => setActiveTab(id)}
      >
        ${label}${count > 0 ? html` <span class="ml-1 text-xs opacity-70">${count}</span>` : ''}
      </button>
    `;
  }

  return html`
    <div class="max-w-2xl mx-auto px-4 md:px-6 py-6 flex flex-col gap-6">
      <div class="flex flex-col gap-1">
        <h1 class="text-2xl font-bold text-text">Downloads</h1>
      </div>

      <!-- Tab bar with inline "Show last" control when on History tab -->
      <div class="flex items-center gap-1 border-b border-border -mb-3" role="tablist">
        <${TabBtn} id="active" label="Active" count=${active.length} />
        <${TabBtn} id="history" label="History" count=${0} />
        ${activeTab === 'history' && html`
          <div class="ml-auto flex items-center gap-2 pb-1">
            <span class="text-xs text-text-muted">Show last</span>
            <select
              class="input w-20 text-sm h-7 py-0"
              aria-label="History size"
              value=${historySize}
              onChange=${(/** @type {any} */ e) => changeHistorySize(Number(e.target.value))}
            >
              ${HISTORY_SIZES.map(n => html`<option key=${n} value=${n}>${n}</option>`)}
            </select>
          </div>
        `}
      </div>

      <!-- Active tab -->
      ${activeTab === 'active' && html`
        <div class="bg-surface border border-border rounded-xl overflow-hidden">
          ${active.length === 0
            ? html`<p class="px-4 py-6 text-sm text-text-muted text-center">No active downloads.</p>`
            : active.map(e => html`<${ActiveRow} key=${e.id} entry=${e} />`)
          }
        </div>
      `}

      <!-- History tab -->
      ${activeTab === 'history' && html`
        <div class="flex flex-col gap-3">
          <div class="bg-surface border border-border rounded-xl overflow-hidden">
            ${history.length === 0
              ? html`<p class="px-4 py-6 text-sm text-text-muted text-center">No recent downloads.</p>`
              : history.slice(0, historySize).map(e => html`<${HistoryRow} key=${e.id} entry=${e} />`)
            }
          </div>
        </div>
      `}
    </div>
  `;
}

/** @param {HTMLElement} container */
export async function init(container) {
  document.title = 'Downloads - Kani';
  render(html`<${DownloadsPage} />`, container);
}

/** @param {HTMLElement} container */
export function destroy(container) {
  render(null, container);
}
