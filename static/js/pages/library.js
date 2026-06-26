// @ts-check
// Library page — main landing page with filters, category tabs, refresh.

import { h, render } from 'preact';
import htm from 'htm';
import * as api from '../api.js';
import { hasPermission, getState, setState, updateState, subscribe } from '../state.js';
import { navigate } from '../router.js';
import { debounce, getLocal, getLocalInt, setLocal, hasNextPage, confirmDialog, formatChapterTitle, deferredSkeleton } from '../utils.js';

/** @type {Record<string, number>} */
const STATUS_VALUES = { ongoing: 0, completed: 1, hiatus: 2, cancelled: 3, unknown: 4 };
import { Combobox } from '../components/combobox.js';
import { renderCategoryTabs } from '../components/tabs.js';
import { renderPagination } from '../components/pagination.js';
import { renderMangaGrid, createMangaCard, setMangaCardScanning, setMangaCardDownloadProgress, setNewChapterCount } from '../components/manga-card.js';
import { skeletonGrid } from '../components/skeletons.js';
import { startLoading, finishLoading } from '../components/page-loading-bar.js';
import { createErrorState } from '../components/error-state.js';
import { createEmptyState } from '../components/empty-state.js';
import { iconBookOpen, iconChevronDown, iconRefresh, iconSearch } from '../icons.js';
import { showToast, showApiError } from '../components/toast.js';
import { ContextMenu } from '../components/menu.js';
import { setPageHeader, clearPageHeader } from '../components/app-header.js';
const html = htm.bind(h);

// ── Module state ──────────────────────────────────────────────────────────────

let _search = '';
let _statusFilter = /** @type {string|null} */ (null);
let _tagFilter    = /** @type {number|null} */ (null);
let _authorFilter = /** @type {number|null} */ (null);
let _artistFilter = /** @type {number|null} */ (null);
let _catFilter    = /** @type {number|null} */ (null);
let _readingStatusFilter = /** @type {number|null} */ (null);
let _hideNoUnread = false;
let _hideCompletedStatus = false;
let _sortOrder = 'up';
let _page = 1;
let _pageSize = 0;

/** @type {AbortController|null} */ let _abort = null;
/** @type {(() => void)|null} */   let _unsubRefresh = null;
/** @type {(() => void)|null} */   let _unsubInvalidation = null;
/** @type {(() => void)|null} */   let _unsubScanning = null;
/** @type {(() => void)|null} */   let _unsubDownloads = null;
/** @type {(() => void)|null} */   let _unsubScanResult = null;
/** True while a scan triggered from this page is in flight. */
let _scanInProgress = false;
/** @type {(() => void)|null} */   let _destroyPagination = null;
/** @type {(() => void)|null} */   let _destroyTabs = null;
/** @type {IntersectionObserver|null} */ let _sentinelObserver = null;
/** @type {HTMLElement|null} */    let _authorContainer = null;
/** @type {HTMLElement|null} */    let _artistContainer = null;
/** @type {HTMLElement|null} */    let _tagsContainer = null;
/** @type {HTMLElement|null} */    let _gridEl = null;
/** @type {HTMLElement|null} */    let _paginEl = null;
/** @type {HTMLElement|null} */    let _container = null;
/** @type {(() => void)|null} */   let _mountComboboxesFn = null;
/** @type {(() => void)|null} */   let _updateFilterCountFn = null;
/** @type {(() => void)|null} */   let _cancelInitSkeleton = null;

// ── Select mode state ──
let _selectMode = false;
/** @type {Set<number>} */ let _selected = new Set();
/** @type {HTMLElement|null} */ let _bulkBarEl = null;
/** @type {HTMLElement|null} */ let _contextMenuEl = null;
// One-shot flag: absorbs the click that fires on pointer-up after a long-press
// entered select mode. Set by the long-press timer; consumed by the first onCardClick.
let _absorbNextCardClick = false;
// Timestamp of the last _enterSelectMode call — used by the contextmenu guard to absorb
// the OS long-press contextmenu that fires ~100-300ms after the app's 400ms timer.
let _selectModeEnteredAt = 0;
// Long-press timer for manga cards
/** @type {ReturnType<typeof setTimeout>|null} */ let _lpTimer = null;

// ── Init ──────────────────────────────────────────────────────────────────────

function _hasActiveFilters() {
  return !!(_search || _statusFilter || _readingStatusFilter != null ||
    _hideNoUnread || _hideCompletedStatus || _tagFilter != null ||
    _authorFilter != null || _artistFilter != null);
}

function _clearAllFilters() {
  _search = '';
  _statusFilter = null;
  _readingStatusFilter = null;
  _hideNoUnread = false;
  _hideCompletedStatus = false;
  _tagFilter = null;
  _authorFilter = null;
  _artistFilter = null;
  _page = 1;
  if (_container) {
    for (const el of _container.querySelectorAll('.js-search')) /** @type {HTMLInputElement} */ (el).value = '';
    const status = _container.querySelector('.js-status');
    if (status) /** @type {HTMLSelectElement} */ (status).value = '';
    const readingStatus = _container.querySelector('.js-reading-status');
    if (readingStatus) /** @type {HTMLSelectElement} */ (readingStatus).value = '';
    const hideNoUnread = _container.querySelector('.js-hide-no-unread');
    if (hideNoUnread) /** @type {HTMLInputElement} */ (hideNoUnread).checked = false;
    const hideCompleted = _container.querySelector('.js-hide-completed');
    if (hideCompleted) /** @type {HTMLInputElement} */ (hideCompleted).checked = false;
    // Re-mount comboboxes with cleared values (includes tags)
    _mountComboboxesFn?.();
  _updateFilterCountFn?.();
  }
  _updateUrl();
  _fetchLibrary();
}

