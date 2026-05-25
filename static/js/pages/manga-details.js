// @ts-check
// Manga details page — breadcrumb, hero, chapter list, tabbed manage panel.

import { h, render } from 'preact';
import htm from 'htm';
import * as api from '../api.js';
import { hasPermission, getState, subscribe } from '../state.js';
import { navigate } from '../router.js';
import { getLocal, getLocalInt, setLocal, formatDate, escapeHtml, formatChapterTitle, hasNextPage, isChapterDownloaded, confirmDialog } from '../utils.js';
import { createCoverImage } from '../components/cover-image.js';
import { VirtualChapterList } from '../components/virtual-chapter-list.js';
import { renderPagination } from '../components/pagination.js';
import { skeletonMangaHero } from '../components/skeletons.js';
import { startLoading, finishLoading } from '../components/page-loading-bar.js';
import { createErrorState } from '../components/error-state.js';
import { createEmptyState } from '../components/empty-state.js';
import { mountMigrationDialogue } from '../components/migration-dialogue.js';
import { createBreadcrumb } from '../components/breadcrumb.js';
import { mountIntoModalRoot, Modal } from '../components/modal.js';
import { CategorySelector } from '../components/category-selector.js';
import { Combobox } from '../components/combobox.js';
import { showToast } from '../components/toast.js';
import { renderTabs } from '../components/tabs.js';
import { iconDocument, iconX, iconSpinner } from '../icons.js';
const html = htm.bind(h);

// ── Source-filter semantic navigation ─────────────────────────────────────────

/**
 * Cache of filter defs per source ID.
 * @type {Map<string|number, Promise<any[]>>}
 */
const _sourceFilterDefsCache = new Map();

/**
 * Build a URL for navigating from manga-details to a source-side filter search.
 * If the source exposes a filter with the given semantic, routes via filter;
 * otherwise falls back to plain text search (`?q=name`).
 *
 * @param {string|number} sid - source ID
 * @param {string} name       - the search value (author/artist/tag name)
 * @param {'Author'|'Artist'|'Tag'} semantic
 * @returns {Promise<string>} the URL to navigate to
 */
async function _buildSourceMetaUrl(sid, name, semantic) {
  if (!_sourceFilterDefsCache.has(sid)) {
    _sourceFilterDefsCache.set(sid, api.getSourceFilters(sid)
      .then(fl => Array.isArray(fl?.filters) ? fl.filters : [])
      .catch(() => []));
  }
  const defs = await _sourceFilterDefsCache.get(sid);
  const match = defs.find(f => f.semantic === semantic);
  if (match) {
    return `/source/${sid}?filter_name=${encodeURIComponent(match.name)}&filter_value=${encodeURIComponent(name)}`;
  }
  return `/source/${sid}?q=${encodeURIComponent(name)}`;
}

// ── External link warning ─────────────────────────────────────────────────────

/**
 * Shows a confirmation dialog before opening an external link.
 * Includes a "Don't ask again" checkbox that writes to localStorage.
 * @param {string} url
 */
function _showExternalLinkDialog(url) {
  const overlay = document.createElement('div');
  overlay.className = 'fixed inset-0 bg-black/50 z-[9000] flex items-center justify-center p-4';

  const dialog = document.createElement('div');
  dialog.className = 'bg-surface rounded-xl p-6 max-w-sm w-full shadow-xl flex flex-col gap-4';
  dialog.innerHTML = `
    <div class="flex flex-col gap-1">
      <h3 class="text-base font-semibold text-text">External Link</h3>
      <p class="text-sm text-text-muted">This link will open outside the app:</p>
      <p class="text-sm text-accent break-all">${escapeHtml(url)}</p>
    </div>
    <label class="flex items-center gap-2 text-sm text-text-muted cursor-pointer select-none">
      <input type="checkbox" class="js-dont-ask accent-accent" />
      Don't ask again
    </label>
    <div class="flex gap-2 justify-end">
      <button type="button" class="btn-ghost btn-sm js-cancel">Cancel</button>
      <button type="button" class="btn-primary btn-sm js-continue">Open link</button>
    </div>
  `;
  overlay.appendChild(dialog);

  const close = () => overlay.remove();
  overlay.addEventListener('click', (e) => { if (e.target === overlay) close(); });
  dialog.querySelector('.js-cancel').addEventListener('click', close);
  dialog.querySelector('.js-continue').addEventListener('click', () => {
    if (dialog.querySelector('.js-dont-ask').checked) {
      setLocal('kani_skip_external_warning', 'true');
    }
    window.open(url, '_blank', 'noopener,noreferrer');
    close();
  });

  document.body.appendChild(overlay);
  dialog.querySelector('.js-continue').focus();
}

// ── URL state ─────────────────────────────────────────────────────────────────

function _updateUrl() {
  const params = new URLSearchParams(location.search);
  if (_page > 1) { params.set('page', String(_page)); } else { params.delete('page'); }
  if (_sortOrder && _sortOrder !== 'chapter_desc') { params.set('sort', _sortOrder); } else { params.delete('sort'); }
  if (!_isLocal && _remoteSort) { params.set('rsort', _remoteSort); } else { params.delete('rsort'); }
  if (_filterDownloaded) { params.set('dl', '1'); } else { params.delete('dl'); }
  if (_filterUnread) { params.set('unread', '1'); } else { params.delete('unread'); }
  if (_filterScanlator) { params.set('scanlator', _filterScanlator); } else { params.delete('scanlator'); }
  const qs = params.toString();
  history.replaceState(null, '', location.pathname + (qs ? '?' + qs : ''));
}

// ── Module state ──────────────────────────────────────────────────────────────

let _isLocal = false;
let _dbId = 0;
let _sid = 0;
let _mangaId = '';
let _page = 1;
let _chapterPageSize = 0;
let _sortOrder = 'chapter_desc';
let _addedDbId = /** @type {number|null} */ (null);
let _existingDbId = /** @type {number|null} */ (null);
let _autoScan = false;
let _mangaData = /** @type {any} */ (null);
let _scanlatorMode = 'priority';
let _downloadAllPreferredOnly = true;
let _filterDownloaded = false;
let _filterUnread = false;
let _filterScanlator = /** @type {string|null} */ (null);
/** @type {string[]} */ let _availableScanlators = [];
let _allSelected = false;

// Remote source chapter sort options declared by the extension.
// null = not yet fetched; [] = extension has no server-side sort.
/** @type {Array<{id:string,name:string}>|null} */ let _remoteChapterSorts = null;
/** @type {string|null} */ let _remoteSort = null;

let _activeTab = 'chapters';
let _manageMounted = false;
/** @type {HTMLElement|null} */ let _contentSection = null;
/** @type {HTMLElement|null} */ let _btnGroupEl = null;

/** @type {AbortController|null} */ let _abort = null;
/** @type {((e: Event) => void)|null} */ let _sseListener = null;
/** @type {(() => void)|null} */   let _destroyPagination = null;
/** @type {(() => void)|null} */   let _unmountMigration = null;
/** @type {HTMLElement|null} */ let _listContainerEl = null;
/** @type {HTMLElement|null} */ let _paginEl = null;
/** @type {HTMLElement|null} */ let _coverEl = null;

// Infinite-scroll chapter state
/** @type {any[]} */              let _chapters = [];
let _chaptersHasMore = false;
let _chaptersLoading = false;
// Client-side chapter cache: used when a remote source returns all chapters at once
/** @type {any[] | null} */       let _allRemoteChapters = null;
let _selectMode = false;
/** @type {Set<number>} */        let _selected = new Set();
/** @type {(() => void)|null} */  let _chapterResizeListener = null;
/** @type {(() => void)|null} */  let _manageResizeListener = null;
/** @type {(() => void)|null} */  let _heroResizeListener = null;

// Scanlator preferences stored at module level so Read button + Download All can use them
// without requiring the Manage tab to have been visited first.
/** @type {Array<{id:number,manga_id:number,scanlator:string,priority:number,blocked:boolean}>} */
let _scanlatorPrefs = [];


// ── Init ──────────────────────────────────────────────────────────────────────

/**
 * @param {HTMLElement} container
 * @param {{ id?: string, manga_id?: string, db_id?: string }} params
 */
export async function init(container, params) {
  _page = 1;
  _chapterPageSize = getLocalInt('kani_chapter_page_size', 50);
  _sortOrder = getLocal('kani_chapter_sort_order') || 'chapter_desc';
  _addedDbId = null;
  _existingDbId = null;
  _listContainerEl = null;
  _paginEl = null;
  _coverEl = null;
  _heroResizeListener = null;
  _chapters = [];
  _chaptersHasMore = false;
  _chaptersLoading = false;
  _allRemoteChapters = null;
  _selectMode = false;
  _selected = new Set();
  _activeTab = 'chapters';
  _manageMounted = false;
  _contentSection = null;
  _btnGroupEl = null;
  _scanlatorPrefs = [];
  _isLocal = !!params.db_id;
  _dbId = params.db_id ? Number(params.db_id) : 0;
  _sid = params.id ? Number(params.id) : 0;
  _mangaId = params.manga_id ?? '';
  _scanlatorMode = 'priority';
  _downloadAllPreferredOnly = true;
  _filterDownloaded = _dbId ? getLocal(`kani_filter_downloaded_${_dbId}`) === 'true' : false;
  _filterUnread = false;
  _filterScanlator = null;
  _remoteChapterSorts = null;
  _remoteSort = null;
  _availableScanlators = [];
  _allSelected = false;

  // Restore filter/sort/page state from URL (browser back/forward)
  {
    const _urlParams = new URLSearchParams(location.search);
    const _pageParam = _urlParams.get('page');
    if (_pageParam) _page = Math.max(1, parseInt(_pageParam, 10) || 1);
    const _sortParam = _urlParams.get('sort');
    if (_sortParam) { _sortOrder = _sortParam; setLocal('kani_chapter_sort_order', _sortOrder); }
    if (_urlParams.get('dl') === '1') _filterDownloaded = true;
    if (_urlParams.get('unread') === '1') _filterUnread = true;
    const _scanlatorParam = _urlParams.get('scanlator');
    if (_scanlatorParam) _filterScanlator = _scanlatorParam;
  }

  _abort = new AbortController();
  container.innerHTML = skeletonMangaHero();

  // ── Fetch manga data ──
  let info, source, autoDownload;
  try {
    if (_isLocal) {
      const res = await api.getMangaDetails(_dbId, _abort.signal);
      info = res.info ?? res;
      source = res.source ?? null;
      autoDownload = res.auto_download ?? false;
      _autoScan = res.auto_scan ?? false;
      _scanlatorMode = res.scanlator_mode ?? 'priority';
      _downloadAllPreferredOnly = res.download_all_preferred_only ?? true;
      _sid = source?.id ?? 0;
      _mangaId = info?.source_manga_id ?? '';
      // Load scanlator prefs + available scanlators before rendering so the
      // Read button, Download All, and chapter filter controls have the data.
      const [prefs, scanlators] = await Promise.all([
        api.getScanlatorPrefs(_dbId).catch(() => []),
        api.getChapterScanlators(_dbId).catch(() => []),
      ]);
      _scanlatorPrefs = Array.isArray(prefs) ? prefs : [];
      _availableScanlators = Array.isArray(scanlators) ? scanlators : [];
    } else {
      const [details, src, inLib] = await Promise.all([
        api.getRemoteMangaDetails(_sid, _mangaId, _abort.signal),
        api.getSource(_sid).catch(() => null),
        api.checkInLibrary(_sid, _mangaId).catch(() => ({ db_id: null })),
      ]);
      info = details;
      source = src;
      autoDownload = false;
      _existingDbId = inLib?.db_id ?? null;
    }
  } catch (e) {
    container.innerHTML = '';
    container.appendChild(createErrorState({ message: 'Failed to load manga details.' }));
    return;
  }

  _mangaData = info;
  document.title = (info?.title ?? 'Manga') + ' - Kani';

  // ── Render ──
  container.innerHTML = '';
  const wrap = document.createElement('div');
  wrap.className = 'max-w-[1400px] w-full mx-auto px-4 md:px-6 py-4 md:py-6 flex flex-col gap-6 md:gap-8';
  container.appendChild(wrap);

  // Breadcrumb (full width, above two-column layout)
  const _fromSourceId = new URLSearchParams(location.search).get('from_source');
  const crumbs = _isLocal
    ? (_fromSourceId && source
        ? [
            { label: 'Sources', href: '/sources' },
            { label: source.name, href: `/source/${source.id}` },
            { label: 'Library', href: `/source/${source.id}` },
            { label: info?.title ?? 'Manga' },
          ]
        : [{ label: 'Library', href: '/' }, { label: info?.title ?? 'Manga' }])
    : [
      { label: 'Sources', href: '/sources' },
      { label: source?.name ?? 'Source', href: `/source/${_sid}` },
      { label: info?.title ?? 'Manga' },
    ];
  wrap.appendChild(createBreadcrumb(crumbs, { truncateLast: false }));

  // Two-column layout: left = hero/meta/CTA, right = tabs + content
  const layout = document.createElement('div');
  layout.className = 'flex flex-col md:flex-row gap-6 md:gap-8 md:items-start';
  wrap.appendChild(layout);

  const leftCol = document.createElement('div');
  leftCol.className = 'w-full flex flex-col md:w-1/4 md:shrink-0';
  layout.appendChild(leftCol);

  const rightCol = document.createElement('div');
  rightCol.className = 'w-full min-w-0 flex flex-col gap-4 md:flex-1';
  layout.appendChild(rightCol);

  _renderHero(leftCol, info, source, autoDownload);


  // ── Tags (top of right column) ──
  if (info?.tags?.length) {
    const tags = document.createElement('div');
    tags.className = 'flex flex-wrap gap-2';
    for (const tag of info.tags) {
      const a = document.createElement('a');
      if (_isLocal) {
        a.href = `/?tag_id=${tag.id}`;
        a.addEventListener('click', e => { e.preventDefault(); navigate(`/?tag_id=${tag.id}`); });
      } else {
        a.href = `/source/${_sid}?q=${encodeURIComponent(tag.name)}`;
        a.addEventListener('click', e => {
          e.preventDefault();
          _buildSourceMetaUrl(_sid, tag.name, 'Tag').then(url => navigate(url));
        });
      }
      a.className = 'chip text-xs';
      a.textContent = tag.name;
      tags.appendChild(a);
    }
    rightCol.appendChild(tags);
  }

  // For remote sources: fetch extension-declared chapter sort options before first render
  // so the sort dropdown can show the right options immediately.
  if (!_isLocal && _sid) {
    const sorts = await api.getRemoteChapterSorts(_sid, _mangaId).catch(() => []);
    _remoteChapterSorts = Array.isArray(sorts) ? sorts : [];
    const _rsortParam = new URLSearchParams(location.search).get('rsort');
    if (_remoteChapterSorts.length > 0) {
      if (_rsortParam && _remoteChapterSorts.some(s => s.id === _rsortParam)) {
        _remoteSort = _rsortParam;
      } else {
        _remoteSort = _remoteChapterSorts[0].id;
      }
    }
  }

  if (_isLocal) {
    _renderTabs(rightCol);
    await _fetchChapters(/** @type {HTMLElement} */(_contentSection));
  } else {
    const chapterSection = document.createElement('div');
    rightCol.appendChild(chapterSection);
    _contentSection = chapterSection;
    await _fetchChapters(chapterSection);
  }

  // SSE listener
  _sseListener = (e) => {
    const data = /** @type {CustomEvent} */ (e).detail;
    if (!data) return;
    if (
      (data.type === 'manga_refreshed' || data.type === 'scan_complete') &&
      (data.manga_id === _dbId || data.db_id === _dbId)
    ) {
      if (_activeTab === 'chapters' && _contentSection) {
        _fetchChapters(_contentSection);
      }
    }
    // When a chapter download completes, update its downloaded flag in the cached
    // chapter list so the UI reflects the new state without a full re-fetch.
    if (data.type === 'chapter_completed' && _isLocal) {
      const chId = Number(data.chapter_id);
      let updated = false;
      _chapters = _chapters.map(ch => {
        if (ch.id === chId && !ch.downloaded) { updated = true; return { ...ch, downloaded: true }; }
        return ch;
      });
      if (updated) _renderChapterList();
    }
  };
  window.addEventListener('kani:sse', _sseListener);
}

