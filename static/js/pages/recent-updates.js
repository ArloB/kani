// @ts-check
// Recent updates page — paginated list of newly added chapters grouped by date then manga.

import * as api from '../api.js';
import { hasPermission } from '../state.js';
import { renderPagination } from '../components/pagination.js';
import { getParam, replaceState as urlReplaceState } from '../url-params.js';
import { getMangaCoverUrl } from '../api.js';
import { formatChapterTitle, hasNextPage, formatDate, escapeHtml, deferredSkeleton, addPullToRefresh } from '../utils.js';
import { skeletonUpdateList } from '../components/skeletons.js';
import { startLoading, finishLoading } from '../components/page-loading-bar.js';
import { createErrorState } from '../components/error-state.js';
import { createEmptyState } from '../components/empty-state.js';
import { iconBookOpen, iconDownload, iconCheck, iconSpinner } from '../icons.js';
import { getState, updateState, subscribe } from '../state.js';
import { setPageHeader, clearPageHeader } from '../components/app-header.js';

// ── Helpers ───────────────────────────────────────────────────────────────────

/**
 * Returns the download control HTML for a chapter row.
 * Exactly one of: a read link (green tick), a download button, or empty string.
 * @param {{ id: number, title: string, is_downloaded: boolean }} ch
 * @param {boolean} canDownload
 */
function _renderDownloadControl(ch, canDownload) {
  if (ch.is_downloaded) {
    return `<a href="/reader/${ch.id}" class="icon-xs text-success shrink-0 dl-btn" aria-label="Read ${escapeHtml(ch.title)}" title="Read">${iconCheck}</a>`;
  }
  if (canDownload) {
    return `<button class="dl-btn shrink-0" aria-label="Download ${escapeHtml(ch.title)}" data-chapter-id="${ch.id}" title="Download">${iconDownload}</button>`;
  }
  return '';
}

// ── Module state ──────────────────────────────────────────────────────────────

let _page = 1;
/** @type {AbortController | null} */
let _abort = null;
/** @type {(() => void) | null} */
let _destroyPagination = null;
/** @type {(() => void) | null} */
let _unsubProgress = null;
/** @type {(() => void) | null} */
let _removePullToRefresh = null;
/** @type {HTMLElement | null} */
let _listEl = null;

// ── Init ──────────────────────────────────────────────────────────────────────

/** @param {HTMLElement} container */
export async function init(container) {
  document.title = 'Recent Updates - Kani';
  _page = parseInt(getParam('page') ?? '1', 10) || 1;
  _listEl = null;
  _unsubProgress?.();
  _unsubProgress = null;
  setPageHeader({ crumbs: [{ label: 'Updates' }] });

  container.innerHTML = `
    <div class="max-w-page mx-auto w-full overflow-x-hidden px-4 md:px-6 py-4 md:py-6 flex flex-col gap-6">
      <div class="js-list flex flex-col gap-6 min-w-0" aria-live="polite" aria-busy="true"></div>
      <div class="js-pagination"></div>
    </div>
  `;

  _listEl = /** @type {HTMLElement} */ (container.querySelector('.js-list'));
  const paginEl = /** @type {HTMLElement} */ (container.querySelector('.js-pagination'));

  // Subscribe to download progress to update button states
  _unsubProgress = subscribe('chaptersProgress', _onProgressUpdate);

  await _fetch(_listEl, paginEl);

  _removePullToRefresh = addPullToRefresh(document.documentElement, () => _fetch(_listEl, paginEl));
}

// ── URL state ─────────────────────────────────────────────────────────────────

function _updateUrl() {
  urlReplaceState(_page > 1 ? { page: _page } : {});
}

// ── Fetch ─────────────────────────────────────────────────────────────────────