/** @param {HTMLElement} container */
export async function init(container) {
  _container = container;
  document.title = 'Library - Kani';
  _pageSize = getLocalInt('kani_library_page_size', 24);

  // Restore filter state from URL params
  const urlParams = new URLSearchParams(location.search);
  _page       = parseInt(urlParams.get('page') ?? '1', 10) || 1;
  _search     = urlParams.get('search') ?? '';
  _statusFilter = urlParams.get('status') || null;
  _tagFilter  = urlParams.get('tag_id')    ? Number(urlParams.get('tag_id'))    : null;
  _authorFilter = urlParams.get('author_id') ? Number(urlParams.get('author_id')) : null;
  _artistFilter = urlParams.get('artist_id') ? Number(urlParams.get('artist_id')) : null;
  _catFilter  = urlParams.get('cat_id')    ? Number(urlParams.get('cat_id'))    : null;
  _readingStatusFilter = urlParams.get('reading_status') ? Number(urlParams.get('reading_status')) : null;
  _hideNoUnread = urlParams.get('hide_no_unread') === '1';
  _hideCompletedStatus = urlParams.get('hide_completed') === '1';
  _sortOrder  = urlParams.get('sort') ?? 'updated_desc';

  // Build refresh button for the global header
  let refreshBtn = /** @type {HTMLButtonElement|null} */ (null);

  if (hasPermission('library:refresh')) {
    refreshBtn = document.createElement('button');
    refreshBtn.type = 'button';
    refreshBtn.className = 'btn-primary btn-sm flex items-center gap-2';
    refreshBtn.innerHTML = `<span class="icon-sm shrink-0">${iconRefresh}</span><span>Refresh All</span>`;
  }

  let scanAllBtn = /** @type {HTMLButtonElement|null} */ (null);
  if (hasPermission('library:refresh')) {
    scanAllBtn = document.createElement('button');
    scanAllBtn.type = 'button';
    scanAllBtn.className = 'btn-ghost btn-sm';
    scanAllBtn.textContent = 'Scan all';
    scanAllBtn.addEventListener('click', async () => {
      if (!scanAllBtn || scanAllBtn.disabled) return;
      scanAllBtn.disabled = true;
      _scanInProgress = true;
      try {
        await api.scanMangaMultiple('all');
        // Card spinners and completion toast are driven by SSE events.
        // Fallback: re-enable button if SSE never delivers 'completed' within 2 minutes.
        setTimeout(() => {
          if (scanAllBtn && scanAllBtn.disabled) {
            scanAllBtn.disabled = false;
            scanAllBtn.textContent = 'Scan all';
            _scanInProgress = false;
            setState('scanningMangaIds', new Set());
          }
        }, 120_000);
      } catch (e) {
        showApiError(e);
        scanAllBtn.disabled = false;
        scanAllBtn.textContent = 'Scan all';
        _scanInProgress = false;
        setState('scanningMangaIds', new Set());
      }
    });
  }

  const _hdrActions = /** @type {HTMLElement[]} */ ([]);
  if (scanAllBtn) _hdrActions.push(scanAllBtn);
  if (refreshBtn) _hdrActions.push(refreshBtn);
  setPageHeader({ crumbs: [{ label: 'Library' }], actions: _hdrActions.length ? _hdrActions : null });

  container.innerHTML = `
    <div class="max-w-page mx-auto w-full px-3 sm:px-4 md:px-6 py-4 md:py-6 flex flex-col gap-4">

      <!-- Category tabs (horizontal scroll on mobile) -->
      <div class="js-tabs overflow-x-auto [scrollbar-width:none] [-webkit-overflow-scrolling:touch]"></div>

      <!-- Search bar (mobile only — on desktop it lives inside the filter bar) -->
      <div class="relative lg:hidden">
        <span class="absolute left-3 top-1/2 -translate-y-1/2 text-text-muted pointer-events-none icon-sm" aria-hidden="true">${iconSearch}</span>
        <input
          type="search"
          class="input js-search w-full pl-9"
          placeholder="Search library…"
          value="${_search.replace(/"/g, '&quot;').replace(/</g, '&lt;')}"
          aria-label="Search library"
        />
      </div>

      <!-- Filter toggle + page size (mobile only) -->
      <div class="flex items-center gap-2 lg:hidden">
        <button
          type="button"
          class="flex-1 flex items-center justify-between gap-3 px-4 py-3 rounded-xl border border-border bg-surface text-sm font-medium text-text hover:bg-surface-2 transition-colors js-filter-toggle"
          aria-expanded="false"
          aria-controls="library-filters"
        >
          <span class="flex items-center gap-2">
            Filters
            <span class="js-filter-count hidden items-center justify-center min-w-5 h-5 px-1.5 rounded-full bg-accent text-white text-xs font-medium"></span>
          </span>
          <span class="icon-sm transition-transform js-filter-chevron">${iconChevronDown}</span>
        </button>
        <select class="input js-page-size w-20 shrink-0" aria-label="Items per page">
          ${[12, 24, 48, 96].map(n => `<option value="${n}"${n === _pageSize ? ' selected' : ''}>${n}</option>`).join('')}
        </select>
      </div>

      <!-- Filter bar (hidden on mobile until toggled, always visible on lg+) -->
      <div id="library-filters" class="js-filters hidden flex-col gap-3 lg:flex lg:flex-col lg:gap-3">
        <!-- Row 1: Search + Sort + Page size (primary controls, desktop only) -->
        <div class="flex flex-col lg:flex-row lg:items-center gap-2">
          <div class="relative hidden lg:block lg:flex-1">
            <span class="absolute left-3 top-1/2 -translate-y-1/2 text-text-muted pointer-events-none icon-sm" aria-hidden="true">${iconSearch}</span>
            <input
              type="search"
              class="input js-search pl-9 w-full"
              placeholder="Search library…"
              value="${_search.replace(/"/g, '&quot;').replace(/</g, '&lt;')}"
              aria-label="Search library"
            />
          </div>
          <select class="input js-sort w-full lg:w-auto" aria-label="Sort order">
            ${[
              ['updated_desc','Updated ↓'],['updated_asc','Updated ↑'],
              ['name_asc','Title A–Z'],['name_desc','Title Z–A'],
              ['added_desc','Added ↓'],['added_asc','Added ↑'],
              ['score_desc','Score ↓'],['score_asc','Score ↑'],
              ['last_read_desc','Last Read'],
            ].map(([v,l]) =>
              `<option value="${v}"${v === _sortOrder ? ' selected' : ''}>${l}</option>`
            ).join('')}
          </select>
          <select class="input js-page-size w-full lg:w-20 hidden lg:block" aria-label="Items per page">
            ${[12, 24, 48, 96].map(n => `<option value="${n}"${n === _pageSize ? ' selected' : ''}>${n}</option>`).join('')}
          </select>
        </div>
        <!-- Row 2: Filter dropdowns + toggles -->
        <div class="flex flex-col lg:flex-row lg:flex-wrap lg:items-center gap-2">
          <select class="input js-status w-full lg:w-auto lg:min-w-36" aria-label="Status filter">
            ${['', 'ongoing', 'completed', 'hiatus', 'cancelled', 'unknown'].map(v =>
              `<option value="${v}"${v === (_statusFilter ?? '') ? ' selected' : ''}>${v || 'All statuses'}</option>`
            ).join('')}
          </select>
          <select class="input js-reading-status w-full lg:w-auto lg:min-w-40" aria-label="Reading status filter">
            <option value="">All reading states</option>
            ${[['0','Reading'],['1','On Hold'],['2','Dropped'],['3','Plan to Read'],['4','Completed'],['5','Rereading']].map(([v,l]) =>
              `<option value="${v}"${String(_readingStatusFilter) === v ? ' selected' : ''}>${l}</option>`
            ).join('')}
          </select>
          <div class="js-tags-combobox w-full lg:w-auto lg:min-w-36 lg:max-w-48"></div>
          <div class="js-author-combobox w-full lg:w-auto lg:max-w-44"></div>
          <div class="js-artist-combobox w-full lg:w-auto lg:max-w-44"></div>
          <label class="flex items-center gap-3 text-sm text-text cursor-pointer whitespace-nowrap select-none">
            <span>Hide read</span>
            <label class="kani-toggle">
              <input type="checkbox" class="kani-toggle__input js-hide-no-unread" ${_hideNoUnread ? 'checked' : ''} />
              <span class="kani-toggle__track"></span>
            </label>
          </label>
          <label class="flex items-center gap-3 text-sm text-text cursor-pointer whitespace-nowrap select-none">
            <span>Hide completed</span>
            <label class="kani-toggle">
              <input type="checkbox" class="kani-toggle__input js-hide-completed" ${_hideCompletedStatus ? 'checked' : ''} />
              <span class="kani-toggle__track"></span>
            </label>
          </label>
        </div>
      </div>

      <!-- Continue Reading shelf -->
      <div class="js-shelf hidden flex flex-col gap-2">
        <h2 class="text-sm font-semibold text-text-muted uppercase tracking-wide">Continue Reading</h2>
        <div class="js-shelf-row flex gap-3 overflow-x-auto [scrollbar-width:none] [-webkit-overflow-scrolling:touch] pb-1"></div>
      </div>

      <!-- Grid -->
      <div class="js-grid" aria-live="polite" aria-busy="false"></div>

      <!-- Pagination -->
      <div class="js-pagination"></div>
    </div>
  `;

  _gridEl          = /** @type {HTMLElement} */ (container.querySelector('.js-grid'));
  _paginEl         = /** @type {HTMLElement} */ (container.querySelector('.js-pagination'));
  _authorContainer = /** @type {HTMLElement} */ (container.querySelector('.js-author-combobox'));
  _artistContainer = /** @type {HTMLElement} */ (container.querySelector('.js-artist-combobox'));
  _tagsContainer   = /** @type {HTMLElement} */ (container.querySelector('.js-tags-combobox'));

  const tabsEl           = /** @type {HTMLElement} */ (container.querySelector('.js-tabs'));
  const searchEls        = /** @type {NodeListOf<HTMLInputElement>} */ (container.querySelectorAll('.js-search'));
  const statusEl         = /** @type {HTMLSelectElement} */ (container.querySelector('.js-status'));
  const readingStatusEl  = /** @type {HTMLSelectElement} */ (container.querySelector('.js-reading-status'));
  const hideNoUnreadEl   = /** @type {HTMLInputElement} */ (container.querySelector('.js-hide-no-unread'));
  const hideCompletedEl  = /** @type {HTMLInputElement} */ (container.querySelector('.js-hide-completed'));
  const sortEl           = /** @type {HTMLSelectElement} */ (container.querySelector('.js-sort'));
  const sizeEls          = /** @type {NodeListOf<HTMLSelectElement>} */ (container.querySelectorAll('.js-page-size'));
  const shelfEl          = /** @type {HTMLElement} */ (container.querySelector('.js-shelf'));
  const shelfRowEl       = /** @type {HTMLElement} */ (container.querySelector('.js-shelf-row'));
  const filterToggle    = /** @type {HTMLButtonElement} */ (container.querySelector('.js-filter-toggle'));
  const filtersEl       = /** @type {HTMLElement} */ (container.querySelector('.js-filters'));
  const filterCountEl   = /** @type {HTMLElement} */ (container.querySelector('.js-filter-count'));
  const filterChevronEl = /** @type {HTMLElement} */ (container.querySelector('.js-filter-chevron'));
  _gridEl.addEventListener('contextmenu', (e) => {
    const card = /** @type {HTMLElement|null} */ (/** @type {HTMLElement} */ (e.target)?.closest('[data-manga-id]'));
    if (!card) return;
    e.preventDefault();
    const id = parseInt(card.dataset.mangaId ?? '', 10);
    if (isNaN(id)) return;
    // In select mode, right-click toggles selection like a left-click.
    // Guard absorbs the OS contextmenu that fires ~100-300ms after a long-press that
    // already entered select mode (the timer fires at 400ms; OS fires at ~500-700ms).
    if (_selectMode) {
      if (Date.now() - _selectModeEnteredAt < 500) return;
      _toggleMangaSelected(id, card);
      return;
    }
    const titleEl = card.querySelector('.title span');
    const title = titleEl?.textContent ?? '';
    _showMangaContextMenu({ id, title }, e.clientX, e.clientY, card);
  });

  // Long-press to enter select mode (touch / stylus / long right-hold)
  _gridEl.addEventListener('pointerdown', (e) => {
    if (_selectMode) return;
    const card = /** @type {HTMLElement|null} */ (/** @type {HTMLElement} */ (e.target)?.closest('[data-manga-id]'));
    if (!card) return;
    if (e.pointerType === 'mouse' && e.button !== 0) return;
    _lpTimer = setTimeout(() => {
      _lpTimer = null;
      const id = parseInt(card.dataset.mangaId ?? '', 10);
      if (!isNaN(id)) {
        // Set flag before entering select mode so the click that fires on pointer-up
        // (immediately after this timer) is absorbed by onCardClick.
        _absorbNextCardClick = true;
        // Safety reset in case click never fires (e.g. touch + OS contextmenu cancels it)
        setTimeout(() => { _absorbNextCardClick = false; }, 300);
        _enterSelectMode();
        _toggleMangaSelected(id, card);
      }
    }, 400);
  });
  const _cancelLp = () => { if (_lpTimer != null) { clearTimeout(_lpTimer); _lpTimer = null; } };
  _gridEl.addEventListener('pointerup', _cancelLp);
  _gridEl.addEventListener('pointercancel', _cancelLp);
  _gridEl.addEventListener('pointermove', _cancelLp);

  function _updateFilterCount() {
    if (!filterCountEl) return;
    const count = [_statusFilter, _readingStatusFilter != null ? true : null, _hideNoUnread || null, _hideCompletedStatus || null, _tagFilter != null ? true : null, _authorFilter != null ? true : null, _artistFilter != null ? true : null].filter(Boolean).length;
    if (count > 0) {
      filterCountEl.textContent = String(count);
      filterCountEl.classList.remove('hidden');
      filterCountEl.classList.add('inline-flex');
    } else {
      filterCountEl.classList.add('hidden');
      filterCountEl.classList.remove('inline-flex');
    }
  }
  _updateFilterCountFn = _updateFilterCount;
  _updateFilterCount();

  // Mobile filter toggle
  filterToggle?.addEventListener('click', () => {
    const isExpanded = filterToggle.getAttribute('aria-expanded') === 'true';
    if (isExpanded) {
      filtersEl.classList.add('hidden');
      filtersEl.classList.remove('flex');
      filterChevronEl?.classList.remove('rotate-180');
    } else {
      filtersEl.classList.remove('hidden');
      filtersEl.classList.add('flex');
      filterChevronEl?.classList.add('rotate-180');
    }
    filterToggle.setAttribute('aria-expanded', String(!isExpanded));
  });

  // Show skeleton only if data takes > 150 ms
  _cancelInitSkeleton = deferredSkeleton(() => { if (_gridEl) _gridEl.innerHTML = skeletonGrid(_pageSize); });

  // ── Fetch filter options in parallel ──
  const [tags, authors, artists, categories] = await Promise.allSettled([
    api.getTags(), api.getAuthors(), api.getArtists(), api.getCategories(),
  ]).then(r => r.map(s => s.status === 'fulfilled' ? s.value : []));

  // Category tabs: "All" + one per category
  const catList = Array.isArray(categories) ? categories : [];
  const tabItems = [{ id: null, name: 'All' }, ...catList.map(c => ({ id: c.id, name: c.name }))];
  const { destroy: destroyTabs } = renderCategoryTabs(tabsEl, {
    tabs: tabItems,
    activeId: _catFilter,
    onSelect: (id) => { _catFilter = id; _page = 1; _updateUrl(); _fetchLibrary(); },
  });
  _destroyTabs = destroyTabs;

  // Mount Preact Combobox for tags/author/artist
  const tagOptions    = (Array.isArray(tags)    ? tags    : []).map(t => ({ id: t.id ?? t, name: t.name ?? t }));
  const authorOptions = (Array.isArray(authors) ? authors : []).map(a => ({ id: a.id, name: a.name }));
  const artistOptions = (Array.isArray(artists) ? artists : []).map(a => ({ id: a.id, name: a.name }));

  function _mountComboboxes() {
    if (_tagsContainer) {
      render(html`<${Combobox}
        options=${tagOptions}
        value=${_tagFilter}
        onChange=${(id) => { _tagFilter = id; _page = 1; _mountComboboxes(); _updateFilterCount(); _updateUrl(); _fetchLibrary(); }}
        placeholder="All tags"
      />`, _tagsContainer);
    }
    if (_authorContainer) {
      render(html`<${Combobox}
        options=${authorOptions}
        value=${_authorFilter}
        onChange=${(id) => { _authorFilter = id; _page = 1; _mountComboboxes(); _updateFilterCount(); _updateUrl(); _fetchLibrary(); }}
        placeholder="Author"
      />`, _authorContainer);
    }
    if (_artistContainer) {
      render(html`<${Combobox}
        options=${artistOptions}
        value=${_artistFilter}
        onChange=${(id) => { _artistFilter = id; _page = 1; _mountComboboxes(); _updateFilterCount(); _updateUrl(); _fetchLibrary(); }}
        placeholder="Artist"
      />`, _artistContainer);
    }
  }
  _mountComboboxesFn = _mountComboboxes;
  _mountComboboxes();

  // ── Wire events ──
  for (const searchEl of searchEls) {
    searchEl.addEventListener('input', debounce(() => {
      _search = searchEl.value.trim();
      // Keep the other input in sync
      for (const other of searchEls) { if (other !== searchEl) other.value = searchEl.value; }
      _page = 1;
      _updateUrl(true);
      _fetchLibrary();
    }, 300));
  }

  statusEl.addEventListener('change', () => {
    _statusFilter = statusEl.value || null;
    _page = 1;
    _updateFilterCount();
    _updateUrl();
    _fetchLibrary();
  });

  readingStatusEl?.addEventListener('change', () => {
    _readingStatusFilter = readingStatusEl.value ? Number(readingStatusEl.value) : null;
    _page = 1;
    _updateFilterCount();
    _updateUrl();
    _fetchLibrary();
  });

  hideNoUnreadEl?.addEventListener('change', () => {
    _hideNoUnread = hideNoUnreadEl.checked;
    _page = 1;
    _updateFilterCount();
    _updateUrl();
    _fetchLibrary();
  });

  hideCompletedEl?.addEventListener('change', () => {
    _hideCompletedStatus = hideCompletedEl.checked;
    _page = 1;
    _updateFilterCount();
    _updateUrl();
    _fetchLibrary();
  });

  sortEl.addEventListener('change', () => {
    _sortOrder = sortEl.value;
    _page = 1;
    _updateUrl();
    _fetchLibrary();
  });

  for (const sizeEl of sizeEls) {
    sizeEl.addEventListener('change', () => {
      _pageSize = Number(sizeEl.value);
      setLocal('kani_library_page_size', String(_pageSize));
      _page = 1;
      // Keep both selects in sync
      for (const el of sizeEls) el.value = sizeEl.value;
      _fetchLibrary();
    });
  }

  refreshBtn?.addEventListener('click', async () => {
    try {
      await api.startRefreshAll();
    } catch { /* ignore — SSE will update state */ }
  });

  // ── Refresh state subscription ──

  function _applyRefreshState(state) {
    const isRunning = state.type === 'running';
    const pct = isRunning && state.total > 0 ? Math.round((state.completed / state.total) * 100) : 0;

    if (_scanInProgress) {
      // A scan is running — progress indicator belongs in Scan All, not Refresh All.
      if (scanAllBtn) {
        if (isRunning) {
          const label = pct > 0 ? `${pct}%` : 'Scanning…';
          scanAllBtn.innerHTML = `<span class="icon-sm shrink-0 animate-spin">${iconRefresh}</span><span>${label}</span>`;
          // disabled was already set by the click handler; keep it set.
        }
        // Reset on done/idle is handled by _unsubScanResult; no action needed here.
      }
      // Keep Refresh All in its normal enabled state during a scan.
      if (refreshBtn) {
        refreshBtn.disabled = false;
        refreshBtn.innerHTML = `<span class="icon-sm shrink-0">${iconRefresh}</span><span>Refresh All</span>`;
      }
    } else {
      // A refresh is running (or idle) — progress indicator belongs in Refresh All.
      if (!refreshBtn) return;
      if (isRunning) {
        refreshBtn.disabled = true;
        const label = pct > 0 ? `${pct}%` : 'Refreshing…';
        refreshBtn.innerHTML = `<span class="icon-sm shrink-0 animate-spin">${iconRefresh}</span><span>${label}</span>`;
      } else {
        refreshBtn.disabled = false;
        refreshBtn.innerHTML = `<span class="icon-sm shrink-0">${iconRefresh}</span><span>Refresh All</span>`;
      }
    }
  }

  _unsubRefresh = subscribe('refreshState', _applyRefreshState);
  _applyRefreshState(getState('refreshState'));

  // ── Library invalidation subscription (SSE scan/refresh complete) ──
  let _lastInvalidation = getState('libraryInvalidation');
  _unsubInvalidation = subscribe('libraryInvalidation', (val) => {
    if (val !== _lastInvalidation) {
      _lastInvalidation = val;
      _fetchLibrary();
    }
  });

  // ── Scan spinner subscription ──
  let _prevScanningIds = /** @type {Set<number>} */ (new Set());
  _unsubScanning = subscribe('scanningMangaIds', (/** @type {Set<number>} */ ids) => {
    if (!_gridEl) return;
    for (const id of ids) {
      if (!_prevScanningIds.has(id)) setMangaCardScanning(id, true, _gridEl);
    }
    for (const id of _prevScanningIds) {
      if (!ids.has(id)) setMangaCardScanning(id, false, _gridEl);
    }
    _prevScanningIds = ids;
  });

  // ── Per-manga download progress subscription ──
  _unsubDownloads = subscribe('chaptersProgress', (/** @type {Map<number, import('../state.js').ChapterProgress>} */ map) => {
    if (!_gridEl) return;
    // Aggregate progress by manga: sum pages of in-progress chapters.
    /** @type {Map<number, { completed: number, total: number }>} */
    const byManga = new Map();
    for (const ch of map.values()) {
      if (ch.status !== 'in_progress') continue;
      const cur = byManga.get(ch.mangaId) ?? { completed: 0, total: 0 };
      byManga.set(ch.mangaId, {
        completed: cur.completed + ch.completedPages,
        total: cur.total + ch.totalPages,
      });
    }
    // Apply or remove progress bars on visible cards.
    for (const card of /** @type {NodeListOf<HTMLElement>} */ (_gridEl.querySelectorAll('[data-manga-id]'))) {
      const id = Number(card.dataset.mangaId);
      const prog = byManga.get(id);
      if (prog && prog.total > 0) {
        setMangaCardDownloadProgress(id, Math.round((prog.completed / prog.total) * 100), _gridEl);
      } else {
        setMangaCardDownloadProgress(id, null, _gridEl);
      }
    }
  });

  // ── Scan result subscription (shows toast + badges on SSE 'completed') ──
  _unsubScanResult = subscribe('scanResult', (/** @type {{ total: number, failed: number, newChapters: number, perManga: Map<number, number> } | null} */ result) => {
    if (!result || !_scanInProgress) return;
    _scanInProgress = false;
    if (scanAllBtn) {
      scanAllBtn.disabled = false;
      scanAllBtn.textContent = 'Scan all';
    }

    // Show completion toast
    const parts = [`Scanned ${result.total} manga`];
    if (result.newChapters > 0) parts.push(`${result.newChapters} new chapter${result.newChapters !== 1 ? 's' : ''} found`);
    else parts.push('no new chapters');
    if (result.failed > 0) parts.push(`${result.failed} failed`);
    showToast(parts.join(' — '), { type: result.failed > 0 ? 'warn' : 'success' });

    // Apply new-chapter badges to visible cards
    if (_gridEl && result.perManga.size > 0) {
      for (const [mangaId, count] of result.perManga) {
        setNewChapterCount(mangaId, count, _gridEl);
      }
    }
  });

  // ── Continue-reading shelf ──
  if (shelfEl && shelfRowEl) {
    api.getContinueReadingShelf(12).then(items => {
      if (!shelfRowEl || !Array.isArray(items) || items.length === 0) return;
      shelfEl.classList.remove('hidden');
      shelfEl.classList.add('flex');
      for (const item of items) {
        const card = document.createElement('a');
        card.className = 'flex flex-col gap-1 shrink-0 w-24 cursor-pointer';
        card.href = `/reader/${item.chapter_id}`;
        card.addEventListener('click', e => { e.preventDefault(); navigate(`/reader/${item.chapter_id}`); });

        const cover = document.createElement('div');
        cover.className = 'w-full aspect-[2/3] rounded bg-surface-2 overflow-hidden'; /* justified: manga cover ratio */
        const coverSrc = item.local_cover_path
          ? `/rest/manga/${item.manga_id}/cover?size=sm`
          : item.cover_url ?? null;
        if (coverSrc) {
          const img = document.createElement('img');
          img.src = coverSrc;
          img.alt = item.manga_name;
          img.className = 'w-full h-full object-cover';
          img.loading = 'lazy';
          cover.appendChild(img);
        }
        card.appendChild(cover);

        const title = document.createElement('p');
        title.className = 'text-xs text-text truncate';
        title.textContent = item.manga_name;
        card.appendChild(title);

        const ch = document.createElement('p');
        ch.className = 'text-xs text-text-muted';
        ch.textContent = formatChapterTitle({ chapter_number: item.chapter_number });
        card.appendChild(ch);

        shelfRowEl.appendChild(card);
      }
    }).catch(() => {});
  }

  // Initial fetch
  _fetchLibrary();
}

