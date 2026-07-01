// @ts-check
// Reader page — full-screen chapter reader with page-by-page and scroll modes.

import * as api from '../api.js';
import { iconChevronLeft, iconChevronRight, iconX, iconMenu, iconSettings } from '../icons.js';
import { navigate } from '../router.js';
import { getLocal, getLocalJson, setLocalJson, debounce, formatChapterTitle } from '../utils.js';
import { t } from '../i18n.js';
import { getState, subscribe } from '../state.js';
import { registerShortcuts, getShortcuts, setF1Override } from '../shortcuts.js';
import { createEmptyState } from '../components/empty-state.js';
import { renderTabs } from '../components/tabs.js';
import { loadReaderPrefs, setReaderPref, cancelReaderPrefsSync } from '../reader-prefs.js';
import { mkReaderSection, mkAccordionSection, mkToggleRow, mkSliderRow, mkSegmentedRow, mkSelectRow, mkActionBtn } from '../components/reader/reader-controls.js';

/** @type {(() => void) | null} */
let _destroyFn = null;

// Carries bar-visibility state across chapter navigations (touch only).
let _pendingBarsVisible = false;

/** @param {HTMLElement} container @param {{ id?: string }} params */
export async function init(container, { id }) {
  const chapterId = Number(id);
  document.title = t('reader.title');

  /** @type {Array<() => void>} */
  const _cleanup = [];

  document.body.classList.add('overflow-hidden');

  container.innerHTML = `
    <div id="reader-root" class="fixed inset-0 z-40 flex flex-col select-none overflow-hidden" style="background-color:#000"><!-- audit-ignore: reader bg is its own black/white/sepia system -->

      <!-- Mobile-only top bar (hidden on md+). Slides in from top when bars visible.
           min-h-14 + safe-area padding lets the bar absorb the iOS notch / Dynamic Island. -->
      <div id="reader-top"
        class="md:hidden absolute top-0 inset-x-0 z-30 flex items-center gap-2 px-3 min-h-14 bg-surface border-b border-border/60 transition-transform duration-150"
        style="transform: translateY(-100%); padding-top: env(safe-area-inset-top, 0px)">
        <button id="reader-back-mobile"
          class="btn-icon shrink-0"
          aria-label="${t('reader.aria.back')}">${iconChevronLeft}</button>
        <span id="reader-title-mobile" class="flex-1 text-sm font-medium text-text truncate"></span>
        <button id="reader-menu-btn"
          class="btn-icon shrink-0"
          aria-label="${t('reader.aria.open_menu')}">${iconMenu}</button>
      </div>

      <!-- Page canvas: flex-col so pagesEl can use flex-1; isolation:isolate so
           tint blend modes blend within the canvas group, not against the root. -->
      <div id="reader-canvas" class="flex-1 flex flex-col min-h-0 relative" style="isolation:isolate">
        <!-- Page viewer -->
        <div id="reader-pages"
          class="flex-1 overflow-y-auto overflow-x-hidden flex flex-col items-center"
          style="overscroll-behavior: none"
          tabindex="0" aria-label="${t('reader.aria.chapter_pages')}" aria-live="polite">
          <div class="flex items-center justify-center min-h-full w-full">
            <p class="text-muted text-sm">${t('common.loading')}</p>
          </div>
        </div>

        <!-- Page tint overlay: sibling of pagesEl inside the isolated canvas so it
             isn't destroyed by pagesEl.innerHTML = ''. The isolation:isolate on the
             canvas wrapper confines blend modes to the canvas group. -->
        <div id="reader-tint" class="absolute inset-0 pointer-events-none" style="z-index:1;display:none"></div>
      </div>

      <!-- Page-number overlay badge: shown when pageOverlay pref is enabled. -->
      <div id="reader-page-num" class="absolute bottom-20 right-3 pointer-events-none select-none" style="z-index:2;display:none">
        <span class="text-xs tabular-nums rounded px-1.5 py-0.5 bg-black/50 text-white/80"></span><!-- audit-ignore: page-number badge over arbitrary page content -->
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
        class="absolute bottom-0 inset-x-0 flex items-center gap-3 px-4 h-14 bg-surface/90 backdrop-blur-sm border-t border-border/40 transition-transform duration-150 reader-bar"
        style="transform:translateY(100%)">
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
        class="absolute bottom-0 inset-x-0 pointer-events-none reader-bar-hover"
        style="height:64px">
      </div>

      <!-- Side panel backdrop -->
      <div id="reader-side-backdrop"
        class="hidden absolute inset-0 bg-scrim z-30">
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
            aria-label="${t('reader.aria.back_to_manga')}">${iconChevronLeft}</button>
          <span id="reader-side-title" class="flex-1 text-sm font-medium text-muted truncate">—</span>
          <button id="reader-settings-btn"
            class="btn-icon shrink-0"
            aria-label="${t('reader.aria.settings')}">${iconSettings}</button>
          <button id="reader-side-close"
            class="btn-icon shrink-0"
            aria-label="${t('reader.aria.close_menu')}">${iconX}</button>
        </div>

        <div id="reader-side-scroll" class="flex flex-col flex-1 overflow-y-auto">

          <!-- Mobile back button (full-width, only on mobile) -->
          <div class="md:hidden px-3 py-3 border-b border-border shrink-0">
            <button id="reader-side-back-mobile"
              class="btn-ghost w-full flex items-center justify-center gap-1">
              ${iconChevronLeft} ${t('reader.back_to_manga')}
            </button>
          </div>

          <!-- Prev / Chapter dropdown / Next -->
          <div class="px-3 py-3 flex gap-1.5 border-b border-border shrink-0 items-center">
            <button id="reader-side-prev"
              class="btn-ghost flex items-center justify-center gap-0.5 shrink-0 px-2"
              disabled>${iconChevronLeft}</button>
            <select id="reader-chapter-select"
              class="input text-sm flex-1 min-w-0 text-center h-9 py-0"
              disabled>
              <option>—</option>
            </select>
            <button id="reader-side-next"
              class="btn-ghost flex items-center justify-center gap-0.5 shrink-0 px-2"
              disabled>${iconChevronRight}</button>
          </div>

          <div class="px-3 py-4 border-b border-border flex flex-col gap-3">
            <div id="reader-mode-mount"></div>
            <div id="reader-fit-mount"></div>
            <div id="reader-dir-row"></div>
          </div>

          <!-- Hidden inputs kept for backwards-compat with existing JS refs -->
          <div style="display:none">
            <input id="reader-smooth-input" type="checkbox">
            <div id="reader-double-row">
              <input id="reader-double-input" type="checkbox">
            </div>
            <div id="reader-spread-row">
              <input id="reader-spread-input" type="checkbox">
            </div>
            <div id="reader-spread-offset-mount"></div>
          </div>

        </div>
      </div>

    </div>
  `;

  const readerRoot     = /** @type {HTMLElement}       */ (container.querySelector('#reader-root'));
  const canvasEl       = /** @type {HTMLElement}       */ (container.querySelector('#reader-canvas'));
  const tintOverlay    = /** @type {HTMLElement}       */ (container.querySelector('#reader-tint'));
  const pageNumOverlay = /** @type {HTMLElement}       */ (container.querySelector('#reader-page-num'));
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
  const sidePrev        = /** @type {HTMLButtonElement} */ (container.querySelector('#reader-side-prev'));
  const sideNext        = /** @type {HTMLButtonElement} */ (container.querySelector('#reader-side-next'));
  const chapterSelect   = /** @type {HTMLSelectElement} */ (container.querySelector('#reader-chapter-select'));
  const settingsBtn     = /** @type {HTMLButtonElement} */ (container.querySelector('#reader-settings-btn'));
  const sideTitle    = /** @type {HTMLElement}       */ (container.querySelector('#reader-side-title'));
  const panelScroll  = /** @type {HTMLElement}       */ (container.querySelector('#reader-side-scroll'));
  const backMobile   = /** @type {HTMLButtonElement} */ (container.querySelector('#reader-back-mobile'));
  const titleMobile  = /** @type {HTMLElement}       */ (container.querySelector('#reader-title-mobile'));
  const modeMountEl  = /** @type {HTMLElement}       */ (container.querySelector('#reader-mode-mount'));
  const dirRow       = /** @type {HTMLElement}       */ (container.querySelector('#reader-dir-row'));
  const fitMountEl   = /** @type {HTMLElement}       */ (container.querySelector('#reader-fit-mount'));
  const smoothInput  = /** @type {HTMLInputElement}  */ (container.querySelector('#reader-smooth-input'));
  const doubleInput  = /** @type {HTMLInputElement}  */ (container.querySelector('#reader-double-input'));
  const doubleRow    = /** @type {HTMLElement}        */ (container.querySelector('#reader-double-row'));
  const spreadInput  = /** @type {HTMLInputElement}  */ (container.querySelector('#reader-spread-input'));
  const spreadRow    = /** @type {HTMLElement}        */ (container.querySelector('#reader-spread-row'));

  /** @type {import('../reader-prefs.js').ReaderPrefs|null} */
  let _prefs        = null;

  /** @type {string[]} */
  let _pages        = [];
  let _currentPage  = 0;
  // Initialised from localStorage as a fast pre-load; overwritten after loadReaderPrefs resolves.
  const _VALID_MODES = /** @type {const} */ (['scroll', 'paged', 'webtoon', 'continuous-paged']);
  const _storedMode = getLocal('kani_reader_mode') ?? '';
  let _mode = /** @type {import('../reader-prefs.js').ReadingMode} */ (
    _VALID_MODES.includes(/** @type {any} */ (_storedMode)) ? _storedMode : 'scroll'
  );
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
  /** All chapters for this manga, lazily loaded for the chapter dropdown. */
  let _allChapters  = /** @type {Array<{id:number,chapter_number:number,title:string,is_read:boolean}>|null} */ (null);
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

  // ── Presentation ─────────────────────────────────────────────────────────

  /** Apply CSS filter + background colour from prefs to the page container. */
  function _applyPresentation() {
    if (!_prefs) return;
    const { brightness: br, contrast: co, saturation: sa, grayscale: gs, invert: inv, bg, bgTintPage } = _prefs;
    const needsFilter = br !== 100 || co !== 100 || sa !== 100 || gs || inv;
    pagesEl.style.filter = needsFilter
      ? [`brightness(${br}%)`, `contrast(${co}%)`, `saturate(${sa}%)`,
         gs ? 'grayscale(1)' : '', inv ? 'invert(1)' : ''].filter(Boolean).join(' ')
      : '';
    const bgMap = /** @type {Record<string,string>} */ ({ black: '#000', white: '#fff', sepia: '#f5e6c8' }); // audit-ignore: reader background palette
    const bgColor = bgMap[bg] ?? '#000'; // audit-ignore: reader background fallback
    readerRoot.style.backgroundColor = bgColor;
    // canvasEl also needs the bg color so blend modes on pagesEl (inside the isolation
    // context) have the colored backdrop to blend against. Without this, bgTintPage's
    // multiply blend sees transparent (outside the isolation boundary) and has no effect.
    canvasEl.style.backgroundColor = bgColor;
    if (bgTintPage && bg !== 'black') {
      pagesEl.style.mixBlendMode = 'multiply';
    } else {
      pagesEl.style.mixBlendMode = '';
    }
  }

  /** Apply a semi-transparent colour overlay with the chosen blend mode to the page area. */
  function _applyTint() {
    if (!_prefs) return;
    const { tintOpacity: op, tintColor: col, tintBlend: blend } = _prefs;
    if (!op) { tintOverlay.style.display = 'none'; return; }
    const r = parseInt(col.slice(1, 3), 16);
    const g = parseInt(col.slice(3, 5), 16);
    const b = parseInt(col.slice(5, 7), 16);
    tintOverlay.style.display          = '';
    tintOverlay.style.backgroundColor  = `rgba(${r},${g},${b},${op / 100})`; // audit-ignore: built from user-chosen tint colour
    tintOverlay.style.mixBlendMode     = blend;
  }

  function _updatePageOverlay() {
    const on = _prefs?.pageOverlay ?? false;
    if (!on || _pages.length === 0) { pageNumOverlay.style.display = 'none'; return; }
    pageNumOverlay.style.display = '';
    const span = pageNumOverlay.querySelector('span');
    if (span) span.textContent = `${_currentPage + 1} / ${_pages.length}`;
  }

  /**
   * Apply percentage-based clip-path to remove border strips from an image.
   * Crop values are 0–50 (percent of the element's own dimension per side).
   *
   * clip-path inset() % are relative to the element's own box: correct.
   * margin % is always relative to the containing block's WIDTH (even for top/bottom),
   * which equals the image width in our layouts.  For left/right that's correct.
   * For top/bottom it's an approximation: on a 2:3 manga page 1% width ≈ 0.67% height,
   * so we scale by the natural aspect ratio (h/w) when dimensions are known.
   */
  function _applyCropToImg(/** @type {HTMLElement} */ el) {
    const { cropTop: ct = 0, cropBottom: cb = 0, cropLeft: cl = 0, cropRight: cr = 0 } = _prefs ?? /** @type {any} */ ({});
    if (!ct && !cb && !cl && !cr) {
      el.style.clipPath    = '';
      el.style.marginTop   = el.style.marginBottom = '';
      el.style.marginLeft  = el.style.marginRight  = '';
      return;
    }
    el.style.clipPath = `inset(${ct}% ${cr}% ${cb}% ${cl}%)`;
    // Look up aspect ratio to correct the vertical margin approximation.
    const imgEl = /** @type {HTMLImageElement} */ (el);
    const nw = imgEl.naturalWidth  || 0;
    const nh = imgEl.naturalHeight || 0;
    const ratio = (nw > 0 && nh > 0) ? nh / nw : 1.5; // fallback: 2:3 manga page
    el.style.marginTop    = `-${ct * ratio}%`;
    el.style.marginBottom = `-${cb * ratio}%`;
    el.style.marginLeft   = `-${cl}%`;
    el.style.marginRight  = `-${cr}%`;
  }

  /**
   * Re-apply crop styles to all currently visible images in-place.
   * Used by crop sliders to avoid a full DOM re-render (which causes flicker).
   * Canvas composites (spread pages) are NOT updated here — those still need _renderPages().
   */
  function _applyCropToAllImages() {
    pagesEl.querySelectorAll('img').forEach(img => _applyCropToImg(/** @type {HTMLImageElement} */ (img)));
  }

  /** Cropped natural width of an image element after applying percentage prefs. */
  const _cW = (/** @type {HTMLImageElement} */ img) => {
    const { cropLeft: cl = 0, cropRight: cr = 0 } = _prefs ?? /** @type {any} */ ({});
    return Math.max(1, img.naturalWidth  * (1 - (cl + cr) / 100));
  };
  /** Cropped natural height of an image element after applying percentage prefs. */
  const _cH = (/** @type {HTMLImageElement} */ img) => {
    const { cropTop: ct = 0, cropBottom: cb = 0 } = _prefs ?? /** @type {any} */ ({});
    return Math.max(1, img.naturalHeight * (1 - (ct + cb) / 100));
  };

  /**
   * Draw a page image onto a canvas at (dx, dy), honouring crop prefs
   * via the drawImage source-rect so the canvas bitmap is trimmed correctly.
   * @param {CanvasRenderingContext2D} ctx
   * @param {HTMLImageElement} img
   * @param {number} dx
   * @param {number} dy
   */
  function _drawPage(ctx, img, dx, dy) {
    const { cropLeft: cl = 0, cropTop: ct = 0 } = _prefs ?? /** @type {any} */ ({});
    const sw = _cW(img);
    const sh = _cH(img);
    ctx.drawImage(img, cl, ct, sw, sh, dx, dy, sw, sh);
  }

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

  /**
   * Navigate to a different chapter, optionally landing on a specific page.
   * @param {number} chId
   * @param {number} [targetPage] — 0-based page index to jump to on arrival
   */
  async function _navigateChapter(chId, targetPage) {
    _pendingBarsVisible = _barsVisible && !_isFinePointer();
    const suffix = targetPage != null && targetPage > 0 ? `?page=${targetPage}` : '';

    try {
      await api.getChapterPages(chId);
      navigate(`/reader/${chId}${suffix}`);
      return;
    } catch { /* not downloaded yet */ }

    try { await api.downloadChapter(chId); } catch { /* already queued or downloading */ }

    let _dlDone = false;

    /** @param {{ totalPages: number, completedPages: number } | null} p */
    function _renderDlOverlay(p) {
      const progressText = p && p.totalPages > 0
        ? t('reader.dl.progress', { completed: p.completedPages, total: p.totalPages })
        : '';
      pagesEl.innerHTML = `
        <div class="flex flex-col items-center justify-center gap-4 min-h-full text-center px-6">
          <svg class="animate-spin w-8 h-8 text-accent" viewBox="0 0 24 24" fill="none">
            <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"/>
            <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8v8H4z"/>
          </svg>
          <p class="text-sm text-text">${t('reader.dl.loading')}</p>
          ${progressText ? `<p class="text-xs text-text-muted js-dl-progress">${progressText}</p>` : '<p class="text-xs text-text-muted js-dl-progress"></p>'}
          <button class="btn-ghost btn-sm js-dl-cancel">${t('common.cancel')}</button>
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
        navigate(`/reader/${chId}${suffix}`);
        return;
      }

      if (p.status === 'failed' || p.status === 'cancelled') {
        _dlDone = true;
        unsub();
        pagesEl.innerHTML = `
          <div class="flex flex-col items-center justify-center gap-4 min-h-full text-center px-6">
            <p class="text-sm text-danger">${t('reader.dl.status', { status: p.status })}</p>
            <button class="btn-ghost btn-sm js-dl-retry">${t('common.retry')}</button>
            <button class="btn-ghost btn-sm js-dl-back">${t('common.cancel')}</button>
          </div>
        `;
        pagesEl.querySelector('.js-dl-retry')?.addEventListener('click', () => _navigateChapter(chId), { once: true });
        pagesEl.querySelector('.js-dl-back')?.addEventListener('click', () => _renderPages(), { once: true });
        return;
      }

      // Update progress text in-place (avoid full re-render to prevent losing the cancel listener).
      const progressEl = pagesEl.querySelector('.js-dl-progress');
      if (progressEl && p.totalPages > 0) {
        progressEl.textContent = t('reader.dl.progress', { completed: p.completedPages, total: p.totalPages });
      }
    });

    _cleanup.push(() => { _dlDone = true; unsub(); });
  }

  // ── Side panel ───────────────────────────────────────────────────────────

  /** Callbacks invoked each time the side panel opens — avoids MutationObserver hacks. */
  const _panelOpenCallbacks = /** @type {Array<() => void>} */ ([]);

  function _openPanel() {
    _panelOpen = true;
    _positionPanel();
    sidePanel.style.transform = 'translateX(0)';
    backdrop.classList.remove('hidden');
    if (_hideTimer) clearTimeout(_hideTimer);
    _loadChapterList();
    for (const fn of _panelOpenCallbacks) fn();
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

  // ── Chapter dropdown lazy load ─────────────────────────────────────────────

  async function _loadChapterList() {
    if (!_mangaId || _allChapters !== null) return;
    _allChapters = []; // mark as loading (prevents duplicate requests)
    try {
      // page_size is capped at 200 by the API; paginate to collect all chapters.
      const PAGE_SIZE = 200;
      let page = 1;
      const all = [];
      while (true) {
        const result = await api.getLocalChapters(_mangaId, page, PAGE_SIZE, 'chapter_asc');
        const batch = result?.chapters ?? [];
        all.push(...batch);
        if (!result?.has_next_page) break;
        page++;
      }
      _allChapters = all.map((/** @type {any} */ ch) => ({
        id:             Number(ch.id),
        chapter_number: ch.chapter_number ?? ch.number ?? 0,
        title:          formatChapterTitle(ch),
        is_read:        ch.is_read ?? ch.read ?? false,
      }));
      _populateChapterSelect();
    } catch { _allChapters = null; /* allow retry on next panel open */ }
  }

  function _populateChapterSelect() {
    if (!_allChapters || _allChapters.length === 0) return;
    chapterSelect.innerHTML = '';
    for (const ch of _allChapters) {
      const opt = document.createElement('option');
      opt.value = String(ch.id);
      opt.textContent = ch.title;
      if (ch.id === chapterId) opt.selected = true;
      chapterSelect.appendChild(opt);
    }
    chapterSelect.disabled = false;
  }

  chapterSelect.addEventListener('change', () => {
    const id = Number(chapterSelect.value);
    if (id && id !== chapterId) { _closePanel(); _navigateChapter(id); }
  });

  sideBack.addEventListener('click',    () => _navigateToManga());
  sideBackMob.addEventListener('click', () => _navigateToManga());
  backMobile.addEventListener('click',  () => _navigateToManga());

  // ── Indicator bar ────────────────────────────────────────────────────────

  function _showBars() {
    _barsVisible = true;
    fullBar.style.transform = '';
    segsEl.style.pointerEvents = 'auto';
    if (!_isDesktop()) topBar.style.transform = '';
    if (_hideTimer) clearTimeout(_hideTimer);
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
    miniStrip.style.pointerEvents = 'auto';
    miniStrip.addEventListener('click', (e) => {
      e.stopPropagation();
      if (_barsVisible) _hideBars(); else _showBars();
    });
  }

  // ── Jump to page ──────────────────────────────────────────────────────────

  segLeft.style.cursor        = 'pointer';
  segLeft.style.pointerEvents = 'auto';
  segLeft.title               = t('reader.jump_to_page');
  segLeft.addEventListener('click', (e) => {
    e.stopPropagation();
    if (_pages.length === 0) return;
    const input = document.createElement('input');
    input.type  = 'number';
    input.min   = '1';
    input.max   = String(_pages.length);
    input.value = String(_currentPage + 1);
    input.className = 'w-10 text-xs text-center bg-transparent border-b border-accent outline-none tabular-nums text-text';
    segLeft.replaceWith(input);
    input.select();
    const _commit = () => {
      const p = Math.max(0, Math.min(_pages.length - 1, (Number(input.value) || 1) - 1));
      input.replaceWith(segLeft);
      if (p !== _currentPage) { _currentPage = p; _renderPages(); _reportProgress(); }
      else _renderSegments();
    };
    input.addEventListener('keydown', (ev) => {
      if (ev.key === 'Enter')  { ev.preventDefault(); _commit(); }
      if (ev.key === 'Escape') { input.replaceWith(segLeft); }
    });
    input.addEventListener('blur', _commit, { once: true });
    input.focus();
  });

  // ── Smooth scroll toggle ─────────────────────────────────────────────────

  smoothInput.checked = _smoothScroll;
  smoothInput.addEventListener('change', () => {
    _smoothScroll = smoothInput.checked;
    if (_prefs) setReaderPref(_prefs, 'smoothScroll', _smoothScroll);
  });

  // ── Double-page toggle ────────────────────────────────────────────────────

  function _applyDoublePageVisibility() {
    doubleRow.style.display = _mode === 'paged' ? '' : 'none';
    spreadRow.style.display = '';
    // Webtoon: hide direction control (reading is always top-to-bottom).
    dirRow.style.display = _mode === 'webtoon' ? 'none' : '';
  }

  doubleInput.checked = _doublePage;
  doubleInput.addEventListener('change', () => {
    _doublePage = doubleInput.checked;
    if (_prefs) setReaderPref(_prefs, 'doublePage', _doublePage);
    _applyDoublePageVisibility();
    _renderPages();
  });

  // ── Auto-combine spreads toggle ───────────────────────────────────────────

  spreadInput.checked = _autoSpread;
  spreadInput.addEventListener('change', () => {
    _autoSpread = spreadInput.checked;
    if (_prefs) setReaderPref(_prefs, 'autoSpread', _autoSpread);
    _lastLayoutPage = -2;
    _renderPages();
  });

  // ── Dir / Fit / Mode segmented controls ──────────────────────────────────
  // Built post-prefs-load (after the await below) so `selected` reflects the
  // loaded preference. Mount points are injected here; rows are appended there.

  // ── Segment rendering ─────────────────────────────────────────────────────

  function _renderSegments() {
    miniStrip.innerHTML = '';
    segsEl.innerHTML    = '';
    _updatePageOverlay();

    const total = _pages.length;
    if (total === 0) {
      segLeft.textContent  = '—';
      segRight.textContent = '—';
      return;
    }

    segLeft.textContent  = String(_currentPage + 1);
    segRight.textContent = String(total);

    for (let i = 0; i < total; i++) {
      const color = _failed.has(i)     ? 'bg-danger/70'
                  : i === _currentPage ? 'bg-accent'
                  : i < _currentPage   ? 'bg-accent/50'
                  : _loaded.has(i)     ? 'seg-loaded'
                  :                      'seg-unloaded';

      const mini = document.createElement('div');
      mini.className = `flex-1 h-full ${color}`;
      miniStrip.appendChild(mini);

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
      const spreadOffset = _prefs?.spreadOffset ?? false;
      if (!spreadOffset && from === 0) return 1; // page 0 always shown alone unless offset
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
    return from - 1; // fallback: -1 when from===0 signals prev-chapter navigation
  }

  // ── Prefetch ─────────────────────────────────────────────────────────────

  // Rolling fetch-time log for smart preload.
  const FETCH_WINDOW = 8;
  /** @type {number[]} */
  const _fetchMsLog = [];
  /** @type {number|null} — cached result; null = stale, recompute on next call */
  let _cachedPreloadN = /** @type {number|null} */ (null);

  function _recordFetchMs(ms) {
    _fetchMsLog.push(ms);
    if (_fetchMsLog.length > FETCH_WINDOW) _fetchMsLog.shift();
    _cachedPreloadN = null;
  }

  function _prefetch(pageIndex) {
    if (_mode !== 'paged' && _mode !== 'continuous-paged') return;
    const preloadN = _adaptivePreload();
    for (let i = 1; i <= preloadN; i++) {
      const prefIdx = pageIndex + i;
      const url = _pages[prefIdx];
      if (url && !_loaded.has(prefIdx) && !_failed.has(prefIdx)) {
        const img = new Image();
        const fetchStart = performance.now();
        img.addEventListener('load', () => {
          _recordFetchMs(performance.now() - fetchStart);
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

  // ── Zoom & pan state ──────────────────────────────────────────────────────
  let _zoomScale = 1;
  let _zoomTx    = 0;
  let _zoomTy    = 0;
  const ZOOM_MIN = 1.0;
  const ZOOM_MAX = 5.0;

  /** Returns the element to zoom in paged mode; null otherwise (no-op).
   * continuous-paged is excluded because _cpTrack also owns the translateY snap —
   * both writes target style.transform and would race each other. */
  function _zoomTarget() {
    return _mode === 'paged'
      ? /** @type {HTMLElement|null} */ (pagesEl.firstElementChild)
      : null;
  }

  function _applyZoom() {
    const t = _zoomTarget();
    if (!t) return;
    if (_zoomScale <= 1) {
      t.style.transform = '';
      t.style.transformOrigin = '';
      t.style.cursor = '';
    } else {
      t.style.transformOrigin = '0 0';
      t.style.transform = `translate(${_zoomTx}px,${_zoomTy}px) scale(${_zoomScale})`;
      t.style.cursor = 'grab';
    }
  }

  function _resetZoom() {
    const t = _zoomTarget();
    _zoomScale = 1; _zoomTx = 0; _zoomTy = 0;
    if (t) { t.style.transform = ''; t.style.transformOrigin = ''; t.style.cursor = ''; }
  }

  function _clampPan() {
    const rect = pagesEl.getBoundingClientRect();
    const maxTx = 0,  minTx = Math.min(0, rect.width  * (1 - _zoomScale));
    const maxTy = 0,  minTy = Math.min(0, rect.height * (1 - _zoomScale));
    _zoomTx = Math.max(minTx, Math.min(maxTx, _zoomTx));
    _zoomTy = Math.max(minTy, Math.min(maxTy, _zoomTy));
  }

  function _zoomAt(factor, clientX, clientY) {
    const t = _zoomTarget();
    let cx, cy;
    if (t) {
      const r = t.getBoundingClientRect();
      cx = clientX - (r.left - _zoomTx);
      cy = clientY - (r.top  - _zoomTy);
    } else {
      const r = pagesEl.getBoundingClientRect();
      cx = clientX - r.left;
      cy = clientY - r.top;
    }
    const prev = _zoomScale;
    _zoomScale = Math.max(ZOOM_MIN, Math.min(ZOOM_MAX, _zoomScale * factor));
    if (_zoomScale <= 1) { _zoomTx = 0; _zoomTy = 0; _zoomScale = 1; }
    else {
      _zoomTx = cx - (_zoomScale / prev) * (cx - _zoomTx);
      _zoomTy = cy - (_zoomScale / prev) * (cy - _zoomTy);
      _clampPan();
    }
    _applyZoom();
  }

  // ── Image class helpers (used by both scroll and paged renderers) ─────────

  /** Returns CSS classes for page images given current fit mode and layout context. */
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
    if (_fit === 'height') return 'max-h-full w-auto';
    if (_fit === 'width')  return 'max-w-[50vw] max-h-full';
    return 'max-w-[50vw] max-h-full object-contain'; // both
  }

  /** CSS classes for a spread canvas (already a bitmap — no object-contain). */
  function _spreadClass() {
    if (_fit === 'height') return 'max-h-full w-auto';
    if (_fit === 'width')  return 'max-w-full h-auto';
    return 'max-w-full max-h-full';
  }

  /**
   * CSS classes for the paged/continuous-paged outer container.
   * Enables native scroll on the non-constrained axis in fit-width/height modes.
   */
  function _pagedContainerClass() {
    const overflow = _fit === 'width'  ? 'overflow-y-auto overflow-x-hidden'
                   : _fit === 'height' ? 'overflow-x-auto overflow-y-hidden'
                   : 'overflow-hidden';
    // fit-width: align to top so the image starts at scrollTop=0 (items-center would push
    // the top of a tall image above the scroll origin, making it unreachable).
    const align = _fit === 'width' ? 'items-start' : 'items-center';
    return `flex-1 ${overflow} ${align} relative flex justify-center`;
  }

  // ── Continuous-paged renderer ────────────────────────────────────────────
  // Renders a window of pages as absolutely-positioned slots. Navigation snaps
  // by translating the container — no re-render on page change.

  /** @type {HTMLElement|null} */
  let _cpTrack = null;

  function _renderContinuousPaged() {
    const preload = _adaptivePreload();
    const windowStart = Math.max(0, _currentPage - preload);
    const windowEnd   = Math.min(_pages.length - 1, _currentPage + preload);

    pagesEl.className = _pagedContainerClass();

    const track = document.createElement('div');
    track.className = 'absolute inset-0 flex flex-col items-center reader-cp-track';
    track.style.willChange = 'transform';
    _cpTrack = track;

    for (let i = windowStart; i <= windowEnd; i++) {
      const slot = document.createElement('div');
      slot.className = 'w-full flex-shrink-0 flex items-center justify-center';
      slot.style.height = '100%';

      const img = document.createElement('img');
      img.src           = _pages[i] ?? '';
      img.className     = _imgClass('paged-single');
      img.style.aspectRatio = '2/3'; // placeholder until dimensions are known
      img.alt           = `Page ${i + 1}`;
      img.dataset.index = String(i);
      const _i = i;
      img.addEventListener('load', () => {
        img.style.aspectRatio = '';
        _applyCropToImg(img);
        _loaded.add(_i); _failed.delete(_i);
        if (img.naturalWidth > 0) _setDims(_i, img.naturalWidth, img.naturalHeight);
        _renderSegments();
      });
      img.addEventListener('error', () => { _failed.add(_i); _loaded.delete(_i); _renderSegments(); });
      if (img.complete && img.naturalWidth) {
        img.style.aspectRatio = '';
        _loaded.add(i); _setDims(i, img.naturalWidth, img.naturalHeight);
      }
      _applyCropToImg(img);
      slot.appendChild(img);
      track.appendChild(slot);
    }

    pagesEl.appendChild(track);
    _cpSnapToPage(_currentPage, windowStart);
  }

  function _cpSnapToPage(pageIdx, windowStart) {
    if (!_cpTrack) return;
    const offset = pageIdx - (windowStart ?? _currentPage - (_prefs?.preloadCount ?? 2));
    const h = pagesEl.getBoundingClientRect().height || window.innerHeight;
    _cpTrack.style.transform = `translateY(${-offset * h}px)`;
  }

  function _renderPages() {
    _cpTrack = null;
    if (_scrollObs) { _scrollObs.disconnect(); _scrollObs = null; }
    _resetZoom();
    pagesEl.innerHTML = '';

    if (_pages.length === 0) {
      pagesEl.className = 'flex-1 overflow-y-auto overflow-x-hidden flex flex-col items-center';
      const emptyWrap = document.createElement('div');
      emptyWrap.className = 'flex items-center justify-center min-h-full';
      emptyWrap.appendChild(createEmptyState({ title: t('reader.empty.title'), subtitle: t('reader.empty.subtitle') }));
      pagesEl.appendChild(emptyWrap);
      _renderSegments();
      return;
    }

    const _isScrollLike = _mode === 'scroll' || _mode === 'webtoon';

    if (_isScrollLike) {
      pagesEl.className = _mode === 'webtoon'
        ? 'flex-1 overflow-y-auto overflow-x-hidden flex flex-col items-center'
        : 'flex-1 overflow-y-auto overflow-x-hidden flex flex-col items-center gap-1 py-2';

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
        const W = _cW(leftImg) + _cW(rightImg);
        const H = Math.max(_cH(leftImg), _cH(rightImg));
        const canvas = document.createElement('canvas');
        canvas.className   = _spreadClass(); // bitmap already has correct ratio — no object-contain
        canvas.dataset.index = String(idxA);
        canvas.width  = W;
        canvas.height = H;
        const ctx = canvas.getContext('2d');
        if (ctx) {
          _drawPage(ctx, leftImg,  0,           (H - _cH(leftImg))  / 2);
          _drawPage(ctx, rightImg, _cW(leftImg), (H - _cH(rightImg)) / 2);
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
        img.style.aspectRatio = '2/3'; // reserve space before dimensions are known
        img.dataset.index = String(i);
        const _i = i;
        img.addEventListener('load', () => {
          img.style.aspectRatio = '';
          _applyCropToImg(img);
          _loaded.add(_i); _failed.delete(_i);
          if (img.naturalWidth > 0) {
            _setDims(_i, img.naturalWidth, img.naturalHeight);
            if (_autoSpread) {
              _maybeComposite(_i);
              if (_i > 0) _maybeComposite(_i - 1);
            }
          }
          _renderSegments();
        });
        img.addEventListener('error', () => { _failed.add(_i); _loaded.delete(_i); _renderSegments(); });
        if (img.complete) {
          if (img.naturalWidth) { _loaded.add(i); _setDims(i, img.naturalWidth, img.naturalHeight); }
          else _failed.add(i);
        }
        _applyCropToImg(img);
        _scrollImgs.set(i, img);
        pagesEl.appendChild(img);
      }

      {
        const card = document.createElement('div');
        card.className = 'flex flex-col items-center justify-center py-16 gap-4 w-full shrink-0';
        if (_chapterInfo.next_chapter_id) {
          card.innerHTML = `
            <p class="text-muted text-sm">${t('reader.end.chapter')}</p>
            <button class="btn-ghost flex items-center gap-1">
              ${t('reader.end.next_chapter')} ${iconChevronRight}
            </button>
          `;
          card.querySelector('button')?.addEventListener('click', () => {
            _navigateChapter(_chapterInfo.next_chapter_id);
          });
        } else {
          card.innerHTML = `
            <p class="text-muted text-sm">${t('reader.end.chapter')}</p>
            <button class="btn-ghost flex items-center gap-1">
              ${iconChevronLeft} ${t('reader.back_to_manga')}
            </button>
          `;
          card.querySelector('button')?.addEventListener('click', () => {
            _navigateToManga();
          });
        }
        pagesEl.appendChild(card);
      }

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
      // Webtoon: higher threshold for finer per-panel progress.
      }, { root: pagesEl, threshold: _mode === 'webtoon' ? 0.5 : 0.1 });
      pagesEl.querySelectorAll('[data-index]').forEach(el => _scrollObs?.observe(el));

    } else {
      _currentPage = Math.max(0, Math.min(_pages.length - 1, _currentPage));

      if (_mode === 'continuous-paged') {
        _renderContinuousPaged();
        _renderSegments();
        return;
      }

      pagesEl.className = _pagedContainerClass();

      /** @param {number} pageIdx @param {string} altText @returns {HTMLImageElement} */
      function _makePageImg(pageIdx, altText) {
        const img     = document.createElement('img');
        img.src       = _pages[pageIdx] ?? '';
        img.className = _imgClass(_doublePage ? 'paged-double' : 'paged-single');
        img.style.aspectRatio = '2/3'; // reserve space before dimensions are known
        img.alt       = altText;
        img.addEventListener('load', () => {
          img.style.aspectRatio = ''; // clear placeholder once natural size is known
          _applyCropToImg(img);       // re-apply crop with correct aspect ratio now known
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
          err.innerHTML = `<p class="text-muted text-sm">${t('reader.error.page', { page: pageIdx + 1 })}</p>`;
          pagesEl.appendChild(err);
        });
        if (img.complete) {
          if (img.naturalWidth) {
            img.style.aspectRatio = '';
            _loaded.add(pageIdx);
            _setDims(pageIdx, img.naturalWidth, img.naturalHeight);
          } else {
            _failed.add(pageIdx);
          }
        }
        _applyCropToImg(img);
        return img;
      }

      // When spreadOffset is on, pairs start from page 0 (no lone first page).
      // When off (default), page 0 is always shown alone to match cover convention.
      const _firstAlone = _doublePage && !(_prefs?.spreadOffset ?? false);

      if (_doublePage && _pages.length > 1) {
        const leftIdx  = _direction === 'rtl' ? _currentPage + 1 : _currentPage;
        const rightIdx = _direction === 'rtl' ? _currentPage     : _currentPage + 1;

        if (_firstAlone && _currentPage === 0) {
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
            const W = _cW(leftImg) + _cW(rightImg);
            const H = Math.max(_cH(leftImg), _cH(rightImg));
            canvas.width  = W;
            canvas.height = H;
            const ctx = canvas.getContext('2d');
            if (!ctx) return;
            _drawPage(ctx, leftImg,  0,            (H - _cH(leftImg))  / 2);
            _drawPage(ctx, rightImg, _cW(leftImg),  (H - _cH(rightImg)) / 2);
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
          const W = _cW(leftImg) + _cW(rightImg);
          const H = Math.max(_cH(leftImg), _cH(rightImg));
          canvas.width = W; canvas.height = H;
          const ctx = canvas.getContext('2d');
          if (!ctx) return;
          _drawPage(ctx, leftImg,  0,            (H - _cH(leftImg))  / 2);
          _drawPage(ctx, rightImg, _cW(leftImg),  (H - _cH(rightImg)) / 2);
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
          err.innerHTML = `<p class="text-muted text-sm">${t('reader.error.spread')}</p><button class="btn-ghost">${t('common.retry')}</button>`;
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
            <p class="text-muted text-sm">${t('reader.error.page', { page: failedPage + 1 })}</p>
            <button class="btn-ghost">${t('common.retry')}</button>
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
    if (_mode !== 'paged' && _mode !== 'continuous-paged') return;
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
    _recordPace();

    if (_mode === 'continuous-paged' && _cpTrack) {
      const first = Number(_cpTrack.firstElementChild?.querySelector('[data-index]')?.dataset.index ?? -1);
      const last  = Number(_cpTrack.lastElementChild?.querySelector('[data-index]')?.dataset.index ?? -1);
      if (_currentPage >= first && _currentPage <= last) {
        _cpSnapToPage(_currentPage, first);
        _renderSegments();
        return; // no re-render needed; page already in DOM
      }
    }
    _renderPages();
  }

  // ── Three-zone click ──────────────────────────────────────────────────────

  function _triggerZoneAction(action) {
    switch (action) {
      case 'prev': _goPage(_direction === 'rtl' ? 1  : -1); break;
      case 'next': _goPage(_direction === 'rtl' ? -1 : 1);  break;
      case 'menu':
        if (_isFinePointer()) _openPanel();
        else if (_barsVisible) _hideBars(); else _showBars();
        break;
    }
  }

  pagesEl.addEventListener('click', (e) => {
    const target = /** @type {HTMLElement} */ (e.target);
    if (target.closest('button') || target.closest('a')) return;
    if (_zoomScale > 1) return; // suppress nav while zoomed

    const rect  = pagesEl.getBoundingClientRect();
    const x     = e.clientX - rect.left;
    const third = rect.width / 3;

    const tapLeft   = _prefs?.tapLeft   ?? 'prev';
    const tapCenter = _prefs?.tapCenter ?? 'menu';
    const tapRight  = _prefs?.tapRight  ?? 'next';

    if (_mode === 'paged' || _mode === 'continuous-paged') {
      if (x < third)      { _triggerZoneAction(tapLeft);   return; }
      if (x > 2 * third)  { _triggerZoneAction(tapRight);  return; }
    }
    if (x >= third && x <= 2 * third) {
      _triggerZoneAction(tapCenter);
    }
  });

  // ── Touch/swipe ───────────────────────────────────────────────────────────

  // ── Zoom + swipe touch handler (replaces simple touchstart/touchend) ─────

  let _touchStartX   = 0;
  let _touchStartY   = 0;
  let _pinchDist     = 0;
  let _panActive     = false;
  let _panStartTx    = 0;
  let _panStartTy    = 0;

  pagesEl.addEventListener('touchstart', (e) => {
    if (e.touches.length === 2) {
      e.preventDefault();
      const dx = e.touches[0].clientX - e.touches[1].clientX;
      const dy = e.touches[0].clientY - e.touches[1].clientY;
      _pinchDist = Math.hypot(dx, dy);
      return;
    }
    _touchStartX = e.touches[0].clientX;
    _touchStartY = e.touches[0].clientY;
    if (_zoomScale > 1) {
      e.preventDefault();
      _panActive  = true;
      _panStartTx = _zoomTx;
      _panStartTy = _zoomTy;
    }
  }, { passive: false });

  pagesEl.addEventListener('touchmove', (e) => {
    if (e.touches.length === 2) {
      e.preventDefault();
      const dx = e.touches[0].clientX - e.touches[1].clientX;
      const dy = e.touches[0].clientY - e.touches[1].clientY;
      const newDist = Math.hypot(dx, dy);
      if (_pinchDist > 0 && newDist > 0) {
        const midX = (e.touches[0].clientX + e.touches[1].clientX) / 2;
        const midY = (e.touches[0].clientY + e.touches[1].clientY) / 2;
        _zoomAt(newDist / _pinchDist, midX, midY);
      }
      _pinchDist = newDist;
      return;
    }
    if (_panActive && e.touches.length === 1) {
      e.preventDefault();
      _zoomTx = _panStartTx + (e.touches[0].clientX - _touchStartX);
      _zoomTy = _panStartTy + (e.touches[0].clientY - _touchStartY);
      _clampPan();
      _applyZoom();
    }
  }, { passive: false });

  pagesEl.addEventListener('touchend', (e) => {
    _pinchDist = 0;
    _panActive = false;
    if (_zoomScale > 1) return; // suppress swipe nav while zoomed

    const dx = e.changedTouches[0].clientX - _touchStartX;
    const dy = e.changedTouches[0].clientY - _touchStartY;

    if (Math.abs(dx) > 50 && Math.abs(dx) > Math.abs(dy)) _goPage(dx < 0 ? 1 : -1);
  });

  // ── Wheel zoom (desktop) ──────────────────────────────────────────────────
  // Paged + fit=both: plain wheel zooms.
  // Paged + fit=width/height: plain wheel scrolls the overflow axis; ctrl+wheel zooms.
  // Scroll/webtoon: wheel always scrolls; ctrl+wheel zooms.
  // When already zoomed: wheel always zooms regardless of mode/fit.

  pagesEl.addEventListener('wheel', (e) => {
    const isPaged = _mode === 'paged' || _mode === 'continuous-paged';
    const alreadyZoomed = _zoomScale > 1;

    if (alreadyZoomed) {
      e.preventDefault();
      _zoomAt(e.deltaY < 0 ? 1.1 : 0.9, e.clientX, e.clientY);
      return;
    }

    // ctrl+wheel always zooms (browser pinch-to-zoom emulation sends ctrlKey=true).
    if (e.ctrlKey) {
      e.preventDefault();
      _zoomAt(e.deltaY < 0 ? 1.1 : 0.9, e.clientX, e.clientY);
      return;
    }

    // Plain wheel in paged+both: zoom in/out (no native scroll at zoom=1 in this mode).
    if (isPaged && _fit === 'both') {
      e.preventDefault();
      _zoomAt(e.deltaY < 0 ? 1.1 : 0.9, e.clientX, e.clientY);
      return;
    }

  }, { passive: false });

  // ── Mouse drag-to-pan ─────────────────────────────────────────────────────

  let _mousePanActive = false;
  let _mousePanStartX = 0;
  let _mousePanStartY = 0;
  let _mousePanTx0    = 0;
  let _mousePanTy0    = 0;

  pagesEl.addEventListener('mousedown', (e) => {
    if (_zoomScale <= 1 || e.button !== 0) return;
    const target = /** @type {HTMLElement} */ (e.target);
    if (target.closest('button') || target.closest('a')) return;
    _mousePanActive = true;
    _mousePanStartX = e.clientX;
    _mousePanStartY = e.clientY;
    _mousePanTx0    = _zoomTx;
    _mousePanTy0    = _zoomTy;
    pagesEl.style.cursor = 'grabbing';
    e.preventDefault();
  });
  const _onMouseMove = (/** @type {MouseEvent} */ e) => {
    if (!_mousePanActive) return;
    _zoomTx = _mousePanTx0 + (e.clientX - _mousePanStartX);
    _zoomTy = _mousePanTy0 + (e.clientY - _mousePanStartY);
    _clampPan();
    _applyZoom();
  };
  const _onMouseUp = () => {
    if (!_mousePanActive) return;
    _mousePanActive = false;
    pagesEl.style.cursor = '';
    _applyZoom();
  };
  document.addEventListener('mousemove', _onMouseMove);
  document.addEventListener('mouseup',   _onMouseUp);
  _cleanup.push(() => {
    document.removeEventListener('mousemove', _onMouseMove);
    document.removeEventListener('mouseup',   _onMouseUp);
  });

  // ── Fullscreen ───────────────────────────────────────────────────────────

  function _toggleFullscreen() {
    if (document.fullscreenElement) {
      document.exitFullscreen().catch(() => {});
    } else {
      readerRoot.requestFullscreen().catch(() => {});
    }
  }

  // ── Slideshow ────────────────────────────────────────────────────────────

  let _slideshowActive = false;
  let _slideshowTimer  = /** @type {ReturnType<typeof setTimeout>|null} */ (null);
  /** Timestamp of the most recent _slideshowPlay() call — used to ignore the
      starting tap in _slideshowPauseOnInput so it doesn't immediately stop. */
  let _slideshowStartedAt = 0;
  /** Reference to the Start/Stop button; set once the panel is built. */
  let _ssPlayBtn = /** @type {HTMLButtonElement|null} */ (null);

  function _slideshowStop() {
    _slideshowActive = false;
    if (_slideshowTimer) { clearTimeout(_slideshowTimer); _slideshowTimer = null; }
    if (_ssPlayBtn) _ssPlayBtn.textContent = t('reader.slideshow.start');
  }

  function _slideshowAdvance() {
    if (!_slideshowActive) return;
    const isScrollLike = _mode === 'scroll' || _mode === 'webtoon';
    if (isScrollLike) {
      const scrollable = /** @type {HTMLElement|null} */ (pagesEl);
      if (scrollable) {
        const before = scrollable.scrollTop;
        scrollable.scrollBy({ top: scrollable.clientHeight, behavior: 'smooth' });
        setTimeout(() => {
          if (!_slideshowActive) return;
          if (Math.abs(scrollable.scrollTop - before) < 4 || // didn't scroll
              scrollable.scrollTop + scrollable.clientHeight >= scrollable.scrollHeight - 4) {
            if (_chapterInfo.next_chapter_id) {
              _navigateChapter(_chapterInfo.next_chapter_id);
            }
          }
        }, 600);
      }
    } else {
      // Always advance forward: compensate for RTL so _goPage's direction flip doesn't reverse us.
      _goPage(_direction === 'rtl' ? -1 : 1);
    }
  }

  function _slideshowSchedule() {
    if (!_slideshowActive) return;
    const ms = (_prefs?.slideshowInterval ?? 5) * 1000;
    _slideshowTimer = setTimeout(() => {
      if (!_slideshowActive) return;
      _slideshowAdvance();
      _slideshowSchedule();
    }, ms);
  }

  function _slideshowPlay() {
    _slideshowActive = true;
    _slideshowStartedAt = Date.now();
    _slideshowSchedule();
    if (_ssPlayBtn) _ssPlayBtn.textContent = t('reader.slideshow.stop');
  }

  function _slideshowPauseOnInput() {
    // Ignore the very interaction that started the slideshow (the tap/click that
    // pressed the Start button) so the slideshow doesn't immediately cancel itself.
    if (Date.now() - _slideshowStartedAt < 500) { _resetInactivity(); return; }
    if (_slideshowActive) _slideshowStop();
    _resetInactivity();
  }

  _cleanup.push(_slideshowStop);

  // ── Inactivity timer ──────────────────────────────────────────────────────

  let _inactivityTimer = /** @type {ReturnType<typeof setTimeout>|null} */ (null);

  function _resetInactivity() {
    if (_inactivityTimer) clearTimeout(_inactivityTimer);
    const ms = (_prefs?.inactivityTimeout ?? 0) * 60000;
    if (!ms) return;
    _inactivityTimer = setTimeout(() => {
      if (_slideshowActive) _slideshowStop();
      else _navigateToManga();
    }, ms);
  }

  _cleanup.push(() => { if (_inactivityTimer) clearTimeout(_inactivityTimer); });

  const _onUserInput = () => _slideshowPauseOnInput();
  document.addEventListener('keydown',     _onUserInput, { capture: true, passive: true });
  document.addEventListener('pointerdown', _onUserInput, { capture: true, passive: true });
  _cleanup.push(() => {
    document.removeEventListener('keydown',     _onUserInput, { capture: true });
    document.removeEventListener('pointerdown', _onUserInput, { capture: true });
  });

  // ── Reading-stats pace tracking ───────────────────────────────────────────

  const PACE_WINDOW = 10;
  /** @type {{ time: number }[]} */
  const _paceLog = [];

  function _recordPace() {
    _paceLog.push({ time: Date.now() });
    if (_paceLog.length > PACE_WINDOW + 1) _paceLog.shift();
    _cachedPreloadN = null;
  }

  /** Returns pages-per-minute rolling average, or null if not enough data. */
  function _ppm() {
    if (_paceLog.length < 2) return null;
    const dt = (_paceLog[_paceLog.length - 1].time - _paceLog[0].time) / 60000;
    return dt > 0 ? (_paceLog.length - 1) / dt : null;
  }

  function _etaText() {
    const rate = _ppm();
    if (!rate || _pages.length === 0) return null;
    const remaining = _pages.length - 1 - _currentPage;
    if (remaining <= 0) return '0 min';
    const mins = remaining / rate;
    return mins < 1 ? '<1 min' : `~${Math.round(mins)} min`;
  }

  /**
   * Smart preload count: how many images can fully load within one
   * average page-read interval, clamped to [1, user max].
   * Falls back to the configured max when there isn't enough data yet.
   */
  function _adaptivePreload() {
    if (_cachedPreloadN !== null) return _cachedPreloadN;
    const max = _prefs?.preloadCount ?? 2;
    if (_fetchMsLog.length < 3 || _paceLog.length < 2) return max;
    const avgFetchMs = _fetchMsLog.reduce((a, b) => a + b, 0) / _fetchMsLog.length;
    const pagesPerMin = _ppm();
    if (!pagesPerMin || avgFetchMs <= 0) return max;
    const msPerPage = 60000 / pagesPerMin;
    _cachedPreloadN = Math.max(1, Math.min(max, Math.floor(msPerPage / avgFetchMs)));
    return _cachedPreloadN;
  }

  // ── Volume-key navigation ─────────────────────────────────────────────────

  // keydown: browser doesn't normally fire for hardware volume, but PWA/some
  // Android WebViews do surface AudioVolume* keys.
  const _onVolumeKey = (/** @type {KeyboardEvent} */ e) => {
    if (e.key === 'AudioVolumeUp')   { e.preventDefault(); if (!_panelOpen) _goPage(-1); }
    if (e.key === 'AudioVolumeDown') { e.preventDefault(); if (!_panelOpen) _goPage(1);  }
  };
  document.addEventListener('keydown', _onVolumeKey);
  _cleanup.push(() => document.removeEventListener('keydown', _onVolumeKey));

  // MediaSession: fires from Bluetooth / headset media buttons.
  if ('mediaSession' in navigator) {
    try {
      navigator.mediaSession.setActionHandler('previoustrack', () => { if (!_panelOpen) _goPage(-1); });
      navigator.mediaSession.setActionHandler('nexttrack',     () => { if (!_panelOpen) _goPage(1);  });
    } catch { /* unsupported action */ }
    _cleanup.push(() => {
      try {
        navigator.mediaSession.setActionHandler('previoustrack', null);
        navigator.mediaSession.setActionHandler('nexttrack',     null);
      } catch { /* ignore */ }
    });
  }

  // ── Keyboard ─────────────────────────────────────────────────────────────

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
    { key: 'f', description: 'Toggle fullscreen', handler: () => { if (!_panelOpen) _toggleFullscreen(); } },
    { key: 'Escape', description: 'Back to manga', handler: () => { if (!_panelOpen) _navigateToManga(); } },
  ]));

  // Position the panel immediately (before the await) so there is no flash of
  // the desktop default position on mobile before the JS takes effect.
  _positionPanel();

  // ── Landscape-aware double-page ───────────────────────────────────────────
  const _landscapeMQ = window.matchMedia('(orientation: landscape) and (max-height: 600px)');
  const _onLandscapeChange = (/** @type {MediaQueryListEvent|MediaQueryList} */ mq) => {
    if (_mode !== 'paged') return;
    if (mq.matches && !_doublePage) {
      _doublePage = true;
      doubleInput.checked = true;
      if (_prefs) setReaderPref(_prefs, 'doublePage', true);
      _applyDoublePageVisibility(); _renderPages();
    } else if (!mq.matches && _doublePage) {
      _doublePage = false;
      doubleInput.checked = false;
      if (_prefs) setReaderPref(_prefs, 'doublePage', false);
      _applyDoublePageVisibility(); _renderPages();
    }
  };
  _landscapeMQ.addEventListener('change', _onLandscapeChange);
  _cleanup.push(() => _landscapeMQ.removeEventListener('change', _onLandscapeChange));

  const { row: preloadRow, input: preloadInput } = mkSliderRow({
    label: t('reader.settings.preload'), min: 1, max: 10, value: _prefs?.preloadCount ?? 2,
    onChange: (v) => { if (_prefs) setReaderPref(_prefs, 'preloadCount', v); },
  });

  const saveBtn = mkActionBtn({
    label: t('reader.settings.save_page'),
    onClick: async () => {
      const canvas = /** @type {HTMLCanvasElement|null} */ (pagesEl.querySelector('canvas'));
      if (canvas) {
        canvas.toBlob((blob) => {
          if (!blob) return;
          const url = URL.createObjectURL(blob);
          const a = document.createElement('a');
          a.href = url;
          a.download = `page-${_currentPage + 1}.png`;
          a.click();
          URL.revokeObjectURL(url);
        }, 'image/png');
        return;
      }
      // Single image: fetch the raw bytes so we get the original format.
      const pageUrl = _pages[_currentPage];
      if (!pageUrl) return;
      try {
        const resp = await fetch(pageUrl);
        const blob = await resp.blob();
        const ext  = blob.type.includes('png') ? 'png' : 'jpg';
        const url  = URL.createObjectURL(blob);
        const a    = document.createElement('a');
        a.href     = url;
        a.download = `page-${_currentPage + 1}.${ext}`;
        a.click();
        URL.revokeObjectURL(url);
      } catch { /* network error — ignore */ }
    },
  });

  const _modalOpenCallbacks = /** @type {Array<() => void>} */ ([]);
  const _modalSections = /** @type {Record<string, HTMLElement>} */ ({});
  _modalSections.navigation = mkReaderSection('', saveBtn, preloadRow);

  const fsBtn = mkActionBtn({ label: t('reader.settings.fullscreen'), onClick: () => _toggleFullscreen() });
  const _onFsChange = () => {
    fsBtn.textContent = document.fullscreenElement ? t('reader.settings.exit_fullscreen') : t('reader.settings.fullscreen');
  };
  document.addEventListener('fullscreenchange', _onFsChange);
  _cleanup.push(() => document.removeEventListener('fullscreenchange', _onFsChange));

  // ── Keep screen on ────────────────────────────────────────────────────────
  const displayChildren = /** @type {HTMLElement[]} */ ([fsBtn]);

  if ('wakeLock' in navigator) {
    let _wakeLock = /** @type {WakeLockSentinel|null} */ (null);
    const { row: wakeRow, input: wakeInput } = mkToggleRow({
      label: t('reader.settings.wake_lock'),
      checked: false,
      onChange: async (on) => {
        if (on) {
          try {
            _wakeLock = await navigator.wakeLock.request('screen');
            _wakeLock.addEventListener('release', () => { _wakeLock = null; });
          } catch { wakeInput.checked = false; }
        } else {
          await _wakeLock?.release();
          _wakeLock = null;
        }
      },
    });
    const _onVisibility = async () => {
      if (document.visibilityState === 'visible' && wakeInput.checked) {
        try {
          _wakeLock = await navigator.wakeLock.request('screen');
          _wakeLock.addEventListener('release', () => { _wakeLock = null; });
        } catch { /* best-effort */ }
      }
    };
    document.addEventListener('visibilitychange', _onVisibility);
    _cleanup.push(() => {
      document.removeEventListener('visibilitychange', _onVisibility);
      _wakeLock?.release().catch(() => {});
    });
    displayChildren.push(wakeRow);
  }

  // ── Orientation lock ──────────────────────────────────────────────────────
  // Only show on touch/coarse-pointer devices — orientation lock requires
  // mobile fullscreen and always rejects on desktop browsers.
  if ('lock' in (screen.orientation ?? {}) && !_isFinePointer()) {
    const { row: orientRow } = mkSegmentedRow({
      label: t('reader.settings.orientation'),
      options: [
        { value: 'auto',      label: t('reader.orient.auto') },
        { value: 'portrait',  label: t('reader.orient.portrait') },
        { value: 'landscape', label: t('reader.orient.landscape') },
      ],
      selected: 'auto',
      onSelect: async (v) => {
        try {
          if (v === 'auto') screen.orientation.unlock();
          // @ts-ignore — OrientationLockType not in all TS libs
          else await screen.orientation.lock(v);
        } catch { /* best-effort; unsupported outside fullscreen on some browsers */ }
      },
    });
    _cleanup.push(() => { try { screen.orientation.unlock(); } catch { /* ignore */ } });
    displayChildren.push(orientRow);
  }

  const _displayAccordion = mkAccordionSection(t('reader.panel.display'), {}, ...displayChildren);

  // ── Load chapter ──────────────────────────────────────────────────────────

  try {
    const data = await api.getChapterPages(chapterId);

    _pages = Array.isArray(data?.pages)
      ? data.pages.map(p => api.getChapterPageUrl(chapterId, p.index))
      : [];
    _chapterInfo = data ?? {};
    _mangaId     = data?.manga_id ?? null;
    _loadChapterList();

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

    _prefs = await loadReaderPrefs(_mangaId);
    _mode        = _VALID_MODES.includes(_prefs.mode) ? _prefs.mode : 'scroll';
    _smoothScroll = _prefs.smoothScroll;
    _doublePage  = _prefs.doublePage;
    _direction   = _prefs.direction;
    _fit         = _prefs.fit;
    _autoSpread  = _prefs.autoSpread;
    smoothInput.checked    = _smoothScroll;
    doubleInput.checked    = _doublePage;
    spreadInput.checked    = _autoSpread;
    preloadInput.value     = String(_prefs.preloadCount);
    preloadInput.dispatchEvent(new Event('input'));

    _applyPresentation();
    _applyTint();

    {
      const spreadOffsetMount = container.querySelector('#reader-spread-offset-mount');
      const { row: spreadOffsetRow } = mkToggleRow({
        label: t('reader.settings.spread_offset'),
        checked: _prefs.spreadOffset ?? false,
        onChange: (v) => {
          if (_prefs) { setReaderPref(_prefs, 'spreadOffset', v); _renderPages(); }
        },
      });
      spreadOffsetRow.style.display = _doublePage ? '' : 'none';
      spreadOffsetMount?.appendChild(spreadOffsetRow);
      doubleInput.addEventListener('change', () => {
        spreadOffsetRow.style.display = doubleInput.checked ? '' : 'none';
      });
    }

    // ── Fit / Dir / Mode segmented controls ───────────────────────────────────

    const { row: fitRow, update: _updateFit } = mkSegmentedRow({
      label: t('reader.settings.fit'),
      options: [
        { value: 'both',   label: t('reader.fit.both')   },
        { value: 'width',  label: t('reader.fit.width')  },
        { value: 'height', label: t('reader.fit.height') },
      ],
      selected: _fit,
      onSelect: (v) => {
        _fit = /** @type {'both'|'width'|'height'} */ (v);
        if (_prefs) setReaderPref(_prefs, 'fit', _fit);
        _renderPages();
      },
    });
    fitMountEl.appendChild(fitRow);

    const { row: dirRow_ } = mkSegmentedRow({
      label: t('reader.settings.direction'),
      options: [
        { value: 'rtl', label: t('reader.dir.rtl') },
        { value: 'ltr', label: t('reader.dir.ltr') },
      ],
      selected: _direction,
      onSelect: (v) => {
        _direction = /** @type {'rtl'|'ltr'} */ (v);
        if (_prefs) setReaderPref(_prefs, 'direction', _direction);
        for (const d of _imgDims.values()) d.edgeMatch = undefined;
        _renderPages();
      },
    });
    dirRow.appendChild(dirRow_);

    const { row: modeRow } = mkSegmentedRow({
      options: [
        { value: 'scroll',           label: t('reader.mode.scroll')    },
        { value: 'paged',            label: t('reader.mode.paged')     },
        { value: 'webtoon',          label: t('reader.mode.webtoon')   },
        { value: 'continuous-paged', label: t('reader.mode.continuous') },
      ],
      selected: _mode,
      onSelect: (v) => {
        _mode = /** @type {import('../reader-prefs.js').ReadingMode} */ (v);
        if (_mode === 'webtoon' && _fit === 'both') {
          _fit = 'width';
          if (_prefs) setReaderPref(_prefs, 'fit', _fit);
          _updateFit('width');
        }
        if (_prefs) setReaderPref(_prefs, 'mode', _mode);
        _applyDoublePageVisibility();
        _renderPages();
      },
    });
    modeMountEl.appendChild(modeRow);

    // ── Image adjustments ─────────────────────────────────────────────────
    {
      const p = _prefs;

      const { row: bgRow } = mkSegmentedRow({
        label: t('reader.settings.bg'),
        options: [
          { value: 'black', label: t('reader.bg.black') },
          { value: 'white', label: t('reader.bg.white') },
          { value: 'sepia', label: t('reader.bg.sepia') },
        ],
        selected: p.bg,
        onSelect: (v) => {
          if (_prefs) { setReaderPref(_prefs, 'bg', v); _applyPresentation(); }
        },
      });

      const { row: brRow } = mkSliderRow({ label: t('reader.settings.brightness'), min: 50, max: 200, value: p.brightness, unit: '%',
        onChange: (v) => { if (_prefs) { setReaderPref(_prefs, 'brightness', v); _applyPresentation(); } } });
      const { row: coRow } = mkSliderRow({ label: t('reader.settings.contrast'),   min: 50, max: 200, value: p.contrast,   unit: '%',
        onChange: (v) => { if (_prefs) { setReaderPref(_prefs, 'contrast',   v); _applyPresentation(); } } });
      const { row: saRow } = mkSliderRow({ label: t('reader.settings.saturation'), min: 0,  max: 200, value: p.saturation, unit: '%',
        onChange: (v) => { if (_prefs) { setReaderPref(_prefs, 'saturation', v); _applyPresentation(); } } });

      const { row: gsRow } = mkToggleRow({ label: t('reader.settings.grayscale'), checked: p.grayscale,
        onChange: (v) => { if (_prefs) { setReaderPref(_prefs, 'grayscale', v); _applyPresentation(); } } });
      const { row: invRow } = mkToggleRow({ label: t('reader.settings.invert'),   checked: p.invert,
        onChange: (v) => { if (_prefs) { setReaderPref(_prefs, 'invert',    v); _applyPresentation(); } } });

      const { row: ctRow } = mkSliderRow({ label: t('reader.settings.crop_top'),    min: 0, max: 50, value: p.cropTop,    unit: '%',
        onChange: (v) => { if (_prefs) { setReaderPref(_prefs, 'cropTop',    v); _applyCropToAllImages(); } } });
      const { row: cbRow } = mkSliderRow({ label: t('reader.settings.crop_bottom'), min: 0, max: 50, value: p.cropBottom, unit: '%',
        onChange: (v) => { if (_prefs) { setReaderPref(_prefs, 'cropBottom', v); _applyCropToAllImages(); } } });
      const { row: clRow } = mkSliderRow({ label: t('reader.settings.crop_left'),   min: 0, max: 50, value: p.cropLeft,   unit: '%',
        onChange: (v) => { if (_prefs) { setReaderPref(_prefs, 'cropLeft',   v); _applyCropToAllImages(); } } });
      const { row: crRow } = mkSliderRow({ label: t('reader.settings.crop_right'),  min: 0, max: 50, value: p.cropRight,  unit: '%',
        onChange: (v) => { if (_prefs) { setReaderPref(_prefs, 'cropRight',  v); _applyCropToAllImages(); } } });

      const tintColorRow = document.createElement('div');
      tintColorRow.className = 'flex items-center justify-between gap-3';
      const tintColorLbl = document.createElement('span');
      tintColorLbl.className = 'text-sm text-text';
      tintColorLbl.textContent = t('reader.settings.tint_color');
      const tintColorInput = document.createElement('input');
      tintColorInput.type = 'color';
      tintColorInput.value = p.tintColor;
      tintColorInput.className = 'w-8 h-8 rounded cursor-pointer border border-border bg-transparent shrink-0';
      tintColorInput.addEventListener('input', () => {
        if (_prefs) { setReaderPref(_prefs, 'tintColor', tintColorInput.value); _applyTint(); }
      });
      tintColorRow.appendChild(tintColorLbl);
      tintColorRow.appendChild(tintColorInput);

      const { row: tintOpRow } = mkSliderRow({ label: t('reader.settings.tint_opacity'), min: 0, max: 100, value: p.tintOpacity, unit: '%',
        onChange: (v) => { if (_prefs) { setReaderPref(_prefs, 'tintOpacity', v); _applyTint(); } } });

      const { row: tintBlendRow } = mkSegmentedRow({
        label: t('reader.settings.blend_mode'),
        options: [
          { value: 'multiply', label: t('reader.blend.multiply') },
          { value: 'screen',   label: t('reader.blend.screen')   },
          { value: 'overlay',  label: t('reader.blend.overlay')  },
          { value: 'color',    label: t('reader.blend.color')    },
        ],
        selected: p.tintBlend,
        onSelect: (v) => { if (_prefs) { setReaderPref(_prefs, 'tintBlend', v); _applyTint(); } },
      });

      const { row: bgTintRow } = mkToggleRow({ label: t('reader.settings.tint_bg'), checked: p.bgTintPage,
        onChange: (v) => { if (_prefs) { setReaderPref(_prefs, 'bgTintPage', v); _applyPresentation(); } } });

      _modalSections.image = mkReaderSection('', bgRow, bgTintRow, brRow, coRow, saRow, gsRow, invRow, ctRow, cbRow, clRow, crRow);
      _modalSections.tint  = mkReaderSection('', tintColorRow, tintOpRow, tintBlendRow);
    }

    // ── Tap zones ─────────────────────────────────────────────────────────
    {
      const zoneOptions = [
        { value: 'prev', label: t('reader.zone.prev') },
        { value: 'next', label: t('reader.zone.next') },
        { value: 'menu', label: t('reader.zone.menu') },
        { value: 'none', label: t('reader.zone.none') },
      ];

      const tzHint = document.createElement('p');
      tzHint.className = 'text-xs text-danger hidden';
      tzHint.textContent = t('reader.tap_zone.guard');

      /** Ensure at least one zone stays set to 'menu'; shows hint if blocked. */
      function _guardZone(key, val, fallbackKey) {
        if (!_prefs) return;
        const other1 = key === 'tapLeft'   ? _prefs.tapCenter : _prefs.tapLeft;
        const other2 = key === 'tapRight'  ? _prefs.tapCenter : _prefs.tapRight;
        const wouldLockout = val !== 'menu' && other1 !== 'menu' && other2 !== 'menu';
        if (wouldLockout) {
          tzHint.classList.remove('hidden');
          if (key !== 'tapCenter') {
            if (_prefs) setReaderPref(_prefs, 'tapCenter', 'menu');
            tzcUpdate('menu');
          } else {
            tzcUpdate('menu');
            return;
          }
        } else {
          tzHint.classList.add('hidden');
        }
        setReaderPref(_prefs, key, val);
      }

      const { row: tzlRow, update: tzlUpdate } = mkSegmentedRow({ label: t('reader.settings.zone_left'),   options: zoneOptions, selected: _prefs.tapLeft,
        onSelect: (v) => _guardZone('tapLeft',   v, 'tapCenter') });
      const { row: tzcRow, update: tzcUpdate } = mkSegmentedRow({ label: t('reader.settings.zone_center'), options: zoneOptions, selected: _prefs.tapCenter,
        onSelect: (v) => _guardZone('tapCenter', v, 'tapLeft')  });
      const { row: tzrRow, update: tzrUpdate } = mkSegmentedRow({ label: t('reader.settings.zone_right'),  options: zoneOptions, selected: _prefs.tapRight,
        onSelect: (v) => _guardZone('tapRight',  v, 'tapCenter') });

      _modalSections.controls = mkReaderSection(t('reader.panel.tap_zones'), tzlRow, tzcRow, tzrRow, tzHint);
    }

    // ── Dual-scanlator comparison ─────────────────────────────────────────
    const _alts = data?.scanlator_alternatives ?? [];
    if (_alts.length > 0) {
      const _primaryPages  = _pages.slice();
      const _primaryChId   = chapterId;

      /**
       * @typedef {{ chId: number, scanlator: string|null, volume: number|null }} AltEntry
       */
      /** @type {AltEntry[]} */
      const _allEntries = [
        { chId: _primaryChId, scanlator: data?.scanlator ?? null, volume: data?.chapter_number ?? null },
        ..._alts.map(a => ({ chId: a.chapter_id, scanlator: a.scanlator ?? null, volume: a.volume ?? null })),
      ];

      /**
       * Build a human-readable, collision-free label for an entry.
       * Duplicates the scanlator string (including null→"Unknown") across entries
       * get a volume suffix, then a numeric ID suffix as a last resort.
       * @param {AltEntry} entry
       * @returns {string}
       */
      function _scanlatorLabel(entry) {
        const base = entry.scanlator ?? t('reader.scanlator.unknown');
        const sameBase = _allEntries.filter(e => e.scanlator === entry.scanlator);
        if (sameBase.length === 1) return base;
        const sameVol = sameBase.filter(e => e.volume === entry.volume);
        if (entry.volume != null && sameVol.length === 1) return `${base} (Vol. ${entry.volume})`;
        return `${base} (#${entry.chId})`;
      }

      /** @type {Map<number, string[]>} */
      const _pageCache = new Map([[_primaryChId, _primaryPages]]);

      const { row: selectRow, select: scanlatorSelect } = mkSelectRow({
        options: _allEntries.map(entry => ({
          value: String(entry.chId),
          label: _scanlatorLabel(entry) + (entry.chId === _primaryChId ? ` (${t('reader.scanlator.current')})` : ''),
        })),
        selected: String(_primaryChId),
        onChange: async (val) => {
          scanlatorSelect.disabled = true;
          const chId = Number(val);
          try {
            if (!_pageCache.has(chId)) {
              const d = await api.getChapterPages(chId);
              _pageCache.set(chId, (d?.pages ?? []).map(p => api.getChapterPageUrl(chId, p.index)));
            }
            _pages = /** @type {string[]} */ (_pageCache.get(chId)).slice();
            _currentPage = Math.min(_currentPage, Math.max(0, _pages.length - 1));
            _renderPages();
          } catch {
            const active = [..._pageCache.entries()].find(([, v]) => v === _pages)?.[0] ?? _primaryChId;
            scanlatorSelect.value = String(active);
          } finally {
            scanlatorSelect.disabled = false;
          }
        },
      });

      panelScroll.appendChild(mkAccordionSection(t('reader.panel.scanlators'), {}, selectRow));
    }

    // ── Bookmarks ─────────────────────────────────────────────────────────
    if (_mangaId) {
      /** @type {Set<number>} */
      let _bookmarks = new Set();

      const bookmarkStar = mkActionBtn({ label: t('reader.bookmark.add'), onClick: async () => {
        try {
          const res = await api.toggleBookmark(chapterId, _currentPage);
          if (res.bookmarked) _bookmarks.add(_currentPage);
          else _bookmarks.delete(_currentPage);
          _refreshBookmarkUI();
        } catch { /* ignore */ }
      }});

      const bookmarkList = document.createElement('div');
      bookmarkList.className = 'flex flex-col gap-1';

      function _refreshBookmarkUI() {
        bookmarkStar.textContent = _bookmarks.has(_currentPage)
          ? t('reader.bookmark.remove') : t('reader.bookmark.add');
        bookmarkList.innerHTML = '';
        if (_bookmarks.size === 0) {
          const empty = document.createElement('p');
          empty.className = 'text-xs text-muted';
          empty.textContent = t('reader.bookmarks.empty');
          bookmarkList.appendChild(empty);
        } else {
          const sorted = [..._bookmarks].sort((a, b) => a - b);
          for (const pg of sorted) {
            const row = document.createElement('button');
            row.className = 'text-xs text-left text-text hover:text-accent py-0.5';
            row.textContent = `Page ${pg + 1}`;
            row.addEventListener('click', () => {
              _currentPage = pg; _renderPages(); _closePanel();
            });
            bookmarkList.appendChild(row);
          }
        }
      }

      api.getBookmarks(chapterId).then(pages => {
        _bookmarks = new Set(pages);
        _refreshBookmarkUI();
      }).catch(() => {});

      _panelOpenCallbacks.push(_refreshBookmarkUI);

      panelScroll.appendChild(mkAccordionSection(t('reader.panel.bookmarks'), {}, bookmarkStar, bookmarkList));
    }

    // ── Per-chapter note ──────────────────────────────────────────────────
    {
      const noteArea = document.createElement('textarea');
      noteArea.className = 'w-full text-sm bg-surface-2 border border-border rounded-md px-2 py-1.5 resize-none outline-none focus:border-accent';
      noteArea.rows = 3;
      noteArea.placeholder = t('reader.note.placeholder');

      const _saveNote = debounce(
        () => api.setChapterNote(chapterId, noteArea.value).catch(() => {}),
        1000,
      );
      noteArea.addEventListener('input', _saveNote);
      // Flush immediately on destroy so a note typed within the debounce window isn't lost.
      _cleanup.push(() => {
        _saveNote.cancel();
        if (noteArea.value) api.setChapterNote(chapterId, noteArea.value).catch(() => {});
      });

      api.getChapterNote(chapterId).then(res => {
        if (res?.note) noteArea.value = res.note;
      }).catch(() => {});

      panelScroll.appendChild(mkAccordionSection(t('reader.panel.note'), {}, noteArea));
    }

    panelScroll.appendChild(_displayAccordion);

    const { row: overlayRow } = mkToggleRow({
      label: t('reader.settings.page_overlay'),
      checked: _prefs.pageOverlay,
      onChange: (v) => {
        if (_prefs) { setReaderPref(_prefs, 'pageOverlay', v); _updatePageOverlay(); }
      },
    });

    const { row: ssSpeedRow } = mkSliderRow({
      label: t('reader.settings.slideshow_speed'), min: 3, max: 30, value: _prefs.slideshowInterval, unit: 's',
      onChange: (v) => { if (_prefs) setReaderPref(_prefs, 'slideshowInterval', v); },
    });
    const ssBtn = mkActionBtn({
      label: _slideshowActive ? t('reader.slideshow.stop') : t('reader.slideshow.start'),
      onClick: () => {
        if (_slideshowActive) {
          _slideshowStop();
        } else {
          _slideshowPlay();
          _closeSettingsModal();
          _closePanel();
        }
      },
    });
    _ssPlayBtn = ssBtn;

    const { row: sleepRow } = mkSliderRow({
      label: t('reader.settings.sleep'), min: 0, max: 60, value: _prefs.inactivityTimeout, unit: 'min',
      onChange: (v) => { if (_prefs) { setReaderPref(_prefs, 'inactivityTimeout', v); _resetInactivity(); } },
    });

    const statsDiv = document.createElement('div');
    statsDiv.className = 'flex flex-col gap-1';
    const etaLine = document.createElement('div');
    etaLine.className = 'flex items-center justify-between';
    const etaLbl = document.createElement('span');
    etaLbl.className = 'text-xs text-muted';
    etaLbl.textContent = t('reader.stats.eta');
    const etaVal = document.createElement('span');
    etaVal.className = 'text-xs text-text tabular-nums';
    etaVal.textContent = '—';
    etaLine.appendChild(etaLbl);
    etaLine.appendChild(etaVal);
    const paceLine = document.createElement('div');
    paceLine.className = 'flex items-center justify-between';
    const paceLbl = document.createElement('span');
    paceLbl.className = 'text-xs text-muted';
    paceLbl.textContent = t('reader.stats.pace');
    const paceVal = document.createElement('span');
    paceVal.className = 'text-xs text-text tabular-nums';
    paceVal.textContent = '—';
    paceLine.appendChild(paceLbl);
    paceLine.appendChild(paceVal);
    statsDiv.appendChild(etaLine);
    statsDiv.appendChild(paceLine);

    const _updateStats = () => {
      const eta  = _etaText();
      const rate = _ppm();
      etaVal.textContent  = eta  ?? '—';
      paceVal.textContent = rate ? `${rate.toFixed(1)} p/min` : '—';
    };
    _modalOpenCallbacks.push(_updateStats);

    _modalSections.reading = mkReaderSection('', overlayRow, ssBtn, ssSpeedRow, sleepRow, statsDiv);

    // ── Settings modal ────────────────────────────────────────────────────────

    /** @type {HTMLElement|null} */
    let _settingsModalEl = null;

    /**
     * Open the reader settings modal, optionally jumping to a tab.
     * Built lazily on first call; subsequent calls just show the cached shell.
     * @param {string} [defaultTab]
     */
    function _openSettingsModal(defaultTab = 'layout') {
      if (_settingsModalEl) {
        _settingsModalEl.style.display = '';
        activateTab(defaultTab);
        for (const fn of _modalOpenCallbacks) fn();
        return;
      }

      const overlay = document.createElement('div');
      overlay.className = 'fixed inset-0 flex items-center justify-center p-3 sm:p-6 bg-scrim z-modal';
      overlay.style.zIndex = 'var(--z-modal, 9999)';

      const card = document.createElement('div');
      card.className = 'bg-surface rounded-xl shadow-lg flex flex-col w-full max-w-2xl max-h-[90vh] min-h-0';
      card.addEventListener('click', e => e.stopPropagation());

      const header = document.createElement('div');
      header.className = 'flex items-center justify-between px-5 py-4 border-b border-border shrink-0';
      const htitle = document.createElement('h2');
      htitle.className = 'text-base font-semibold text-text';
      htitle.textContent = t('reader.settings.title');
      const hclose = document.createElement('button');
      hclose.className = 'btn-icon';
      hclose.setAttribute('aria-label', t('reader.aria.close_settings'));
      hclose.innerHTML = iconX;
      hclose.addEventListener('click', _closeSettingsModal);
      header.appendChild(htitle);
      header.appendChild(hclose);

      const tabStrip = document.createElement('div');
      tabStrip.className = 'shrink-0';

      const body = document.createElement('div');
      body.className = 'flex-1 overflow-y-auto min-h-0 p-4 flex flex-col gap-3';

      card.appendChild(header);
      card.appendChild(tabStrip);
      card.appendChild(body);
      overlay.appendChild(card);
      _settingsModalEl = overlay;

      // ── Layout tab ─────────────────────────────────────────────────────────
      const layoutSection = document.createElement('div');
      layoutSection.className = 'flex flex-col gap-3';

      const { row: smRow } = mkToggleRow({
        label: t('reader.settings.smooth_scroll'),
        checked: _prefs.smoothScroll,
        onChange: (v) => {
          _smoothScroll = v; smoothInput.checked = v;
          if (_prefs) setReaderPref(_prefs, 'smoothScroll', v);
          if (_mode === 'scroll' || _mode === 'webtoon') _renderPages();
        },
      });

      const { row: dpRow } = mkToggleRow({
        label: t('reader.settings.double_page'),
        checked: _prefs.doublePage,
        onChange: (v) => {
          _doublePage = v; doubleInput.checked = v;
          if (_prefs) setReaderPref(_prefs, 'doublePage', v);
          _applyDoublePageVisibility(); _renderPages();
        },
      });

      const { row: asRow } = mkToggleRow({
        label: t('reader.settings.auto_spread'),
        checked: _prefs.autoSpread,
        onChange: (v) => {
          _autoSpread = v; spreadInput.checked = v;
          if (_prefs) setReaderPref(_prefs, 'autoSpread', v);
          _renderPages();
        },
      });

      const { row: soRow } = mkToggleRow({
        label: t('reader.settings.spread_offset'),
        checked: _prefs.spreadOffset ?? false,
        onChange: (v) => { if (_prefs) { setReaderPref(_prefs, 'spreadOffset', v); _renderPages(); } },
      });

      layoutSection.appendChild(smRow);
      layoutSection.appendChild(dpRow);
      layoutSection.appendChild(asRow);
      layoutSection.appendChild(soRow);

      const shortcutsEl = document.createElement('div');
      shortcutsEl.className = 'flex flex-col gap-1 border-t border-border pt-3 mt-1';
      const scTitle = document.createElement('p');
      scTitle.className = 'text-xs font-medium text-muted uppercase tracking-wide mb-1';
      scTitle.textContent = t('reader.shortcuts.title');
      shortcutsEl.appendChild(scTitle);
      for (const entry of getShortcuts('reader')) {
        const scRow = document.createElement('div');
        scRow.className = 'flex items-center justify-between gap-4';
        const sdesc = document.createElement('span');
        sdesc.className = 'text-xs text-muted';
        sdesc.textContent = entry.description;
        const skbd = document.createElement('kbd');
        skbd.className = 'text-xs bg-surface-2 border border-border rounded px-1.5 py-0.5 font-mono shrink-0';
        skbd.textContent = entry.key;
        scRow.appendChild(sdesc);
        scRow.appendChild(skbd);
        shortcutsEl.appendChild(scRow);
      }
      _modalSections.controls?.appendChild(shortcutsEl);

      const TABS = [
        { id: 'layout',     name: t('reader.tab.layout'),     section: layoutSection },
        { id: 'image',      name: t('reader.tab.image'),      section: _modalSections.image },
        { id: 'tint',       name: t('reader.tab.tint'),       section: _modalSections.tint },
        { id: 'navigation', name: t('reader.tab.navigation'), section: _modalSections.navigation },
        { id: 'controls',   name: t('reader.tab.controls'),   section: _modalSections.controls },
        { id: 'reading',    name: t('reader.tab.reading'),    section: _modalSections.reading },
      ].filter(tab => tab.section);

      for (const tab of TABS) {
        tab.section.style.display = 'none';
        body.appendChild(tab.section);
      }

      const { update: updateTabs } = renderTabs(tabStrip, {
        tabs: TABS.map(t => ({ id: t.id, name: t.name })),
        activeId: defaultTab,
        onSelect: activateTab,
        variant: 'underline',
      });

      function activateTab(/** @type {string} */ id) {
        for (const t of TABS) t.section.style.display = t.id === id ? '' : 'none';
        updateTabs(id);
      }
      activateTab(defaultTab);

      document.body.appendChild(overlay);

      overlay.addEventListener('click', _closeSettingsModal);

      const _onModalKey = (/** @type {KeyboardEvent} */ e) => {
        if (e.key === 'Escape') _closeSettingsModal();
      };
      document.addEventListener('keydown', _onModalKey);
      _cleanup.push(() => {
        document.removeEventListener('keydown', _onModalKey);
        _settingsModalEl?.remove();
      });

      for (const fn of _modalOpenCallbacks) fn();
    }

    function _closeSettingsModal() {
      if (_settingsModalEl) _settingsModalEl.style.display = 'none';
    }

    settingsBtn?.addEventListener('click', () => { _closePanel(); _openSettingsModal(); });
    setF1Override(() => _openSettingsModal('controls'));
    _cleanup.push(() => setF1Override(null));

    _resetInactivity();

    if (data?.last_page_read != null) {
      _currentPage = data.last_page_read;
    }
    if (_currentPage === -1) _currentPage = _pages.length - 1;

    // ?page= query param overrides the server-stored last_page_read (used by chapter navigation).
    const _qp = new URLSearchParams(location.search);
    const _qpPage = parseInt(_qp.get('page') ?? '', 10);
    if (!isNaN(_qpPage) && _qpPage >= 0) _currentPage = _qpPage;

    _currentPage = Math.max(0, Math.min(Math.max(_pages.length - 1, 0), _currentPage));

    if (data?.chapter_title) {
      titleMobile.textContent = data.chapter_title;
      document.title = data.chapter_title + ' - Kani';

      sideTitle.textContent = '';
      const headerRow = document.createElement('div');
      headerRow.className = 'flex items-center gap-1.5 min-w-0';
      const titleLine = document.createElement('span');
      titleLine.className = 'text-sm font-medium text-text truncate';
      titleLine.textContent = data.chapter_title;
      headerRow.appendChild(titleLine);
      sideTitle.appendChild(headerRow);
      const meta = [data.source_name, data.scanlator].filter(Boolean).join(' · ');
      if (meta) {
        const metaLine = document.createElement('span');
        metaLine.className = 'block text-xs text-muted truncate';
        metaLine.textContent = meta;
        sideTitle.appendChild(metaLine);
      }
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
    pagesEl.innerHTML = `<div class="flex items-center justify-center min-h-full"><p class="text-danger text-sm">${t('reader.error.chapter')}</p></div>`;
  }

  _applyDoublePageVisibility();
  _renderPages();
  if (_pendingBarsVisible) _showBars();
  pagesEl.focus();

  if (getLocal('kani_download_ahead_enabled') === 'true' && _chapterInfo.next_chapter_id) {
    const aheadCount = Math.max(1, Math.min(10, parseInt(getLocal('kani_download_ahead_count') || '3', 10)));
    const _downloadAheadTimer = setTimeout(async () => {
      let nextId = _chapterInfo.next_chapter_id;
      for (let i = 0; i < aheadCount && nextId; i++) {
        try {
          await api.downloadChapter(nextId);
        } catch { /* already downloading/downloaded — ignore */ }
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
    if (_prefs) cancelReaderPrefsSync(_prefs);
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
