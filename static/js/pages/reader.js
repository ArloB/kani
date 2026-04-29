// @ts-check
// Reader page — full-screen chapter reader with page-by-page and scroll modes.

import * as api from '../api.js';
import { iconChevronLeft, iconChevronRight, iconX, iconMenu } from '../icons.js';
import { navigate } from '../router.js';
import { getLocal, setLocal } from '../utils.js';
import { getState, subscribe } from '../state.js';

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
          <div class="px-3 py-4">
            <p class="text-xs font-medium text-muted uppercase tracking-wide mb-3">Options</p>
            <label class="flex items-center justify-between gap-3 cursor-pointer">
              <span class="text-sm text-text">Smooth scroll</span>
              <label class="kani-toggle" aria-label="Smooth scroll">
                <input id="reader-smooth-input" type="checkbox" class="kani-toggle__input">
                <span class="kani-toggle__track"></span>
              </label>
            </label>
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

  /** @type {string[]} */
  let _pages        = [];
  let _currentPage  = 0;
  let _mode         = /** @type {'scroll'|'paged'} */ (getLocal('kani_reader_mode') === 'paged' ? 'paged' : 'scroll');
  let _smoothScroll = getLocal('kani_reader_smooth') === 'true';
  let _barsVisible  = false;
  let _panelOpen    = false;
  let _isHovering   = false;
  let _hideTimer    = /** @type {ReturnType<typeof setTimeout>|null} */ (null);
  let _mangaId      = /** @type {number|null} */ (null);
  let _progressTimer = /** @type {ReturnType<typeof setTimeout>|null} */ (null);
  let _lastReportedPage = -1;

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

  // ── Mode buttons ─────────────────────────────────────────────────────────

  function _applyModeButtons() {
    const isScroll = _mode === 'scroll';
    const on  = 'bg-surface-2 text-text';
    const off = 'text-muted hover:bg-surface-2 hover:text-text';
    modeScroll.className = `flex-1 text-sm px-3 py-2 rounded-md transition-colors ${isScroll ? on : off}`;
    modePaged.className  = `flex-1 text-sm px-3 py-2 rounded-md transition-colors ${!isScroll ? on : off}`;
    modeScroll.setAttribute('aria-pressed', String( isScroll));
    modePaged.setAttribute( 'aria-pressed', String(!isScroll));
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
          pagesEl.querySelector(`img[data-index="${idx}"]`)
            ?.scrollIntoView({ behavior: _smoothScroll ? 'smooth' : 'instant', block: 'start' });
        } else {
          _currentPage = idx;
          _renderPages();
        }
      });
      segsEl.appendChild(seg);
    }
  }

  // ── Prefetch ─────────────────────────────────────────────────────────────

  function _prefetch(pageIndex) {
    if (_mode !== 'paged') return;
    for (let i = 1; i <= 2; i++) {
      const prefIdx = pageIndex + i;
      const url = _pages[prefIdx];
      if (url && !_loaded.has(prefIdx) && !_failed.has(prefIdx)) {
        const img = new Image();
        img.addEventListener('load',  () => { _loaded.add(prefIdx);  _renderSegments(); });
        img.addEventListener('error', () => { _failed.add(prefIdx);  _renderSegments(); });
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

    if (_mode === 'scroll') {
      pagesEl.className = 'flex-1 overflow-y-auto overflow-x-hidden flex flex-col items-center gap-1 py-2';

      for (let i = 0; i < _pages.length; i++) {
        const img         = document.createElement('img');
        img.src           = _pages[i];
        img.className     = 'max-w-full max-h-screen object-contain';
        img.alt           = '';
        img.loading       = 'lazy';
        img.dataset.index = String(i);
        img.addEventListener('load',  () => { _loaded.add(i);  _failed.delete(i); _renderSegments(); });
        img.addEventListener('error', () => { _failed.add(i);  _loaded.delete(i); _renderSegments(); });
        if (img.complete) {
          if (img.naturalWidth) _loaded.add(i); else _failed.add(i);
        }
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
        }
      }, { root: pagesEl, threshold: 0.1 });
      pagesEl.querySelectorAll('img[data-index]').forEach(img => _scrollObs?.observe(img));

    } else {
      // Paged mode
      _currentPage = Math.max(0, Math.min(_pages.length - 1, _currentPage));
      pagesEl.className = 'flex-1 overflow-hidden relative flex items-center justify-center';

      const img     = document.createElement('img');
      img.src       = _pages[_currentPage] ?? '';
      img.className = 'max-w-full max-h-full object-contain';
      img.alt       = `Page ${_currentPage + 1}`;
      img.addEventListener('load', () => {
        _loaded.add(_currentPage);
        _failed.delete(_currentPage);
        _renderSegments();
      });
      img.addEventListener('error', () => {
        const failedPage = _currentPage;
        _failed.add(failedPage);
        _loaded.delete(failedPage);
        _renderSegments();
        // Error overlay with retry
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
      if (img.complete) {
        if (img.naturalWidth) _loaded.add(_currentPage); else _failed.add(_currentPage);
      }
      pagesEl.appendChild(img);
      _prefetch(_currentPage);
    }

    _renderSegments();
  }

  // ── Page navigation ───────────────────────────────────────────────────────

  function _goPage(delta) {
    if (_mode !== 'paged') return;
    const next = _currentPage + delta;

    if (next < 0) {
      if (_chapterInfo.prev_chapter_id) {
        // Signal reader to start at the last page of the previous chapter
        api.setChapterProgress(_chapterInfo.prev_chapter_id, -1).catch(() => {});
        _navigateChapter(_chapterInfo.prev_chapter_id);
      }
      return;
    }
    if (next >= _pages.length) {
      // Reset progress to 0 for completed chapter before navigating away
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
      if (x < third)          { _goPage(-1); return; }
      if (x > 2 * third)      { _goPage(1);  return; }
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

  function _onKeyDown(/** @type {KeyboardEvent} */ e) {
    if (_panelOpen) {
      if (e.key === 'Escape') { e.preventDefault(); _closePanel(); }
      return;
    }
    if (e.key === 'Escape')                                  { _navigateToManga(); return; }
    if (e.key === 'ArrowRight' || e.key === 'ArrowDown') { e.preventDefault(); _goPage(1);  }
    if (e.key === 'ArrowLeft'  || e.key === 'ArrowUp')   { e.preventDefault(); _goPage(-1); }
  }
  document.addEventListener('keydown', _onKeyDown);
  _cleanup.push(() => document.removeEventListener('keydown', _onKeyDown));

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

    // Trust server-side progress; -1 means start at last page (used when navigating backwards)
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
    // Flush progress to server immediately on exit
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