// ── URL sync ──────────────────────────────────────────────────────────────────

function _updateUrl(replace = false) {
  const params = new URLSearchParams();
  if (_page > 1)                params.set('page',            String(_page));
  if (_search)                  params.set('search',          _search);
  if (_statusFilter)            params.set('status',          _statusFilter);
  if (_tagFilter)               params.set('tag_id',          String(_tagFilter));
  if (_authorFilter)            params.set('author_id',       String(_authorFilter));
  if (_artistFilter)            params.set('artist_id',       String(_artistFilter));
  if (_catFilter)               params.set('cat_id',          String(_catFilter));
  if (_readingStatusFilter != null) params.set('reading_status', String(_readingStatusFilter));
  if (_hideNoUnread)            params.set('hide_no_unread',  '1');
  if (_hideCompletedStatus)     params.set('hide_completed',  '1');
  if (_sortOrder && _sortOrder !== 'updated_desc') params.set('sort', _sortOrder);
  const qs = params.toString();
  const url = qs ? '?' + qs : location.pathname;
  if (replace) history.replaceState(null, '', url);
  else history.pushState(null, '', url);
}

// ── Fetch ─────────────────────────────────────────────────────────────────────

function _fetchLibrary() {
  if (!_gridEl || !_paginEl) return;
  const infinite = getLocal('kani_library_pagination') === 'infinite';
  const isAppend = infinite && _page > 1;

  _abort?.abort();
  _abort = new AbortController();

  _sentinelObserver?.disconnect();
  _sentinelObserver = null;
  _destroyPagination?.();
  _destroyPagination = null;
  _paginEl.innerHTML = '';

  if (isAppend) {
    // Show a loading skeleton at the bottom during append
    _paginEl.innerHTML = '<div class="h-14 mx-3 my-2 skeleton rounded-lg"></div>';
  } else {
    _gridEl.classList.add('opacity-50', 'pointer-events-none');
  }
  startLoading();

  api.getLibrary({
    page: _page,
    page_size: _pageSize,
    search: _search || undefined,
    status_filter: _statusFilter ? STATUS_VALUES[_statusFilter] : undefined,
    reading_status_filter: _readingStatusFilter ?? undefined,
    hide_no_unread: _hideNoUnread || undefined,
    hide_completed_status: _hideCompletedStatus || undefined,
    tag_filter: _tagFilter ?? undefined,
    author_filter: _authorFilter ?? undefined,
    artist_filter: _artistFilter ?? undefined,
    category_filter: _catFilter ?? undefined,
    sort_by: _sortOrder,
  }, _abort.signal).then(result => {
    if (!_gridEl || !_paginEl) return;
    finishLoading();
    _paginEl.innerHTML = '';

    const items = Array.isArray(result?.items) ? result.items
      : Array.isArray(result?.manga)            ? result.manga
      : Array.isArray(result)                   ? result
      : [];

    // Clear old content only once new data is ready — prevents blank-flash flicker
    _cancelInitSkeleton?.();
    _cancelInitSkeleton = null;
    if (!isAppend) _gridEl.innerHTML = '';

    if (items.length === 0 && !isAppend) {
      const hasFilters = _hasActiveFilters();
      const hasCategoryOnly = !hasFilters && _catFilter != null;
      let emptyOpts;
      if (hasCategoryOnly) {
        emptyOpts = { icon: iconBookOpen, title: 'This category is empty.' };
      } else if (hasFilters || _catFilter != null) {
        emptyOpts = { icon: iconBookOpen, title: 'No results found.', subtitle: 'Try adjusting your filters.', action: { label: 'Clear Filters', onClick: _clearAllFilters } };
      } else {
        emptyOpts = { icon: iconBookOpen, title: 'Your library is empty.', subtitle: 'Add manga from Sources to get started.', action: { label: 'Browse Sources', href: '/sources' } };
      }
      _gridEl.appendChild(createEmptyState(emptyOpts));
    } else if (items.length > 0) {
      if (infinite) {
        _appendMangaCards(_gridEl, items);
      } else {
        renderMangaGrid(_gridEl, {
          items: items.map(m => ({ id: m.id, title: m.title, cover_image_url: m.cover_url ?? null, new_chapter_count: m.new_chapter_count ?? 0 })),
          getHref: (m) => `/manga/${m.id}`,
          large: true,
          onCardClick: (m) => {
            const cardEl = /** @type {HTMLElement} */ (_gridEl.querySelector(`[data-manga-id="${m.id}"]`));
            if (cardEl) _onCardClick(m, cardEl);
          },
          onMenuClick: (m, btn) => {
            const cardEl = /** @type {HTMLElement} */ (btn.closest('[data-manga-id]'));
            if (_selectMode) { _toggleMangaSelected(m.id, cardEl); return; }
            const rect = btn.getBoundingClientRect();
            _showMangaContextMenu({ id: m.id, title: m.title }, rect.left, rect.bottom + 4, cardEl);
          },
        });
      }
    }

    // Remove dim only after new content is in place
    _gridEl.classList.remove('opacity-50', 'pointer-events-none');

    const hasNext = hasNextPage(result, items.length, _pageSize);
    if (infinite) {
      _setupSentinel(hasNext);
    } else if (_page > 1 || hasNext) {
      const { destroy } = renderPagination(_paginEl, {
        page: _page,
        hasNext,
        total: result?.total_pages ?? undefined,
        onPageChange: (p) => { _page = p; _updateUrl(); _fetchLibrary(); window.scrollTo(0, 0); },
      });
      _destroyPagination = destroy;
    }
  }).catch(e => {
    _cancelInitSkeleton?.();
    _cancelInitSkeleton = null;
    if (e?.name === 'AbortError') return;
    if (!_gridEl) return;
    finishLoading();
    _paginEl.innerHTML = '';
    if (!isAppend) {
      _gridEl.classList.remove('opacity-50', 'pointer-events-none');
      _gridEl.innerHTML = '';
      _gridEl.appendChild(createErrorState({
        message: 'Failed to load library.',
        onRetry: () => _fetchLibrary(),
      }));
    }
  });
}

