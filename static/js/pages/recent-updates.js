// @ts-check
// Recent updates page — paginated list of newly added chapters grouped by date then manga.

import * as api from '../api.js';
import { hasPermission } from '../state.js';
import { renderPagination } from '../components/pagination.js';
import { getMangaCoverUrl } from '../api.js';
import { formatChapterTitle, hasNextPage, formatDate, escapeHtml } from '../utils.js';
import { skeletonUpdateList } from '../components/skeletons.js';
import { startLoading, finishLoading } from '../components/page-loading-bar.js';
import { createErrorState } from '../components/error-state.js';
import { createEmptyState } from '../components/empty-state.js';
import { iconBookOpen, iconDownload } from '../icons.js';

// ── Module state ──────────────────────────────────────────────────────────────

let _page = 1;
/** @type {AbortController | null} */
let _abort = null;
/** @type {(() => void) | null} */
let _destroyPagination = null;

// ── Init ──────────────────────────────────────────────────────────────────────

/** @param {HTMLElement} container */
export async function init(container) {
  document.title = 'Recent Updates - Kani';
  _page = 1;

  container.innerHTML = `
    <div class="max-w-[1400px] mx-auto px-4 md:px-6 py-4 md:py-6 flex flex-col gap-6">
      <h1 class="text-2xl font-bold text-text">Recent Updates</h1>
      <div class="js-list flex flex-col gap-6" aria-live="polite" aria-busy="true"></div>
      <div class="js-pagination"></div>
    </div>
  `;

  const listEl  = /** @type {HTMLElement} */ (container.querySelector('.js-list'));
  const paginEl = /** @type {HTMLElement} */ (container.querySelector('.js-pagination'));

  await _fetch(listEl, paginEl);
}

// ── Fetch ─────────────────────────────────────────────────────────────────────

/** @param {HTMLElement} listEl @param {HTMLElement} paginEl */
async function _fetch(listEl, paginEl) {
  _abort?.abort();
  _abort = new AbortController();

  listEl.innerHTML = skeletonUpdateList(4);
  listEl.setAttribute('aria-busy', 'true');
  startLoading();

  _destroyPagination?.();
  _destroyPagination = null;
  paginEl.innerHTML = '';

  let result;
  try {
    result = await api.getRecentUpdates(_page, _abort.signal);
  } catch (e) {
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
    });
  }

  const canDownload = hasPermission('chapter:download');

  for (const [dateLabel, mangaMap] of byDate) {
    // Date group header
    const dateHeader = document.createElement('div');
    dateHeader.className = 'text-sm font-semibold uppercase tracking-wider text-text-muted mt-6 mb-2 pb-1 border-b border-border-subtle first:mt-2';
    dateHeader.textContent = dateLabel;
    listEl.appendChild(dateHeader);

    // Manga groups within date
    for (const group of mangaMap.values()) {
      const groupEl = document.createElement('div');
      groupEl.className = 'flex flex-col gap-2 bg-surface border border-border rounded-xl p-4';

      // Header: cover + title link
      const coverUrl = getMangaCoverUrl(group.manga_id);
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
          <span class="text-base text-text-muted flex-1 truncate">${escapeHtml(ch.title)}</span>
          ${ch.date_uploaded ? `<span class="text-sm text-text-faint shrink-0">${escapeHtml(formatDate(ch.date_uploaded))}</span>` : ''}
          ${canDownload ? `
            <button
              class="dl-btn shrink-0"
              aria-label="Download ${escapeHtml(ch.title)}"
              data-chapter-id="${ch.id}"
              title="Download"
            >${iconDownload}</button>
          ` : ''}
        `;
        chList.appendChild(li);
      }

      groupEl.appendChild(chList);
      listEl.appendChild(groupEl);
    }
  }

  // Wire download buttons
  if (canDownload) {
    for (const btn of /** @type {NodeListOf<HTMLButtonElement>} */ (listEl.querySelectorAll('[data-chapter-id]'))) {
      const id = Number(btn.dataset.chapterId);
      btn.addEventListener('click', async () => {
        btn.disabled = true;
        try { await api.downloadChapter(id); } catch { btn.disabled = false; }
      });
    }
  }

  const hasNext = hasNextPage(result);
  if (_page > 1 || hasNext) {
    const { destroy } = renderPagination(paginEl, {
      page: _page,
      hasNext,
      onPageChange: (p) => { _page = p; _fetch(listEl, paginEl); window.scrollTo(0, 0); },
    });
    _destroyPagination = destroy;
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
  _abort?.abort();
  _abort = null;
  _destroyPagination?.();
  _destroyPagination = null;
  container.innerHTML = '';
}
