// @ts-check
// Reader page — full-screen chapter reader with page-by-page and scroll modes.

import * as api from '../api.js';
import { iconChevronLeft, iconChevronRight, iconX, iconMenu } from '../icons.js';
import { navigate } from '../router.js';
import { getLocal, setLocal, getLocalJson, setLocalJson } from '../utils.js';
import { getState, subscribe } from '../state.js';
import { registerShortcuts } from '../shortcuts.js';

const BTN_ACTIVE   = 'bg-surface-2 text-text';
const BTN_INACTIVE = 'text-muted hover:bg-surface-2 hover:text-text';

/** @type {(() => void) | null} */
let _destroyFn = null;

// Carries bar-visibility state across chapter navigations (touch only).
let _pendingBarsVisible = false;

/** @param {HTMLElement} container @param {{ id?: string }} params */
export async function init(container, { id }) {
  const chapterId = Number(id);
  document.title = 'Reader - Kani';

  /** @type {Array<() => void>} */
  const _cleanup = [];

  document.body.classList.add('overflow-hidden');

  // Unified indicator bar: collapses to 4px strip, expands to 56px.
  // Uses overflow-hidden + fixed inner layout so the strip colours are always
  // the bottom slice of the same element — no separate strip div needed.
  container.innerHTML = `
    <div id="reader-root" class="fixed inset-0 bg-black z-40 flex flex-col select-none overflow-hidden">

      <!-- Mobile-only top bar (hidden on md+). Slides in from top when bars visible. -->
      <div id="reader-top"
        class="md:hidden absolute top-0 inset-x-0 z-30 flex items-center gap-2 px-3 h-14 bg-surface border-b border-border/60 transition-transform duration-150"
        style="transform: translateY(-100%)">
        <button id="reader-back-mobile"
          class="btn-icon shrink-0"
          aria-label="Back">${iconChevronLeft}</button>
        <span id="reader-title-mobile" class="flex-1 text-sm font-medium text-text truncate"></span>
        <button id="reader-menu-btn"
          class="btn-icon shrink-0"
          aria-label="Open menu">${iconMenu}</button>
      </div>

      <!-- Page viewer -->
      <div id="reader-pages"
        class="flex-1 overflow-y-auto overflow-x-hidden flex flex-col items-center"
        style="overscroll-behavior: none"
        tabindex="0" aria-label="Chapter pages" aria-live="polite">
        <div class="flex items-center justify-center min-h-full w-full">
          <p class="text-muted text-sm">Loading…</p>
        </div>
      </div>

      <!-- Mini progress strip — always visible, 4px, z-20, pointer-events-none.
           Horizontal padding matches the segment area of the full bar:
           px-4 (16) + w-6 (24) + gap-3 (12) = 52px on each side. -->
      <div id="reader-mini-strip"
        class="absolute bottom-0 inset-x-0 z-20 h-1 flex pointer-events-none"
        style="padding-left:52px;padding-right:52px">
      </div>

      <!-- Full indicator bar — slides up from bottom on hover/tap, z-21.
           Sits above the mini strip and overlays the page content. -->
      <div id="reader-full-bar"
        class="absolute bottom-0 inset-x-0 flex items-center gap-3 px-4 h-14 bg-surface/90 backdrop-blur-sm border-t border-border/40 transition-transform duration-150"
        style="z-index:21; transform:translateY(100%)">
        <span id="reader-seg-left"
          class="text-xs text-muted w-6 text-right shrink-0 tabular-nums select-none">—</span>
        <div id="reader-segs"
          class="flex flex-1 gap-0.5 h-7 items-stretch pointer-events-none"></div>
        <span id="reader-seg-right"
          class="text-xs text-muted w-6 shrink-0 tabular-nums select-none">—</span>
      </div>

      <!-- Hover zone — fine-pointer proximity detection. z-9 so mouse events pass
           through the pointer-events-none mini strip (z-20) down to this zone. -->
      <div id="reader-bar-hover"
        class="absolute bottom-0 inset-x-0 pointer-events-none"
        style="height:64px;z-index:9">
      </div>

      <!-- Side panel backdrop -->
      <div id="reader-side-backdrop"
        class="hidden absolute inset-0 bg-black/50 z-30">
      </div>

      <!-- Side panel — left on desktop (md+), right on mobile.
           JS sets the correct side on init. -->
      <div id="reader-side-panel"
        class="absolute top-0 bottom-0 w-72 bg-surface flex flex-col shadow-lg border-border z-40 transition-transform duration-150"
        style="transform: translateX(-100%); left: 0">

        <!-- Panel header -->
        <div class="flex items-center gap-2 px-3 h-14 border-b border-border shrink-0">
          <!-- Desktop back-to-manga button (hidden on mobile) -->
          <button id="reader-side-back"
            class="hidden md:flex btn-icon shrink-0"
            aria-label="Back to manga">${iconChevronLeft}</button>
          <span id="reader-side-title" class="flex-1 text-sm font-medium text-muted truncate">—</span>
          <button id="reader-side-close"
            class="btn-icon shrink-0"
            aria-label="Close menu">${iconX}</button>
        </div>

        <div class="flex flex-col flex-1 overflow-y-auto">

          <!-- Mobile back button (full-width, only on mobile) -->
          <div class="md:hidden px-3 py-3 border-b border-border shrink-0">
            <button id="reader-side-back-mobile"
              class="btn-ghost w-full flex items-center justify-center gap-1">
              ${iconChevronLeft} Back to manga
            </button>
          </div>

          <!-- Prev / Next chapter -->
          <div class="px-3 py-3 flex gap-2 border-b border-border shrink-0">
            <button id="reader-side-prev"
              class="btn-ghost flex-1 flex items-center justify-center gap-1"
              disabled>${iconChevronLeft} Prev</button>
            <button id="reader-side-next"
              class="btn-ghost flex-1 flex items-center justify-center gap-1"
              disabled>Next ${iconChevronRight}</button>
          </div>

          <!-- Reading mode -->
          <div class="px-3 py-4 border-b border-border">
            <p class="text-xs font-medium text-muted uppercase tracking-wide mb-2">Reading Mode</p>
            <div class="flex gap-2">
              <button id="reader-mode-scroll"
                class="flex-1 text-sm px-3 py-2 rounded-md transition-colors"
                aria-pressed="false">Scroll</button>
              <button id="reader-mode-paged"
                class="flex-1 text-sm px-3 py-2 rounded-md transition-colors"
                aria-pressed="false">Paged</button>
            </div>
          </div>

          <!-- Options -->
          <div class="px-3 py-4 flex flex-col gap-3">
            <p class="text-xs font-medium text-muted uppercase tracking-wide">Options</p>
            <label class="flex items-center justify-between gap-3 cursor-pointer">
              <span class="text-sm text-text">Smooth scroll</span>
              <label class="kani-toggle" aria-label="Smooth scroll">
                <input id="reader-smooth-input" type="checkbox" class="kani-toggle__input">
                <span class="kani-toggle__track"></span>
              </label>
            </label>
            <label class="flex items-center justify-between gap-3 cursor-pointer" id="reader-double-row">
              <span class="text-sm text-text">Double page</span>
              <label class="kani-toggle" aria-label="Double page spread">
                <input id="reader-double-input" type="checkbox" class="kani-toggle__input">
                <span class="kani-toggle__track"></span>
              </label>
            </label>
            <label class="flex items-center justify-between gap-3 cursor-pointer" id="reader-spread-row" style="display:none">
              <span class="text-sm text-text">Auto-combine spreads</span>
              <label class="kani-toggle" aria-label="Auto-combine split page spreads">
                <input id="reader-spread-input" type="checkbox" class="kani-toggle__input">
                <span class="kani-toggle__track"></span>
              </label>
            </label>
            <div>
              <p class="text-xs text-muted mb-2">Reading direction</p>
              <div class="flex gap-2">
                <button id="reader-dir-rtl" class="flex-1 text-sm px-2 py-1.5 rounded-md transition-colors" aria-pressed="false">RTL</button>
                <button id="reader-dir-ltr" class="flex-1 text-sm px-2 py-1.5 rounded-md transition-colors" aria-pressed="false">LTR</button>
              </div>
            </div>
            <div>
              <p class="text-xs text-muted mb-2">Image fit</p>
              <div class="flex gap-2">
                <button id="reader-fit-both"  class="flex-1 text-sm px-2 py-1.5 rounded-md transition-colors" aria-pressed="false">Both</button>
                <button id="reader-fit-width" class="flex-1 text-sm px-2 py-1.5 rounded-md transition-colors" aria-pressed="false">Width</button>
                <button id="reader-fit-height" class="flex-1 text-sm px-2 py-1.5 rounded-md transition-colors" aria-pressed="false">Height</button>
              </div>
            </div>
          </div>

        </div>
      </div>

    </div>
  `;

  const topBar       = /** @type {HTMLElement}       */ (container.querySelector('#reader-top'));
  const pagesEl      = /** @type {HTMLElement}       */ (container.querySelector('#reader-pages'));
  const miniStrip    = /** @type {HTMLElement}       */ (container.querySelector('#reader-mini-strip'));
  const fullBar      = /** @type {HTMLElement}       */ (container.querySelector('#reader-full-bar'));
  const barHover     = /** @type {HTMLElement}       */ (container.querySelector('#reader-bar-hover'));
  const segLeft      = /** @type {HTMLElement}       */ (container.querySelector('#reader-seg-left'));
  const segsEl       = /** @type {HTMLElement}       */ (container.querySelector('#reader-segs'));
  const segRight     = /** @type {HTMLElement}       */ (container.querySelector('#reader-seg-right'));
  const menuBtn      = /** @type {HTMLButtonElement} */ (container.querySelector('#reader-menu-btn'));
  const backdrop     = /** @type {HTMLElement}       */ (container.querySelector('#reader-side-backdrop'));
  const sidePanel    = /** @type {HTMLElement}       */ (container.querySelector('#reader-side-panel'));
  const sideClose    = /** @type {HTMLButtonElement} */ (container.querySelector('#reader-side-close'));
  const sideBack     = /** @type {HTMLButtonElement} */ (container.querySelector('#reader-side-back'));
  const sideBackMob  = /** @type {HTMLButtonElement} */ (container.querySelector('#reader-side-back-mobile'));
  const sidePrev     = /** @type {HTMLButtonElement} */ (container.querySelector('#reader-side-prev'));
  const sideNext     = /** @type {HTMLButtonElement} */ (container.querySelector('#reader-side-next'));
  const sideTitle    = /** @type {HTMLElement}       */ (container.querySelector('#reader-side-title'));
  const backMobile   = /** @type {HTMLButtonElement} */ (container.querySelector('#reader-back-mobile'));
  const titleMobile  = /** @type {HTMLElement}       */ (container.querySelector('#reader-title-mobile'));
  const modeScroll   = /** @type {HTMLButtonElement} */ (container.querySelector('#reader-mode-scroll'));
  const modePaged    = /** @type {HTMLButtonElement} */ (container.querySelector('#reader-mode-paged'));
  const smoothInput  = /** @type {HTMLInputElement}  */ (container.querySelector('#reader-smooth-input'));
  const doubleInput  = /** @type {HTMLInputElement}  */ (container.querySelector('#reader-double-input'));
  const doubleRow    = /** @type {HTMLElement}        */ (container.querySelector('#reader-double-row'));
  const spreadInput  = /** @type {HTMLInputElement}  */ (container.querySelector('#reader-spread-input'));
  const spreadRow    = /** @type {HTMLElement}        */ (container.querySelector('#reader-spread-row'));
  const dirRtl       = /** @type {HTMLButtonElement} */ (container.querySelector('#reader-dir-rtl'));
  const dirLtr       = /** @type {HTMLButtonElement} */ (container.querySelector('#reader-dir-ltr'));
  const fitBoth      = /** @type {HTMLButtonElement} */ (container.querySelector('#reader-fit-both'));
  const fitWidth     = /** @type {HTMLButtonElement} */ (container.querySelector('#reader-fit-width'));
  const fitHeight    = /** @type {HTMLButtonElement} */ (container.querySelector('#reader-fit-height'));

  /** @type {string[]} */
  let _pages        = [];
  let _currentPage  = 0;
  let _mode         = /** @type {'scroll'|'paged'} */ (getLocal('kani_reader_mode') === 'paged' ? 'paged' : 'scroll');
  let _smoothScroll = getLocal('kani_reader_smooth') === 'true';
  let _doublePage   = getLocal('kani_reader_double') === 'true';
  let _direction    = /** @type {'rtl'|'ltr'} */ (getLocal('kani_reader_direction') === 'ltr' ? 'ltr' : 'rtl');
  /** @type {'both'|'width'|'height'} */
  const _fitVal = getLocal('kani_reader_fit') ?? '';
  let _fit      = /** @type {'both'|'width'|'height'} */ (
    ['both', 'width', 'height'].includes(_fitVal) ? _fitVal : 'both'
  );
  let _barsVisible  = false;
  let _panelOpen    = false;
  let _isHovering   = false;
  let _hideTimer    = /** @type {ReturnType<typeof setTimeout>|null} */ (null);
  let _mangaId      = /** @type {number|null} */ (null);
  let _progressTimer = /** @type {ReturnType<typeof setTimeout>|null} */ (null);
  let _lastReportedPage = -1;
  let _preloadDone = false;
  let _autoSpread   = getLocal('kani_reader_spread') !== 'false'; // default true
  /**
   * @type {Map<number, {w: number, h: number, edgeMatch?: boolean | null}>}
   * edgeMatch: undefined = not yet checked, null = check in progress, true/false = confirmed result
   */
  const _imgDims    = new Map();
  let _lastLayoutPage = -2;

  /** @type {boolean} */
  let _hasServerAnalysis = false;
  /** @type {Set<number>} */
  let _serverDoublePages = new Set();


  let _dimSaveTimer = /** @type {ReturnType<typeof setTimeout>|null} */ (null);

  function _saveDims() {
    if (_dimSaveTimer) clearTimeout(_dimSaveTimer);
    _dimSaveTimer = setTimeout(() => {
      const entries = [];
      for (const [i, d] of _imgDims) entries.push([i, d.w, d.h]);
      setLocalJson(`kani_dims_${chapterId}`, entries);
    }, 500);
  }

  _cleanup.push(() => { if (_dimSaveTimer) clearTimeout(_dimSaveTimer); });

  function _setDims(idx, w, h) {
    const prev = _imgDims.get(idx);
    _imgDims.set(idx, { w, h, edgeMatch: prev?.edgeMatch });
    _saveDims();
  }


  function _reportProgress() {
    if (_progressTimer) clearTimeout(_progressTimer);
    _progressTimer = setTimeout(() => {
      if (_currentPage !== _lastReportedPage) {
        _lastReportedPage = _currentPage;
        api.setChapterProgress(chapterId, _currentPage).catch(() => {});
      }
    }, 2000);
  }

  _cleanup.push(() => { if (_progressTimer) clearTimeout(_progressTimer); });

  function _maybePreloadNext() {
    if (_preloadDone || !_chapterInfo.next_chapter_id) return;
    const threshold = _mode === 'paged'
      ? _pages.length - 3
      : Math.floor(_pages.length * 0.8);
    if (_currentPage < threshold) return;
    _preloadDone = true;
    const nextId = _chapterInfo.next_chapter_id;
    api.getChapterPages(nextId).then((data) => {
      if (!Array.isArray(data?.pages)) return;
      data.pages.slice(0, 3).forEach((p) => {
        const img = new Image();
        img.src = api.getChapterPageUrl(nextId, p.index);
      });
    }).catch(() => {});
  }
  /** @type {{ prev_chapter_id?: number|null, next_chapter_id?: number|null }} */
  let _chapterInfo  = {};
  /** @type {IntersectionObserver|null} */
  let _scrollObs    = null;
  /** @type {Set<number>} */
  const _loaded     = new Set();
  /** @type {Set<number>} */
  const _failed     = new Set();

  _cleanup.push(() => { _scrollObs?.disconnect(); });

  // ── Helpers ──────────────────────────────────────────────────────────────

  const _isDesktop     = () => window.matchMedia('(min-width: 768px)').matches;
  const _isFinePointer = () => window.matchMedia('(pointer:fine)').matches;

  // Position the side panel (left on desktop, right on mobile).
  function _positionPanel() {
    if (_isDesktop()) {
      sidePanel.style.left  = '0';
      sidePanel.style.right = '';
      sidePanel.style.borderRight = '1px solid var(--color-border)';
      sidePanel.style.borderLeft  = '';
    } else {
      sidePanel.style.right = '0';
      sidePanel.style.left  = '';
      sidePanel.style.borderLeft  = '1px solid var(--color-border)';
      sidePanel.style.borderRight = '';
    }
    if (!_panelOpen) sidePanel.style.transform = _panelClosedTransform();
  }

  const _panelClosedTransform = () =>
    _isDesktop() ? 'translateX(-100%)' : 'translateX(100%)';

  function _navigateToManga() {
    if (_mangaId) navigate(`/manga/${_mangaId}`);
    else history.length > 1 ? history.back() : navigate('/');
  }

  async function _navigateChapter(chId) {
    _pendingBarsVisible = _barsVisible && !_isFinePointer();

    // If the chapter is already downloaded, navigate immediately.
    try {
      await api.getChapterPages(chId);
      navigate(`/reader/${chId}`);
      return;
    } catch { /* not downloaded yet */ }

    // Queue the download (ignore error if already in progress / downloaded).
    try { await api.downloadChapter(chId); } catch { /* already queued or downloading */ }

    // Show overlay and track progress via SSE state.
    let _dlDone = false;

    /** @param {{ totalPages: number, completedPages: number } | null} p */
    function _renderDlOverlay(p) {
      const progressText = p && p.totalPages > 0
        ? `${p.completedPages} / ${p.totalPages} pages`
        : '';
      pagesEl.innerHTML = `
        <div class="flex flex-col items-center justify-center gap-4 min-h-full text-center px-6">
          <svg class="animate-spin w-8 h-8 text-accent" viewBox="0 0 24 24" fill="none">
            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"/>
            <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8v8H4z"/>
          </svg>
          <p class="text-sm text-text">Downloading chapter…</p>
          ${progressText ? `<p class="text-xs text-text-muted js-dl-progress">${progressText}</p>` : '<p class="text-xs text-text-muted js-dl-progress"></p>'}
          <button class="btn-ghost btn-sm js-dl-cancel">Cancel</button>
        </div>
      `;
      pagesEl.querySelector('.js-dl-cancel')?.addEventListener('click', () => {
        _dlDone = true;
        unsub();
        _renderPages();
      }, { once: true });
    }

    _renderDlOverlay(/** @type {any} */ (getState('chaptersProgress').get(chId)) ?? null);

    const unsub = subscribe('chaptersProgress', (/** @type {Map<number,any>} */ map) => {
      if (_dlDone) { unsub(); return; }
      const p = map.get(chId);
      if (!p) return;

      if (p.status === 'completed') {
        _dlDone = true;
        unsub();
        navigate(`/reader/${chId}`);
        return;
      }

      if (p.status === 'failed' || p.status === 'cancelled') {
        _dlDone = true;
        unsub();
        pagesEl.innerHTML = `
          <div class="flex flex-col items-center justify-center gap-4 min-h-full text-center px-6">
            <p class="text-sm text-danger">Download ${p.status}.</p>
            <button class="btn-ghost btn-sm js-dl-retry">Retry</button>
            <button class="btn-ghost btn-sm js-dl-back">Cancel</button>
          </div>
        `;
        pagesEl.querySelector('.js-dl-retry')?.addEventListener('click', () => _navigateChapter(chId), { once: true });
        pagesEl.querySelector('.js-dl-back')?.addEventListener('click', () => _renderPages(), { once: true });
        return;
      }

      // Update progress text in-place (avoid full re-render to prevent losing the cancel listener).
      const progressEl = pagesEl.querySelector('.js-dl-progress');
      if (progressEl && p.totalPages > 0) {
        progressEl.textContent = `${p.completedPages} / ${p.totalPages} pages`;
      }
    });

    _cleanup.push(() => { _dlDone = true; unsub(); });
  }

  // ── Side panel ───────────────────────────────────────────────────────────

  function _openPanel() {
    _panelOpen = true;
    _positionPanel();
    sidePanel.style.transform = 'translateX(0)';
    backdrop.classList.remove('hidden');
    if (_hideTimer) clearTimeout(_hideTimer);
  }

  function _closePanel() {
    _panelOpen = false;
    sidePanel.style.transform = _panelClosedTransform();
    backdrop.classList.add('hidden');
    // On fine-pointer, restart hide timer if not hovering.
    if (_isFinePointer() && !_isHovering && _barsVisible) {
      _hideTimer = setTimeout(_hideBars, 1500);
    }
  }

  menuBtn.addEventListener('click',   (e) => { e.stopPropagation(); _openPanel(); });
  sideClose.addEventListener('click', ()  => _closePanel());
  backdrop.addEventListener('click',  ()  => _closePanel());

  sideBack.addEventListener('click',    () => _navigateToManga());
  sideBackMob.addEventListener('click', () => _navigateToManga());
  backMobile.addEventListener('click',  () => _navigateToManga());

  // ── Indicator bar ────────────────────────────────────────────────────────

  function _showBars() {
    _barsVisible = true;
    fullBar.style.transform = '';
    segsEl.style.pointerEvents = 'auto';
    // Mobile top bar
    if (!_isDesktop()) topBar.style.transform = '';
    if (_hideTimer) clearTimeout(_hideTimer);
    // On fine-pointer, auto-hide only when not hovering and panel is closed.
    if (_isFinePointer() && !_panelOpen && !_isHovering) {
      _hideTimer = setTimeout(_hideBars, 3000);
    }
  }

  function _hideBars() {
    _barsVisible = false;
    fullBar.style.transform = 'translateY(100%)';
    segsEl.style.pointerEvents = 'none';
    topBar.style.transform = 'translateY(-100%)';
    _closePanel();
  }

  // Fine-pointer: hover zone near bottom reveals the full bar; mouseenter on
  // the full bar itself keeps it open while the cursor is over it.
  if (_isFinePointer()) {
    barHover.style.pointerEvents = 'auto';

    const _onEnter = () => {
      _isHovering = true;
      if (_hideTimer) clearTimeout(_hideTimer);
      if (!_barsVisible) _showBars();
    };
    const _onLeave = () => {
      _isHovering = false;
      if (!_panelOpen) _hideTimer = setTimeout(_hideBars, 200);
    };

    barHover.addEventListener('mouseenter', _onEnter);
    barHover.addEventListener('mouseleave', _onLeave);
    fullBar.addEventListener('mouseenter', _onEnter);
    fullBar.addEventListener('mouseleave', _onLeave);
  } else {
    // Touch: tapping the always-visible mini strip expands the full bar.
    miniStrip.style.pointerEvents = 'auto';
    miniStrip.addEventListener('click', (e) => {
      e.stopPropagation();
      if (_barsVisible) _hideBars(); else _showBars();
    });
  }

  // ── Smooth scroll toggle ─────────────────────────────────────────────────

  smoothInput.checked = _smoothScroll;
  smoothInput.addEventListener('change', () => {
    _smoothScroll = smoothInput.checked;
    setLocal('kani_reader_smooth', String(_smoothScroll));
  });

  // ── Double-page toggle ────────────────────────────────────────────────────

  function _applyDoublePageVisibility() {
    doubleRow.style.display = _mode === 'paged' ? '' : 'none';
    spreadRow.style.display = '';
  }

  doubleInput.checked = _doublePage;
  doubleInput.addEventListener('change', () => {
    _doublePage = doubleInput.checked;
    setLocal('kani_reader_double', String(_doublePage));
    _applyDoublePageVisibility();
    _renderPages();
  });

  // ── Auto-combine spreads toggle ───────────────────────────────────────────

  spreadInput.checked = _autoSpread;
  spreadInput.addEventListener('change', () => {
    _autoSpread = spreadInput.checked;
    setLocal('kani_reader_spread', String(_autoSpread));
    _lastLayoutPage = -2;
    _renderPages();
  });

  // ── Direction buttons ─────────────────────────────────────────────────────

  function _applyDirButtons() {
    dirRtl.className = `flex-1 text-sm px-2 py-1.5 rounded-md transition-colors ${_direction === 'rtl' ? BTN_ACTIVE : BTN_INACTIVE}`;
    dirLtr.className = `flex-1 text-sm px-2 py-1.5 rounded-md transition-colors ${_direction === 'ltr' ? BTN_ACTIVE : BTN_INACTIVE}`;
    dirRtl.setAttribute('aria-pressed', String(_direction === 'rtl'));
    dirLtr.setAttribute('aria-pressed', String(_direction === 'ltr'));
  }

  dirRtl.addEventListener('click', () => {
    _direction = 'rtl'; setLocal('kani_reader_direction', _direction);
    if (_mangaId) api.setMangaTracking(_mangaId, { reading_direction: 'rtl' }).catch(() => {});
    for (const d of _imgDims.values()) d.edgeMatch = undefined;
    _applyDirButtons(); _renderPages();
  });
  dirLtr.addEventListener('click', () => {
    _direction = 'ltr'; setLocal('kani_reader_direction', _direction);
    if (_mangaId) api.setMangaTracking(_mangaId, { reading_direction: 'ltr' }).catch(() => {});
    for (const d of _imgDims.values()) d.edgeMatch = undefined;
    _applyDirButtons(); _renderPages();
  });

  // ── Fit buttons ───────────────────────────────────────────────────────────

  function _applyFitButtons() {
    fitBoth.className   = `flex-1 text-sm px-2 py-1.5 rounded-md transition-colors ${_fit === 'both'   ? BTN_ACTIVE : BTN_INACTIVE}`;
    fitWidth.className  = `flex-1 text-sm px-2 py-1.5 rounded-md transition-colors ${_fit === 'width'  ? BTN_ACTIVE : BTN_INACTIVE}`;
    fitHeight.className = `flex-1 text-sm px-2 py-1.5 rounded-md transition-colors ${_fit === 'height' ? BTN_ACTIVE : BTN_INACTIVE}`;
    fitBoth.setAttribute('aria-pressed',   String(_fit === 'both'));
    fitWidth.setAttribute('aria-pressed',  String(_fit === 'width'));
    fitHeight.setAttribute('aria-pressed', String(_fit === 'height'));
  }

  fitBoth.addEventListener('click', () => {
    _fit = 'both';   setLocal('kani_reader_fit', _fit); _applyFitButtons(); _renderPages();
  });
  fitWidth.addEventListener('click', () => {
    _fit = 'width';  setLocal('kani_reader_fit', _fit); _applyFitButtons(); _renderPages();
  });
  fitHeight.addEventListener('click', () => {
    _fit = 'height'; setLocal('kani_reader_fit', _fit); _applyFitButtons(); _renderPages();
  });

  // ── Mode buttons ─────────────────────────────────────────────────────────

  function _applyModeButtons() {
    const isScroll = _mode === 'scroll';
    modeScroll.className = `flex-1 text-sm px-3 py-2 rounded-md transition-colors ${isScroll ? BTN_ACTIVE : BTN_INACTIVE}`;
    modePaged.className  = `flex-1 text-sm px-3 py-2 rounded-md transition-colors ${!isScroll ? BTN_ACTIVE : BTN_INACTIVE}`;
    modeScroll.setAttribute('aria-pressed', String( isScroll));
    modePaged.setAttribute( 'aria-pressed', String(!isScroll));
    _applyDoublePageVisibility();
  }

  modeScroll.addEventListener('click', () => {
    _mode = 'scroll'; setLocal('kani_reader_mode', _mode);
    _applyModeButtons(); _renderPages();
  });
  modePaged.addEventListener('click', () => {
    _mode = 'paged'; setLocal('kani_reader_mode', _mode);
    _applyModeButtons(); _renderPages();
  });

  // ── Segment rendering ─────────────────────────────────────────────────────

  function _renderSegments() {
    miniStrip.innerHTML = '';
    segsEl.innerHTML    = '';

    const total = _pages.length;
    if (total === 0) {
      segLeft.textContent  = '—';
      segRight.textContent = '—';
      return;
    }

    segLeft.textContent  = String(_currentPage + 1);
    segRight.textContent = String(total);

    for (let i = 0; i < total; i++) {
      // Failed pages are red regardless of read/current state.
      const color = _failed.has(i)     ? 'bg-danger/70'
                  : i === _currentPage ? 'bg-accent'
                  : i < _currentPage   ? 'bg-accent/50'
                  : _loaded.has(i)     ? 'bg-white/20'
                  :                      'bg-white/10';

      // Mini strip: thin, no interaction
      const mini = document.createElement('div');
      mini.className = `flex-1 h-full ${color}`;
      miniStrip.appendChild(mini);

      // Full bar: clickable segments
      const seg = document.createElement('div');
      seg.className = `flex-1 h-full rounded-sm cursor-pointer ${color}`;
      const idx = i;
      seg.addEventListener('click', (e) => {
        e.stopPropagation();
        if (_failed.has(idx)) {
          _failed.delete(idx);
          if (_mode === 'scroll') {
            _renderSegments();
            const img = /** @type {HTMLImageElement|null} */ (
              pagesEl.querySelector(`img[data-index="${idx}"]`)
            );
            if (img) { img.src = ''; img.src = _pages[idx]; }
          } else {
            _currentPage = idx;
            _renderPages();
          }
          return;
        }
        if (_mode === 'scroll') {
          pagesEl.querySelector(`[data-index="${idx}"]`)
            ?.scrollIntoView({ behavior: _smoothScroll ? 'smooth' : 'instant', block: 'start' });
        } else {
          _currentPage = idx;
          _renderPages();
        }
      });
      segsEl.appendChild(seg);
    }
  }

  // ── Spread detection ──────────────────────────────────────────────────────

  /**
   * Returns true if pages `idxA` and `idxA+1` are a split double-page spread
   * (i.e. a single wide scan split into two portrait-oriented files). Both
   * image dimensions must already be known via `_imgDims`.
   * @param {number} idxA
   */
  function _isSpreadPair(idxA) {
    if (!_autoSpread) return false;
    const idxB = idxA + 1;
    if (idxB >= _pages.length) return false;

    if (_hasServerAnalysis) {
      if (!_serverDoublePages.has(idxA)) return false;
      const a = _imgDims.get(idxA);
      const b = _imgDims.get(idxB);
      if (!a || !b) return false;
      if (a.w / a.h >= 1.2) return false;
      return true;
    }

    const a = _imgDims.get(idxA);
    const b = _imgDims.get(idxB);
    if (!a || !b) return false;
    if (a.w >= a.h * 0.95 || b.w >= b.h * 0.95) return false;
    const ratio = (a.w + b.w) / Math.max(a.h, b.h);
    if (ratio < 1.2 || ratio > 2.5) return false;
    if (a.edgeMatch === true)  return true;
    if (a.edgeMatch === false) return false;
    if (a.edgeMatch === null)  return false;
    a.edgeMatch = null;
    _checkEdgeMatch(idxA);
    return false;
  }

  /**
   * Samples a narrow vertical strip of pixels from the right edge of page `idxA`
   * and the left edge of page `idxA + 1`. If they're similar (low average diff),
   * the pages are halves of the same original scan and should be composited.
   * @param {number} idxA
   */
  async function _checkEdgeMatch(idxA) {
    const STRIP_W  = 32;
    const SAMPLE_H = 64;
    try {
      const [bmpA, bmpB] = await Promise.all([
        fetch(_pages[idxA]).then(r => r.blob()).then(b => createImageBitmap(b)),
        fetch(_pages[idxA + 1]).then(r => r.blob()).then(b => createImageBitmap(b)),
      ]);
      const [leftBmp, rightBmp] = _direction === 'rtl' ? [bmpB, bmpA] : [bmpA, bmpB];
      const oc  = new OffscreenCanvas(STRIP_W * 2, SAMPLE_H);
      const ctx = /** @type {OffscreenCanvasRenderingContext2D} */ (oc.getContext('2d'));
      ctx.drawImage(leftBmp,  leftBmp.width  - STRIP_W, 0, STRIP_W, leftBmp.height,  0,       0, STRIP_W, SAMPLE_H);
      ctx.drawImage(rightBmp, 0,                         0, STRIP_W, rightBmp.height, STRIP_W, 0, STRIP_W, SAMPLE_H);
      const pxA = ctx.getImageData(0,       0, STRIP_W, SAMPLE_H).data;
      const pxB = ctx.getImageData(STRIP_W, 0, STRIP_W, SAMPLE_H).data;

      /** @param {Uint8ClampedArray} data */
      const variance = (data) => {
        let sum = 0, sumSq = 0, n = 0;
        for (let i = 0; i < data.length; i += 4) {
          const luma = (data[i] * 299 + data[i+1] * 587 + data[i+2] * 114) / 1000;
          sum += luma; sumSq += luma * luma; n++;
        }
        const mean = sum / n;
        return (sumSq / n) - (mean * mean);
      };
      const current = _imgDims.get(idxA);
      if (!current) return;
      if (variance(pxA) < 200 || variance(pxB) < 200) {
        current.edgeMatch = false;
        return;
      }

      let diff = 0;
      for (let y = 0; y < SAMPLE_H; y++) {
        for (let x = 0; x < STRIP_W; x++) {
          const iA = (y * STRIP_W + (STRIP_W - 1 - x)) * 4;
          const iB = (y * STRIP_W + x) * 4;
          diff += Math.abs(pxA[iA] - pxB[iB]) + Math.abs(pxA[iA+1] - pxB[iB+1]) + Math.abs(pxA[iA+2] - pxB[iB+2]);
        }
      }
      const avgDiff = diff / (SAMPLE_H * STRIP_W * 3);
      current.edgeMatch = avgDiff < 20;

      if (current.edgeMatch && _mode === 'paged' && _lastLayoutPage !== idxA) {
        if (_currentPage === idxA) {
          _lastLayoutPage = idxA;
          _renderPages();
        } else if (_currentPage === idxA + 1) {
          _currentPage    = idxA;
          _lastLayoutPage = idxA;
          _renderPages();
        }
      }
    } catch {
      const current = _imgDims.get(idxA);
      if (current) current.edgeMatch = false;
    }
  }

  /**
   * Returns true if page `idx` is a pre-combined wide spread (landscape orientation).
   * In double-page mode such pages are displayed alone rather than paired.
   * @param {number} idx
   */
  function _isWideImage(idx) {
    const d = _imgDims.get(idx);
    if (_hasServerAnalysis) {
      if (!_serverDoublePages.has(idx)) return false;
      if (!d) return false;
      return d.w / d.h >= 1.2;
    }
    return !!d && d.w / d.h >= 1.2;
  }

  // ── Page stop navigation ─────────────────────────────────────────────────

  /**
   * Returns the page index of the next stop after `from` in paged mode.
   * Accounts for: first-page-alone, wide images, spread pairs.
   * @param {number} from
   */
  function _nextStop(from) {
    if (_doublePage) {
      if (from === 0) return 1; // page 0 always shown alone
      if (_isWideImage(from) || (from + 1 < _pages.length && _isWideImage(from + 1)))
        return from + 1;
      return from + 2;
    }
    if (_autoSpread && _isSpreadPair(from)) return from + 2;
    return from + 1;
  }

  /**
   * Returns the page index of the previous stop before `from` in paged mode.
   * Reconstructs the backward path by inverting `_nextStop`.
   * @param {number} from
   */
  function _prevStop(from) {
    for (let offset = 1; offset <= 2; offset++) {
      const c = from - offset;
      if (c >= 0 && _nextStop(c) === from) return c;
    }
    return Math.max(0, from - 1); // fallback
  }

  // ── Prefetch ─────────────────────────────────────────────────────────────

  function _prefetch(pageIndex) {
    if (_mode !== 'paged') return;
    for (let i = 1; i <= 2; i++) {
      const prefIdx = pageIndex + i;
      const url = _pages[prefIdx];
      if (url && !_loaded.has(prefIdx) && !_failed.has(prefIdx)) {
        const img = new Image();
        img.addEventListener('load', () => {
          _loaded.add(prefIdx); _renderSegments();
          if (img.naturalWidth > 0) {
            _setDims(prefIdx, img.naturalWidth, img.naturalHeight);
            if (_autoSpread && _mode === 'paged' && !_doublePage &&
                prefIdx === _currentPage + 1 && _isSpreadPair(_currentPage) &&
                _lastLayoutPage !== _currentPage) {
              _lastLayoutPage = _currentPage;
              _renderPages();
            }
          }
        });
        img.addEventListener('error', () => { _failed.add(prefIdx); _renderSegments(); });
        img.src = url;
      }
    }
  }

  // ── Page rendering ────────────────────────────────────────────────────────

  function _renderPages() {
    if (_scrollObs) { _scrollObs.disconnect(); _scrollObs = null; }
    pagesEl.innerHTML = '';

    if (_pages.length === 0) {
      pagesEl.className = 'flex-1 overflow-y-auto overflow-x-hidden flex flex-col items-center';
      pagesEl.innerHTML = '<div class="flex items-center justify-center min-h-full"><p class="text-muted text-sm">No pages found.</p></div>';
      _renderSegments();
      return;
    }

    /** Returns the CSS class string for an image given current fit mode and context. */
    function _imgClass(ctx = 'scroll') {
      if (ctx === 'scroll') {
        if (_fit === 'height') return 'max-h-screen w-auto';
        if (_fit === 'width')  return 'max-w-full h-auto';
        return 'max-w-full max-h-screen object-contain'; // both
      }
      if (ctx === 'paged-single') {
        if (_fit === 'height') return 'max-h-full w-auto';
        if (_fit === 'width')  return 'max-w-full h-auto';
        return 'max-w-full max-h-full object-contain'; // both
      }
      // paged-double: each image shares half the width
      if (_fit === 'height') return 'max-h-full w-auto';
      if (_fit === 'width')  return 'max-w-[50vw] max-h-full';
      return 'max-w-[50vw] max-h-full object-contain'; // both
    }

    /** CSS class for a spread canvas (no object-contain — canvas is already the bitmap). */
    function _spreadClass() {
      if (_fit === 'height') return 'max-h-full w-auto';
      if (_fit === 'width')  return 'max-w-full h-auto';
      return 'max-w-full max-h-full';
    }

    if (_mode === 'scroll') {
      pagesEl.className = 'flex-1 overflow-y-auto overflow-x-hidden flex flex-col items-center gap-1 py-2';

      // Track img elements by page index so we can replace pairs with canvases.
      /** @type {Map<number, HTMLImageElement>} */
      const _scrollImgs = new Map();

      /**
       * If pages idxA and idxA+1 are a spread pair and both are already loaded,
       * replace both img elements with a single composited canvas in-place.
       * @param {number} idxA
       */
      const _maybeComposite = (idxA) => {
        if (!_isSpreadPair(idxA)) return;
        const imgA = _scrollImgs.get(idxA);
        const imgB = _scrollImgs.get(idxA + 1);
        if (!imgA || !imgB || !imgA.parentElement) return;

        const [leftImg, rightImg] = _direction === 'rtl' ? [imgB, imgA] : [imgA, imgB];
        const W = leftImg.naturalWidth + rightImg.naturalWidth;
        const H = Math.max(leftImg.naturalHeight, rightImg.naturalHeight);
        const canvas = document.createElement('canvas');
        canvas.className   = _imgClass('scroll');
        canvas.dataset.index = String(idxA);
        canvas.width  = W;
        canvas.height = H;
        const ctx = canvas.getContext('2d');
        if (ctx) {
          ctx.drawImage(leftImg,  0,                   (H - leftImg.naturalHeight)  / 2);
          ctx.drawImage(rightImg, leftImg.naturalWidth, (H - rightImg.naturalHeight) / 2);
        }
        _loaded.add(idxA); _loaded.add(idxA + 1);

        _scrollObs?.unobserve(imgA);
        _scrollObs?.unobserve(imgB);
        imgA.replaceWith(canvas);
        imgB.remove();
        _scrollImgs.delete(idxA);
        _scrollImgs.delete(idxA + 1);
        _scrollObs?.observe(canvas);
        _renderSegments();
      };

      for (let i = 0; i < _pages.length; i++) {
        const img         = document.createElement('img');
        img.src           = _pages[i];
        img.className     = _imgClass('scroll');
        img.alt           = '';
        img.loading       = 'lazy';
        img.dataset.index = String(i);
        const _i = i;
        img.addEventListener('load', () => {
          _loaded.add(_i); _failed.delete(_i);
          if (img.naturalWidth > 0) {
            _setDims(_i, img.naturalWidth, img.naturalHeight);
            if (_autoSpread) {
              _maybeComposite(_i);          // this page starts a pair
              if (_i > 0) _maybeComposite(_i - 1); // this page completes a pair
            }
          }
          _renderSegments();
        });
        img.addEventListener('error', () => { _failed.add(_i); _loaded.delete(_i); _renderSegments(); });
        if (img.complete) {
          if (img.naturalWidth) { _loaded.add(i); _setDims(i, img.naturalWidth, img.naturalHeight); }
          else _failed.add(i);
        }
        _scrollImgs.set(i, img);
        pagesEl.appendChild(img);
      }

      // End-of-chapter card
      {
        const card = document.createElement('div');
        card.className = 'flex flex-col items-center justify-center py-16 gap-4 w-full shrink-0';
        if (_chapterInfo.next_chapter_id) {
          card.innerHTML = `
            <p class="text-muted text-sm">End of chapter</p>
            <button class="btn-ghost flex items-center gap-1">
              Next chapter ${iconChevronRight}
            </button>
          `;
          card.querySelector('button')?.addEventListener('click', () => {
            _navigateChapter(_chapterInfo.next_chapter_id);
          });
        } else {
          card.innerHTML = `
            <p class="text-muted text-sm">End of chapter</p>
            <button class="btn-ghost flex items-center gap-1">
              ${iconChevronLeft} Back to manga
            </button>
          `;
          card.querySelector('button')?.addEventListener('click', () => {
            _navigateToManga();
          });
        }
        pagesEl.appendChild(card);
      }

      // IntersectionObserver for current-page tracking
      /** @type {Set<number>} */
      const visible = new Set();
      _scrollObs = new IntersectionObserver((entries) => {
        for (const e of entries) {
          const idx = Number(/** @type {HTMLElement} */ (e.target).dataset.index);
          if (!isNaN(idx)) {
            if (e.isIntersecting) visible.add(idx);
            else visible.delete(idx);
          }
        }
        if (visible.size > 0) {
          _currentPage = Math.min(...visible);
          _renderSegments();
          _reportProgress();
          _maybePreloadNext();
        }
      }, { root: pagesEl, threshold: 0.1 });
      pagesEl.querySelectorAll('[data-index]').forEach(el => _scrollObs?.observe(el));

    } else {
      // Paged mode
      _currentPage = Math.max(0, Math.min(_pages.length - 1, _currentPage));
      pagesEl.className = 'flex-1 overflow-hidden relative flex items-center justify-center';

      /** @param {number} pageIdx @param {string} altText @returns {HTMLImageElement} */
      function _makePageImg(pageIdx, altText) {
        const img     = document.createElement('img');
        img.src       = _pages[pageIdx] ?? '';
        img.className = _imgClass(_doublePage ? 'paged-double' : 'paged-single');
        img.alt       = altText;
        img.addEventListener('load', () => {
          _loaded.add(pageIdx); _failed.delete(pageIdx);
          if (img.naturalWidth > 0) {
            _setDims(pageIdx, img.naturalWidth, img.naturalHeight);
            if (_lastLayoutPage !== _currentPage) {
              if (_autoSpread && _isSpreadPair(_currentPage)) {
                _lastLayoutPage = _currentPage;
                _renderPages();
                return;
              }
              if (_doublePage && (_isWideImage(_currentPage) || _isWideImage(_currentPage + 1))) {
                _lastLayoutPage = _currentPage;
                _renderPages();
                return;
              }
            }
          }
          _renderSegments();
        });
        img.addEventListener('error', () => {
          _failed.add(pageIdx); _loaded.delete(pageIdx); _renderSegments();
          const err = document.createElement('div');
          err.className = 'absolute inset-0 flex flex-col items-center justify-center gap-3 pointer-events-none';
          err.innerHTML = `<p class="text-muted text-sm">Failed to load page ${pageIdx + 1}</p>`;
          pagesEl.appendChild(err);
        });
        if (img.complete) {
          if (img.naturalWidth) {
            _loaded.add(pageIdx);
            _setDims(pageIdx, img.naturalWidth, img.naturalHeight);
          } else {
            _failed.add(pageIdx);
          }
        }
        return img;
      }

      if (_doublePage && _pages.length > 1) {
        const leftIdx  = _direction === 'rtl' ? _currentPage + 1 : _currentPage;
        const rightIdx = _direction === 'rtl' ? _currentPage     : _currentPage + 1;

        if (_currentPage === 0) {
          const img = _makePageImg(0, 'Page 1');
          img.className = _imgClass('paged-single');
          pagesEl.appendChild(img);
        } else if (_isSpreadPair(_currentPage) && leftIdx < _pages.length && rightIdx < _pages.length) {
          const a = /** @type {{w:number,h:number}} */ (_imgDims.get(leftIdx));
          const b = /** @type {{w:number,h:number}} */ (_imgDims.get(rightIdx));
          const expectedW = a.w + b.w;
          const expectedH = Math.max(a.h, b.h);

          const canvas = document.createElement('canvas');
          canvas.className = _spreadClass();
          canvas.width  = expectedW;
          canvas.height = expectedH;
          canvas.setAttribute('role', 'img');
          canvas.setAttribute('aria-label',
            `Spread: pages ${Math.min(leftIdx, rightIdx) + 1}–${Math.max(leftIdx, rightIdx) + 1}`);

          const leftImg  = new Image();
          const rightImg = new Image();
          leftImg.src  = _pages[leftIdx];
          rightImg.src = _pages[rightIdx];

          let _lReady = false;
          let _rReady = false;
          const _drawSpread = () => {
            if (!_lReady || !_rReady) return;
            const W = leftImg.naturalWidth + rightImg.naturalWidth;
            const H = Math.max(leftImg.naturalHeight, rightImg.naturalHeight);
            canvas.width  = W;
            canvas.height = H;
            const ctx = canvas.getContext('2d');
            if (!ctx) return;
            ctx.drawImage(leftImg,  0,                  (H - leftImg.naturalHeight)  / 2);
            ctx.drawImage(rightImg, leftImg.naturalWidth, (H - rightImg.naturalHeight) / 2);
            _loaded.add(leftIdx); _loaded.add(rightIdx);
            _renderSegments();
          };
          leftImg.addEventListener('load', () => {
            _lReady = true;
            _setDims(leftIdx,  leftImg.naturalWidth,  leftImg.naturalHeight);
            _drawSpread();
          });
          rightImg.addEventListener('load', () => {
            _rReady = true;
            _setDims(rightIdx, rightImg.naturalWidth, rightImg.naturalHeight);
            _drawSpread();
          });
          let _spreadErrorShown = false;
          const _showSpreadError = () => {
            _failed.add(leftIdx); _failed.add(rightIdx);
            _renderSegments();
            if (_spreadErrorShown) return;
            _spreadErrorShown = true;
            canvas.remove();
            const err = document.createElement('div');
            err.className = 'absolute inset-0 flex flex-col items-center justify-center gap-3';
            err.innerHTML = `
              <p class="text-muted text-sm">Failed to load spread pages</p>
              <button class="btn-ghost">Retry</button>
            `;
            err.querySelector('button')?.addEventListener('click', () => {
              _failed.delete(leftIdx); _failed.delete(rightIdx);
              _renderPages();
            });
            pagesEl.appendChild(err);
          };
          leftImg.addEventListener('error',  _showSpreadError);
          rightImg.addEventListener('error', _showSpreadError);
          // Handle already-cached images (naturalWidth set synchronously).
          if (leftImg.complete && leftImg.naturalWidth)   { _lReady = true;  _setDims(leftIdx,  leftImg.naturalWidth,  leftImg.naturalHeight);  }
          if (rightImg.complete && rightImg.naturalWidth) { _rReady = true;  _setDims(rightIdx, rightImg.naturalWidth, rightImg.naturalHeight); }
          _drawSpread();

          pagesEl.appendChild(canvas);
        } else if (_isWideImage(_currentPage) || (_currentPage + 1 < _pages.length && _isWideImage(_currentPage + 1))) {
          const img = _makePageImg(_currentPage, `Page ${_currentPage + 1}`);
          img.className = _imgClass('paged-single');
          pagesEl.appendChild(img);
        } else {
          const spread   = document.createElement('div');
          spread.className = 'flex items-center justify-center gap-0.5 max-w-full h-full';
          if (leftIdx < _pages.length) spread.appendChild(_makePageImg(leftIdx, `Page ${leftIdx + 1}`));
          if (rightIdx < _pages.length && rightIdx !== leftIdx) spread.appendChild(_makePageImg(rightIdx, `Page ${rightIdx + 1}`));
          pagesEl.appendChild(spread);
        }
        _prefetch(_currentPage + 1);
      } else if (_autoSpread && _isSpreadPair(_currentPage) && _currentPage + 1 < _pages.length) {
        const leftIdx  = _direction === 'rtl' ? _currentPage + 1 : _currentPage;
        const rightIdx = _direction === 'rtl' ? _currentPage     : _currentPage + 1;
        const a = /** @type {{w:number,h:number}} */ (_imgDims.get(leftIdx));
        const b = /** @type {{w:number,h:number}} */ (_imgDims.get(rightIdx));

        const canvas = document.createElement('canvas');
        canvas.className = _spreadClass();
        canvas.width  = a.w + b.w;
        canvas.height = Math.max(a.h, b.h);
        canvas.setAttribute('role', 'img');
        canvas.setAttribute('aria-label',
          `Spread: pages ${Math.min(leftIdx, rightIdx) + 1}–${Math.max(leftIdx, rightIdx) + 1}`);

        const leftImg  = new Image();
        const rightImg = new Image();
        leftImg.src  = _pages[leftIdx];
        rightImg.src = _pages[rightIdx];

        let _lReady = false, _rReady = false;
        const _draw = () => {
          if (!_lReady || !_rReady) return;
          const W = leftImg.naturalWidth + rightImg.naturalWidth;
          const H = Math.max(leftImg.naturalHeight, rightImg.naturalHeight);
          canvas.width = W; canvas.height = H;
          const ctx = canvas.getContext('2d');
          if (!ctx) return;
          ctx.drawImage(leftImg,  0,                    (H - leftImg.naturalHeight)  / 2);
          ctx.drawImage(rightImg, leftImg.naturalWidth,  (H - rightImg.naturalHeight) / 2);
          _loaded.add(leftIdx); _loaded.add(rightIdx);
          _renderSegments();
        };
        leftImg.addEventListener('load',  () => { _lReady = true;  _setDims(leftIdx,  leftImg.naturalWidth,  leftImg.naturalHeight);  _draw(); });
        rightImg.addEventListener('load', () => { _rReady = true;  _setDims(rightIdx, rightImg.naturalWidth, rightImg.naturalHeight); _draw(); });
        let _errShown = false;
        const _onErr = () => {
          _failed.add(leftIdx); _failed.add(rightIdx); _renderSegments();
          if (_errShown) return; _errShown = true;
          canvas.remove();
          const err = document.createElement('div');
          err.className = 'absolute inset-0 flex flex-col items-center justify-center gap-3';
          err.innerHTML = `<p class="text-muted text-sm">Failed to load spread pages</p><button class="btn-ghost">Retry</button>`;
          err.querySelector('button')?.addEventListener('click', () => { _failed.delete(leftIdx); _failed.delete(rightIdx); _renderPages(); });
          pagesEl.appendChild(err);
        };
        leftImg.addEventListener('error', _onErr);
        rightImg.addEventListener('error', _onErr);
        if (leftImg.complete  && leftImg.naturalWidth)  { _lReady = true;  _setDims(leftIdx,  leftImg.naturalWidth,  leftImg.naturalHeight);  }
        if (rightImg.complete && rightImg.naturalWidth) { _rReady = true;  _setDims(rightIdx, rightImg.naturalWidth, rightImg.naturalHeight); }
        _draw();
        pagesEl.appendChild(canvas);
        _prefetch(_currentPage + 2);
      } else {
        const img = _makePageImg(_currentPage, `Page ${_currentPage + 1}`);
        img.className = _imgClass('paged-single');
        img.addEventListener('error', () => {
          const failedPage = _currentPage;
          const err = document.createElement('div');
          err.className = 'absolute inset-0 flex flex-col items-center justify-center gap-3';
          err.innerHTML = `
            <p class="text-muted text-sm">Failed to load page ${failedPage + 1}</p>
            <button class="btn-ghost">Retry</button>
          `;
          err.querySelector('button')?.addEventListener('click', () => {
            _failed.delete(failedPage);
            _renderPages();
          });
          pagesEl.appendChild(err);
        });
        pagesEl.appendChild(img);
        _prefetch(_currentPage);
        if (_autoSpread && !_hasServerAnalysis && _currentPage > 0) _isSpreadPair(_currentPage - 1);
      }
    }

    _renderSegments();
  }

  // ── Page navigation ───────────────────────────────────────────────────────

  function _goPage(rawDelta) {
    if (_mode !== 'paged') return;
    const delta = _direction === 'rtl' ? -rawDelta : rawDelta;
    const next  = delta > 0 ? _nextStop(_currentPage) : _prevStop(_currentPage);

    if (next < 0) {
      if (_chapterInfo.prev_chapter_id) {
        api.setChapterProgress(_chapterInfo.prev_chapter_id, -1).catch(() => {});
        _navigateChapter(_chapterInfo.prev_chapter_id);
      }
      return;
    }
    if (next >= _pages.length) {
      api.setChapterProgress(chapterId, 0).catch(() => {});
      if (_chapterInfo.next_chapter_id) {
        _navigateChapter(_chapterInfo.next_chapter_id);
      } else {
        _navigateToManga();
      }
      return;
    }

    _currentPage = next;
    _reportProgress();
    _maybePreloadNext();
    _renderPages();
  }

  // ── Three-zone click ──────────────────────────────────────────────────────
  // Desktop (fine-pointer): left/right navigate; middle opens panel.
  // Mobile (touch): left/right navigate; middle shows bars (top + bottom).

  pagesEl.addEventListener('click', (e) => {
    const target = /** @type {HTMLElement} */ (e.target);
    if (target.closest('button') || target.closest('a')) return;

    const rect  = pagesEl.getBoundingClientRect();
    const x     = e.clientX - rect.left;
    const third = rect.width / 3;

    if (_mode === 'paged') {
      if (x < third)     { _goPage(_direction === 'rtl' ? 1 : -1); return; }
      if (x > 2 * third) { _goPage(_direction === 'rtl' ? -1 : 1); return; }
    }

    // Middle zone (or scroll mode anywhere in centre third)
    if (x >= third && x <= 2 * third) {
      if (_isFinePointer()) {
        _openPanel();
      } else {
        // Mobile: toggle bars (both top bar and expanded indicator)
        if (_barsVisible) _hideBars(); else _showBars();
      }
    }
  });

  // ── Touch/swipe ───────────────────────────────────────────────────────────

  let _touchStartX = 0;
  pagesEl.addEventListener('touchstart', (e) => { _touchStartX = e.touches[0].clientX; }, { passive: true });
  pagesEl.addEventListener('touchend',   (e) => {
    const dx = e.changedTouches[0].clientX - _touchStartX;
    if (Math.abs(dx) > 50) _goPage(dx < 0 ? 1 : -1);
  });

  // ── Keyboard ─────────────────────────────────────────────────────────────

  // Panel-open shortcuts (Escape closes panel) bypass ShortcutManager.
  function _onPanelKeyDown(/** @type {KeyboardEvent} */ e) {
    if (_panelOpen && e.key === 'Escape') { e.preventDefault(); _closePanel(); }
  }
  document.addEventListener('keydown', _onPanelKeyDown);
  _cleanup.push(() => document.removeEventListener('keydown', _onPanelKeyDown));

  _cleanup.push(registerShortcuts('reader', [
    { key: ['ArrowRight', 'ArrowDown', 'l', 'd'], description: 'Next page',     handler: () => { if (!_panelOpen) _goPage(1);  } },
    { key: ['ArrowLeft',  'ArrowUp',   'h', 'a'], description: 'Previous page', handler: () => { if (!_panelOpen) _goPage(-1); } },
    { key: ']', description: 'Next chapter',     handler: () => { if (!_panelOpen && _chapterInfo.next_chapter_id) _navigateChapter(_chapterInfo.next_chapter_id); } },
    { key: '[', description: 'Previous chapter', handler: () => { if (!_panelOpen && _chapterInfo.prev_chapter_id) _navigateChapter(_chapterInfo.prev_chapter_id); } },
    { key: 'Escape', description: 'Back to manga', handler: () => { if (!_panelOpen) _navigateToManga(); } },
  ]));

  // Position the panel immediately (before the await) so there is no flash of
  // the desktop default position on mobile before the JS takes effect.
  _positionPanel();

  // ── Load chapter ──────────────────────────────────────────────────────────

  try {
    const data = await api.getChapterPages(chapterId);

    _pages = Array.isArray(data?.pages)
      ? data.pages.map(p => api.getChapterPageUrl(chapterId, p.index))
      : [];
    _chapterInfo = data ?? {};
    _mangaId     = data?.manga_id ?? null;

    _hasServerAnalysis = data?.spread_analysed === true;
    _serverDoublePages = new Set(
      (data?.pages ?? []).filter(p => p.double_page).map(p => p.index)
    );

    const _cachedDims = getLocalJson(`kani_dims_${chapterId}`);
    if (Array.isArray(_cachedDims)) {
      for (const entry of _cachedDims) {
        const [i, w, h] = entry;
        if (typeof i === 'number' && typeof w === 'number' && typeof h === 'number') {
          _imgDims.set(i, { w, h });
        }
      }
    }

    // Load per-manga reading direction, falling back to localStorage
    if (_mangaId) {
      try {
        const tracking = await api.getMangaTracking(_mangaId);
        if (tracking?.reading_direction === 'ltr' || tracking?.reading_direction === 'rtl') {
          _direction = tracking.reading_direction;
        }
      } catch { /* keep localStorage default */ }
    }

    if (data?.last_page_read != null) {
      _currentPage = data.last_page_read;
    }
    if (_currentPage === -1) _currentPage = _pages.length - 1;
    _currentPage = Math.max(0, Math.min(Math.max(_pages.length - 1, 0), _currentPage));

    if (data?.chapter_title) {
      titleMobile.textContent = data.chapter_title;
      sideTitle.textContent   = data.chapter_title;
      document.title = data.chapter_title + ' - Kani';
    }

    if (data?.prev_chapter_id) {
      sidePrev.disabled = false;
      sidePrev.addEventListener('click', () => _navigateChapter(data.prev_chapter_id));
    }
    if (data?.next_chapter_id) {
      sideNext.disabled = false;
      sideNext.addEventListener('click', () => _navigateChapter(data.next_chapter_id));
    }
  } catch {
    pagesEl.innerHTML = '<div class="flex items-center justify-center min-h-full"><p class="text-danger text-sm">Failed to load chapter pages.</p></div>';
  }

  _applyModeButtons();
  _applyDirButtons();
  _applyFitButtons();
  _applyDoublePageVisibility();
  _renderPages();
  if (_pendingBarsVisible) _showBars();
  pagesEl.focus();

  // Download-ahead: after a 1s delay, silently queue the next N chapters
  if (getLocal('kani_download_ahead_enabled') === 'true' && _chapterInfo.next_chapter_id) {
    const aheadCount = Math.max(1, Math.min(10, parseInt(getLocal('kani_download_ahead_count') || '3', 10)));
    const _downloadAheadTimer = setTimeout(async () => {
      let nextId = _chapterInfo.next_chapter_id;
      for (let i = 0; i < aheadCount && nextId; i++) {
        try {
          await api.downloadChapter(nextId);
        } catch { /* already downloading/downloaded — ignore */ }
        // Walk forward: get next chapter's manifest for its next_chapter_id
        if (i < aheadCount - 1) {
          try {
            const nextManifest = await api.getChapterPages(nextId);
            nextId = nextManifest?.next_chapter_id ?? null;
          } catch {
            break;
          }
        }
      }
    }, 1000);
    _cleanup.push(() => clearTimeout(_downloadAheadTimer));
  }

  _destroyFn = () => {
    if (_hideTimer) clearTimeout(_hideTimer);
    if (_currentPage !== _lastReportedPage) {
      api.setChapterProgress(chapterId, _currentPage).catch(() => {});
    }
    document.body.classList.remove('overflow-hidden');
    for (const fn of _cleanup) fn();
    container.innerHTML = '';
  };
}

/** @param {HTMLElement} _container */
export function destroy(_container) {
  _destroyFn?.();
  _destroyFn = null;
}
