// @ts-check
// Global search page — search manga across multiple sources with scope filtering.

import * as api from '../api.js';
import { hasPermission } from '../state.js';
import { navigate } from '../router.js';
import { getParam, replaceState as urlReplaceState } from '../url-params.js';
import { debounce, escapeHtml } from '../utils.js';
import { skeletonGrid } from '../components/skeletons.js';
import { startLoading, finishLoading } from '../components/page-loading-bar.js';
import { createErrorState } from '../components/error-state.js';
import { createEmptyState } from '../components/empty-state.js';
import { createMangaCard } from '../components/manga-card.js';
import { iconSearch, iconChevronLeft, iconChevronRight } from '../icons.js';
import { setPageHeader, clearPageHeader } from '../components/app-header.js';

// ── Module state ──────────────────────────────────────────────────────────────

let _query = '';
/** @type {'FavouritedOnly'|'AllEnabled'|{Sources: number[]}} */
let _scope = 'FavouritedOnly';
/** @type {AbortController|null} */ let _abort = null;
/** @type {any[]} */               let _sources = [];
/** Per-source pagination state: sourceId → { page, hasNext, loading } */
/** @type {Map<number, { page: number, hasNext: boolean, loading: boolean }>} */
let _sourcePages = new Map();
/** @type {Map<number, IntersectionObserver>} */
let _sourceObservers = new Map();

// ── Init ──────────────────────────────────────────────────────────────────────

/** @param {HTMLElement} container */
export async function init(container) {
  document.title = 'Search - Kani';
  _query = getParam('q') ?? '';
  _scope = 'FavouritedOnly';
  const scopeParam = getParam('scope');
  if (scopeParam === 'AllEnabled') {
    _scope = 'AllEnabled';
  } else if (scopeParam?.startsWith('sources:')) {
    const ids = scopeParam.slice(8).split(',').map(Number).filter(Boolean);
    if (ids.length) _scope = { Sources: ids };
  }
  setPageHeader({ crumbs: [{ label: 'Search' }] });

  if (!hasPermission('source:browse')) {
    container.innerHTML = '';
    container.appendChild(createErrorState({ message: 'You do not have permission to search sources.' }));
    return;
  }

  container.innerHTML = `
    <div class="max-w-page mx-auto w-full overflow-x-hidden px-4 md:px-6 py-4 md:py-6 flex flex-col gap-4">
      <!-- Large centered search bar -->
      <div class="flex flex-col items-center gap-4 py-4 md:py-8">
        <div class="relative w-full max-w-2xl">
          <span class="absolute left-4 top-1/2 -translate-y-1/2 text-text-muted pointer-events-none icon-md" aria-hidden="true">${iconSearch}</span>
          <input
            type="search"
            class="input w-full pl-11 h-12 text-base"
            id="search-input"
            placeholder="Search manga across sources…"
            aria-label="Search manga"
            autofocus
          />
        </div>
        <!-- Scope chips -->
        <div class="flex flex-wrap justify-center gap-2" id="scope-chips" role="group" aria-label="Search scope"></div>
      </div>

      <!-- Results -->
      <div id="search-results" aria-live="polite" aria-busy="false"></div>
    </div>
  `;

  const searchInput = /** @type {HTMLInputElement} */ (container.querySelector('#search-input'));
  const chipsEl     = /** @type {HTMLElement} */ (container.querySelector('#scope-chips'));
  const resultsEl   = /** @type {HTMLElement} */ (container.querySelector('#search-results'));

  searchInput.value = _query;

  // Load sources for scope chips
  try {
    const all = await api.getSources();
    _sources = (Array.isArray(all) ? all : []).filter(s => s.enabled);
  } catch {
    _sources = [];
  }

  function _updateUrl() {
    /** @type {Record<string, string>} */
    const params = {};
    if (_query) params.q = _query;
    if (_scope === 'AllEnabled') params.scope = 'AllEnabled';
    else if (typeof _scope === 'object' && 'Sources' in _scope)
      params.scope = `sources:${_scope.Sources.join(',')}`;
    urlReplaceState(params);
  }

  _renderChips(chipsEl, resultsEl);

  if (_query) _fetchSearch(resultsEl);

  const _debouncedSearch = debounce(() => {
    _query = searchInput.value.trim();
    _updateUrl();
    if (_query) _fetchSearch(resultsEl);
    else { resultsEl.innerHTML = ''; resultsEl.setAttribute('aria-busy', 'false'); }
  }, 500);

  searchInput.addEventListener('input', _debouncedSearch);

  function _renderChips(el, results) {
    el.innerHTML = '';

    const isFav = _scope === 'FavouritedOnly';
    const isAll = _scope === 'AllEnabled';
    const sourcesScope = typeof _scope === 'object' && 'Sources' in _scope ? _scope.Sources : null;

    const mkChip = (label, active, onClick) => {
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.className = active ? 'chip chip-active' : 'chip';
      btn.textContent = label;
      btn.setAttribute('aria-pressed', String(active));
      btn.addEventListener('click', onClick);
      el.appendChild(btn);
    };

    mkChip('Favourites', isFav, () => {
      _scope = 'FavouritedOnly';
      _updateUrl();
      _renderChips(el, results);
      if (_query) _fetchSearch(results);
    });

    mkChip('All enabled', isAll, () => {
      _scope = 'AllEnabled';
      _updateUrl();
      _renderChips(el, results);
      if (_query) _fetchSearch(results);
    });

    for (const src of _sources) {
      const isActive = sourcesScope?.includes(src.id) ?? false;
      mkChip(src.name, isActive, () => {
        if (sourcesScope) {
          const next = isActive
            ? sourcesScope.filter(id => id !== src.id)
            : [...sourcesScope, src.id];
          _scope = next.length === 0 ? 'AllEnabled' : { Sources: next };
        } else {
          _scope = { Sources: [src.id] };
        }
        _updateUrl();
        _renderChips(el, results);
        if (_query) _fetchSearch(results);
      });
    }
  }
}