// ── Hero ──────────────────────────────────────────────────────────────────────

function _renderHero(leftCol, info, source, autoDownload) {
  // Append a cache-buster to local covers so stale browser-cached images from
  // a previously-deleted manga don't bleed into a freshly-added one at the same id.
  const coverUrl = _isLocal
    ? api.getMangaCoverUrl(_dbId) + '?v=' + Date.now()
    : (info?.cover_url ?? info?.cover_image_url ?? null);

  const isDesktop = () => window.innerWidth >= 768;

  // ── Cover ──
  const coverInner = document.createElement('div');
  coverInner.className = 'aspect-[2/3] rounded-xl overflow-hidden bg-surface-2 shrink-0 cursor-pointer';
  coverInner.appendChild(createCoverImage({ url: coverUrl, alt: info?.title ?? '' }));
  _coverEl = coverInner;

  // Cover lightbox — click expands to full-screen overlay with FLIP animation
  coverInner.addEventListener('click', () => {
    if (!coverUrl) return;
    const rect = coverInner.getBoundingClientRect();
    const overlay = document.createElement('div');
    overlay.className = 'fixed inset-0 z-[9999] flex items-center justify-center';
    overlay.style.cssText = 'background:rgba(0,0,0,0);transition:background 250ms ease';

    const img = document.createElement('img');
    img.src = coverUrl;
    img.alt = info?.title ?? '';
    img.className = 'shadow-2xl object-contain';
    // Start at cover position — no transition yet so the browser paints this position first
    img.style.cssText = `
      position: fixed;
      top: ${rect.top}px; left: ${rect.left}px;
      width: ${rect.width}px; height: ${rect.height}px;
      border-radius: 0.75rem;
      object-fit: contain;
    `;
    overlay.appendChild(img);
    document.body.appendChild(overlay);

    // Force a layout reflow so the browser commits the cover position before we
    // add the transition and move to the centred state. Without this, Chrome
    // batches both style writes and skips the expand animation entirely.
    img.getBoundingClientRect();

    const EASE = 'cubic-bezier(0.4,0,0.2,1)';
    img.style.transition = `top 280ms ${EASE}, left 280ms ${EASE}, width 280ms ${EASE}, height 280ms ${EASE}, border-radius 280ms ease`;
    overlay.style.background = 'rgba(0,0,0,0.75)';

    const vw = window.innerWidth, vh = window.innerHeight;
    const maxW = vw * 0.9, maxH = vh * 0.9;
    const scale = Math.min(maxW / rect.width, maxH / rect.height, 3);
    const newW = rect.width * scale, newH = rect.height * scale;
    img.style.top = ((vh - newH) / 2) + 'px';
    img.style.left = ((vw - newW) / 2) + 'px';
    img.style.width = newW + 'px';
    img.style.height = newH + 'px';
    img.style.borderRadius = '1rem';

    const close = () => {
      overlay.style.background = 'rgba(0,0,0,0)';
      img.style.top = rect.top + 'px';
      img.style.left = rect.left + 'px';
      img.style.width = rect.width + 'px';
      img.style.height = rect.height + 'px';
      setTimeout(() => overlay.remove(), 280);
    };
    overlay.addEventListener('click', close);
  });

  // ── Meta rows ──
  const meta = document.createElement('div');
  meta.className = 'flex flex-col gap-1.5';

  if (source || info?.source_id || _sid) {
    const p = document.createElement('p');
    p.className = 'text-base md:text-sm flex items-center gap-2';
    const sname = escapeHtml(source?.name || info?.source_name || 'Source');
    const sid = source?.id || info?.source_id || _sid;
    const baseUrl = source?.base_url || info?.base_url || null;
    // Primary link: external source website (opens in new tab).
    // Internal source page (/source/:id) is accessible via the Sources nav item.
    if (baseUrl) {
      p.innerHTML = `<span class="font-semibold text-text">Source:</span> <a href="${escapeHtml(baseUrl)}" target="_blank" rel="noopener noreferrer" class="text-accent hover:underline focus-visible:outline-none focus-visible:underline">${sname}</a>`;
    } else {
      // No base URL available — fall back to the internal source page
      p.innerHTML = `<span class="font-semibold text-text">Source:</span> <a href="/source/${sid}" class="text-accent hover:underline focus-visible:outline-none focus-visible:underline">${sname}</a>`;
      p.querySelector('a')?.addEventListener('click', e => { e.preventDefault(); navigate(`/source/${sid}`); });
    }
    meta.appendChild(p);
  }

  if (info?.status && info.status !== 'Unknown') {
    const statusEl = document.createElement('p');
    statusEl.className = 'text-base md:text-sm';
    const statusVal = info.status.toLowerCase();
    const statusDisplay = info.status.charAt(0).toUpperCase() + info.status.slice(1);
    statusEl.innerHTML = `<span class="font-semibold text-text">Status:</span> <a href="/?status=${statusVal}" class="text-accent hover:underline focus-visible:outline-none focus-visible:underline">${escapeHtml(statusDisplay)}</a>`;
    statusEl.querySelector('a')?.addEventListener('click', e => { e.preventDefault(); navigate(`/?status=${statusVal}`); });
    meta.appendChild(statusEl);
  }

  if (info?.authors?.length) {
    const p = document.createElement('p');
    p.className = 'text-base md:text-sm';
    if (_isLocal) {
      p.innerHTML = '<span class="font-semibold text-text">Authors:</span> ' + info.authors.map((a, i) =>
        `<a class="text-accent hover:underline focus-visible:outline-none focus-visible:underline" href="/?author_id=${a.id}" data-idx="${i}">${escapeHtml(a.name)}</a>`
      ).join(', ');
      p.querySelectorAll('a').forEach(el => {
        const id = info.authors[Number(el.dataset.idx)].id;
        el.addEventListener('click', e => { e.preventDefault(); navigate(`/?author_id=${id}`); });
      });
    } else {
      p.innerHTML = '<span class="font-semibold text-text">Authors:</span> ' + info.authors.map((a, i) =>
        `<a class="text-accent hover:underline focus-visible:outline-none focus-visible:underline" href="/source/${_sid}?q=${encodeURIComponent(a.name)}" data-idx="${i}">${escapeHtml(a.name)}</a>`
      ).join(', ');
      p.querySelectorAll('a').forEach(el => {
        const name = info.authors[Number(el.dataset.idx)].name;
        el.addEventListener('click', e => {
          e.preventDefault();
          _buildSourceMetaUrl(_sid, name, 'Author').then(url => navigate(url));
        });
      });
    }
    meta.appendChild(p);
  }

  if (info?.artists?.length) {
    const p = document.createElement('p');
    p.className = 'text-base md:text-sm';
    if (_isLocal) {
      p.innerHTML = '<span class="font-semibold text-text">Artists:</span> ' + info.artists.map((a, i) =>
        `<a class="text-accent hover:underline focus-visible:outline-none focus-visible:underline" href="/?artist_id=${a.id}" data-idx="${i}">${escapeHtml(a.name)}</a>`
      ).join(', ');
      p.querySelectorAll('a').forEach(el => {
        const id = info.artists[Number(el.dataset.idx)].id;
        el.addEventListener('click', e => { e.preventDefault(); navigate(`/?artist_id=${id}`); });
      });
    } else {
      p.innerHTML = '<span class="font-semibold text-text">Artists:</span> ' + info.artists.map((a, i) =>
        `<a class="text-accent hover:underline focus-visible:outline-none focus-visible:underline" href="/source/${_sid}?q=${encodeURIComponent(a.name)}" data-idx="${i}">${escapeHtml(a.name)}</a>`
      ).join(', ');
      p.querySelectorAll('a').forEach(el => {
        const name = info.artists[Number(el.dataset.idx)].name;
        el.addEventListener('click', e => {
          e.preventDefault();
          _buildSourceMetaUrl(_sid, name, 'Artist').then(url => navigate(url));
        });
      });
    }
    meta.appendChild(p);
  }

  // ── titleMetaCard: meta, used on mobile inside heroRow ──
  const titleMetaCard = document.createElement('div');
  titleMetaCard.className = 'flex flex-col gap-2 min-w-0';
  titleMetaCard.appendChild(meta);

  // ── contentCard: meta + buttons + description, used on desktop with gradient ──
  const contentCard = document.createElement('div');
  contentCard.className = 'flex flex-col gap-3 min-w-0';
  contentCard.style.position = 'relative';
  contentCard.style.zIndex = '1';

  // ── heroRow: top section whose children swap based on viewport ──
  const heroRow = document.createElement('div');
  leftCol.appendChild(heroRow);

  // ── Button group ──
  _btnGroupEl = document.createElement('div');
  _btnGroupEl.className = 'flex flex-col gap-2';
  _renderBtnGroup(source);

  // ── Description ──
  /** @type {HTMLElement|null} */ let descWrap = null;
  /** @type {HTMLElement|null} */ let desc = null;
  let expanded = false;

  if (info?.description_html || info?.description) {
    descWrap = document.createElement('div');
    desc = document.createElement('div');
    desc.className = 'text-sm text-text-muted leading-relaxed line-clamp-3';
    desc.innerHTML = info.description_html ?? `<p>${escapeHtml(info.description)}</p>`;

    // Style links and attach external link warning handler
    desc.querySelectorAll('a[href]').forEach(link => {
      link.classList.add('text-accent', 'underline', 'decoration-accent/50', 'hover:decoration-accent');
      link.setAttribute('target', '_blank');
      link.setAttribute('rel', 'noopener noreferrer');
      link.addEventListener('click', (e) => {
        if (getLocal('kani_skip_external_warning') === 'true') return;
        e.preventDefault();
        _showExternalLinkDialog(/** @type {HTMLAnchorElement} */(link).href);
      });
    });

    descWrap.appendChild(desc);

    const toggle = document.createElement('button');
    toggle.type = 'button';
    toggle.className = 'mt-1 text-xs text-accent hover:underline focus-visible:outline-none focus-visible:underline';
    toggle.textContent = 'Show more';
    // Hide until we can check whether the text actually overflows (checked post-layout)
    toggle.style.display = 'none';
    descWrap.appendChild(toggle);

    // After layout: only show the toggle if the description actually overflows 3 lines
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        if (desc && desc.scrollHeight > desc.offsetHeight + 2) {
          toggle.style.display = '';
        }
      });
    });

    toggle.addEventListener('click', () => {
      expanded = !expanded;
      toggle.textContent = expanded ? 'Show less' : 'Show more';

      if (isDesktop()) {
        const slideAmount = _coverEl ? Math.round(_coverEl.offsetHeight * 0.55) : 80;
        if (expanded) {
          const clampedHeight = desc.offsetHeight;
          desc.dataset.clampedHeight = String(clampedHeight);
          desc.classList.remove('line-clamp-3');
          desc.style.overflow = 'hidden'; // prevent text spilling during animation
          const fullHeight = desc.scrollHeight;

          desc.style.maxHeight = clampedHeight + 'px';
          // eslint-disable-next-line no-unused-expressions
          desc.offsetHeight; // force reflow

          desc.style.transition = 'max-height 0.4s ease';
          desc.style.maxHeight = fullHeight + 'px';
          contentCard.style.marginTop = `-${slideAmount}px`;

          let settled = false;
          const expand = () => {
            if (settled) return;
            settled = true;
            contentCard.removeEventListener('transitionend', expand);
            clearTimeout(safety);
            const descTop = descWrap.getBoundingClientRect().top;
            const maxH = Math.max(80, window.innerHeight - descTop - 48);
            desc.style.maxHeight = maxH + 'px';
            desc.style.overflow = '';
            desc.style.overflowY = 'auto';
            desc.style.scrollbarWidth = 'none';
          };
          const safety = setTimeout(expand, 450);
          contentCard.addEventListener('transitionend', expand, { once: true });
        } else {
          const currentHeight = desc.offsetHeight;
          // Clip the outer wrapper so the text can't overflow the card during collapse
          if (descWrap) descWrap.style.overflow = 'hidden';
          desc.style.overflow = 'hidden';
          desc.style.overflowY = '';
          desc.style.scrollbarWidth = '';
          desc.style.maxHeight = currentHeight + 'px';
          // eslint-disable-next-line no-unused-expressions
          desc.offsetHeight; // force reflow

          desc.style.transition = 'max-height 0.4s ease';
          desc.style.maxHeight = (desc.dataset.clampedHeight || '72') + 'px';
          contentCard.style.marginTop = '-0.5rem';

          let settled = false;
          const collapse = () => {
            if (settled) return;
            settled = true;
            contentCard.removeEventListener('transitionend', collapse);
            clearTimeout(safety);
            desc.classList.add('line-clamp-3');
            desc.style.overflow = '';
            desc.style.transition = '';
            desc.style.maxHeight = '';
            if (descWrap) descWrap.style.overflow = '';
          };
          const safety = setTimeout(collapse, 450);
          contentCard.addEventListener('transitionend', collapse, { once: true });
        }
      } else {
        desc.classList.toggle('line-clamp-3', !expanded);
        desc.style.maxHeight = expanded ? '50vh' : '';
        desc.style.overflowY = expanded ? 'auto' : '';
      }
    });
  }

  function _applyHeroLayout() {
    if (!isDesktop()) {
      // ── Mobile: cover (35%) + titleMetaCard side-by-side; buttons + desc full-width below ──
      heroRow.style.cssText = 'display:flex;flex-direction:row;align-items:flex-start;gap:0.75rem';
      coverInner.style.width = '35%';
      coverInner.style.marginLeft = '';
      coverInner.style.marginRight = '';

      // heroRow contains: cover + titleMetaCard
      if (!heroRow.contains(coverInner)) heroRow.insertBefore(coverInner, heroRow.firstChild);
      if (!heroRow.contains(titleMetaCard)) heroRow.appendChild(titleMetaCard);
      if (heroRow.contains(contentCard)) heroRow.removeChild(contentCard);
      titleMetaCard.style.flex = '1 1 0%';

      // meta belongs inside titleMetaCard
      if (!titleMetaCard.contains(meta)) titleMetaCard.appendChild(meta);

      // buttons + desc are direct children of leftCol (full width, with vertical breathing room)
      _btnGroupEl.style.paddingTop = '0.5rem';
      _btnGroupEl.style.paddingBottom = '0.5rem';
      if (!leftCol.contains(_btnGroupEl)) leftCol.appendChild(_btnGroupEl);
      if (descWrap && !leftCol.contains(descWrap)) leftCol.appendChild(descWrap);

      // Clear desktop-only contentCard styles
      contentCard.style.backgroundImage = '';
      contentCard.style.paddingTop = '';
      contentCard.style.marginTop = '';
      contentCard.style.transition = '';
    } else {
      // ── Desktop: cover full-width, contentCard (meta+btns+desc) slides up with gradient ──
      heroRow.style.cssText = '';
      _btnGroupEl.style.paddingTop = '';
      _btnGroupEl.style.paddingBottom = '';

      // heroRow contains: cover + contentCard
      if (!heroRow.contains(coverInner)) heroRow.insertBefore(coverInner, heroRow.firstChild);
      if (!heroRow.contains(contentCard)) heroRow.appendChild(contentCard);
      if (heroRow.contains(titleMetaCard)) heroRow.removeChild(titleMetaCard);

      // meta + btns + desc belong inside contentCard
      if (!contentCard.contains(meta)) contentCard.insertBefore(meta, contentCard.firstChild);
      if (!contentCard.contains(_btnGroupEl)) {
        if (descWrap && contentCard.contains(descWrap)) {
          contentCard.insertBefore(_btnGroupEl, descWrap);
        } else {
          contentCard.appendChild(_btnGroupEl);
        }
      }
      if (descWrap && !contentCard.contains(descWrap)) contentCard.appendChild(descWrap);

      // Desktop gradient + slide
      contentCard.style.backgroundImage = 'linear-gradient(to bottom, transparent, var(--color-bg) 3rem)';
      contentCard.style.paddingTop = '3rem';
      if (!expanded) contentCard.style.marginTop = '-0.5rem';
      contentCard.style.transition = 'margin-top 0.35s ease';

      // Constrain cover so column stays within viewport height
      const colTop = leftCol.getBoundingClientRect().top;
      const available = window.innerHeight - colTop - 48;
      const maxCoverH = Math.max(120, available - contentCard.offsetHeight + 8);
      const naturalH = leftCol.offsetWidth * 1.5;
      if (naturalH > maxCoverH) {
        const w = Math.round(maxCoverH * (2 / 3));
        coverInner.style.width = w + 'px';
        coverInner.style.marginLeft = 'auto';
        coverInner.style.marginRight = 'auto';
      } else {
        coverInner.style.width = '100%';
        coverInner.style.marginLeft = '';
        coverInner.style.marginRight = '';
      }
    }
  }

  _applyHeroLayout();

  _heroResizeListener = _applyHeroLayout;
  window.addEventListener('resize', _heroResizeListener);
}