/** @param {HTMLElement} listEl @param {HTMLElement} paginEl */
async function _fetch(listEl, paginEl) {
  _abort?.abort();
  _abort = new AbortController();

  listEl.setAttribute('aria-busy', 'true');
  startLoading();
  const cancelSkeleton = deferredSkeleton(() => { listEl.innerHTML = skeletonUpdateList(4); });

  _destroyPagination?.();
  _destroyPagination = null;
  paginEl.innerHTML = '';

  let result;
  try {
    result = await api.getRecentUpdates(_page, _abort.signal);
  } catch (e) {
    cancelSkeleton();
    if (e?.name === 'AbortError') return;
    listEl.innerHTML = '';
    listEl.setAttribute('aria-busy', 'false');
    finishLoading();
    listEl.appendChild(createErrorState({
      message: 'Failed to load recent updates.',
      onRetry: () => _fetch(listEl, paginEl),
    }));
    return;
  }

  cancelSkeleton();
  finishLoading();
  listEl.innerHTML = '';
  listEl.setAttribute('aria-busy', 'false');

  const updates = Array.isArray(result?.recent_updates) ? result.recent_updates
    : Array.isArray(result?.updates)                    ? result.updates
    : Array.isArray(result)                             ? result
    : [];

  if (updates.length === 0) {
    listEl.appendChild(createEmptyState({
      icon: iconBookOpen,
      title: 'No recent updates',
      subtitle: 'Scan your library to find new chapters.',
    }));
    return;
  }

  // Group by date, then by manga
  /** @type {Map<string, Map<number, { manga_id: number, manga_title: string, chapters: any[] }>>} */
  const byDate = new Map();
  /** Tracks the newest timestamp seen for each dateKey so we can sort groups newest-first. */
  const rawDates = /** @type {Map<string, number>} */ (new Map());
  for (const item of updates) {
    const rawDate = item.discovered_at ?? item.date_uploaded ?? null;
    const dateKey = rawDate ? _relativeDate(rawDate) : 'Unknown date';
    const mid = item.manga_id ?? item.manga?.id;
    const title = item.manga_name ?? item.manga_title ?? item.manga?.title ?? '';

    if (!byDate.has(dateKey)) byDate.set(dateKey, new Map());
    const byManga = /** @type {Map<number, any>} */ (byDate.get(dateKey));
    if (!byManga.has(mid)) byManga.set(mid, { manga_id: mid, manga_title: title, chapters: [] });
    byManga.get(mid).chapters.push({
      id: item.chapter_id ?? item.id,
      title: formatChapterTitle(item),
      date_uploaded: rawDate,
      is_downloaded: item.is_downloaded ?? false,
    });
    // Track newest raw timestamp per date group for sort ordering.
    const ts = rawDate ? new Date(rawDate).getTime() : 0;
    if (!rawDates.has(dateKey) || ts > /** @type {number} */ (rawDates.get(dateKey))) {
      rawDates.set(dateKey, ts);
    }
  }

  const canDownload = hasPermission('chapter:download');

  // Sort date groups newest-first regardless of Map insertion order.
  const sortedGroups = [...byDate.entries()].sort(
    ([a], [b]) => (rawDates.get(b) ?? 0) - (rawDates.get(a) ?? 0),
  );

  for (const [dateLabel, mangaMap] of sortedGroups) {
    // Date group header
    const dateHeader = document.createElement('div');
    dateHeader.className = 'text-sm font-semibold uppercase tracking-wider text-text-muted mt-6 mb-2 pb-1 border-b border-border-subtle first:mt-2';
    dateHeader.textContent = dateLabel;
    listEl.appendChild(dateHeader);

    // Manga groups within date
    for (const group of mangaMap.values()) {
      const groupEl = document.createElement('div');
      groupEl.className = 'flex flex-col gap-2 bg-surface border border-border rounded-xl p-4 min-w-0';

      // Header: cover + title link
      const coverUrl = getMangaCoverUrl(group.manga_id, 'sm');
      const mangaHref = `/manga/${group.manga_id}`;

      groupEl.innerHTML = `
        <div class="flex items-center gap-3">
          <a href="${escapeHtml(mangaHref)}" class="w-16 h-24 rounded-md overflow-hidden shrink-0 bg-surface-2 block focus-visible:ring-2 focus-visible:ring-accent focus-visible:outline-none">
            <img src="${escapeHtml(coverUrl)}" alt="${escapeHtml(group.manga_title)}" class="w-full h-full object-cover" loading="lazy" />
          </a>
          <a href="${escapeHtml(mangaHref)}" class="text-base font-semibold text-text hover:text-accent transition-colors flex-1 truncate focus-visible:outline-none focus-visible:underline">
            ${escapeHtml(group.manga_title)}
          </a>
        </div>
      `;

      // Chapter list
      const chList = document.createElement('ul');
      chList.className = 'flex flex-col gap-0 mt-2';
      chList.setAttribute('role', 'list');

      for (const ch of group.chapters) {
        const li = document.createElement('li');
        li.className = 'flex items-center justify-between gap-2 py-2.5 border-b border-border-subtle last:border-b-0';
        li.setAttribute('role', 'listitem');
        li.dataset.chapterId = String(ch.id);
        li.innerHTML = `
          <span class="text-sm text-text flex-1 truncate min-w-0">${escapeHtml(ch.title)}</span>
          ${ch.date_uploaded ? `<span class="text-xs text-text-faint shrink-0">${escapeHtml(formatDate(ch.date_uploaded))}</span>` : ''}
          <span class="js-dl-ctrl shrink-0">${_renderDownloadControl(ch, canDownload)}</span>
        `;
        chList.appendChild(li);
      }

      groupEl.appendChild(chList);
      listEl.appendChild(groupEl);
    }
  }

  // Wire download buttons with in-flight guard
  if (canDownload) {
    for (const btn of /** @type {NodeListOf<HTMLButtonElement>} */ (listEl.querySelectorAll('button[data-chapter-id]'))) {
      const id = Number(btn.dataset.chapterId);
      btn.addEventListener('click', async () => {
        /** @type {Set<number>} */
        const inFlight = getState('inFlightChapters');
        if (inFlight.has(id)) return;
        updateState('inFlightChapters', (/** @type {Set<number>} */ s) => { const n = new Set(s); n.add(id); return n; });
        btn.disabled = true;
        btn.innerHTML = iconSpinner;
        try {
          await api.downloadChapter(id);
          // Progress subscription handles button state during and after download
        } catch {
          btn.innerHTML = iconDownload;
          btn.disabled = false;
        } finally {
          updateState('inFlightChapters', (/** @type {Set<number>} */ s) => { const n = new Set(s); n.delete(id); return n; });
        }
      });
    }
  }

  // Sync any already-in-progress downloads on render
  _onProgressUpdate(getState('chaptersProgress'));

  const hasNext = hasNextPage(result);
  if (_page > 1 || hasNext) {
    const { destroy } = renderPagination(paginEl, {
      page: _page,
      hasNext,
      total: result?.total_pages ?? undefined,
      onPageChange: (p) => { _page = p; _updateUrl(); _fetch(listEl, paginEl); window.scrollTo(0, 0); },
    });
    _destroyPagination = destroy;
  }
}

