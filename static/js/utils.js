// @ts-check

import { getState, setState } from './state.js';
import { showApiError } from './components/toast.js';

/**
 * Returns a debounced version of `fn` that delays invocation by `ms`.
 * The returned function has a `.cancel()` method to clear a pending call.
 * @param {Function} fn
 * @param {number} ms
 * @returns {Function & { cancel: () => void }}
 */
export function debounce(fn, ms) {
  /** @type {number | undefined} */
  let timer = undefined;
  /** @param {any[]} args */
  const debounced = (...args) => {
    clearTimeout(timer);
    timer = setTimeout(() => { timer = undefined; fn(...args); }, ms);
  };
  debounced.cancel = () => { clearTimeout(timer); timer = undefined; };
  return debounced;
}

/**
 * @param {string} key
 * @returns {string}
 */
export function getLocal(key) {
  try { return localStorage.getItem(key) ?? ''; } catch { return ''; }
}

/**
 * @param {string} key
 * @param {string} value
 */
export function setLocal(key, value) {
  try { localStorage.setItem(key, value); } catch { /* quota exceeded */ }
}

/**
 * @param {string} key
 * @param {number} fallback
 * @returns {number}
 */
export function getLocalInt(key, fallback) {
  const v = parseInt(getLocal(key), 10);
  return Number.isFinite(v) ? v : fallback;
}

/**
 * @param {string} value
 * @returns {any | null}
 */
export function getJsonSafe(value) {
  try { return JSON.parse(value); } catch { return null; }
}

/**
 * @param {string} key
 * @returns {any | null}
 */
export function getLocalJson(key) {
  return getJsonSafe(getLocal(key));
}

/**
 * @param {string} key
 * @param {any} obj
 */
export function setLocalJson(key, obj) {
  setLocal(key, JSON.stringify(obj));
}

/** @type {Record<string, string>} */
const HTML_ESCAPES = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' };

/**
 * Escapes a string for safe insertion as text content in HTML.
 * @param {string} str
 * @returns {string}
 */