// ── Button group ──────────────────────────────────────────────────────────────

function _renderBtnGroup(source) {
  if (!_btnGroupEl) return;
  _btnGroupEl.innerHTML = '';

  if (_isLocal) {
    const readBtn = document.createElement('button');
    readBtn.type = 'button';
    readBtn.className = 'btn-primary w-full';
    readBtn.textContent = 'Read';
    readBtn.addEventListener('click', async () => {
      if (readBtn.disabled) return;
      readBtn.disabled = true;
      try {
        const info = await api.getContinueReading(_dbId);
        if (info) {
          const href = info.last_page > 0
            ? `/reader/${info.chapter_id}?page=${info.last_page}`
            : `/reader/${info.chapter_id}`;
          navigate(href);
          return;
        }
        // No downloaded chapter — find the next preferred unread+undownloaded chapter.
        // _findNextPreferredChapter() respects _scanlatorMode and _scanlatorPrefs.
        const nextUnread = _findNextPreferredChapter();
        if (!nextUnread) {
          readBtn.disabled = false;
          // Check if there are unread chapters at all, or if scanlator prefs filtered them out
          const hasAnyUnread = _chapters.some(ch => !ch.read);
          if (hasAnyUnread) {
            showToast('No chapters match your scanlator preferences. Adjust them in the Manage tab.', { type: 'warning' });
          } else {
            showToast('All chapters are read.');
          }
          return;
        }
        // Auto-download and wait for the SSE 'completed' status (max 5 minutes).
        const originalText = readBtn.textContent;
        readBtn.innerHTML = `<span class="inline-block animate-spin [&_svg]:w-4 [&_svg]:h-4">${iconSpinner}</span> Downloading…`;
        try {
          await api.downloadChapter(nextUnread.id);
          await new Promise((resolve, reject) => {
            let timeout;
            // Check immediately in case the download was instant
            if (getState('chaptersProgress').get(nextUnread.id)?.status === 'completed') {
              resolve(undefined);
              return;
            }
            const unsub = subscribe('chaptersProgress', () => {
              if (getState('chaptersProgress').get(nextUnread.id)?.status === 'completed') {
                clearTimeout(timeout);
                unsub();
                resolve(undefined);
              }
            });
            timeout = setTimeout(() => { unsub(); reject(new Error('timeout')); }, 5 * 60 * 1000);
          });
          navigate(`/reader/${nextUnread.id}`);
        } catch {
          showToast('Download failed. Try downloading the chapter manually.');
          readBtn.textContent = originalText;
          readBtn.disabled = false;
        }
      } catch {
        readBtn.disabled = false;
      }
    });
    // Update label once tracking is available.
    api.getMangaTracking(_dbId).then(t => {
      if (t && t.chapters_read > 0) readBtn.textContent = 'Continue Reading';
      else readBtn.textContent = 'Start Reading';
    }).catch(() => { });
    _btnGroupEl.appendChild(readBtn);

    const actionRow = document.createElement('div');
    actionRow.className = 'flex gap-2';

    if (hasPermission('chapter:download')) {
      const dlBtn = document.createElement('button');
      dlBtn.type = 'button';
      dlBtn.className = 'btn-ghost btn-sm flex-1';
      dlBtn.textContent = 'Download All';

      const cancelBtn = document.createElement('button');
      cancelBtn.type = 'button';
      cancelBtn.className = 'btn-ghost btn-sm flex-1';
      cancelBtn.textContent = 'Cancel All';
      cancelBtn.style.display = 'none';

      dlBtn.addEventListener('click', async () => {
        dlBtn.disabled = true;
        try {
          await api.downloadAll(_dbId);
          showToast('Queued all chapters for download');
          dlBtn.style.display = 'none';
          cancelBtn.style.display = '';
          // Refresh the chapter list so in-progress states are visible
          _page = 1;
          if (_activeTab !== 'chapters') {
            document.querySelector('[data-tab="chapters"]')?.click();
          } else {
            _fetchChapters(/** @type {HTMLElement} */ (_contentSection));
          }
        } catch {
          showToast('Failed to queue downloads');
        } finally {
          dlBtn.disabled = false;
        }
      });

      cancelBtn.addEventListener('click', async () => {
        cancelBtn.disabled = true;
        try {
          await api.cancelAllDownloads(_dbId);
          showToast('Cancelled all downloads');
        } catch {
          showToast('Failed to cancel downloads');
        } finally {
          cancelBtn.disabled = false;
          cancelBtn.style.display = 'none';
          dlBtn.style.display = '';
          _page = 1;
          if (_activeTab !== 'chapters') {
            document.querySelector('[data-tab="chapters"]')?.click();
          } else {
            _fetchChapters(/** @type {HTMLElement} */ (_contentSection));
          }
        }
      });

      actionRow.appendChild(dlBtn);
      actionRow.appendChild(cancelBtn);
    }

    if (hasPermission('library:refresh')) {
      const scanBtn = document.createElement('button');
      scanBtn.type = 'button';
      scanBtn.className = 'btn-ghost btn-sm flex-1';
      scanBtn.textContent = 'Scan';
      scanBtn.addEventListener('click', async () => {
        scanBtn.disabled = true;
        try {
          const res = await api.scanManga(_dbId);
          const count = res?.new_chapters ?? 0;
          scanBtn.textContent = count > 0 ? `${count} new chapter${count !== 1 ? 's' : ''}` : 'No new chapters';
          setTimeout(() => { scanBtn.textContent = 'Scan'; }, 3000);
        } finally { scanBtn.disabled = false; }
      });
      actionRow.appendChild(scanBtn);
    }

    if (actionRow.children.length > 0) _btnGroupEl.appendChild(actionRow);
  } else {
    // Source view — Add to Library / Go to Library Entry
    const inLibrary = !!_existingDbId || !!_addedDbId;

    if (inLibrary) {
      const existId = _existingDbId ?? _addedDbId;
      const goBtn = document.createElement('a');
      goBtn.className = 'btn-primary w-full text-center';
      goBtn.textContent = 'Go to Library Entry';
      goBtn.href = `/manga/${existId}`;
      goBtn.addEventListener('click', e => { e.preventDefault(); navigate(`/manga/${existId}`); });
      _btnGroupEl.appendChild(goBtn);
    } else if (hasPermission('library:add')) {
      const addBtn = document.createElement('button');
      addBtn.type = 'button';
      addBtn.className = 'btn-primary w-full';
      addBtn.textContent = 'Add to Library';
      addBtn.addEventListener('click', async () => {
        addBtn.disabled = true;
        try {
          const res = await api.saveToLibrary(_sid, _mangaId);
          _addedDbId = res?.db_id ?? res?.id ?? null;
          _renderBtnGroup(source);
        } catch { addBtn.disabled = false; }
      });
      _btnGroupEl.appendChild(addBtn);
    }
  }
}