// ── Progress tracking ─────────────────────────────────────────────────────────

/**
 * Called whenever chaptersProgress state changes.
 * Updates download button appearance for any visible chapter rows.
 * @param {Map<number, any>} progress
 */
function _onProgressUpdate(progress) {
  if (!_listEl) return;
  for (const btn of /** @type {NodeListOf<HTMLButtonElement>} */ (_listEl.querySelectorAll('button[data-chapter-id]'))) {
    const id = Number(btn.dataset.chapterId);
    const p = progress?.get(id);
    if (!p) continue;
    const ctrl = /** @type {HTMLElement | null} */ (btn.closest('.js-dl-ctrl'));
    if (p.status === 'completed' || p.status === 'completed_hidden') {
      const title = btn.getAttribute('aria-label')?.replace('Download ', '') ?? '';
      const ch = { id, title, is_downloaded: true };
      if (ctrl) ctrl.innerHTML = _renderDownloadControl(ch, true);
    } else if (p.status === 'in_progress') {
      btn.disabled = true;
      btn.innerHTML = iconSpinner;
    } else if (p.status === 'failed' || p.status === 'cancelled') {
      btn.innerHTML = iconDownload;
      btn.disabled = false;
      updateState('inFlightChapters', (/** @type {Set<number>} */ s) => { const n = new Set(s); n.delete(id); return n; });
    }
  }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/** @param {string} dateStr */
function _relativeDate(dateStr) {
  try {
    const d = new Date(dateStr);
    const now = new Date();
    const diffDays = Math.floor((now.getTime() - d.getTime()) / 86_400_000);
    if (diffDays === 0) return 'Today';
    if (diffDays === 1) return 'Yesterday';
    return d.toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' });
  } catch {
    return formatDate(dateStr);
  }
}

// ── Destroy ───────────────────────────────────────────────────────────────────

/** @param {HTMLElement} container */
export function destroy(container) {
  clearPageHeader();
  _abort?.abort();
  _abort = null;
  _removePullToRefresh?.();
  _removePullToRefresh = null;
  _destroyPagination?.();
  _destroyPagination = null;
  _unsubProgress?.();
  _unsubProgress = null;
  _listEl = null;
  container.innerHTML = '';
}
