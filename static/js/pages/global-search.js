// @ts-check
// Global search page — search manga across multiple sources with scope filtering.

import * as api from '../api.js';
import { hasPermission } from '../state.js';
import { navigate } from '../router.js';
import { debounce, escapeHtml } from '../utils.js';
import { skeletonGrid } from '../components/skeletons.js';
import { startLoading, finishLoading } from '../components/page-loading-bar.js';
import { createErrorState } from '../components/error-state.js';
import { createEmptyState } from '../components/empty-state.js';
import { createMangaCard } from '../components/manga-card.js';
import { iconSearch } from '../icons.js';

// ── Module state ──────────────────────────────────────────────────────────────

let _query = '';
/** @type {'FavouritedOnly'|'AllEnabled'|{Sources: number[]}} */
let _scope = 'FavouritedOnly';
/** @type {AbortController|null} */ let _abort = null;
/** @type {any[]} */               let _sources = [];

// ── Init ──────────────────────────────────────────────────────────────────────

/** @param {HTMLElement} container */
export async function init(container) {
  document.title = 'Search - Kani';
  _query = '';
  _scope = 'FavouritedOnly';

  if (!hasPermission('source:browse')) {
    container.innerHTML = '';
    container.appendChild(createErrorState({ message: 'You do not have permission to search sources.' }));
    return;
  }

  container.innerHTML = `
    <div class="max-w-[1400px] mx-auto px-4 md:px-6 py-4 md:py-6 flex flex-col gap-4">
      <!-- Large centered search bar -->
      <div class="flex flex-col items-center gap-4 py-4 md:py-8">
        <div class="relative w-full max-w-2xl">
          <span class="absolute left-4 top-1/2 -translate-y-1/2 text-text-muted pointer-events-none [&_svg]:w-5 [&_svg]:h-5" aria-hidden="true">${iconSearch}</span>
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

  // Load sources for scope chips
  try {
    const all = await api.getSources();
    _sources = (Array.isArray(all) ? all : []).filter(s => s.enabled);
  } catch {
    _sources = [];
  }

  _renderChips(chipsEl, resultsEl);

  const _debouncedSearch = debounce(() => {
    _query = searchInput.value.trim();
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
      _renderChips(el, results);
      if (_query) _fetchSearch(results);
    });

    mkChip('All enabled', isAll, () => {
      _scope = 'AllEnabled';
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

  const wrap = document.createElement('div');
  wrap.className = 'flex flex-col gap-8';
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
      const row = document.createElement('div');
      row.className = 'manga-row';
      row.setAttribute('role', 'list');
      for (const manga of sourceResult.manga) {
        const mangaId = encodeURIComponent(manga.id);
        const card = createMangaCard({
          manga: { id: manga.id, title: manga.title, source_id: sid, cover_image_url: manga.cover_url ?? null },
          href: `/source/${sid}/manga/${mangaId}`,
          extraClass: 'manga-row__item',
        });
        card.setAttribute('role', 'listitem');
        row.appendChild(card);
      }
      section.appendChild(row);
    }

    wrap.appendChild(section);
  }

  resultsEl.appendChild(wrap);
}

// ── Destroy ───────────────────────────────────────────────────────────────────

/** @param {HTMLElement} container */
export function destroy(container) {
  _abort?.abort();
  _abort = null;
  container.innerHTML = '';
}