export function escapeHtml(str) {
  return String(str ?? '').replace(/[&<>"']/g, c => HTML_ESCAPES[c]);
}

/**
 * Formats a date as "Jan 01, 2025".
 * Accepts an ISO date string, or a Unix timestamp in seconds (number).
 * @param {string | number | null | undefined} val
 * @returns {string}
 */
export function formatDate(val) {
  if (val == null || val === '') return '';
  try {
    // Unix timestamp in seconds → convert to ms for Date constructor
    const d = typeof val === 'number' ? new Date(val * 1000) : new Date(val);
    if (isNaN(d.getTime())) return '';
    return d.toLocaleDateString('en-US', {
      month: 'short', day: '2-digit', year: 'numeric',
    });
  } catch {
    return String(val);
  }
}

/**
 * Returns a Promise that resolves after `ms` milliseconds.
 * @param {number} ms
 * @returns {Promise<void>}
 */
export function sleep(ms) {
  return new Promise(resolve => setTimeout(resolve, ms));
}

const _CONFIRM_SKIP_PREFIX = 'kani-confirm-skip-';

/** @param {string} key */
export function resetConfirmDialog(key) {
  localStorage.removeItem(_CONFIRM_SKIP_PREFIX + key);
}

export function resetAllConfirmDialogs() {
  for (let i = localStorage.length - 1; i >= 0; i--) {
    const k = localStorage.key(i);
    if (k?.startsWith(_CONFIRM_SKIP_PREFIX)) localStorage.removeItem(k);
  }
}

/**
 * Shows a confirmation modal dialog and resolves with true (confirmed) or false (cancelled).
 * Pass `rememberKey` to enable a "Don't ask again" checkbox backed by localStorage.
 * @param {{ title?: string, message: string, confirmLabel?: string, cancelLabel?: string, danger?: boolean, rememberKey?: string }} opts
 * @returns {Promise<boolean>}
 */
export function confirmDialog({ title = 'Are you sure?', message, confirmLabel = 'Confirm', cancelLabel = 'Cancel', danger = false, rememberKey }) {
  if (rememberKey && localStorage.getItem(_CONFIRM_SKIP_PREFIX + rememberKey) === '1') {
    return Promise.resolve(true);
  }

  return new Promise(resolve => {
    const titleId = 'confirm-dialog-title-' + Math.random().toString(36).slice(2);
    const overlay = document.createElement('div');
    overlay.className = 'fixed inset-0 bg-scrim z-top flex items-center justify-center p-4';
    overlay.setAttribute('role', 'dialog');
    overlay.setAttribute('aria-modal', 'true');
    overlay.setAttribute('aria-labelledby', titleId);

    const rememberHtml = rememberKey
      ? `<label class="flex items-center gap-2 text-xs text-text-muted cursor-pointer select-none">
           <input type="checkbox" class="js-remember accent-accent" />
           Don't ask again
         </label>`
      : '';

    const dialog = document.createElement('div');
    dialog.className = 'bg-surface rounded-xl shadow-xl w-full max-w-sm flex flex-col overflow-hidden';
    dialog.innerHTML = `
      <div class="px-6 pt-5 pb-4 flex flex-col gap-2">
        <h2 id="${titleId}" class="text-base font-semibold text-text">${escapeHtml(title)}</h2>
        <p class="text-sm text-text-muted">${escapeHtml(message)}</p>
      </div>
      <div class="flex items-center justify-between gap-2 px-6 py-4 border-t border-border-subtle">
        <div>${rememberHtml}</div>
        <div class="flex items-center gap-2">
          <button type="button" class="btn-ghost js-cancel">${escapeHtml(cancelLabel)}</button>
          <button type="button" class="${danger ? 'btn-danger' : 'btn-primary'} js-confirm">${escapeHtml(confirmLabel)}</button>
        </div>
      </div>
    `;

    overlay.appendChild(dialog);

    const _trigger = /** @type {HTMLElement|null} */ (document.activeElement instanceof HTMLElement ? document.activeElement : null);
    const close = (/** @type {boolean} */ result) => {
      if (result && rememberKey) {
        const cb = /** @type {HTMLInputElement|null} */ (dialog.querySelector('.js-remember'));
        if (cb?.checked) localStorage.setItem(_CONFIRM_SKIP_PREFIX + rememberKey, '1');
      }
      overlay.remove();
      if (_trigger) _trigger.focus();
      resolve(result);
    };

    overlay.addEventListener('click', (e) => { if (e.target === overlay) close(false); });
    overlay.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') { e.preventDefault(); close(false); return; }
      if (e.key === 'Tab') {
        const focusable = /** @type {HTMLElement[]} */ ([...dialog.querySelectorAll('button, input')]);
        if (focusable.length < 2) return;
        const first = focusable[0];
        const last = focusable[focusable.length - 1];
        if (e.shiftKey ? document.activeElement === first : document.activeElement === last) {
          e.preventDefault();
          (e.shiftKey ? last : first).focus();
        }
      }
    });
    dialog.querySelector('.js-cancel')?.addEventListener('click', () => close(false));
    dialog.querySelector('.js-confirm')?.addEventListener('click', () => close(true));

    document.body.appendChild(overlay);
    setTimeout(() => /** @type {HTMLElement|null} */ (dialog.querySelector('.js-confirm'))?.focus(), 10);
  });
}

/** Preferred alias for new call sites — same promise-returning, focus-trapped confirm dialog. */
export const openConfirm = confirmDialog;

/**
 * @param {number} val
 * @param {number} min
 * @param {number} max
 * @returns {number}
 */
export function clamp(val, min, max) {
  return Math.min(Math.max(val, min), max);
}

/**
 * Extracts `has_next_page` from an API result, falling back to a length comparison
 * when the field is absent. Pass `pageSize = 0` (or omit both) when no fallback is needed.
 * @param {any} result
 * @param {number} [itemsLength]
 * @param {number} [pageSize]
 * @returns {boolean}
 */
export function hasNextPage(result, itemsLength = 0, pageSize = 0) {
  if (result?.has_next_page != null) return Boolean(result.has_next_page);
  if (result?.has_next      != null) return Boolean(result.has_next);
  return pageSize > 0 && itemsLength > pageSize;
}

/**
 * Returns true if a chapter is downloaded, considering both the stored status and
 * any in-flight progress event (status === 'completed').
 * @param {{ download_status?: number | null, downloaded?: boolean } | null} chapter
 * @param {{ status?: string } | null} [progress]
 * @returns {boolean}
 */