/** Appends manga cards to the persistent grid inside `_gridEl`. */
function _appendMangaCards(gridEl, items) {
  let grid = /** @type {HTMLElement|null} */ (gridEl.querySelector('.manga-grid--large'));
  if (!grid) {
    grid = document.createElement('div');
    grid.className = 'manga-grid--large';
    gridEl.appendChild(grid);
  }
  for (const m of items) {
    grid.appendChild(createMangaCard({
      manga: { id: m.id, title: m.title, cover_image_url: m.cover_url ?? null, new_chapter_count: m.new_chapter_count ?? 0 },
      href: `/manga/${m.id}`,
      onCardClick: (manga) => {
        const cardEl = /** @type {HTMLElement} */ (gridEl.querySelector(`[data-manga-id="${manga.id}"]`));
        if (cardEl) _onCardClick(manga, cardEl);
      },
      onMenuClick: (manga, btn) => {
        const cardEl = /** @type {HTMLElement} */ (btn.closest('[data-manga-id]'));
        if (_selectMode) { _toggleMangaSelected(manga.id, cardEl); return; }
        const rect = btn.getBoundingClientRect();
        _showMangaContextMenu({ id: manga.id, title: manga.title }, rect.left, rect.bottom + 4, cardEl);
      },
    }));
  }
}

