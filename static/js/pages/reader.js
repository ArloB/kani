// @ts-check
// Reader page — full-screen chapter reader with page-by-page and scroll modes.

import * as api from '../api.js';
import { navigate } from '../router.js';
import { getLocal, getLocalJson, setLocalJson, formatChapterTitle } from '../utils.js';
import { t } from '../i18n.js';
import { getState, subscribe } from '../cache.js';
import { registerShortcuts, getShortcuts, setF1Override } from '../shortcuts.js';
import { createEmptyState } from '../components/empty-state.js';
import { loadReaderPrefs, setReaderPref, cancelReaderPrefsSync } from '../reader-prefs.js';
import { createReaderEngine } from './reader/engine.js';
import { createSlideshow } from './reader/slideshow.js';
import { createChromeVisibility } from './reader/chrome-visibility.js';
import { createIndicatorBar } from '../components/reader/indicator-bar.js';
import { createDownloadOverlay } from '../components/reader/download-overlay.js';
import { createTopBar, createSidePanelHeader, createChapterNav } from '../components/reader/chrome.js';
import { signal, effect } from '@preact/signals';
import { h, render } from 'preact';
import { ReaderSettingsModal } from '../components/reader/settings-modal.js';
import { SegmentedRow } from '../components/reader/settings-controls.js';
import { mountPanelData } from './reader/panel-data.js';
import { ActionBtn, ToggleRow } from '../components/reader/settings-controls.js';
import {
  iconReaderScroll, iconReaderPaged, iconReaderWebtoon, iconReaderContinuous,
  iconFitBoth, iconFitWidth, iconFitHeight, iconArrowLeftLine, iconArrowRightLine,
} from '../icons.js';

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
        style="transform: translateY(-100%); padding-top: env(safe-area-inset-top, 0px)"></div>

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

        <div id="reader-dl-overlay" class="hidden absolute inset-0" style="z-index:3"></div>
      </div>

      <!-- Page-number overlay badge: shown when pageOverlay pref is enabled. -->
      <div id="reader-page-num" class="absolute bottom-20 right-3 pointer-events-none select-none" style="z-index:2;display:none">
        <span class="text-xs font-medium tabular-nums rounded-md px-2 py-1 bg-black/70 text-white backdrop-blur-sm ring-1 ring-white/15"></span><!-- audit-ignore: page-number badge over arbitrary page content -->
      </div>

      <!-- Mini progress strip — always visible, 4px, z-20, pointer-events-none.
           Horizontal padding matches the segment area of the full bar:
           px-4 (16) + w-6 (24) + gap-3 (12) = 52px on each side. -->
      <div id="reader-mini-strip"
        class="absolute bottom-0 inset-x-0 z-20 h-1.5 flex gap-px pointer-events-none"
        style="padding-left:52px;padding-right:52px">
      </div>

      <!-- Page-count strip mode (miniStrip = 'pagecount'): small centred counter. -->
      <div id="reader-mini-count" class="absolute bottom-1 inset-x-0 z-20 flex justify-center pointer-events-none select-none" style="display:none">
        <span class="text-[11px] leading-tight tabular-nums rounded px-1.5 bg-black/55 text-white/85 backdrop-blur-sm"></span><!-- audit-ignore: counter chip over arbitrary page content -->
      </div>

      <!-- Full indicator bar — slides up from bottom on hover/tap, z-21.
           Sits above the mini strip and overlays the page content. -->
      <div id="reader-full-bar"
        class="absolute bottom-0 inset-x-0 flex items-center gap-3 px-4 h-14 bg-surface/90 backdrop-blur-sm border-t border-border/40 transition-transform duration-150 reader-bar"
        style="transform:translateY(100%)">
        <span id="reader-seg-left"
          class="text-xs text-muted w-6 text-right shrink-0 tabular-nums select-none">—</span>
        <div id="reader-segs"
          class="flex flex-1 gap-0.5 h-9 items-stretch pointer-events-none"></div>
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
      <div id="reader-side-panel" role="dialog" aria-modal="false" aria-label="${t('reader.aria.open_menu')}" tabindex="-1"
        class="absolute top-0 bottom-0 w-72 bg-surface flex flex-col shadow-lg border-border z-40 transition-transform duration-150 outline-none"
        style="transform: translateX(-100%); left: 0">

        <div id="reader-side-header" class="flex items-center gap-2 px-3 h-14 border-b border-border shrink-0"></div>

        <div id="reader-side-scroll" class="flex flex-col flex-1 overflow-y-auto">

          <div id="reader-chapter-nav"></div>

          <div class="px-3 py-4 border-b border-border flex flex-col gap-3">
            <p class="text-xs font-medium text-muted uppercase tracking-wide">${t('reader.group.reading')}</p>
            <div id="reader-mode-mount"></div>
            <div id="reader-fit-mount"></div>
            <div id="reader-dir-row"></div>
            <div id="reader-double-mount"></div>
            <div id="reader-fs-mount"></div>
          </div>

        </div>
      </div>

    </div>
  `;

  const readerRoot     = /** @type {HTMLElement}       */ (container.querySelector('#reader-root'));
  const canvasEl       = /** @type {HTMLElement}       */ (container.querySelector('#reader-canvas'));
  const tintOverlay    = /** @type {HTMLElement}       */ (container.querySelector('#reader-tint'));
  const dlOverlayEl    = /** @type {HTMLElement}       */ (container.querySelector('#reader-dl-overlay'));
  const pageNumOverlay = /** @type {HTMLElement}       */ (container.querySelector('#reader-page-num'));
  const topBar       = /** @type {HTMLElement}       */ (container.querySelector('#reader-top'));
  const pagesEl      = /** @type {HTMLElement}       */ (container.querySelector('#reader-pages'));
  const miniStrip    = /** @type {HTMLElement}       */ (container.querySelector('#reader-mini-strip'));
  const miniCount    = /** @type {HTMLElement}       */ (container.querySelector('#reader-mini-count'));
  const fullBar      = /** @type {HTMLElement}       */ (container.querySelector('#reader-full-bar'));
  const barHover     = /** @type {HTMLElement}       */ (container.querySelector('#reader-bar-hover'));
  const segLeft      = /** @type {HTMLElement}       */ (container.querySelector('#reader-seg-left'));
  const segsEl       = /** @type {HTMLElement}       */ (container.querySelector('#reader-segs'));
  const segRight     = /** @type {HTMLElement}       */ (container.querySelector('#reader-seg-right'));
  const backdrop     = /** @type {HTMLElement}       */ (container.querySelector('#reader-side-backdrop'));
  const sidePanel    = /** @type {HTMLElement}       */ (container.querySelector('#reader-side-panel'));
  const sideHeaderEl = /** @type {HTMLElement}       */ (container.querySelector('#reader-side-header'));
  const chapterNavEl = /** @type {HTMLElement}       */ (container.querySelector('#reader-chapter-nav'));
  const panelScroll  = /** @type {HTMLElement}       */ (container.querySelector('#reader-side-scroll'));
  const modeMountEl  = /** @type {HTMLElement}       */ (container.querySelector('#reader-mode-mount'));
  const dirRow       = /** @type {HTMLElement}       */ (container.querySelector('#reader-dir-row'));
  const fitMountEl   = /** @type {HTMLElement}       */ (container.querySelector('#reader-fit-mount'));
  const doubleMountEl = /** @type {HTMLElement}      */ (container.querySelector('#reader-double-mount'));
  const fsMountEl    = /** @type {HTMLElement}       */ (container.querySelector('#reader-fs-mount'));

  /** @type {import('../reader-prefs.js').ReaderPrefs|null} */
  let _prefs        = null;
  /** @type {import('@preact/signals').Signal<import('../reader-prefs.js').ReaderPrefs|null>} */
  const prefsSignal = signal(null);
  const tapHintSignal = signal(false);
  const slideshowSignal = signal(false);
  const statsSignal = signal({ eta: '—', pace: '—' });

  // Initialised from localStorage as a fast pre-load; overwritten after loadReaderPrefs resolves.
  const _VALID_MODES = /** @type {const} */ (['scroll', 'paged', 'webtoon', 'continuous-paged']);
  const _storedMode = getLocal('kani_reader_mode') ?? '';
  const _storedFit = getLocal('kani_reader_fit') ?? '';

  /**
   * Shared reader/engine state (Wave 10 R2/R4). The engine and the vanilla
   * chrome both read and write these fields; keeping them in one object lets
   * the render/nav cluster migrate into the engine without desyncing.
   * @type {{
   *   pages: string[], currentPage: number,
   *   mode: import('../reader-prefs.js').ReadingMode,
   *   smoothScroll: boolean, doublePage: boolean,
   *   direction: 'rtl'|'ltr', fit: 'both'|'width'|'height',
   *   autoSpread: boolean,
   *   imgDims: Map<number, {w: number, h: number, edgeMatch?: boolean|null}>,
   *   lastLayoutPage: number, hasServerAnalysis: boolean,
   *   serverDoublePages: Set<number>,
   *   chapterInfo: { prev_chapter_id?: number|null, next_chapter_id?: number|null },
   *   scrollObs: IntersectionObserver|null,
   *   loaded: Set<number>, failed: Set<number>,
   *   cpTrack: HTMLElement|null, preloadDone: boolean,
   *   cachedPreloadN: number|null,
   * }}
   */
  // Reader-state store. The scalar reader fields are signal-backed (transparent
  // getters/setters) so the engine mutates them imperatively as before while the
  // chrome (indicator, page overlay) reacts via effects — no onSegments bridge.
  // `loadVersion` is the notification channel for the in-place-mutated loaded/
  // failed Sets (which signals can't observe directly).
  const _sig = {
    pages:        signal(/** @type {string[]} */ ([])),
    currentPage:  signal(0),
    mode:         signal(/** @type {import('../reader-prefs.js').ReadingMode} */ (
                    _VALID_MODES.includes(/** @type {any} */ (_storedMode)) ? _storedMode : 'scroll')),
    smoothScroll: signal(getLocal('kani_reader_smooth') === 'true'),
    doublePage:   signal(getLocal('kani_reader_double') === 'true'),
    direction:    signal(/** @type {'rtl'|'ltr'} */ (getLocal('kani_reader_direction') === 'ltr' ? 'ltr' : 'rtl')),
    fit:          signal(/** @type {'both'|'width'|'height'} */ (['both', 'width', 'height'].includes(_storedFit) ? _storedFit : 'both')),
    autoSpread:   signal(getLocal('kani_reader_spread') !== 'false'), // default true
    loadVersion:  signal(0),
  };
  const state = {
    get pages() { return _sig.pages.value; },
    set pages(v) { _sig.pages.value = v; },
    get currentPage() { return _sig.currentPage.value; },
    set currentPage(v) { _sig.currentPage.value = v; },
    get mode() { return _sig.mode.value; },
    set mode(v) { _sig.mode.value = v; },
    get smoothScroll() { return _sig.smoothScroll.value; },
    set smoothScroll(v) { _sig.smoothScroll.value = v; },
    get doublePage() { return _sig.doublePage.value; },
    set doublePage(v) { _sig.doublePage.value = v; },
    get direction() { return _sig.direction.value; },
    set direction(v) { _sig.direction.value = v; },
    get fit() { return _sig.fit.value; },
    set fit(v) { _sig.fit.value = v; },
    get autoSpread() { return _sig.autoSpread.value; },
    set autoSpread(v) { _sig.autoSpread.value = v; },
    get loadVersion() { return _sig.loadVersion.value; },
    set loadVersion(v) { _sig.loadVersion.value = v; },
    imgDims:      new Map(),
    lastLayoutPage: -2,
    hasServerAnalysis: false,
    serverDoublePages: new Set(),
    chapterInfo:  {},
    scrollObs:    null,
    loaded:       new Set(),
    failed:       new Set(),
    cpTrack:      null,
    preloadDone:  false,
    cachedPreloadN: null,
  };
  let _mangaId      = /** @type {number|null} */ (null);
  /** All chapters for this manga, lazily loaded for the chapter dropdown. */
  let _allChapters  = /** @type {Array<{id:number,chapter_number:number,title:string,is_read:boolean}>|null} */ (null);
  let _progressTimer = /** @type {ReturnType<typeof setTimeout>|null} */ (null);
  let _lastReportedPage = -1;

  const _engine = createReaderEngine({
    pagesEl, canvasEl, tintOverlay, readerRoot, pageNumOverlay,
    state, chapterId,
    getPrefs: () => _prefs,
    reportProgress: _reportProgress,
    navigateChapter: _navigateChapter,
    navigateToManga: _navigateToManga,
  });
  _cleanup.push(() => _engine.destroy());

  const _dlOverlay = createDownloadOverlay(dlOverlayEl, readerRoot);
  _cleanup.push(() => _dlOverlay.hide());

  let _onSettings = () => {};
  const _topBar = createTopBar(topBar, { onBack: () => _navigateToManga(), onMenu: () => _openPanel() });
  const _sideHeader = createSidePanelHeader(sideHeaderEl, {
    onBack:     () => _navigateToManga(),
    onSettings: () => _onSettings(),
    onClose:    () => _closePanel(),
  });
  const _chapterNav = createChapterNav(chapterNavEl, {
    onBack:   () => _navigateToManga(),
    onPrev:   () => { if (state.chapterInfo.prev_chapter_id) _navigateChapter(state.chapterInfo.prev_chapter_id); },
    onNext:   () => { if (state.chapterInfo.next_chapter_id) _navigateChapter(state.chapterInfo.next_chapter_id); },
    onSelect: (id) => { if (id && id !== chapterId) { _closePanel(); _navigateChapter(id); } },
  });
  _topBar.update();
  _sideHeader.update();
  _chapterNav.update();


  function _reportProgress() {
    if (_progressTimer) clearTimeout(_progressTimer);
    _progressTimer = setTimeout(() => {
      if (state.currentPage !== _lastReportedPage) {
        _lastReportedPage = state.currentPage;
        api.setChapterProgress(chapterId, state.currentPage).catch(() => {});
      }
    }, 2000);
  }

  _cleanup.push(() => { if (_progressTimer) clearTimeout(_progressTimer); });

  // ── Presentation ─────────────────────────────────────────────────────────

  function _applyPresentation() { _engine.applyPresentation(); }

  function _applyTint() { _engine.applyTint(); }

  function _updatePageOverlay() { _engine.updatePageOverlay(); }

  // Bottom progress strip mode: full segments / page-count text / off.
  function _applyMiniStrip() {
    const mode = _prefs?.miniStrip ?? 'full';
    miniStrip.style.display = mode === 'full' ? '' : 'none';
    miniCount.style.display = mode === 'pagecount' ? '' : 'none';
  }

  function _setPref(/** @type {string} */ key, /** @type {any} */ value) {
    if (_prefs) setReaderPref(_prefs, key, value);
  }


  // ── Helpers ──────────────────────────────────────────────────────────────

  const _isDesktop     = () => window.matchMedia('(min-width: 768px)').matches;
  const _isFinePointer = () => window.matchMedia('(pointer:fine)').matches;

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
    _pendingBarsVisible = _chrome.isBarsVisible() && !_isFinePointer();
    const suffix = targetPage != null && targetPage > 0 ? `?page=${targetPage}` : '';

    try {
      await api.getChapterPages(chId);
      navigate(`/reader/${chId}${suffix}`);
      return;
    } catch (err) {
      if (/** @type {any} */ (err)?.status !== 404) {
        navigate(`/reader/${chId}${suffix}`);
        return;
      }
    }

    try { await api.downloadChapter(chId); } catch { /* already queued or downloading */ }

    let _dlDone = false;

    const _onDlCancel = () => { _dlDone = true; unsub(); _dlOverlay.hide(); _engine.render(); };
    const _showDl = (/** @type {any} */ p) => _dlOverlay.showLoading({ progress: p, onCancel: _onDlCancel });

    _showDl(/** @type {any} */ (getState('chaptersProgress').get(chId)) ?? null);

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
        _dlOverlay.showError({
          status: p.status,
          onRetry: () => _navigateChapter(chId),
          onBack:  () => { _dlOverlay.hide(); _engine.render(); },
        });
        return;
      }

      _showDl(p);
    });

    _cleanup.push(() => { _dlDone = true; unsub(); });
  }

  /**
   * Download the current chapter with a progress overlay, resolving with the
   * page manifest once complete. Used when the reader is opened directly on a
   * chapter that has not been downloaded yet.
   * @returns {Promise<any>}
   */
  function _downloadCurrentChapter() {
    return new Promise((resolve, reject) => {
      let _done = false;

      const _onDlCancel = () => { _done = true; unsub(); reject(new Error('cancelled')); _navigateToManga(); };
      const _showDl = (/** @type {any} */ p) => _dlOverlay.showLoading({ progress: p, onCancel: _onDlCancel });

      api.downloadChapter(chapterId).catch(() => { /* already queued or downloading */ });
      _showDl(/** @type {any} */ (getState('chaptersProgress').get(chapterId)) ?? null);

      const unsub = subscribe('chaptersProgress', (/** @type {Map<number,any>} */ map) => {
        if (_done) { unsub(); return; }
        if (!pagesEl.isConnected) { _done = true; unsub(); reject(new Error('unmounted')); return; }
        const p = map.get(chapterId);
        if (!p) return;

        if (p.status === 'completed') {
          _done = true;
          unsub();
          _dlOverlay.hide();
          api.getChapterPages(chapterId).then(resolve, reject);
          return;
        }
        if (p.status === 'failed' || p.status === 'cancelled') {
          _done = true;
          unsub();
          _dlOverlay.hide();
          reject(new Error(p.status));
          return;
        }
        _showDl(p);
      });

      _cleanup.push(() => { _done = true; unsub(); });
    });
  }

  // ── Side panel ───────────────────────────────────────────────────────────

  /** Callbacks invoked each time the side panel opens — avoids MutationObserver hacks. */
  const _panelOpenCallbacks = /** @type {Array<() => void>} */ ([]);

  // Bars / drawer / hover / three-zone tap — one cohesive visibility unit.
  const _chrome = createChromeVisibility({
    fullBar, segsEl, topBar, sidePanel, backdrop, miniStrip, barHover, pagesEl,
    state, engine: _engine, getPrefs: () => _prefs,
    isDesktop: _isDesktop, isFinePointer: _isFinePointer,
    loadChapterList: _loadChapterList, panelOpenCallbacks: _panelOpenCallbacks,
  });
  const _openPanel  = () => _chrome.openPanel();
  const _closePanel = () => _chrome.closePanel();

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
    _chapterNav.update({
      chapters:  _allChapters.map(ch => ({ id: ch.id, title: ch.title })),
      currentId: chapterId,
    });
  }

  // ── Jump to page ──────────────────────────────────────────────────────────

  segLeft.style.cursor        = 'pointer';
  segLeft.style.pointerEvents = 'auto';
  segLeft.title               = t('reader.jump_to_page');
  segLeft.addEventListener('click', (e) => {
    e.stopPropagation();
    if (state.pages.length === 0) return;
    const input = document.createElement('input');
    input.type  = 'number';
    input.min   = '1';
    input.max   = String(state.pages.length);
    input.value = String(state.currentPage + 1);
    input.className = 'w-10 text-xs text-center bg-transparent border-b border-accent outline-none tabular-nums text-text';
    segLeft.replaceWith(input);
    input.select();
    const _commit = () => {
      const p = Math.max(0, Math.min(state.pages.length - 1, (Number(input.value) || 1) - 1));
      input.replaceWith(segLeft);
      if (p !== state.currentPage) { state.currentPage = p; _engine.render(); _reportProgress(); }
      else state.loadVersion++;
    };
    input.addEventListener('keydown', (ev) => {
      if (ev.key === 'Enter')  { ev.preventDefault(); _commit(); }
      if (ev.key === 'Escape') { input.replaceWith(segLeft); }
    });
    input.addEventListener('blur', _commit, { once: true });
    input.focus();
  });

  function _applyDoublePageVisibility() {
    // Webtoon: hide direction control (reading is always top-to-bottom).
    dirRow.style.display = state.mode === 'webtoon' ? 'none' : '';
  }

  // ── Dir / Fit / Mode segmented controls ──────────────────────────────────
  // Built post-prefs-load (after the await below) so `selected` reflects the
  // loaded preference. Mount points are injected here; rows are appended there.

  // ── Segment rendering ─────────────────────────────────────────────────────

  const _indicator = createIndicatorBar({ miniStrip, segsEl, segLeft, segRight, onSegClick: _onSegClick });
  _cleanup.push(() => _indicator.destroy());

  // Reactive chrome sync: re-runs when currentPage / pages change (the
  // signal-backed reads below subscribe) or when loaded/failed change (via the
  // loadVersion bump the engine emits at load/error). Replaces the old
  // imperative onSegments/_renderSegments bridge.
  _cleanup.push(effect(() => {
    void state.loadVersion;
    _updatePageOverlay();
    _indicator.update({
      total:       state.pages.length,
      currentPage: state.currentPage,
      loaded:      state.loaded,
      failed:      state.failed,
    });
    if ((_prefs?.miniStrip ?? 'full') === 'pagecount') {
      const span = miniCount.querySelector('span');
      if (span) span.textContent = state.pages.length
        ? `${state.currentPage + 1} / ${state.pages.length}` : '';
    }
  }));

  function _onSegClick(/** @type {number} */ idx, /** @type {MouseEvent} */ e) {
    e.stopPropagation();
    if (state.failed.has(idx)) {
      state.failed.delete(idx);
      if (state.mode === 'scroll') {
        state.loadVersion++;
        const img = /** @type {HTMLImageElement|null} */ (
          pagesEl.querySelector(`img[data-index="${idx}"]`)
        );
        if (img) { img.src = ''; img.src = state.pages[idx]; }
      } else {
        state.currentPage = idx;
        _engine.render();
      }
      return;
    }
    if (state.mode === 'scroll') {
      pagesEl.querySelector(`[data-index="${idx}"]`)
        ?.scrollIntoView({ behavior: state.smoothScroll ? 'smooth' : 'instant', block: 'start' });
    } else {
      state.currentPage = idx;
      _engine.render();
    }
  }


  // ── Fullscreen ───────────────────────────────────────────────────────────

  function _toggleFullscreen() {
    if (document.fullscreenElement) {
      document.exitFullscreen().catch(() => {});
    } else {
      readerRoot.requestFullscreen().catch(() => {});
    }
  }

  // ── Slideshow + inactivity ────────────────────────────────────────────────
  const _slideshow = createSlideshow({
    state, pagesEl, engine: _engine,
    getPrefs: () => _prefs,
    slideshowSignal,
    navigateChapter: _navigateChapter,
    navigateToManga: _navigateToManga,
  });
  _cleanup.push(() => _slideshow.destroy());

  // ── Volume-key navigation ─────────────────────────────────────────────────

  // keydown: browser doesn't normally fire for hardware volume, but PWA/some
  // Android WebViews do surface AudioVolume* keys.
  const _onVolumeKey = (/** @type {KeyboardEvent} */ e) => {
    if (e.key === 'AudioVolumeUp')   { e.preventDefault(); if (!_chrome.isPanelOpen()) _engine.goPage(-1); }
    if (e.key === 'AudioVolumeDown') { e.preventDefault(); if (!_chrome.isPanelOpen()) _engine.goPage(1);  }
  };
  document.addEventListener('keydown', _onVolumeKey);
  _cleanup.push(() => document.removeEventListener('keydown', _onVolumeKey));

  // MediaSession: fires from Bluetooth / headset media buttons.
  if ('mediaSession' in navigator) {
    try {
      navigator.mediaSession.setActionHandler('previoustrack', () => { if (!_chrome.isPanelOpen()) _engine.goPage(-1); });
      navigator.mediaSession.setActionHandler('nexttrack',     () => { if (!_chrome.isPanelOpen()) _engine.goPage(1);  });
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
    if (_chrome.isPanelOpen() && e.key === 'Escape') { e.preventDefault(); _closePanel(); }
  }
  document.addEventListener('keydown', _onPanelKeyDown);
  _cleanup.push(() => document.removeEventListener('keydown', _onPanelKeyDown));

  _cleanup.push(registerShortcuts('reader', [
    { key: ['ArrowRight', 'ArrowDown', 'l', 'd'], description: 'Next page',     handler: () => { if (!_chrome.isPanelOpen()) _engine.goPage(1);  } },
    { key: ['ArrowLeft',  'ArrowUp',   'h', 'a'], description: 'Previous page', handler: () => { if (!_chrome.isPanelOpen()) _engine.goPage(-1); } },
    { key: ']', description: 'Next chapter',     handler: () => { if (!_chrome.isPanelOpen() && state.chapterInfo.next_chapter_id) _navigateChapter(state.chapterInfo.next_chapter_id); } },
    { key: '[', description: 'Previous chapter', handler: () => { if (!_chrome.isPanelOpen() && state.chapterInfo.prev_chapter_id) _navigateChapter(state.chapterInfo.prev_chapter_id); } },
    { key: 'f', description: 'Toggle fullscreen', handler: () => { if (!_chrome.isPanelOpen()) _toggleFullscreen(); } },
    { key: 'Escape', description: 'Back to manga', handler: () => { if (!_chrome.isPanelOpen()) _navigateToManga(); } },
  ]));

  // Position the panel immediately (before the await) so there is no flash of
  // the desktop default position on mobile before the JS takes effect.
  _chrome.positionPanel();

  // ── Landscape-aware double-page ───────────────────────────────────────────
  const _landscapeMQ = window.matchMedia('(orientation: landscape) and (max-height: 600px)');
  const _onLandscapeChange = (/** @type {MediaQueryListEvent|MediaQueryList} */ mq) => {
    if (state.mode !== 'paged') return;
    // Landscape mobile forces double-page on EPHEMERALLY (state only, never
    // persisted); portrait restores the user's saved doublePage preference so
    // rotation can't clobber their explicit choice (D8).
    const target = mq.matches ? true : (_prefs?.doublePage ?? false);
    if (state.doublePage !== target) {
      state.doublePage = target;
      _applyDoublePageVisibility(); _engine.render();
    }
  };
  _landscapeMQ.addEventListener('change', _onLandscapeChange);
  _cleanup.push(() => _landscapeMQ.removeEventListener('change', _onLandscapeChange));

  const _onSavePage = async () => {
    const canvas = /** @type {HTMLCanvasElement|null} */ (pagesEl.querySelector('canvas'));
    if (canvas) {
      canvas.toBlob((blob) => {
        if (!blob) return;
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = `page-${state.currentPage + 1}.png`;
        a.click();
        URL.revokeObjectURL(url);
      }, 'image/png');
      return;
    }
    // Single image: fetch the raw bytes so we get the original format.
    const pageUrl = state.pages[state.currentPage];
    if (!pageUrl) return;
    try {
      const resp = await fetch(pageUrl);
      const blob = await resp.blob();
      const ext  = blob.type.includes('png') ? 'png' : 'jpg';
      const url  = URL.createObjectURL(blob);
      const a    = document.createElement('a');
      a.href     = url;
      a.download = `page-${state.currentPage + 1}.${ext}`;
      a.click();
      URL.revokeObjectURL(url);
    } catch { /* network error — ignore */ }
  };

  const _modalOpenCallbacks = /** @type {Array<() => void>} */ ([]);

  // ── Display state (fullscreen / wake-lock / orientation) ─────────────────
  // Side effects stay imperative here; fullscreen feeds the sidebar quick action,
  // wake-lock/orientation feed the settings-modal Controls tab, via these signals.
  const fullscreenLabelSignal = signal(t('reader.settings.fullscreen'));
  const _onFsChange = () => {
    fullscreenLabelSignal.value = document.fullscreenElement
      ? t('reader.settings.exit_fullscreen') : t('reader.settings.fullscreen');
  };
  document.addEventListener('fullscreenchange', _onFsChange);
  _cleanup.push(() => document.removeEventListener('fullscreenchange', _onFsChange));

  const _showWake = 'wakeLock' in navigator;
  const wakeCheckedSignal = signal(false);
  /** @type {(on: boolean) => void} */
  let _onWake = () => {};
  if (_showWake) {
    let _wakeLock = /** @type {WakeLockSentinel|null} */ (null);
    _onWake = async (on) => {
      if (on) {
        try {
          _wakeLock = await navigator.wakeLock.request('screen');
          _wakeLock.addEventListener('release', () => { _wakeLock = null; });
          wakeCheckedSignal.value = true;
        } catch { wakeCheckedSignal.value = false; }
      } else {
        await _wakeLock?.release();
        _wakeLock = null;
        wakeCheckedSignal.value = false;
      }
    };
    const _onVisibility = async () => {
      if (document.visibilityState === 'visible' && wakeCheckedSignal.value) {
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
  }

  // Orientation lock: only on touch/coarse-pointer devices — it requires mobile
  // fullscreen and always rejects on desktop browsers.
  const _showOrient = 'lock' in (screen.orientation ?? {}) && !_isFinePointer();
  const orientSignal = signal('auto');
  /** @type {(v: string) => void} */
  let _onOrient = () => {};
  if (_showOrient) {
    _onOrient = async (v) => {
      orientSignal.value = v;
      try {
        if (v === 'auto') screen.orientation.unlock();
        // @ts-ignore — OrientationLockType not in all TS libs
        else await screen.orientation.lock(v);
      } catch { /* best-effort; unsupported outside fullscreen on some browsers */ }
    };
    _cleanup.push(() => { try { screen.orientation.unlock(); } catch { /* ignore */ } });
  }

  // ── Load chapter ──────────────────────────────────────────────────────────

  try {
    let data;
    try {
      data = await api.getChapterPages(chapterId);
    } catch (err) {
      if (/** @type {any} */ (err)?.status !== 404) throw err;
      data = await _downloadCurrentChapter();
    }

    state.pages = Array.isArray(data?.pages)
      ? data.pages.map(p => api.getChapterPageUrl(chapterId, p.index))
      : [];
    state.chapterInfo = data ?? {};
    _mangaId     = data?.manga_id ?? null;
    _loadChapterList();

    state.hasServerAnalysis = data?.spread_analysed === true;
    state.serverDoublePages = new Set(
      (data?.pages ?? []).filter(p => p.double_page).map(p => p.index)
    );

    const _cachedDims = getLocalJson(`kani_dims_${chapterId}`);
    if (Array.isArray(_cachedDims)) {
      for (const entry of _cachedDims) {
        const [i, w, h] = entry;
        if (typeof i === 'number' && typeof w === 'number' && typeof h === 'number') {
          state.imgDims.set(i, { w, h });
        }
      }
    }

    _prefs = await loadReaderPrefs(_mangaId);
    prefsSignal.value = { ..._prefs };
    _prefs = new Proxy(_prefs, {
      set(t, k, v) { /** @type {any} */ (t)[k] = v; prefsSignal.value = { ...t }; return true; },
    });
    state.mode        = _VALID_MODES.includes(_prefs.mode) ? _prefs.mode : 'scroll';
    state.smoothScroll = _prefs.smoothScroll;
    state.doublePage  = _prefs.doublePage;
    state.direction   = _prefs.direction;
    state.fit         = _prefs.fit;
    state.autoSpread  = _prefs.autoSpread;

    _applyPresentation();
    _applyTint();
    _applyMiniStrip();

    // ── Layout-tab handlers (state mutation + engine calls stay in reader) ──
    const _layoutHandlers = {
      smooth: (/** @type {boolean} */ v) => {
        state.smoothScroll = v;
        if (_prefs) setReaderPref(_prefs, 'smoothScroll', v);
        if (state.mode === 'scroll' || state.mode === 'webtoon') _engine.render();
      },
      double: (/** @type {boolean} */ v) => {
        state.doublePage = v;
        if (_prefs) setReaderPref(_prefs, 'doublePage', v);
        _applyDoublePageVisibility(); _engine.render();
      },
      autoSpread: (/** @type {boolean} */ v) => {
        state.autoSpread = v;
        if (_prefs) setReaderPref(_prefs, 'autoSpread', v);
        _engine.render();
      },
      spreadOffset: (/** @type {boolean} */ v) => {
        if (_prefs) { setReaderPref(_prefs, 'spreadOffset', v); _engine.render(); }
      },
    };

    // ── Fit / Dir / Mode segmented controls (single source of truth: state.*) ─
    // Controls read/write state.* directly (E12); a primed effect per key
    // persists it, skipping the initial loaded value so opening a chapter is
    // not itself a write.
    for (const key of /** @type {const} */ (['mode', 'fit', 'direction'])) {
      let primed = false;
      _cleanup.push(effect(() => {
        const v = state[key]; // subscribe to the signal-backed field
        if (primed) { if (_prefs) setReaderPref(_prefs, key, v); }
        else primed = true;
      }));
    }

    const _onFit = (/** @type {string} */ v) => {
      state.fit = /** @type {'both'|'width'|'height'} */ (v);
      _engine.render();
    };
    _cleanup.push(effect(() => render(h(SegmentedRow, {
      label: t('reader.settings.fit'),
      options: [
        { value: 'both',   label: t('reader.fit.both'),   icon: iconFitBoth },
        { value: 'width',  label: t('reader.fit.width'),  icon: iconFitWidth },
        { value: 'height', label: t('reader.fit.height'), icon: iconFitHeight },
      ],
      selected: state.fit,
      onSelect: _onFit,
    }), fitMountEl)));

    const _onDir = (/** @type {string} */ v) => {
      state.direction = /** @type {'rtl'|'ltr'} */ (v);
      for (const d of state.imgDims.values()) d.edgeMatch = undefined;
      _engine.render();
    };
    _cleanup.push(effect(() => render(h(SegmentedRow, {
      label: t('reader.settings.direction'),
      options: [
        { value: 'rtl', label: t('reader.dir.rtl'), icon: iconArrowLeftLine },
        { value: 'ltr', label: t('reader.dir.ltr'), icon: iconArrowRightLine },
      ],
      selected: state.direction,
      onSelect: _onDir,
    }), dirRow)));

    const _onMode = (/** @type {string} */ v) => {
      state.mode = /** @type {import('../reader-prefs.js').ReadingMode} */ (v);
      if (state.mode === 'webtoon' && state.fit === 'both') state.fit = 'width';
      _applyDoublePageVisibility();
      _engine.render();
    };
    _cleanup.push(effect(() => render(h(SegmentedRow, {
      options: [
        { value: 'scroll',           label: t('reader.mode.scroll'),     icon: iconReaderScroll },
        { value: 'paged',            label: t('reader.mode.paged'),      icon: iconReaderPaged },
        { value: 'webtoon',          label: t('reader.mode.webtoon'),    icon: iconReaderWebtoon },
        { value: 'continuous-paged', label: t('reader.mode.continuous'), icon: iconReaderContinuous },
      ],
      selected: state.mode,
      onSelect: _onMode,
    }), modeMountEl)));

    // Double-page quick toggle + Fullscreen quick action (sidebar Reading section).
    _cleanup.push(effect(() => render(h(ToggleRow, {
      label: t('reader.settings.double_page'),
      checked: prefsSignal.value?.doublePage ?? state.doublePage,
      onChange: (/** @type {boolean} */ v) => _layoutHandlers.double(v),
    }), doubleMountEl)));
    _cleanup.push(effect(() => render(h(ActionBtn, {
      label: fullscreenLabelSignal.value,
      onClick: () => _toggleFullscreen(),
    }), fsMountEl)));

    // ── Tap zones (guard keeps ≥1 zone on 'menu') ─────────────────────────
    const _onZone = (/** @type {string} */ key, /** @type {string} */ val) => {
      if (!_prefs) return;
      const other1 = key === 'tapLeft'  ? _prefs.tapCenter : _prefs.tapLeft;
      const other2 = key === 'tapRight' ? _prefs.tapCenter : _prefs.tapRight;
      const wouldLockout = val !== 'menu' && other1 !== 'menu' && other2 !== 'menu';
      if (wouldLockout) {
        tapHintSignal.value = true;
        if (key === 'tapCenter') return;
        setReaderPref(_prefs, 'tapCenter', 'menu');
      } else {
        tapHintSignal.value = false;
      }
      setReaderPref(_prefs, key, val);
    };
    const _shortcuts = getShortcuts('reader');

    // ── Data-driven panel accordions (scanlators / bookmarks / note) ────────
    mountPanelData({
      panelScroll, data, chapterId, mangaId: _mangaId,
      state, engine: _engine, closePanel: _closePanel,
      panelOpenCallbacks: _panelOpenCallbacks, cleanup: _cleanup,
    });

    const _onOverlay = (/** @type {boolean} */ v) => {
      if (_prefs) { setReaderPref(_prefs, 'pageOverlay', v); _updatePageOverlay(); }
    };
    const _onEndCard = (/** @type {boolean} */ v) => {
      if (_prefs) { setReaderPref(_prefs, 'endCardInPaged', v); _engine.render(); }
    };
    const _onMiniStrip = (/** @type {string} */ v) => {
      if (_prefs) { setReaderPref(_prefs, 'miniStrip', v); _applyMiniStrip(); }
    };
    const _onSleep = (/** @type {number} */ v) => {
      if (_prefs) { setReaderPref(_prefs, 'inactivityTimeout', v); _slideshow.resetInactivity(); }
    };
    const _onSlideshow = () => {
      if (_slideshow.isActive()) {
        _slideshow.stop();
      } else {
        _slideshow.play();
        _closeSettingsModal();
        _closePanel();
      }
    };

    const _updateStats = () => {
      const eta  = _engine.etaText();
      const rate = _engine.ppm();
      statsSignal.value = {
        eta: eta ?? '—',
        pace: rate ? `${rate.toFixed(1)} p/min` : '—',
      };
    };
    _modalOpenCallbacks.push(_updateStats);

    // ── Settings modal (shared Modal + Tabs, signal-driven) ─────────────────
    // null = closed; otherwise the active tab id.
    const settingsTabSignal = signal(/** @type {string|null} */ (null));

    const _settingsModalHost = document.createElement('div');
    (document.getElementById('modal-root') || document.body).appendChild(_settingsModalHost);
    _cleanup.push(() => { render(null, _settingsModalHost); _settingsModalHost.remove(); });

    _cleanup.push(effect(() => render(h(ReaderSettingsModal, {
      open: settingsTabSignal.value !== null,
      tab: settingsTabSignal.value ?? 'layout',
      onClose: _closeSettingsModal,
      onTab: (/** @type {string} */ id) => { settingsTabSignal.value = id; },
      ctx: {
        prefs: prefsSignal.value,
        setPref: _setPref,
        applyPresentation: _applyPresentation,
        applyCropToAll: () => _engine.applyCropToAll(),
        applyTint: _applyTint,
        layoutHandlers: _layoutHandlers,
        onSavePage: _onSavePage,
        onZone: _onZone,
        tapHint: tapHintSignal.value,
        shortcuts: _shortcuts,
        onOverlay: _onOverlay,
        onEndCard: _onEndCard,
        onMiniStrip: _onMiniStrip,
        onSleep: _onSleep,
        slideshowActive: slideshowSignal.value,
        onSlideshow: _onSlideshow,
        stats: statsSignal.value,
        showWake: _showWake,
        wakeChecked: wakeCheckedSignal.value,
        onWake: _onWake,
        showOrient: _showOrient,
        orient: orientSignal.value,
        onOrient: _onOrient,
      },
    }), _settingsModalHost)));

    /** @param {string} [defaultTab] */
    function _openSettingsModal(defaultTab = 'layout') {
      settingsTabSignal.value = defaultTab;
      for (const fn of _modalOpenCallbacks) fn();
    }

    function _closeSettingsModal() {
      settingsTabSignal.value = null;
    }

    _onSettings = () => { _closePanel(); _openSettingsModal(); };
    setF1Override(() => _openSettingsModal('controls'));
    _cleanup.push(() => setF1Override(null));

    _slideshow.resetInactivity();

    if (data?.last_page_read != null) {
      state.currentPage = data.last_page_read;
    }
    if (state.currentPage === -1) state.currentPage = state.pages.length - 1;

    // ?page= query param overrides the server-stored last_page_read (used by chapter navigation).
    const _qp = new URLSearchParams(location.search);
    const _qpPage = parseInt(_qp.get('page') ?? '', 10);
    if (!isNaN(_qpPage) && _qpPage >= 0) state.currentPage = _qpPage;

    state.currentPage = Math.max(0, Math.min(Math.max(state.pages.length - 1, 0), state.currentPage));

    if (data?.chapter_title) {
      document.title = data.chapter_title + ' - Kani';
      const meta = [data.source_name, data.scanlator].filter(Boolean).join(' · ');
      _topBar.update({ title: data.chapter_title });
      _sideHeader.update({ title: data.chapter_title, meta });
    }

    _chapterNav.update({
      hasPrev: !!data?.prev_chapter_id,
      hasNext: !!data?.next_chapter_id,
    });
  } catch {
    pagesEl.innerHTML = `<div class="flex items-center justify-center min-h-full"><p class="text-danger text-sm">${t('reader.error.chapter')}</p></div>`;
  }

  _applyDoublePageVisibility();
  _engine.render();
  if (_pendingBarsVisible) _chrome.showBars();
  pagesEl.focus();

  if (getLocal('kani_download_ahead_enabled') === 'true' && state.chapterInfo.next_chapter_id) {
    const aheadCount = Math.max(1, Math.min(10, parseInt(getLocal('kani_download_ahead_count') || '3', 10)));
    const _downloadAheadTimer = setTimeout(async () => {
      let nextId = state.chapterInfo.next_chapter_id;
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
    _chrome.destroy();
    if (state.currentPage !== _lastReportedPage) {
      api.setChapterProgress(chapterId, state.currentPage).catch(() => {});
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