// ── Tabs (local only) ─────────────────────────────────────────────────────────

function _renderTabs(wrap) {
  const tabContent = document.createElement('div');
  _contentSection = tabContent;

  const tabBar = document.createElement('div');
  renderTabs(tabBar, {
    tabs: [{ id: 'chapters', name: 'Chapters' }, { id: 'manage', name: 'Manage' }],
    activeId: _activeTab,
    onSelect: switchTab,
  });

  wrap.appendChild(tabBar);
  wrap.appendChild(tabContent);

  function switchTab(/** @type {string} */ tab) {
    _activeTab = tab;
    // Clear content and re-render
    if (_listContainerEl) { render(null, _listContainerEl); _listContainerEl = null; }
    _destroyPagination?.();
    _destroyPagination = null;
    if (_manageResizeListener) { window.removeEventListener('resize', _manageResizeListener); _manageResizeListener = null; }
    tabContent.style.height = '';
    tabContent.style.overflowY = '';
    tabContent.style.scrollbarWidth = '';
    tabContent.innerHTML = '';

    if (tab === 'chapters') {
      _manageMounted = false;
      _fetchChapters(tabContent);
    } else {
      _manageMounted = false;
      _renderManageTab(tabContent);
    }
  }

  switchTab(_activeTab);
}

// ── Chapter helpers ───────────────────────────────────────────────────────────

/** Maps a raw API chapter to the local chapter model. */
function _mapChapter(ch) {
  return {
    id: Number(ch.id),
    title: formatChapterTitle(ch),
    chapter_number: ch.number ?? ch.chapter_number,
    source_chapter_id: ch.source_chapter_id ?? null,
    scanlator: ch.scanlator ?? null,
    date_uploaded: ch.date_uploaded ?? null,
    downloaded: isChapterDownloaded(ch, null),
    read: ch.is_read ?? false,
    last_page_read: ch.last_page_read ?? 0,
    is_orphaned: ch.is_orphaned ?? false,
  };
}

/**
 * Sorts a chapter array client-side to match a given sort order key.
 * Used when a remote source returns all chapters at once (no server paging).
 * @param {any[]} chapters
 * @param {string} order
 * @returns {any[]}
 */
function _sortChaptersClientSide(chapters, order) {
  const cmp = (a, b, key, asc) => {
    const va = a[key] ?? null;
    const vb = b[key] ?? null;
    if (va === null && vb === null) return 0;
    if (va === null) return asc ? -1 : 1;
    if (vb === null) return asc ? 1 : -1;
    return asc ? (va > vb ? 1 : va < vb ? -1 : 0) : (va < vb ? 1 : va > vb ? -1 : 0);
  };
  const sorted = [...chapters];
  switch (order) {
    case 'chapter_asc':  sorted.sort((a, b) => cmp(a, b, 'chapter_number', true));  break;
    case 'chapter_desc': sorted.sort((a, b) => cmp(a, b, 'chapter_number', false)); break;
    case 'uploaded_asc':  sorted.sort((a, b) => cmp(a, b, 'date_uploaded', true));  break;
    case 'uploaded_desc': sorted.sort((a, b) => cmp(a, b, 'date_uploaded', false)); break;
    case 'scanlator_asc':  sorted.sort((a, b) => cmp(a, b, 'scanlator', true));  break;
    case 'scanlator_desc': sorted.sort((a, b) => cmp(a, b, 'scanlator', false)); break;
    // volume and language aren't typically available on remote chapters; fall back to chapter order
    default: sorted.sort((a, b) => cmp(a, b, 'chapter_number', false)); break;
  }
  return sorted;
}

/**
 * Selects preferred undownloaded chapters into _selected via the server-side
 * preferred_only filter, which applies scanlator preferences and download rules
 * and picks one version per chapter number.
 */
async function _selectPreferredUndownloaded() {
  _selectMode = true;
  _selected.clear();
  _allSelected = false;
  if (_isLocal) {
    const res = await api.getChapterIds(_dbId, { preferredOnly: true, sortOrder: _sortOrder }).catch(() => null);
    const ids = res?.ids ?? [];
    for (const id of ids) _selected.add(id);
  } else {
    for (const ch of _chapters) {
      if (!ch.downloaded) _selected.add(ch.id);
    }
  }
  _renderChapterList();
}

/**
 * Returns the first unread+undownloaded chapter that passes scanlator preference
 * filtering, respecting _scanlatorMode and _scanlatorPrefs.
 * @returns {any|null}
 */
function _findNextPreferredChapter() {
  const candidates = _chapters.filter(ch => !ch.read && !ch.downloaded);
  if (!candidates.length) return null;

  // Group by chapter_number, sorted ascending
  /** @type {Map<number|string, any[]>} */
  const byNumber = new Map();
  for (const ch of candidates) {
    const num = ch.chapter_number ?? ch.id;
    if (!byNumber.has(num)) byNumber.set(num, []);
    byNumber.get(num).push(ch);
  }
  const sortedNums = [...byNumber.keys()].sort((a, b) => Number(a) - Number(b));

  for (const num of sortedNums) {
    const group = byNumber.get(num);
    let eligible = group;
    if (_scanlatorMode === 'whitelist') {
      eligible = group.filter(ch =>
        _scanlatorPrefs.some(p => p.scanlator === ch.scanlator && !p.blocked)
      );
    } else if (_scanlatorMode === 'priority') {
      eligible = group.filter(ch =>
        !_scanlatorPrefs.some(p => p.scanlator === ch.scanlator && p.blocked)
      );
    }
    if (!eligible.length) continue;
    // Pick highest priority
    let best = eligible[0];
    for (const ch of eligible.slice(1)) {
      const chPrio = _scanlatorPrefs.find(p => p.scanlator === ch.scanlator)?.priority ?? -1;
      const bestPrio = _scanlatorPrefs.find(p => p.scanlator === best.scanlator)?.priority ?? -1;
      if (chPrio > bestPrio) best = ch;
    }
    return best;
  }
  return null;
}

// ── Chapters ──────────────────────────────────────────────────────────────────

async function _fetchChapters(sectionEl) {
  const infinite = getLocal('kani_chapter_pagination') === 'infinite';

  // Tear down previous render and resize listener
  if (_listContainerEl) { render(null, _listContainerEl); _listContainerEl = null; }
  _destroyPagination?.();
  _destroyPagination = null;
  if (_chapterResizeListener) {
    window.removeEventListener('resize', _chapterResizeListener);
    _chapterResizeListener = null;
  }

  // Reset accumulated chapters on a fresh fetch (page 1)
  if (_page === 1) {
    _chapters = [];
    _chaptersHasMore = false;
    _chaptersLoading = false;
  }

  sectionEl.className = 'flex flex-col gap-3';
  sectionEl.innerHTML = '';
  startLoading();

  // Header with sort controls
  const header = document.createElement('div');
  header.className = 'flex items-center justify-between gap-3 flex-wrap';

  const headerTitle = document.createElement('h2');
  headerTitle.className = 'text-xl font-semibold text-text';
  headerTitle.textContent = 'Chapters';
  header.appendChild(headerTitle);

  const controls = document.createElement('div');
  controls.className = 'flex items-center gap-2 flex-wrap';

  const sortEl = document.createElement('select');
  sortEl.className = 'input w-auto text-sm';
  sortEl.setAttribute('aria-label', 'Sort order');

  if (!_isLocal && _remoteChapterSorts && _remoteChapterSorts.length > 0) {
    // Remote paged source with server-side sort options from the extension.
    for (const { id, name } of _remoteChapterSorts) {
      const opt = document.createElement('option');
      opt.value = id; opt.textContent = name;
      if (id === _remoteSort) opt.selected = true;
      sortEl.appendChild(opt);
    }
    sortEl.addEventListener('change', () => {
      _remoteSort = sortEl.value;
      _allRemoteChapters = null; // invalidate any cached full list
      _page = 1;
      _updateUrl();
      _fetchChapters(sectionEl);
    });
  } else {
    // Local library or remote all-at-once source: client-side sort options.
    for (const [v, l] of [
      ['chapter_desc', 'Chapter ↓'], ['chapter_asc', 'Chapter ↑'],
      ['uploaded_desc', 'Date ↓'], ['uploaded_asc', 'Date ↑'],
      ['volume_desc', 'Volume ↓'], ['volume_asc', 'Volume ↑'],
      ['language_asc', 'Language A–Z'], ['language_desc', 'Language Z–A'],
      ['scanlator_asc', 'Scanlator A–Z'], ['scanlator_desc', 'Scanlator Z–A'],
    ]) {
      const opt = document.createElement('option');
      opt.value = v; opt.textContent = l;
      if (v === _sortOrder) opt.selected = true;
      sortEl.appendChild(opt);
    }
    sortEl.addEventListener('change', () => {
      _sortOrder = sortEl.value;
      setLocal('kani_chapter_sort_order', _sortOrder);
      _page = 1;
      if (_allRemoteChapters !== null) {
        // Re-sort cached chapters client-side; no network request needed
        _allRemoteChapters = _sortChaptersClientSide(_allRemoteChapters, _sortOrder);
      }
      _updateUrl();
      _fetchChapters(sectionEl);
    });
  }
  controls.appendChild(sortEl);


  // Page size selector — only shown in paginated mode
  if (!infinite) {
    const sizeEl = document.createElement('select');
    sizeEl.className = 'input w-20 text-sm';
    sizeEl.setAttribute('aria-label', 'Page size');
    for (const n of [20, 50, 100]) {
      const opt = document.createElement('option');
      opt.value = String(n); opt.textContent = String(n);
      if (n === _chapterPageSize) opt.selected = true;
      sizeEl.appendChild(opt);
    }
    sizeEl.addEventListener('change', () => {
      _chapterPageSize = Number(sizeEl.value);
      setLocal('kani_chapter_page_size', String(_chapterPageSize));
      _page = 1;
      _updateUrl();
      _fetchChapters(sectionEl);
    });
    controls.appendChild(sizeEl);
  }

  // Server-side chapter filters — only for local library manga
  if (_isLocal) {
    const dlBtn = document.createElement('button');
    dlBtn.type = 'button';
    dlBtn.className = 'btn-ghost btn-sm' + (_filterDownloaded ? ' text-accent' : '');
    dlBtn.textContent = 'Downloaded';
    dlBtn.title = _filterDownloaded ? 'Show all chapters' : 'Show downloaded chapters only';
    dlBtn.addEventListener('click', () => {
      _filterDownloaded = !_filterDownloaded;
      setLocal(`kani_filter_downloaded_${_dbId}`, String(_filterDownloaded));
      _page = 1;
      _updateUrl();
      _fetchChapters(sectionEl);
    });
    controls.appendChild(dlBtn);

    const unreadBtn = document.createElement('button');
    unreadBtn.type = 'button';
    unreadBtn.className = 'btn-ghost btn-sm' + (_filterUnread ? ' text-accent' : '');
    unreadBtn.textContent = 'Unread';
    unreadBtn.title = _filterUnread ? 'Show all chapters' : 'Show unread chapters only';
    unreadBtn.addEventListener('click', () => {
      _filterUnread = !_filterUnread;
      _page = 1;
      _updateUrl();
      _fetchChapters(sectionEl);
    });
    controls.appendChild(unreadBtn);

    if (_availableScanlators.length > 1) {
      const scanSel = document.createElement('select');
      scanSel.className = 'input w-auto text-sm';
      scanSel.setAttribute('aria-label', 'Filter by scanlator');
      const allOpt = document.createElement('option');
      allOpt.value = '';
      allOpt.textContent = 'All scanlators';
      scanSel.appendChild(allOpt);
      for (const s of _availableScanlators) {
        const opt = document.createElement('option');
        opt.value = s;
        opt.textContent = s;
        if (s === _filterScanlator) opt.selected = true;
        scanSel.appendChild(opt);
      }
      scanSel.addEventListener('change', () => {
        _filterScanlator = scanSel.value || null;
        _page = 1;
        _updateUrl();
        _fetchChapters(sectionEl);
      });
      controls.appendChild(scanSel);
    }
  }

  header.appendChild(controls);
  sectionEl.appendChild(header);

  const listEl = document.createElement('div');
  sectionEl.appendChild(listEl);

  const paginEl = document.createElement('div');
  if (!infinite) sectionEl.appendChild(paginEl);

  let result;
  // For remote sources with a full-list cache, skip the network fetch on subsequent pages.
  if (!_isLocal && _allRemoteChapters !== null) {
    result = null; // will use _allRemoteChapters below
  } else {
    try {
      result = _isLocal
        ? await api.getLocalChapters(_dbId, _page, _chapterPageSize, _sortOrder, _abort?.signal, {
            filterDownloaded: _filterDownloaded ? true : null,
            filterUnread: _filterUnread ? true : null,
            filterScanlator: _filterScanlator,
          })
        : await api.getRemoteChapters(_sid, _mangaId, _page, _chapterPageSize, _abort?.signal,
            _remoteChapterSorts?.length ? _remoteSort : null);
    } catch (e) {
      if (e?.name === 'AbortError') return;
      finishLoading();
      listEl.appendChild(createErrorState({ message: 'Failed to load chapters.' }));
      return;
    }
  }

  finishLoading();

  // Detect whether the source returned all chapters at once (no server-side paging).
  // If so, cache them and apply client-side pagination or progressive loading.
  if (!_isLocal && result !== null && _allRemoteChapters === null) {
    const raw = Array.isArray(result?.chapters) ? result.chapters
      : Array.isArray(result) ? result : [];
    // A source is truly server-paged only if it returned has_next_page: true.
    // When has_next_page is false or absent, the source returned everything at once.
    const serverPaged = result?.has_next_page === true || result?.has_next === true;
    if (!serverPaged) {
      // Source returned the entire list — cache and apply client-side sort
      _allRemoteChapters = _sortChaptersClientSide(raw.map(_mapChapter), _sortOrder);
    }
  }

  let mapped;
  let hasNext;

  if (!_isLocal && _allRemoteChapters !== null) {
    // Client-side paging from the cached full list
    const start = (_page - 1) * _chapterPageSize;
    mapped = _allRemoteChapters.slice(start, start + _chapterPageSize);
    hasNext = start + _chapterPageSize < _allRemoteChapters.length;
    if (mapped.length === 0 && _allRemoteChapters.length === 0) {
      listEl.appendChild(createEmptyState({ icon: iconDocument, title: 'No chapters found.' }));
      return;
    }
  } else {
    const rawChapters = Array.isArray(result?.chapters) ? result.chapters
      : Array.isArray(result) ? result : [];
    if (rawChapters.length === 0 && _chapters.length === 0) {
      listEl.appendChild(createEmptyState({ icon: iconDocument, title: 'No chapters found.' }));
      return;
    }
    mapped = rawChapters.map(_mapChapter);
    hasNext = hasNextPage(result, rawChapters.length, _chapterPageSize);
  }

  if (infinite) {
    if (!_isLocal && _allRemoteChapters !== null) {
      // Cached list: start with first chunk; subsequent chunks loaded via _loadMoreChapters
      const start = (_page - 1) * _chapterPageSize;
      _chapters = _allRemoteChapters.slice(0, start + _chapterPageSize);
      _chaptersHasMore = start + _chapterPageSize < _allRemoteChapters.length;
    } else {
      _chapters = [..._chapters, ...mapped];
      _chaptersHasMore = hasNext;
    }
    _listContainerEl = listEl;
    _renderChapterList();

    // Re-render on window resize (desktop height recalculation)
    _chapterResizeListener = () => _renderChapterList();
    window.addEventListener('resize', _chapterResizeListener);
  } else {
    _chapters = mapped;
    _chaptersHasMore = false;

    // Render pagination first so its height is measurable when sizing the list
    if (_page > 1 || hasNext) {
      const { destroy } = renderPagination(paginEl, {
        page: _page,
        hasNext,
        onPageChange: (p) => { _page = p; _updateUrl(); _fetchChapters(sectionEl); },
      });
      _destroyPagination = destroy;
      _paginEl = paginEl;
    } else {
      _paginEl = null;
    }

    _listContainerEl = listEl;
    _renderChapterList();

    // Re-render on window resize (desktop height recalculation)
    _chapterResizeListener = () => _renderChapterList();
    window.addEventListener('resize', _chapterResizeListener);
  }
}