/** Sets up (or clears) the IntersectionObserver sentinel for infinite scroll. */
function _setupSentinel(hasNext) {
  _sentinelObserver?.disconnect();
  _sentinelObserver = null;
  if (!_paginEl || !hasNext) return;

  const sentinel = document.createElement('div');
  sentinel.className = 'js-sentinel h-px';
  _paginEl.appendChild(sentinel);

  _sentinelObserver = new IntersectionObserver((entries) => {
    if (entries[0].isIntersecting) {
      _sentinelObserver?.disconnect();
      _sentinelObserver = null;
      _page++;
      _fetchLibrary();
    }
  }, { rootMargin: '200px' });
  _sentinelObserver.observe(sentinel);
}

// ── Select mode ───────────────────────────────────────────────────────────────

/**
 * Per-card click handler — handles both navigation (normal mode) and selection toggling
 * (select mode). Using per-card callbacks means the click is processed directly on the
 * <a> element, which is completely isolated from the context-menu button's event path,
 * eliminating any ghost-click interaction.
 * @param {{ id: number, title: string }} manga
 * @param {HTMLElement} cardEl
 */
function _onCardClick(manga, cardEl) {
  // Absorb the click that fires on pointer-up after a long-press that entered select mode
  if (_absorbNextCardClick) { _absorbNextCardClick = false; return; }
  if (_selectMode) {
    _toggleMangaSelected(manga.id, cardEl);
  } else {
    navigate('/manga/' + manga.id);
  }
}