export function isChapterDownloaded(chapter, progress) {
  if (progress?.status === 'completed') return true;
  return !!(chapter?.download_status >= 2 || chapter?.downloaded);
}

/**
 * Formats a chapter title as "Vol. X. Ch. Y. - Title" (volume omitted if absent).
 * Accepts fields from either local chapters (number/title/volume) or recent updates
 * (chapter_number/chapter_name).
 * @param {{ volume?: number | null, number?: number | null, chapter_number?: number | null, title?: string | null, chapter_name?: string | null }} ch
 * @returns {string}
 */
export function formatChapterTitle(ch) {
  const num = ch.number ?? ch.chapter_number ?? '?';
  let s = '';
  if (ch.volume != null) s += `Vol. ${ch.volume} - `;
  s += `Ch. ${num}`;
  const name = ch.title ?? ch.chapter_name ?? null;
  if (name) s += `: ${name}`;
  return s || `Ch. ${num}`;
}

/**
 * Formats a date string or Date as a human-readable relative time (e.g. "2 hours ago").
 * Returns null if the input is null/undefined/unparseable.
 * @param {string | Date | null | undefined} dateInput
 * @returns {string | null}
 */
export function formatRelativeTime(dateInput) {
  if (dateInput == null) return null;
  const date = typeof dateInput === 'string' ? new Date(dateInput) : dateInput;
  if (isNaN(date.getTime())) return null;
  const diffMs = Date.now() - date.getTime();
  const diffSec = Math.floor(diffMs / 1000);
  if (diffSec < 60) return 'just now';
  const diffMin = Math.floor(diffSec / 60);
  if (diffMin < 60) return diffMin === 1 ? '1 minute ago' : `${diffMin} minutes ago`;
  const diffHr = Math.floor(diffMin / 60);
  if (diffHr < 24) return diffHr === 1 ? '1 hour ago' : `${diffHr} hours ago`;
  const diffDay = Math.floor(diffHr / 24);
  if (diffDay < 30) return diffDay === 1 ? '1 day ago' : `${diffDay} days ago`;
  const diffMonth = Math.floor(diffDay / 30);
  if (diffMonth < 12) return diffMonth === 1 ? '1 month ago' : `${diffMonth} months ago`;
  const diffYear = Math.floor(diffMonth / 12);
  return diffYear === 1 ? '1 year ago' : `${diffYear} years ago`;
}

/**
 * Mounts a skeleton only after a delay, cancelling if real content arrives first.
 * Prevents skeleton flicker on fast connections.
 *
 * @param {() => void} mountFn   — called after `delayMs` if not cancelled
 * @param {number} [delayMs=150]
 * @returns {() => void}         — cancel function; call when real content is ready
 */
/** Compact relative time: "just now", "5m ago", "3h ago", or a locale date string. */
export function fmtCompactDate(dateStr) {
  try {
    const d = new Date(dateStr + 'Z');
    const diff = Date.now() - d.getTime();
    if (diff < 60_000) return 'just now';
    if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m ago`;
    if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)}h ago`;
    return d.toLocaleDateString();
  } catch { return dateStr; }
}

export function deferredSkeleton(mountFn, delayMs = 150) {
  const t = setTimeout(mountFn, delayMs);
  return () => clearTimeout(t);
}

/**
 * Returns an accessible label for a consecutive-error count badge so AT
 * users don't rely on color alone to infer severity.
 * @param {number} count
 * @returns {string}
 */
export function errorCountAriaLabel(count) {
  if (count >= 3) return `${count} errors — unhealthy`;
  return `${count} ${count === 1 ? 'error' : 'errors'}`;
}

/**
 * Attaches a horizontal swipe handler to `el`.
 * Does not interfere with vertical scrolling. Safe to use alongside the
 * reader's own pinch/zoom touch handler (different element).
 * @param {HTMLElement} el
 * @param {{ onLeft?: () => void, onRight?: () => void, threshold?: number }} opts
 * @returns {() => void} cleanup
 */