/** Re-renders VirtualChapterList into `_listContainerEl` with current infinite-scroll state. */
function _renderChapterList() {
  if (!_listContainerEl) return;
  const readerHrefFn = (ch) => _isLocal
    ? `/reader/${ch.id}`
    : `/source/${_sid}/manga/${encodeURIComponent(_mangaId)}/chapter/${encodeURIComponent(ch.source_chapter_id ?? ch.id)}`;
  const paginH = _paginEl ? (_paginEl.offsetHeight + 12) : 0; // 12px = gap-3
  const height = window.innerWidth >= 768
    ? Math.max(200, window.innerHeight - _listContainerEl.getBoundingClientRect().top - 48 - paginH - 12)
    : undefined;
  render(html`<${VirtualChapterList}
    chapters=${_chapters}
    readerHrefFn=${readerHrefFn}
    inLibrary=${_isLocal}
    mangaId=${_dbId || null}
    hasMore=${_chaptersHasMore}
    loading=${_chaptersLoading}
    canDownload=${hasPermission('chapter:download')}
    canDelete=${hasPermission('chapter:delete')}
    allSelectedProp=${_allSelected}
    onLoadMore=${_loadMoreChapters}
    onToggleRead=${(id, isRead) => {
      const ch = _chapters.find(c => c.id === id);
      if (!ch) return;
      const coalesce = getLocal('kani_coalesce_read') === 'true';
      if (coalesce && ch.chapter_number != null) {
        const siblingIds = _chapters
          .filter(c => c.id !== id && c.chapter_number === ch.chapter_number)
          .map(c => c.id);
        // Fire-and-forget — visual update happens synchronously below
        if (siblingIds.length) api.setChapterReadStatus(siblingIds, isRead).catch(() => { });
        _chapters = _chapters.map(c =>
          c.chapter_number === ch.chapter_number
            ? { ...c, read: isRead, last_page_read: isRead ? 0 : c.last_page_read }
            : c
        );
      } else {
        _chapters = _chapters.map(c =>
          c.id === id ? { ...c, read: isRead, last_page_read: isRead ? 0 : c.last_page_read } : c
        );
      }
      _renderChapterList();
    }}
    onMarkUpTo=${(chapterNumber, isRead) => {
      _chapters = _chapters.map(ch => {
        if (ch.chapter_number == null) return ch;
        if (isRead ? ch.chapter_number <= chapterNumber : ch.chapter_number >= chapterNumber) {
          return { ...ch, read: isRead };
        }
        return ch;
      });
      _renderChapterList();
    }}
    selectMode=${_selectMode}
    selected=${_selected}
    onToggleSelect=${(id) => {
      if (_selected.has(id)) { _selected.delete(id); _allSelected = false; } else _selected.add(id);
      _renderChapterList();
    }}
    onSelectAll=${async () => {
      let allIds;
      if (_isLocal) {
        const res = await api.getChapterIds(_dbId, {
          filterDownloaded: _filterDownloaded ? true : null,
          filterUnread: _filterUnread ? true : null,
          filterScanlator: _filterScanlator,
          sortOrder: _sortOrder,
        }).catch(() => null);
        allIds = res?.ids ?? _chapters.map(ch => ch.id);
      } else {
        allIds = _chapters.map(ch => ch.id);
      }
      const allAlreadySelected = allIds.every(id => _selected.has(id));
      if (allAlreadySelected) {
        _selected.clear();
        _allSelected = false;
      } else {
        for (const id of allIds) _selected.add(id);
        _allSelected = true;
      }
      _renderChapterList();
    }}
    onFlipSelection=${async () => {
      let allIds;
      if (_isLocal) {
        const res = await api.getChapterIds(_dbId, {
          filterDownloaded: _filterDownloaded ? true : null,
          filterUnread: _filterUnread ? true : null,
          filterScanlator: _filterScanlator,
          sortOrder: _sortOrder,
        }).catch(() => null);
        allIds = res?.ids ?? _chapters.map(ch => ch.id);
      } else {
        allIds = _chapters.map(ch => ch.id);
      }
      _selected = new Set(allIds.filter(id => !_selected.has(id)));
      _allSelected = false;
      _renderChapterList();
    }}
    onSelectUndownloaded=${async () => {
      let ids;
      if (_isLocal) {
        const res = await api.getChapterIds(_dbId, { filterDownloaded: false, sortOrder: _sortOrder }).catch(() => null);
        ids = res?.ids ?? _chapters.filter(ch => !ch.downloaded).map(ch => ch.id);
      } else {
        ids = _chapters.filter(ch => !ch.downloaded).map(ch => ch.id);
      }
      _selected = new Set(ids);
      _allSelected = false;
      _renderChapterList();
    }}
    onSelectUnread=${async () => {
      let ids;
      if (_isLocal) {
        const res = await api.getChapterIds(_dbId, { filterUnread: true, sortOrder: _sortOrder }).catch(() => null);
        ids = res?.ids ?? _chapters.filter(ch => !ch.read).map(ch => ch.id);
      } else {
        ids = _chapters.filter(ch => !ch.read).map(ch => ch.id);
      }
      _selected = new Set(ids);
      _allSelected = false;
      _renderChapterList();
    }}
    onBulkRead=${async (isRead) => {
      const ids = [..._selected];
      if (!ids.length) return;
      try {
        await api.setChapterReadStatus(ids, isRead);
        // Selection mode always acts per-chapter — coalescing is only for context-menu actions.
        const idSet = new Set(ids);
        _chapters = _chapters.map(ch => idSet.has(ch.id) ? { ...ch, read: isRead } : ch);
        _selected.clear();
        _selectMode = false;
        _allSelected = false;
        _renderChapterList();
      } catch (err) {
        console.error('bulk read failed:', err);
      }
    }}
    onBulkDownload=${async () => {
      const ids = [..._selected].filter(id => {
        const ch = _chapters.find(c => c.id === id);
        return ch && !ch.downloaded;
      });
      if (!ids.length) return;
      for (const id of ids) {
        try { await api.downloadChapter(id); } catch { }
      }
      _selected.clear();
      _selectMode = false;
      _allSelected = false;
      _renderChapterList();
      showToast(`Queued ${ids.length} chapter${ids.length !== 1 ? 's' : ''} for download`);
    }}
    onBulkDelete=${async () => {
      const ids = [..._selected].filter(id => {
        const ch = _chapters.find(c => c.id === id);
        return ch && ch.downloaded;
      });
      if (!ids.length) return;
      for (const id of ids) {
        try { await api.deleteChapter(id); } catch { }
      }
      const idSet = new Set(ids);
      _chapters = _chapters
        .filter(ch => !(idSet.has(ch.id) && ch.is_orphaned))
        .map(ch => idSet.has(ch.id) ? { ...ch, download_status: 0, page_count: null, downloaded: false } : ch);
      _selected.clear();
      _selectMode = false;
      _allSelected = false;
      _renderChapterList();
      showToast(`Deleted ${ids.length} downloaded chapter${ids.length !== 1 ? 's' : ''}`);
    }}
    onExitSelect=${() => {
      _selectMode = false;
      _selected.clear();
      _allSelected = false;
      _renderChapterList();
    }}
    onEnterSelectWithChapter=${(id) => {
      _selectMode = true;
      _selected.clear();
      _allSelected = false;
      _selected.add(id);
      _renderChapterList();
    }}
    onDelete=${(id) => {
      const ch = _chapters.find(c => c.id === id);
      if (!ch) return;
      if (ch.is_orphaned) {
        _chapters = _chapters.filter(c => c.id !== id);
      } else {
        _chapters = _chapters.map(c => c.id === id ? { ...c, download_status: 0, page_count: null, downloaded: false } : c);
      }
      _renderChapterList();
    }}
    height=${height}
  />`, _listContainerEl);
}

/** Loads the next page of chapters and appends them to `_chapters`. */
async function _loadMoreChapters() {
  if (_chaptersLoading || !_chaptersHasMore || !_listContainerEl) return;
  _chaptersLoading = true;
  _renderChapterList();
  _page++;
  _updateUrl();

  if (!_isLocal && _allRemoteChapters !== null) {
    // Serve next chunk from client-side cache — no network request needed
    const end = _page * _chapterPageSize;
    _chapters = _allRemoteChapters.slice(0, end);
    _chaptersHasMore = end < _allRemoteChapters.length;
    _chaptersLoading = false;
    _renderChapterList();
    return;
  }

  try {
    const result = _isLocal
      ? await api.getLocalChapters(_dbId, _page, _chapterPageSize, _sortOrder, _abort?.signal, {
          filterDownloaded: _filterDownloaded ? true : null,
          filterUnread: _filterUnread ? true : null,
          filterScanlator: _filterScanlator,
        })
      : await api.getRemoteChapters(_sid, _mangaId, _page, _chapterPageSize, _abort?.signal,
          _remoteChapterSorts?.length ? _remoteSort : null);
    const rawChapters = Array.isArray(result?.chapters) ? result.chapters
      : Array.isArray(result) ? result
        : [];
    const mapped = rawChapters.map(_mapChapter);
    _chapters = [..._chapters, ...mapped];
    _chaptersHasMore = hasNextPage(result, rawChapters.length, _chapterPageSize);
  } catch (e) {
    if (e?.name !== 'AbortError') console.error('Failed to load more chapters:', e);
    _page--;
    _updateUrl();
  }
  _chaptersLoading = false;
  _renderChapterList();
}