// ── Fetch ─────────────────────────────────────────────────────────────────────

/** @param {HTMLElement} resultsEl */
async function _fetchSearch(resultsEl) {
  _abort?.abort();
  _abort = new AbortController();

  resultsEl.innerHTML = `<div class="flex flex-col gap-6">${[1,2,3].map(() => `
    <div class="flex flex-col gap-3">
      <div class="skeleton h-5 w-32 rounded"></div>
      <div class="flex gap-3 overflow-hidden">${[1,2,3,4].map(() => `<div class="skeleton rounded-lg shrink-0 w-32 h-48"></div>`).join('')}</div>
    </div>
  `).join('')}</div>`;
  resultsEl.setAttribute('aria-busy', 'true');
  startLoading();

  let result;
  try {
    result = await api.globalSearch(_query, _scope, 1, 24, _abort.signal);
  } catch (e) {
    if (e?.name === 'AbortError') return;
    resultsEl.innerHTML = '';
    resultsEl.setAttribute('aria-busy', 'false');
    finishLoading();
    resultsEl.appendChild(createErrorState({ message: 'Search failed. Try again.' }));
    return;
  }

  finishLoading();
  resultsEl.innerHTML = '';
  resultsEl.setAttribute('aria-busy', 'false');

  const sourceResults = Array.isArray(result?.results) ? result.results
    : Array.isArray(result)                             ? result
    : [];

  if (sourceResults.length === 0) {
    resultsEl.appendChild(createEmptyState({
      icon: iconSearch,
      title: 'No results found.',
      subtitle: 'Try a different search term or scope.',
    }));
    return;
  }

  // Initialise per-source page tracking from the fresh global search (page 1)
  _sourcePages = new Map();
  for (const sr of sourceResults) {
    _sourcePages.set(sr.source_id, { page: 1, hasNext: sr.has_next_page ?? false, loading: false });
  }

  const wrap = document.createElement('div');
  wrap.className = 'flex flex-col gap-8 min-w-0';
  wrap.setAttribute('role', 'list');

  for (const sourceResult of sourceResults) {
    const sid = sourceResult.source_id;
    const section = document.createElement('div');
    section.className = 'flex flex-col gap-3';
    section.setAttribute('role', 'listitem');

    // Source header + see-all link
    const header = document.createElement('div');
    header.className = 'flex items-center justify-between gap-3';
    header.innerHTML = `
      <h2 class="text-sm font-semibold text-text">${escapeHtml(sourceResult.source_name ?? String(sid))}</h2>
      <a href="/source/${encodeURIComponent(sid)}?q=${encodeURIComponent(_query)}" class="text-xs text-accent hover:underline focus-visible:outline-none focus-visible:underline">See all →</a>
    `;
    section.appendChild(header);

    if (!sourceResult.manga?.length) {
      const empty = document.createElement('p');
      empty.className = 'text-sm text-text-muted';
      empty.textContent = 'No results from this source.';
      section.appendChild(empty);
    } else {
      const wrapper = document.createElement('div');
      wrapper.className = 'manga-row-wrapper';

      const navLeft = document.createElement('button');
      navLeft.type = 'button';
      navLeft.className = 'manga-row-nav';
      navLeft.setAttribute('data-dir', 'left');
      navLeft.setAttribute('aria-label', 'Previous page');
      navLeft.style.display = 'none';
      navLeft.innerHTML = iconChevronLeft;

      const navRight = document.createElement('button');
      navRight.type = 'button';
      navRight.className = 'manga-row-nav';
      navRight.setAttribute('data-dir', 'right');
      navRight.setAttribute('aria-label', 'Next page');
      navRight.style.display = 'none';
      navRight.innerHTML = iconChevronRight;

      const row = document.createElement('div');
      row.className = 'manga-row';
      row.setAttribute('role', 'list');

      /** Append manga cards to the row (before the sentinel if present) */
      function _appendCards(mangaList) {
        for (const manga of mangaList) {
          const navId  = manga.source_manga_id ?? manga.id;
          const cardId = manga.db_id ?? manga.id;
          const card = createMangaCard({
            manga: { id: cardId, title: manga.title, source_id: sid, cover_image_url: manga.cover_url ?? null },
            href: `/source/${sid}/manga/${encodeURIComponent(navId)}`,
            extraClass: 'manga-row__item',
          });
          card.setAttribute('role', 'listitem');
          row.appendChild(card);
        }
      }

      /** Sync nav button visibility to current scroll position */
      function _updateNav() {
        const atStart = row.scrollLeft <= 2;
        const atEnd   = row.scrollLeft + row.clientWidth >= row.scrollWidth - 2;
        navLeft.style.display  = atStart ? 'none' : '';
        navRight.style.display = atEnd   ? 'none' : '';
      }

      _appendCards(sourceResult.manga);

      // Sentinel triggers append-on-scroll for this row
      const sentinel = document.createElement('div');
      sentinel.className = 'js-sentinel w-px shrink-0 self-stretch';
      row.appendChild(sentinel);

      const { hasNext: initialHasNext } = _sourcePages.get(sid) ?? { hasNext: false };
      if (initialHasNext) _observeRow(row, sentinel, sid, _updateNav);

      row.addEventListener('scroll', _updateNav, { passive: true });
      // Re-check after images load (layout may shift)
      requestAnimationFrame(_updateNav);

      navLeft.addEventListener('click', () => {
        row.scrollBy({ left: -(row.clientWidth * 0.8), behavior: 'smooth' });
      });

      navRight.addEventListener('click', () => {
        row.scrollBy({ left: row.clientWidth * 0.8, behavior: 'smooth' });
      });

      wrapper.appendChild(navLeft);
      wrapper.appendChild(row);
      wrapper.appendChild(navRight);
      section.appendChild(wrapper);
    }

    wrap.appendChild(section);
  }

  resultsEl.appendChild(wrap);
}