function _enterSelectMode() {
  _selectModeEnteredAt = Date.now();
  _selectMode = true;
  _selected.clear();
  _gridEl?.classList.add('cursor-pointer', 'select-none', 'is-select-mode');
  _renderBulkBar();
}

function _exitSelectMode() {
  _selectMode = false;
  _selected.clear();
  // Remove selection overlays from all cards
  _gridEl?.querySelectorAll('[data-manga-id]').forEach(card => {
    card.classList.remove('ring-2', 'ring-accent');
    card.querySelector('.js-select-overlay')?.remove();
  });
  _gridEl?.classList.remove('cursor-pointer', 'select-none', 'is-select-mode');
  _bulkBarEl?.remove();
  _bulkBarEl = null;
}

/**
 * @param {number} id
 * @param {HTMLElement} cardEl
 */
function _toggleMangaSelected(id, cardEl) {
  const isSelected = _selected.has(id);
  if (isSelected) {
    _selected.delete(id);
    cardEl.classList.remove('ring-2', 'ring-accent');
    cardEl.querySelector('.js-select-overlay')?.remove();
  } else {
    _selected.add(id);
    cardEl.classList.add('ring-2', 'ring-accent', 'rounded-sm');
    if (!cardEl.querySelector('.js-select-overlay')) {
      const overlay = document.createElement('div');
      overlay.className = 'js-select-overlay absolute top-1 right-1 w-5 h-5 bg-accent rounded-full flex items-center justify-center text-white text-xs font-bold pointer-events-none z-10';
      overlay.textContent = '✓';
      overlay.style.fontSize = '10px';
      const coverWrap = cardEl.querySelector('.relative');
      if (coverWrap) /** @type {HTMLElement} */ (coverWrap).appendChild(overlay);
    }
  }
  _updateBulkBar();
}