// ── Manage tab ────────────────────────────────────────────────────────────────

async function _renderManageTab(contentEl) {
  if (_manageMounted) return;
  _manageMounted = true;

  contentEl.className = 'flex flex-col gap-8';

  // Constrain to viewport height on desktop (same as chapter list)
  function _applyManageHeight() {
    if (window.innerWidth >= 768) {
      const top = contentEl.getBoundingClientRect().top;
      contentEl.style.height = Math.max(200, window.innerHeight - top - 48) + 'px';
      contentEl.style.overflowY = 'auto';
      contentEl.style.scrollbarWidth = 'none';
    } else {
      contentEl.style.height = '';
      contentEl.style.overflowY = '';
      contentEl.style.scrollbarWidth = '';
    }
  }
  _applyManageHeight();
  _manageResizeListener = _applyManageHeight;
  window.addEventListener('resize', _manageResizeListener);

  // ── Helpers ──

  /** Section header with title + subtitle and a bottom border */
  const mkSectionHeader = (title, subtitle) => {
    const el = document.createElement('div');
    el.className = 'flex flex-col gap-0.5 pb-2 border-b border-border-subtle';
    const h = document.createElement('h2');
    h.className = 'text-sm font-semibold text-text';
    h.textContent = title;
    el.appendChild(h);
    const s = document.createElement('p');
    s.className = 'text-xs text-text-muted';
    s.textContent = subtitle;
    el.appendChild(s);
    return el;
  };

  /** Untitled card — horizontal padding only; rows manage their own vertical padding */
  const mkCard = () => {
    const card = document.createElement('div');
    card.className = 'bg-surface border border-border rounded-xl px-4 md:px-6 py-1';
    return card;
  };

  /** Titled card: heading + subtitle, then a full-width separator, then body content */
  const mkTitledCard = (title, subtitle) => {
    const card = document.createElement('div');
    card.className = 'bg-surface border border-border rounded-xl p-4 md:p-6';
    const h = document.createElement('h3');
    h.className = 'text-sm font-semibold text-text';
    h.textContent = title;
    card.appendChild(h);
    const s = document.createElement('p');
    s.className = 'text-xs text-text-muted mt-0.5';
    s.textContent = subtitle;
    card.appendChild(s);
    const sep = document.createElement('div');
    sep.className = 'border-t border-border-subtle mt-3 mb-4';
    card.appendChild(sep);
    return card;
  };

  /** Description-left, control-right action row. Use inside mkCard via mkItem. */
  const mkRow = (label, sublabel, control) => {
    const row = document.createElement('div');
    row.className = 'flex items-center justify-between gap-4';
    const text = document.createElement('div');
    const lEl = document.createElement('p');
    lEl.className = 'text-sm font-medium text-text';
    lEl.textContent = label;
    text.appendChild(lEl);
    if (sublabel) {
      const sEl = document.createElement('p');
      sEl.className = 'text-xs text-text-muted mt-0.5';
      sEl.textContent = sublabel;
      text.appendChild(sEl);
    }
    row.appendChild(text);
    control.classList.add('shrink-0');
    row.appendChild(control);
    return row;
  };

  /** Wraps a row with vertical padding and a bottom divider; first child has less top padding. */
  const mkItem = (rowEl) => {
    const item = document.createElement('div');
    item.className = 'py-4 first:pt-3 last:pb-3 border-b border-border-subtle last:border-b-0';
    item.appendChild(rowEl);
    return item;
  };

  // ── 1. Library ──────────────────────────────────────────────────────────────

  const hasLibSection =
    hasPermission('library:refresh') ||
    (_autoScan && hasPermission('library:manage'));

  if (hasLibSection) {
    const section = document.createElement('div');
    section.className = 'flex flex-col gap-3';
    section.appendChild(mkSectionHeader('Library', 'Sync this manga\'s metadata and configure download behaviour.'));

    const card = mkCard();

    if (hasPermission('library:refresh')) {
      const refreshBtn = document.createElement('button');
      refreshBtn.type = 'button';
      refreshBtn.className = 'btn-ghost btn-sm';
      refreshBtn.textContent = 'Refresh';
      refreshBtn.addEventListener('click', async () => {
        refreshBtn.disabled = true;
        try {
          await api.refreshManga(_dbId);
          refreshBtn.textContent = 'Done';
          setTimeout(() => { refreshBtn.textContent = 'Refresh'; }, 3000);
        } finally { refreshBtn.disabled = false; }
      });
      card.appendChild(mkItem(mkRow('Refresh metadata', 'Re-fetch title, cover, and description from source', refreshBtn)));
    }

    if (_autoScan && hasPermission('library:manage')) {
      const toggle = document.createElement('label');
      toggle.className = 'kani-toggle cursor-pointer';
      toggle.innerHTML = `<input type="checkbox" class="kani-toggle__input" aria-label="Auto-download new chapters"><span class="kani-toggle__track"></span>`;
      const input = /** @type {HTMLInputElement} */ (toggle.querySelector('.kani-toggle__input'));
      api.getMangaDetails(_dbId).then(res => { input.checked = res?.auto_download ?? false; }).catch(() => { });
      input.addEventListener('change', async () => {
        try { await api.toggleAutoDownload(_dbId, input.checked); } catch { input.checked = !input.checked; }
      });
      card.appendChild(mkItem(mkRow('Auto-download', 'Automatically download new chapters when found', toggle)));
    }

    if (hasPermission('chapter:download')) {
      const toggle = document.createElement('label');
      toggle.className = 'kani-toggle cursor-pointer';
      toggle.innerHTML = `<input type="checkbox" class="kani-toggle__input" aria-label="Download All: preferred only"><span class="kani-toggle__track"></span>`;
      const input = /** @type {HTMLInputElement} */ (toggle.querySelector('.kani-toggle__input'));
      input.checked = _downloadAllPreferredOnly;
      input.addEventListener('change', async () => {
        _downloadAllPreferredOnly = input.checked;
        try { await api.toggleDownloadAllPreferred(_dbId, input.checked); } catch { input.checked = !input.checked; _downloadAllPreferredOnly = input.checked; }
      });
      card.appendChild(mkItem(mkRow('Download All: preferred only', 'When enabled, "Download All" downloads one version per chapter using scanlator preferences', toggle)));
    }

    section.appendChild(card);
    contentEl.appendChild(section);
  }

  // ── 1b. Tracking ────────────────────────────────────────────────────────────

  {
    const trackSection = document.createElement('div');
    trackSection.className = 'flex flex-col gap-3';
    trackSection.appendChild(mkSectionHeader('Tracking', 'Set your reading status and score for this manga.'));

    const trackCard = mkTitledCard('Status & Score', 'Track your progress');

    const statusOptions = [
      { value: '', label: 'Not tracked' },
      { value: 'reading', label: 'Reading' },
      { value: 'on_hold', label: 'On Hold' },
      { value: 'dropped', label: 'Dropped' },
      { value: 'plan_to_read', label: 'Plan to Read' },
      { value: 'completed', label: 'Completed' },
      { value: 'rereading', label: 'Rereading' },
    ];

    const statusSelect = document.createElement('select');
    statusSelect.className = 'bg-surface border border-border rounded-lg px-3 py-1.5 text-sm text-text';
    for (const opt of statusOptions) {
      const o = document.createElement('option');
      o.value = opt.value;
      o.textContent = opt.label;
      statusSelect.appendChild(o);
    }

    const scoreInput = document.createElement('input');
    scoreInput.type = 'number';
    scoreInput.min = '0';
    scoreInput.max = '10';
    scoreInput.step = '0.5';
    scoreInput.placeholder = '—';
    scoreInput.className = 'bg-surface border border-border rounded-lg px-3 py-1.5 text-sm text-text w-20';

    const progressText = document.createElement('span');
    progressText.className = 'text-sm text-text-muted';
    progressText.textContent = '—';

    // Tracking enabled toggle
    const toggleId = `tracking-enabled-${_dbId}`;
    const toggleLabel = document.createElement('label');
    toggleLabel.className = 'kani-toggle';
    toggleLabel.setAttribute('for', toggleId);
    toggleLabel.innerHTML = `
      <input type="checkbox" id="${toggleId}" class="kani-toggle__input js-tracking-enabled" checked>
      <span class="kani-toggle__track"></span>
    `;
    const trackingToggle = /** @type {HTMLInputElement} */ (toggleLabel.querySelector('.js-tracking-enabled'));

    trackCard.appendChild(mkItem(mkRow('Sync enabled', 'Sync this manga with external trackers', toggleLabel)));
    trackCard.appendChild(mkItem(mkRow('Status', 'Your reading status', statusSelect)));
    trackCard.appendChild(mkItem(mkRow('Score', 'Rate 0–10', scoreInput)));
    trackCard.appendChild(mkItem(mkRow('Progress', 'Chapters read / total', progressText)));

    trackSection.appendChild(trackCard);
    contentEl.appendChild(trackSection);

    // Load tracking data
    api.getMangaTracking(_dbId).then(tracking => {
      trackingToggle.checked = tracking.tracking_enabled ?? true;
      if (tracking.status) statusSelect.value = tracking.status;
      if (tracking.score != null) scoreInput.value = String(tracking.score);
      progressText.textContent = `${tracking.chapters_read} / ${tracking.total_chapters}`;
    }).catch(() => { });

    // Save tracking enabled toggle
    trackingToggle.addEventListener('change', () => {
      api.setMangaTracking(_dbId, { tracking_enabled: trackingToggle.checked }).catch(() => { });
    });

    // Save on change
    statusSelect.addEventListener('change', () => {
      const body = statusSelect.value ? { status: statusSelect.value } : { status: null };
      api.setMangaTracking(_dbId, body).catch(() => { });
    });

    let scoreTimer = null;
    scoreInput.addEventListener('input', () => {
      if (scoreTimer) clearTimeout(scoreTimer);
      scoreTimer = setTimeout(() => {
        const val = parseFloat(scoreInput.value);
        if (!isNaN(val) && val >= 0 && val <= 10) {
          api.setMangaTracking(_dbId, { score: val }).catch(() => { });
        }
      }, 800);
    });
  }

  // ── 1c. External Trackers ───────────────────────────────────────────────────

  {
    const extSection = document.createElement('div');
    extSection.className = 'flex flex-col gap-3';
    extSection.appendChild(mkSectionHeader('External Trackers', 'Sync progress with AniList, MyAnimeList, etc.'));

    const extCard = mkCard();
    const extBody = document.createElement('div');
    extBody.className = 'py-3 text-sm text-text-muted';
    extBody.textContent = 'Loading trackers...';
    extCard.appendChild(extBody);
    extSection.appendChild(extCard);
    contentEl.appendChild(extSection);

    // Load tracker data
    Promise.all([api.getTrackers(), api.getTrackerMappings(_dbId)])
      .then(([trackers, mappings]) => {
        extBody.textContent = '';
        const configuredTrackers = trackers.filter(t => t.configured);
        if (!configuredTrackers.length) {
          extBody.textContent = 'No trackers configured. Add OAuth app credentials in Settings → Trackers.';
          return;
        }

        for (const t of configuredTrackers) {
          const mapping = mappings.find(m => m.tracker_id === t.id);
          const row = document.createElement('div');
          row.className = 'flex items-center justify-between gap-4 py-3 border-b border-border-subtle last:border-b-0';

          const info = document.createElement('div');
          const nameEl = document.createElement('p');
          nameEl.className = 'text-sm font-medium text-text';
          nameEl.textContent = t.name;
          info.appendChild(nameEl);

          const statusEl = document.createElement('p');
          statusEl.className = 'text-xs text-text-muted mt-0.5';
          if (!t.linked) {
            statusEl.textContent = 'Not linked — link in Settings';
          } else if (mapping?.tracker_manga_id) {
            statusEl.textContent = `Mapped to ID: ${mapping.tracker_manga_id}`;
          } else {
            statusEl.textContent = 'Linked but not mapped to this manga';
          }
          info.appendChild(statusEl);
          row.appendChild(info);

          const btnGroup = document.createElement('div');
          btnGroup.className = 'flex items-center gap-2 shrink-0';

          if (t.linked && !mapping?.tracker_manga_id) {
            const searchBtn = document.createElement('button');
            searchBtn.type = 'button';
            searchBtn.className = 'btn-ghost btn-sm';
            searchBtn.textContent = 'Search & Link';
            searchBtn.addEventListener('click', async () => {
              const query = prompt(`Search ${t.name} for manga title:`);
              if (!query) return;
              try {
                const results = await api.searchTrackerManga(t.id, query);
                if (!results.length) { alert('No results found.'); return; }
                const choice = prompt(
                  results.map((r, i) => `${i + 1}. ${r.title} (${r.tracker_manga_id})`).join('\n') +
                  '\n\nEnter number to link:'
                );
                const idx = parseInt(choice, 10) - 1;
                if (idx >= 0 && idx < results.length) {
                  await api.setTrackerMapping(_dbId, t.id, results[idx].tracker_manga_id);
                  statusEl.textContent = `Mapped to ID: ${results[idx].tracker_manga_id}`;
                }
              } catch (err) {
                alert('Search failed: ' + (err.message || err));
              }
            });
            btnGroup.appendChild(searchBtn);
          }

          if (t.linked && mapping?.tracker_manga_id) {
            const syncBtn = document.createElement('button');
            syncBtn.type = 'button';
            syncBtn.className = 'btn-ghost btn-sm';
            syncBtn.textContent = 'Sync';
            syncBtn.addEventListener('click', async () => {
              syncBtn.disabled = true;
              syncBtn.textContent = 'Syncing...';
              try {
                await api.syncMangaTrackers(_dbId);
                syncBtn.textContent = 'Done';
                setTimeout(() => { syncBtn.textContent = 'Sync'; }, 2000);
              } catch (err) {
                syncBtn.textContent = 'Failed';
                setTimeout(() => { syncBtn.textContent = 'Sync'; }, 2000);
              } finally { syncBtn.disabled = false; }
            });
            btnGroup.appendChild(syncBtn);

            const unlinkBtn = document.createElement('button');
            unlinkBtn.type = 'button';
            unlinkBtn.className = 'btn-ghost btn-sm text-danger';
            unlinkBtn.textContent = 'Unmap';
            unlinkBtn.addEventListener('click', async () => {
              await api.deleteTrackerMapping(_dbId, t.id);
              statusEl.textContent = 'Linked but not mapped to this manga';
            });
            btnGroup.appendChild(unlinkBtn);
          }

          row.appendChild(btnGroup);
          extBody.appendChild(row);
        }
      })
      .catch(() => {
        extBody.textContent = 'Failed to load tracker info.';
      });
  }

  // ── 2. Filters & Preferences ────────────────────────────────────────────────

  if (hasPermission('library:manage')) {
    const [cats, mangaCats, rules, scanlatorPrefs] = await Promise.allSettled([
      api.getCategories(),
      api.getMangaCategories(_dbId),
      api.getDownloadRules(_dbId),
      api.getScanlatorPrefs(_dbId),
    ]).then(r => r.map(s => s.status === 'fulfilled' ? s.value : []));
    // Keep module-level copy in sync (may override the eagerly-fetched copy with
    // a more recent snapshot now that we're in the Manage tab).
    _scanlatorPrefs = Array.isArray(scanlatorPrefs) ? scanlatorPrefs : [];

    const section = document.createElement('div');
    section.className = 'flex flex-col gap-3';
    section.appendChild(mkSectionHeader('Filters & Preferences', 'Control how chapters are organised, filtered, and prioritised.'));

    const catsCard = mkTitledCard('Categories', 'Assign this manga to categories to keep your library organised. Toggle a category to add or remove it.');
    _renderCategoriesBody(catsCard, cats, mangaCats);
    section.appendChild(catsCard);

    const rulesCard = mkTitledCard('Download Filters', 'Controls which chapters are automatically downloaded during scans. Rules are applied when new chapters are found.');
    _renderRulesBody(rulesCard, rules);
    section.appendChild(rulesCard);

    const prefsCard = mkTitledCard('Scanlator Preferences', 'Priority and block settings for scanlators. Affects both auto-download and reader navigation.');
    _renderScanlatorBody(prefsCard, scanlatorPrefs, _scanlatorMode);
    section.appendChild(prefsCard);

    contentEl.appendChild(section);
  }

  // ── 3. Danger Zone ──────────────────────────────────────────────────────────

  const hasDangerSection =
    (hasPermission('library:manage') && _sid) ||
    hasPermission('library:delete');

  if (hasDangerSection) {
    const section = document.createElement('div');
    section.className = 'flex flex-col gap-3';
    section.appendChild(mkSectionHeader('Danger Zone', 'These actions are difficult or impossible to reverse. Proceed with care.'));

    const card = mkCard();

    if (hasPermission('library:manage') && _sid) {
      const migrateBtn = document.createElement('button');
      migrateBtn.type = 'button';
      migrateBtn.className = 'btn-ghost btn-sm';
      migrateBtn.textContent = 'Migrate';
      migrateBtn.addEventListener('click', () => {
        const coverUrl = api.getMangaCoverUrl(_dbId);
        _unmountMigration = mountMigrationDialogue({
          dbId: _dbId,
          currentSourceId: _sid,
          currentSourceName: _mangaData?.source_name ?? '',
          currentTitle: _mangaData?.title ?? '',
          currentCoverUrl: coverUrl,
          onComplete: (newSid, newMid) => { _unmountMigration?.(); navigate(`/source/${newSid}/manga/${encodeURIComponent(newMid)}`); },
          onClose: () => { _unmountMigration?.(); _unmountMigration = null; },
        });
      });
      card.appendChild(mkItem(mkRow('Migrate source', 'Move this manga to a different source plugin', migrateBtn)));
    }

    if (hasPermission('library:delete')) {
      const removeBtn = document.createElement('button');
      removeBtn.type = 'button';
      removeBtn.className = 'btn-danger btn-sm';
      removeBtn.textContent = 'Remove';
      removeBtn.addEventListener('click', async () => {
        const confirmed = await confirmDialog({
          title: 'Remove from Library?',
          message: 'This will permanently remove this manga and all downloaded chapters. This cannot be undone.',
          confirmLabel: 'Remove',
          danger: true,
        });
        if (!confirmed) return;
        removeBtn.disabled = true;
        try { await api.deleteManga(_dbId); navigate('/'); }
        catch { removeBtn.disabled = false; }
      });
      card.appendChild(mkItem(mkRow('Remove from Library', 'Permanently deletes all chapter data for this manga', removeBtn)));
    }

    section.appendChild(card);
    contentEl.appendChild(section);
  }
}