// ── Per-row infinite scroll ────────────────────────────────────────────────────

/**
 * @param {HTMLElement} row
 * @param {HTMLElement} sentinel
 * @param {number} sid
 */
function _observeRow(row, sentinel, sid, onUpdate) {
  _sourceObservers.get(sid)?.disconnect();

  const observer = new IntersectionObserver(async ([entry]) => {
    if (!entry.isIntersecting) return;
    const state = _sourcePages.get(sid);
    if (!state?.hasNext || state?.loading) return;

    _sourcePages.set(sid, { ...state, loading: true });

    const skels = [];
    for (let i = 0; i < 4; i++) {
      const s = document.createElement('div');
      s.className = 'manga-row__item';
      const sInner = document.createElement('div');
      sInner.className = 'skeleton rounded-sm w-full aspect-[2/3]';
      s.appendChild(sInner);
      row.insertBefore(s, sentinel);
      skels.push(s);
    }

    try {
      const res = await api.searchManga(sid, _query, state.page + 1, 24, undefined, _abort?.signal);
      const manga = Array.isArray(res?.manga) ? res.manga : Array.isArray(res) ? res : [];
      const nextHasNext = res?.has_next_page ?? false;
      _sourcePages.set(sid, { page: state.page + 1, hasNext: nextHasNext, loading: false });
      skels.forEach(s => s.remove());
      for (const m of manga) {
        const navId  = m.source_manga_id ?? m.id;
        const cardId = m.db_id ?? m.id;
        const card = createMangaCard({
          manga: { id: cardId, title: m.title, source_id: sid, cover_image_url: m.cover_url ?? null },
          href: `/source/${sid}/manga/${encodeURIComponent(navId)}`,
          extraClass: 'manga-row__item',
        });
        card.setAttribute('role', 'listitem');
        row.insertBefore(card, sentinel);
      }
      if (!nextHasNext) {
        observer.disconnect();
        _sourceObservers.delete(sid);
        sentinel.remove();
      }
      requestAnimationFrame(onUpdate);
    } catch (e) {
      if (e?.name !== 'AbortError') {
        _sourcePages.set(sid, { ...state, loading: false });
      }
      skels.forEach(s => s.remove());
    }
  }, { root: row, rootMargin: '0px 200px 0px 0px' });

  observer.observe(sentinel);
  _sourceObservers.set(sid, observer);
}

// ── Destroy ───────────────────────────────────────────────────────────────────

/** @param {HTMLElement} container */
export function destroy(container) {
  clearPageHeader();
  _abort?.abort();
  _abort = null;
  _sourcePages = new Map();
  for (const obs of _sourceObservers.values()) obs.disconnect();
  _sourceObservers = new Map();
  container.innerHTML = '';
}