export function addSwipeHandler(el, { onLeft, onRight, threshold = 50 } = {}) {
  let startX = 0;
  let startY = 0;

  function onTouchStart(/** @type {TouchEvent} */ e) {
    if (e.touches.length !== 1) return;
    startX = e.touches[0].clientX;
    startY = e.touches[0].clientY;
  }

  function onTouchEnd(/** @type {TouchEvent} */ e) {
    if (e.changedTouches.length !== 1) return;
    const dx = e.changedTouches[0].clientX - startX;
    const dy = e.changedTouches[0].clientY - startY;
    if (Math.abs(dx) < threshold || Math.abs(dx) <= Math.abs(dy)) return;
    if (dx < 0) onLeft?.();
    else onRight?.();
  }

  el.addEventListener('touchstart', onTouchStart, { passive: true });
  el.addEventListener('touchend', onTouchEnd, { passive: true });
  return () => {
    el.removeEventListener('touchstart', onTouchStart);
    el.removeEventListener('touchend', onTouchEnd);
  };
}

/**
 * Attaches a pull-to-refresh handler to `el`.
 * Fires `onRefresh` when the user pulls down past `threshold` px while at the
 * top of the scroll container. Respects `prefers-reduced-motion`.
 * @param {HTMLElement} el
 * @param {() => Promise<void> | void} onRefresh
 * @param {{ threshold?: number }} opts
 * @returns {() => void} cleanup
 */
export function addPullToRefresh(el, onRefresh, { threshold = 60 } = {}) {
  const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  let startY = 0;
  let pulling = false;

  /** @type {HTMLElement | null} */
  let indicator = null;

  function _ensureIndicator() {
    if (indicator) return indicator;
    indicator = document.createElement('div');
    indicator.className = 'flex items-center justify-center h-10 text-text-muted text-sm opacity-0 transition-opacity duration-200';
    indicator.setAttribute('aria-hidden', 'true');
    indicator.textContent = '↓ Release to refresh';
    el.insertAdjacentElement('beforebegin', indicator);
    return indicator;
  }

  function _removeIndicator() {
    indicator?.remove();
    indicator = null;
  }

  function onTouchStart(/** @type {TouchEvent} */ e) {
    if (el.scrollTop > 0) return;
    if (e.touches.length !== 1) return;
    startY = e.touches[0].clientY;
    pulling = false;
  }

  function onTouchMove(/** @type {TouchEvent} */ e) {
    if (el.scrollTop > 0) return;
    const dy = e.touches[0].clientY - startY;
    if (dy <= 0) return;
    pulling = dy >= threshold;
    if (!reduced) {
      const ind = _ensureIndicator();
      ind.style.opacity = pulling ? '1' : String(dy / threshold);
    }
  }

  async function onTouchEnd() {
    if (!pulling) { _removeIndicator(); return; }
    pulling = false;
    _removeIndicator();
    await onRefresh();
  }

  el.addEventListener('touchstart', onTouchStart, { passive: true });
  el.addEventListener('touchmove', onTouchMove, { passive: true });
  el.addEventListener('touchend', onTouchEnd, { passive: true });
  return () => {
    el.removeEventListener('touchstart', onTouchStart);
    el.removeEventListener('touchmove', onTouchMove);
    el.removeEventListener('touchend', onTouchEnd);
    _removeIndicator();
  };
}

/** @type {HTMLElement | null} */
let _liveRegion = null;

/**
 * Writes `message` to a singleton `aria-live="polite"` region so screen readers
 * announce it without moving focus. Call for results that have no visible toast.
 * @param {string} message
 */
export function announce(message) {
  if (!_liveRegion) {
    _liveRegion = document.createElement('div');
    _liveRegion.setAttribute('aria-live', 'polite');
    _liveRegion.setAttribute('aria-atomic', 'true');
    _liveRegion.className = 'sr-only';
    document.body.appendChild(_liveRegion);
  }
  _liveRegion.textContent = '';
  requestAnimationFrame(() => { if (_liveRegion) _liveRegion.textContent = message; });
}

/**
 * Applies an optimistic state update, runs `apiFn`, and rolls back + shows an
 * error toast on failure.
 * @param {string} stateKey
 * @param {any} optimisticValue
 * @param {() => Promise<any>} apiFn
 */
export async function optimisticUpdate(stateKey, optimisticValue, apiFn) {
  const prev = getState(stateKey);
  setState(stateKey, optimisticValue);
  try {
    await apiFn();
  } catch (e) {
    setState(stateKey, prev);
    showApiError(e);
  }
}