function _renderCategoriesBody(body, allCats, mangaCats) {
  const memberIds = new Set((Array.isArray(mangaCats) ? mangaCats : []).map(c => c.id ?? c));
  const all = Array.isArray(allCats) ? allCats : [];

  if (all.length === 0) {
    body.appendChild(createEmptyState({ title: 'No categories. Create some in Settings.' }));
    return;
  }

  const chips = document.createElement('div');
  chips.className = 'flex flex-wrap gap-2 p-1';

  const _render = () => {
    chips.innerHTML = '';
    for (const cat of all) {
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.className = memberIds.has(cat.id) ? 'chip chip-active' : 'chip';
      btn.textContent = cat.name;
      btn.setAttribute('aria-pressed', String(memberIds.has(cat.id)));
      btn.addEventListener('click', async () => {
        if (memberIds.has(cat.id)) memberIds.delete(cat.id); else memberIds.add(cat.id);
        try { await api.setMangaCategories(_dbId, [...memberIds]); } catch { /* revert */ }
        _render();
      });
      chips.appendChild(btn);
    }
  };
  _render();
  body.appendChild(chips);
}

function _renderRulesBody(body, initialRules) {
  let rules = Array.isArray(initialRules) ? [...initialRules] : [];

  const wrap = document.createElement('div');
  wrap.className = 'flex flex-col gap-3';
  body.appendChild(wrap);

  /** Returns a human-readable label for a rule kind. */
  function _ruleLabel(kind) {
    if (typeof kind === 'string') return kind; // ExcludeFractional
    const [k, v] = Object.entries(kind)[0] ?? ['', ''];
    const labels = {
      LanguageInclude: `Language = ${v}`,
      LanguageExclude: `Language ≠ ${v}`,
      TitleContains: `Title contains "${v}"`,
      TitleExcludes: `Title excludes "${v}"`,
      ChapterNumberMin: `Chapter ≥ ${v}`,
      ChapterNumberMax: `Chapter ≤ ${v}`,
      MaxAgeDays: `Max age: ${v} days`,
      PublishedAfter: `Published after ${new Date(Number(v) * 1000).toLocaleDateString()}`,
    };
    return labels[k] ?? `${k}: ${v}`;
  }

  // Hoist language options outside rerender so we don't re-fetch on every change.
  let langOptions = /** @type {Array<{id:number,name:string}>} */ ([]);
  let langCmbVal = '';
  /** @type {HTMLDivElement|null} */ let langCmbMount = null;

  const _renderLangCmb = () => {
    if (!langCmbMount) return;
    // Filter out languages already present in rules
    const used = new Set(
      rules
        .filter(r => typeof r.kind === 'object' && ('LanguageInclude' in r.kind || 'LanguageExclude' in r.kind))
        .map(r => /** @type {string} */ (Object.values(r.kind)[0]))
    );
    const opts = langOptions.filter(o => !used.has(o.name));
    // Reset selection if the previously chosen lang is no longer available
    if (langCmbVal && !opts.some(o => o.name === langCmbVal)) langCmbVal = '';
    render(html`<${Combobox}
      options=${opts}
      value=${opts.find(o => o.name === langCmbVal)?.id ?? null}
      onChange=${(id) => { langCmbVal = opts.find(o => o.id === id)?.name ?? ''; }}
      placeholder="Select language…"
    />`, langCmbMount);
  };

  api.getChapterLanguages(_dbId).then(langs => {
    langOptions = (Array.isArray(langs) ? langs : []).map((l, i) => ({ id: i, name: l }));
    _renderLangCmb();
  }).catch(() => { });

  const rerender = () => {
    wrap.innerHTML = '';

    if (rules.length > 0) {
      const ul = document.createElement('ul');
      ul.className = 'flex flex-col divide-y divide-border-subtle';
      for (const rule of rules) {
        const li = document.createElement('li');
        li.className = 'flex items-center justify-between gap-2 py-2';
        li.innerHTML = `
          <span class="text-sm text-text">${escapeHtml(_ruleLabel(rule.kind))}</span>
          <button class="btn-icon text-danger js-rm" data-id="${rule.id}" aria-label="Remove rule">${iconX}</button>
        `;
        li.querySelector('.js-rm')?.addEventListener('click', async (e) => {
          const id = Number(/** @type {HTMLElement} */(e.currentTarget).dataset.id);
          try {
            await api.deleteDownloadRule(id);
            rules = rules.filter(r => r.id !== id);
            rerender();
          } catch { /* ignore */ }
        });
        ul.appendChild(li);
      }
      wrap.appendChild(ul);
    } else {
      wrap.appendChild(createEmptyState({ title: 'No download filters.' }));
    }

    // Add form
    const form = document.createElement('div');
    form.className = 'flex flex-wrap items-center gap-2 mt-2';
    form.innerHTML = `
      <select class="input w-auto text-sm js-rule-type">
        <optgroup label="Language">
          <option value="LanguageInclude">Language include</option>
          <option value="LanguageExclude">Language exclude</option>
        </optgroup>
        <optgroup label="Title">
          <option value="TitleContains">Title contains</option>
          <option value="TitleExcludes">Title excludes</option>
        </optgroup>
        <optgroup label="Chapter number">
          <option value="ChapterNumberMin">Chapter ≥ (min)</option>
          <option value="ChapterNumberMax">Chapter ≤ (max)</option>
        </optgroup>
        <optgroup label="Other">
          <option value="ExcludeFractional">Exclude fractional chapters</option>
          <option value="MaxAgeDays">Max age (days)</option>
          <option value="PublishedAfter">Published after (epoch)</option>
        </optgroup>
      </select>
      <div class="js-rule-cmb-wrap flex-1 min-w-[150px]" style="display:none"></div>
      <input type="text" class="input flex-1 min-w-[100px] text-sm js-rule-val" placeholder="Value…" />
      <button type="button" class="btn-ghost btn-sm js-rule-add">Add</button>
    `;
    const typeEl = /** @type {HTMLSelectElement} */ (form.querySelector('.js-rule-type'));
    const valEl = /** @type {HTMLInputElement} */ (form.querySelector('.js-rule-val'));
    // Refresh the mount reference after innerHTML rebuild
    langCmbMount = /** @type {HTMLDivElement} */ (form.querySelector('.js-rule-cmb-wrap'));

    // Hide value input for no-value and combobox rules
    typeEl.addEventListener('change', () => {
      const type = typeEl.value;
      if (type === 'ExcludeFractional') {
        valEl.style.display = 'none';
        if (langCmbMount) langCmbMount.style.display = 'none';
      } else if (type === 'LanguageInclude' || type === 'LanguageExclude') {
        valEl.style.display = 'none';
        if (langCmbMount) { langCmbMount.style.display = ''; _renderLangCmb(); }
      } else {
        valEl.style.display = '';
        if (langCmbMount) langCmbMount.style.display = 'none';
      }
    });
    // Trigger change initially to set correct state based on default select value
    typeEl.dispatchEvent(new Event('change'));

    form.querySelector('.js-rule-add')?.addEventListener('click', async () => {
      const type = typeEl.value;
      const isCmb = type === 'LanguageInclude' || type === 'LanguageExclude';
      const valText = isCmb ? langCmbVal : valEl.value.trim();
      if ((!isCmb && type !== 'ExcludeFractional' && !valText) || (isCmb && !valText)) return;

      const kind = type === 'ExcludeFractional'
        ? 'ExcludeFractional'
        : {
          [type]: ['ChapterNumberMin', 'ChapterNumberMax', 'MaxAgeDays', 'PublishedAfter'].includes(type)
            ? Number(valText) : valText
        };
      try {
        const newRule = await api.addDownloadRule(_dbId, kind);
        if (newRule && newRule.id) rules.push({ id: newRule.id, manga_id: _dbId, kind });
        valEl.value = '';
        langCmbVal = '';
        rerender();
      } catch (e) {
        showToast(e?.hint ?? e?.message ?? 'Failed to add rule', { type: 'error' });
      }
    });
    wrap.appendChild(form);
    // Render combobox into the fresh mount point
    _renderLangCmb();
  };
  rerender();
}