function _updateBulkBar() {
  if (!_bulkBarEl) return;
  const countEl = _bulkBarEl.querySelector('.js-select-count');
  if (countEl) countEl.textContent = `${_selected.size} selected`;

  const hasSelection = _selected.size > 0;
  for (const btn of /** @type {NodeListOf<HTMLButtonElement>} */ (_bulkBarEl.querySelectorAll('.js-bulk-action'))) {
    btn.disabled = !hasSelection;
  }
}

function _renderBulkBar() {
  _bulkBarEl?.remove();

  const bar = document.createElement('div');
  bar.className = [
    'fixed bottom-0 md:bottom-6 inset-x-0 md:inset-x-auto md:left-1/2 md:-translate-x-1/2',
    'z-40 md:w-auto md:min-w-96',
    'bg-surface border border-border-subtle rounded-none md:rounded-2xl shadow-xl',
    'flex items-center gap-2 px-4 py-3 flex-wrap',
  ].join(' ');

  bar.innerHTML = `
    <span class="text-sm font-medium text-text-muted js-select-count flex-1">0 selected</span>
    <button type="button" class="btn-icon js-select-all" title="Select all visible">All</button>
    <button type="button" class="btn-secondary btn-sm js-bulk-action js-bulk-download" disabled title="Download all selected">Download</button>
    <button type="button" class="btn-secondary btn-sm js-bulk-action js-bulk-scan" disabled title="Scan for new chapters">Scan</button>
    <button type="button" class="btn-secondary btn-sm js-bulk-action js-bulk-mark-read" disabled>Mark read</button>
    <button type="button" class="btn-secondary btn-sm js-bulk-action js-bulk-mark-unread" disabled>Mark unread</button>
    <button type="button" class="btn-secondary btn-sm js-bulk-action js-bulk-categories" disabled>Categories</button>
    <button type="button" class="btn-danger btn-sm js-bulk-action js-bulk-delete" disabled>Delete</button>
    <button type="button" class="btn-ghost btn-sm js-bulk-cancel">Cancel</button>
  `;
  _bulkBarEl = bar;
  document.body.appendChild(bar);

  bar.querySelector('.js-bulk-cancel')?.addEventListener('click', () => _exitSelectMode());

  // Select all visible
  bar.querySelector('.js-select-all')?.addEventListener('click', () => {
    const cards = /** @type {NodeListOf<HTMLElement>} */ (_gridEl?.querySelectorAll('[data-manga-id]') ?? []);
    for (const card of cards) {
      const id = parseInt(card.dataset.mangaId ?? '', 10);
      if (!isNaN(id) && !_selected.has(id)) _toggleMangaSelected(id, card);
    }
  });

  // Download all selected
  bar.querySelector('.js-bulk-download')?.addEventListener('click', async () => {
    const ids = [..._selected];
    let done = 0;
    for (const id of ids) {
      try { await api.downloadAll(id); } catch { /* ignore */ }
      done++;
    }
    showToast(`Download queued for ${done} manga.`);
    _exitSelectMode();
  });

  // Scan selected for new chapters
  bar.querySelector('.js-bulk-scan')?.addEventListener('click', async () => {
    const ids = [..._selected].map(Number);
    // Disable all actions during scan
    for (const btn of /** @type {NodeListOf<HTMLButtonElement>} */ (bar.querySelectorAll('.js-bulk-action'))) btn.disabled = true;
    _scanInProgress = true;
    try {
      await api.scanMangaMultiple(ids);
      // Card spinners and completion toast driven by SSE events.
      // Exit select mode so the user can see scan progress on cards.
      _exitSelectMode();
    } catch (e) {
      showApiError(e);
      _scanInProgress = false;
      setState('scanningMangaIds', new Set());
      for (const btn of /** @type {NodeListOf<HTMLButtonElement>} */ (bar.querySelectorAll('.js-bulk-action'))) btn.disabled = false;
    }
  });

  // Mark as read
  bar.querySelector('.js-bulk-mark-read')?.addEventListener('click', async () => {
    const ids = [..._selected];
    let done = 0;
    for (const id of ids) {
      try { await api.markChaptersUpTo(id, 99999, true); done++; } catch { /* ignore */ }
    }
    showToast(`Marked ${done} manga as read.`);
    _exitSelectMode();
  });

  // Mark as unread
  bar.querySelector('.js-bulk-mark-unread')?.addEventListener('click', async () => {
    const ids = [..._selected];
    let done = 0;
    for (const id of ids) {
      try { await api.markChaptersUpTo(id, 99999, false); done++; } catch { /* ignore */ }
    }
    showToast(`Marked ${done} manga as unread.`);
    _exitSelectMode();
  });

  // Add to categories
  bar.querySelector('.js-bulk-categories')?.addEventListener('click', () => {
    _showBulkCategoryModal([..._selected]);
  });

  // Delete selected
  bar.querySelector('.js-bulk-delete')?.addEventListener('click', async () => {
    const count = _selected.size;
    const ok = await confirmDialog({ title: 'Remove from Library?', message: `Remove ${count} manga from your library? Downloaded chapters will be deleted.`, confirmLabel: 'Remove', danger: true });
    if (!ok) return;
    const ids = [..._selected];
    _exitSelectMode();
    let done = 0;
    for (const id of ids) {
      try { await api.deleteManga(id); done++; } catch { /* ignore */ }
    }
    showToast(`Deleted ${done} manga.`);
    _page = 1;
    _fetchLibrary();
  });
}