function _renderScanlatorBody(body, initialPrefs, initialMode) {
  let prefs = Array.isArray(initialPrefs) ? [...initialPrefs] : [];
  let mode = initialMode ?? 'priority';

  const wrap = document.createElement('div');
  wrap.className = 'flex flex-col gap-3';
  body.appendChild(wrap);

  const rerender = () => {
    // Keep module-level copy in sync so Read button / Download All always
    // see the latest preferences even before getContinueReading is called.
    _scanlatorPrefs = [...prefs];
    wrap.innerHTML = '';

    // Mode selector
    const modeRow = document.createElement('div');
    modeRow.className = 'flex items-center gap-2';
    modeRow.innerHTML = `
      <span class="text-sm font-medium text-text">Mode:</span>
      <button type="button" class="btn-sm js-mode-priority ${mode === 'priority' ? 'btn-primary' : 'btn-ghost'}">Priority</button>
      <button type="button" class="btn-sm js-mode-whitelist ${mode === 'whitelist' ? 'btn-primary' : 'btn-ghost'}">Whitelist</button>
    `;
    const modeDesc = document.createElement('p');
    modeDesc.className = 'text-xs text-text-muted';
    modeDesc.textContent = mode === 'priority'
      ? 'All scanlators accepted. Use priority to prefer, and block to exclude.'
      : 'Only listed scanlators are accepted.';
    modeRow.querySelector('.js-mode-priority')?.addEventListener('click', async () => {
      try { await api.setScanlatorMode(_dbId, 'priority'); mode = 'priority'; _scanlatorMode = 'priority'; rerender(); } catch { /* ignore */ }
    });
    modeRow.querySelector('.js-mode-whitelist')?.addEventListener('click', async () => {
      try { await api.setScanlatorMode(_dbId, 'whitelist'); mode = 'whitelist'; _scanlatorMode = 'whitelist'; rerender(); } catch { /* ignore */ }
    });
    wrap.appendChild(modeRow);
    wrap.appendChild(modeDesc);

    // Sort by priority descending so highest priority is shown first
    const sortedPrefs = [...prefs].sort((a, b) => (b.priority ?? 0) - (a.priority ?? 0));

    /** Reassign priorities based on current order and persist to server */
    async function _savePriorityOrder() {
      // Index 0 = highest priority; assign priority = (total - index)
      for (let i = 0; i < sortedPrefs.length; i++) {
        const pref = sortedPrefs[i];
        const newPriority = sortedPrefs.length - i;
        if (pref.priority !== newPriority) {
          pref.priority = newPriority;
          api.setScanlatorPref(_dbId, pref.scanlator, newPriority, pref.blocked).catch(() => {});
        }
      }
      // Sync back into prefs
      for (const sp of sortedPrefs) {
        const p = prefs.find(p => p.id === sp.id);
        if (p) p.priority = sp.priority;
      }
      _scanlatorPrefs = [...prefs];
    }

    if (sortedPrefs.length > 0) {
      const ul = document.createElement('ul');
      ul.className = 'flex flex-col divide-y divide-border-subtle';

      /** @type {HTMLElement|null} */ let dragSrc = null;

      for (const pref of sortedPrefs) {
        const li = document.createElement('li');
        li.className = 'flex items-center gap-3 py-2 cursor-grab active:cursor-grabbing';
        li.draggable = true;
        li.dataset.prefId = String(pref.id);

        const blockedClass = pref.blocked ? 'text-danger line-through' : 'text-text';
        li.innerHTML = `
          <span class="text-text-muted shrink-0 cursor-grab select-none [&_svg]:w-4 [&_svg]:h-4" aria-hidden="true">
            <svg viewBox="0 0 24 24" fill="currentColor" xmlns="http://www.w3.org/2000/svg">
              <circle cx="9" cy="6" r="1.5"/><circle cx="15" cy="6" r="1.5"/>
              <circle cx="9" cy="12" r="1.5"/><circle cx="15" cy="12" r="1.5"/>
              <circle cx="9" cy="18" r="1.5"/><circle cx="15" cy="18" r="1.5"/>
            </svg>
          </span>
          <span class="flex-1 text-sm ${blockedClass}">${escapeHtml(pref.scanlator || '* (Any scanlator)')}</span>
          <div class="flex items-center gap-2 shrink-0">
            ${mode === 'priority' ? `<button type="button" class="btn-sm ${pref.blocked ? 'btn-danger' : 'btn-ghost'} js-pref-block" title="${pref.blocked ? 'Unblock' : 'Block'}">${pref.blocked ? 'Blocked' : 'Block'}</button>` : ''}
            <button class="btn-icon text-danger js-pref-rm" data-id="${pref.id}" aria-label="Remove ${escapeHtml(pref.scanlator)}">${iconX}</button>
          </div>
        `;

        // Drag-and-drop handlers
        li.addEventListener('dragstart', (e) => {
          dragSrc = li;
          e.dataTransfer?.setData('text/plain', String(pref.id));
          li.classList.add('opacity-50');
        });
        li.addEventListener('dragend', () => {
          dragSrc = null;
          li.classList.remove('opacity-50');
          ul.querySelectorAll('li[data-pref-id]').forEach(el => el.classList.remove('border-t-2', 'border-t-accent'));
        });
        li.addEventListener('dragover', (e) => {
          e.preventDefault();
          if (dragSrc && dragSrc !== li) {
            ul.querySelectorAll('li[data-pref-id]').forEach(el => el.classList.remove('border-t-2', 'border-t-accent'));
            li.classList.add('border-t-2', 'border-t-accent');
          }
        });
        li.addEventListener('drop', (e) => {
          e.preventDefault();
          if (!dragSrc || dragSrc === li) return;
          const srcId = Number(dragSrc.dataset.prefId);
          const tgtId = Number(li.dataset.prefId);
          const srcIdx = sortedPrefs.findIndex(p => p.id === srcId);
          const tgtIdx = sortedPrefs.findIndex(p => p.id === tgtId);
          if (srcIdx < 0 || tgtIdx < 0) return;
          const [moved] = sortedPrefs.splice(srcIdx, 1);
          sortedPrefs.splice(tgtIdx, 0, moved);
          _savePriorityOrder().then(() => rerender());
        });

        li.querySelector('.js-pref-rm')?.addEventListener('click', async (e) => {
          const id = Number(/** @type {HTMLElement} */(e.currentTarget).dataset.id);
          try {
            await api.deleteScanlatorPref(id);
            prefs = prefs.filter(p => p.id !== id);
            _scanlatorPrefs = [...prefs];
            rerender();
          } catch { /* ignore */ }
        });
        li.querySelector('.js-pref-block')?.addEventListener('click', async () => {
          const newBlocked = !pref.blocked;
          try {
            await api.setScanlatorPref(_dbId, pref.scanlator, pref.priority, newBlocked);
            pref.blocked = newBlocked;
            _scanlatorPrefs = [...prefs];
            rerender();
          } catch { /* ignore */ }
        });
        ul.appendChild(li);
      }

      // Fallback row (priority mode only — not draggable)
      if (mode === 'priority') {
        const fallbackLi = document.createElement('li');
        fallbackLi.className = 'flex items-center gap-3 py-2 opacity-50';
        fallbackLi.title = 'Always present as the lowest-priority fallback — cannot be removed';
        fallbackLi.innerHTML = `
          <span class="shrink-0 [&_svg]:w-4 [&_svg]:h-4 text-transparent" aria-hidden="true">
            <svg viewBox="0 0 24 24" fill="currentColor"><circle cx="9" cy="12" r="1.5"/></svg>
          </span>
          <span class="flex-1 text-sm text-text-muted italic">All scanlators (fallback)</span>
        `;
        ul.appendChild(fallbackLi);
      }
      wrap.appendChild(ul);
    } else {
      const emptyWrap = document.createElement('div');
      emptyWrap.className = 'flex flex-col gap-0';
      emptyWrap.appendChild(createEmptyState({ title: mode === 'priority' ? 'No preferences set — all scanlators accepted equally.' : 'No whitelisted scanlators.' }));
      // Fallback row only in priority mode
      if (mode === 'priority') {
        const fallbackLi = document.createElement('div');
        fallbackLi.className = 'flex items-center gap-3 py-2 opacity-50';
        fallbackLi.title = 'Always present as the lowest-priority fallback — cannot be removed';
        fallbackLi.innerHTML = `
          <span class="shrink-0 [&_svg]:w-4 [&_svg]:h-4 text-transparent" aria-hidden="true">
            <svg viewBox="0 0 24 24" fill="currentColor"><circle cx="9" cy="12" r="1.5"/></svg>
          </span>
          <span class="flex-1 text-sm text-text-muted italic">All scanlators (fallback)</span>
        `;
        emptyWrap.appendChild(fallbackLi);
      }
      wrap.appendChild(emptyWrap);
    }

    const form = document.createElement('div');
    form.className = 'flex flex-wrap items-center gap-2 mt-2';
    form.innerHTML = `
      <div class="js-sc-cmb-wrap flex-1 min-w-[200px]"></div>
      <button type="button" class="btn-ghost btn-sm js-sc-add">Add</button>
    `;
    scCmbMount = /** @type {HTMLDivElement} */ (form.querySelector('.js-sc-cmb-wrap'));

    form.querySelector('.js-sc-add')?.addEventListener('click', async () => {
      const name = scCmbVal.trim();
      if (!name) return;
      // New items go to the top (highest priority = current length + 1)
      const priority = prefs.length + 1;
      try {
        const finalName = name === '* (Any scanlator)' ? '' : name;
        await api.setScanlatorPref(_dbId, finalName, priority, false);
        const existing = prefs.find(p => p.scanlator === finalName);
        if (existing) { existing.priority = priority; existing.blocked = false; }
        else prefs.push({ id: Date.now(), manga_id: _dbId, scanlator: finalName, priority, blocked: false });
        scCmbVal = '';
        _scanlatorPrefs = [...prefs];
        rerender();
      } catch (e) {
        showToast(e?.hint ?? e?.message ?? 'Failed to add preference', { type: 'error' });
      }
    });
    wrap.appendChild(form);
    _renderScCmb();
  };

  // Hoist scanlator API call outside rerender so it fires only once.
  let scOptions = /** @type {Array<{id:number,name:string}>} */ ([{ id: -1, name: '* (Any scanlator)' }]);
  let scCmbVal = '';
  /** @type {HTMLDivElement|null} */ let scCmbMount = null;

  const _renderScCmb = () => {
    if (!scCmbMount) return;
    const used = new Set(prefs.map(p => p.scanlator === '' ? '* (Any scanlator)' : p.scanlator));
    const opts = scOptions.filter(o => !used.has(o.name));
    if (scCmbVal && !opts.some(o => o.name === scCmbVal)) scCmbVal = '';
    render(html`<${Combobox}
      options=${opts}
      value=${opts.find(o => o.name === scCmbVal)?.id ?? null}
      onChange=${(id) => { scCmbVal = opts.find(o => o.id === id)?.name ?? ''; }}
      placeholder="Select scanlator…"
    />`, scCmbMount);
  };

  api.getChapterScanlators(_dbId).then(scanlators => {
    scOptions = [
      { id: -1, name: '* (Any scanlator)' },
      ...(Array.isArray(scanlators) ? scanlators : []).map((s, i) => ({ id: i, name: s })),
    ];
    _renderScCmb();
  }).catch(() => { });

  rerender();
}

// ── Destroy ───────────────────────────────────────────────────────────────────

export function destroy(container) {
  _abort?.abort();
  _abort = null;
  if (_sseListener) { window.removeEventListener('kani:sse', _sseListener); _sseListener = null; }
  _destroyPagination?.();
  _destroyPagination = null;
  if (_chapterResizeListener) { window.removeEventListener('resize', _chapterResizeListener); _chapterResizeListener = null; }
  if (_manageResizeListener) { window.removeEventListener('resize', _manageResizeListener); _manageResizeListener = null; }
  if (_heroResizeListener) { window.removeEventListener('resize', _heroResizeListener); _heroResizeListener = null; }
  if (_listContainerEl) { render(null, _listContainerEl); _listContainerEl = null; }
  _chapters = [];
  _chaptersHasMore = false;
  _chaptersLoading = false;
  _allRemoteChapters = null;
  _unmountMigration?.();
  _unmountMigration = null;
  _activeTab = 'chapters';
  _manageMounted = false;
  _contentSection = null;
  _btnGroupEl = null;
  container.innerHTML = '';
}