/**
 * @param {{ id: number, title: string }} manga
 * @param {number} x
 * @param {number} y
 * @param {HTMLElement} cardEl
 */
function _showMangaContextMenu(manga, x, y, cardEl) {
  _closeContextMenu();

  /** @type {import('../components/menu.js').MenuItem[]} */
  const items = [
    { label: 'Select', action: () => {
      if (!_selectMode) _enterSelectMode();
      if (!_selected.has(manga.id)) _toggleMangaSelected(manga.id, cardEl);
    }},
    { divider: true },
    ...(hasPermission('chapter:download') ? [{ label: 'Download All', action: async () => {
      try { await api.downloadAll(manga.id); showToast('Download queued.'); }
      catch { showToast('Failed to queue download.'); }
    }}] : []),
    { label: 'Mark All Read', action: async () => {
      try { await api.markChaptersUpTo(manga.id, 99999, true); showToast('Marked as read.'); }
      catch { showToast('Failed.'); }
    }},
    { label: 'Mark All Unread', action: async () => {
      try { await api.markChaptersUpTo(manga.id, 99999, false); showToast('Marked as unread.'); }
      catch { showToast('Failed.'); }
    }},
    { label: 'Set Categories', action: () => _showBulkCategoryModal([manga.id]) },
    { divider: true },
    { label: 'Remove from Library', danger: true, action: async () => {
      const ok = await confirmDialog({ title: 'Remove from Library?', message: `Remove "${manga.title}" from your library? Downloaded chapters will be deleted.`, confirmLabel: 'Remove', danger: true });
      if (!ok) return;
      try {
        await api.deleteManga(manga.id);
        showToast('Removed from library.');
        _page = 1;
        _fetchLibrary();
      } catch { showToast('Failed to remove manga.'); }
    }},
  ];

  const container = document.createElement('div');
  document.body.appendChild(container);
  _contextMenuEl = container;

  render(html`<${ContextMenu} items=${items} trigger=${{ x, y }} onClose=${_closeContextMenu} />`, container);
}

function _closeContextMenu() {
  if (!_contextMenuEl) return;
  render(null, _contextMenuEl);
  _contextMenuEl.remove();
  _contextMenuEl = null;
}

/** @param {number[]} mangaIds */
async function _showBulkCategoryModal(mangaIds) {
  let allCats = [];
  try { allCats = await api.getCategories(); } catch { return; }

  const overlay = document.createElement('div');
  overlay.className = 'fixed inset-0 z-50 flex items-center justify-center bg-bg/70 backdrop-blur-sm p-4';

  const dialog = document.createElement('div');
  dialog.className = 'bg-surface rounded-2xl shadow-xl w-full max-w-sm flex flex-col overflow-hidden';
  dialog.setAttribute('role', 'dialog');
  dialog.setAttribute('aria-label', 'Set categories');

  if (allCats.length === 0) {
    dialog.innerHTML = `
      <div class="px-6 py-4 border-b border-border-subtle flex items-center justify-between gap-4">
        <h2 class="text-base font-semibold text-text">Set categories</h2>
        <button type="button" class="btn-icon js-close" aria-label="Close">✕</button>
      </div>
      <div class="px-6 py-5">
        <p class="text-sm text-text-muted">No categories yet. Create some in Settings → Library.</p>
      </div>
    `;
  } else {
    const checkboxes = allCats.map(c =>
      `<label class="flex items-center gap-3 text-sm text-text cursor-pointer py-1">
        <input type="checkbox" value="${c.id}" class="js-cat-check">
        ${c.name}
      </label>`
    ).join('');

    dialog.innerHTML = `
      <div class="px-6 py-4 border-b border-border-subtle flex items-center justify-between gap-4">
        <h2 class="text-base font-semibold text-text">Set categories</h2>
        <button type="button" class="btn-icon js-close" aria-label="Close">✕</button>
      </div>
      <div class="px-6 py-5 flex flex-col gap-2">
        <p class="text-xs text-text-muted mb-2">Selected categories will be applied to all ${mangaIds.length} manga.</p>
        ${checkboxes}
      </div>
      <div class="px-6 py-4 border-t border-border-subtle flex items-center justify-end gap-3">
        <button type="button" class="btn-secondary btn-sm js-cancel">Cancel</button>
        <button type="button" class="btn-primary btn-sm js-apply">Apply</button>
      </div>
    `;
  }

  overlay.appendChild(dialog);
  document.body.appendChild(overlay);

  const closeModal = () => { if (overlay.parentNode) document.body.removeChild(overlay); };
  dialog.querySelector('.js-close')?.addEventListener('click', closeModal);
  dialog.querySelector('.js-cancel')?.addEventListener('click', closeModal);
  overlay.addEventListener('click', e => { if (e.target === overlay) closeModal(); });

  dialog.querySelector('.js-apply')?.addEventListener('click', async () => {
    const selectedCatIds = [...dialog.querySelectorAll('.js-cat-check:checked')]
      .map(el => parseInt(/** @type {HTMLInputElement} */ (el).value, 10));
    closeModal();
    let done = 0;
    for (const id of mangaIds) {
      try { await api.setMangaCategories(id, selectedCatIds); done++; } catch { /* ignore */ }
    }
    showToast(`Categories updated for ${done} manga.`);
    _exitSelectMode();
    _page = 1;
    _fetchLibrary();
  });
}

// ── Destroy ───────────────────────────────────────────────────────────────────

/** @param {HTMLElement} container */
export function destroy(container) {
  clearPageHeader();
  _abort?.abort();
  _abort = null;
  _unsubRefresh?.();
  _unsubRefresh = null;
  _unsubInvalidation?.();
  _unsubInvalidation = null;
  _unsubScanning?.();
  _unsubScanning = null;
  _unsubDownloads?.();
  _unsubDownloads = null;
  _unsubScanResult?.();
  _unsubScanResult = null;
  _destroyPagination?.();
  _destroyPagination = null;
  _sentinelObserver?.disconnect();
  _sentinelObserver = null;
  _destroyTabs?.();
  _destroyTabs = null;
  if (_tagsContainer)   render(null, _tagsContainer);
  if (_authorContainer) render(null, _authorContainer);
  if (_artistContainer) render(null, _artistContainer);
  _tagsContainer   = null;
  _authorContainer = null;
  _artistContainer = null;
  _gridEl = null;
  _paginEl = null;
  _container = null;
  _mountComboboxesFn = null;
  _updateFilterCountFn = null;
  _cancelInitSkeleton?.();
  _cancelInitSkeleton = null;
  _bulkBarEl?.remove();
  _bulkBarEl = null;
  _closeContextMenu();
  _selectMode = false;
  _selected.clear();
  container.innerHTML = '';
}
