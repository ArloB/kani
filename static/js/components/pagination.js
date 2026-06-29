// @ts-check
// Pagination — square 34×34 tile buttons.
// When `total` pages is known: shows windowed numbered tiles with ellipsis.
// When `total` is unknown: falls back to prev / current-page / next tiles.

import { iconChevronLeft, iconChevronRight } from '../icons.js';

/**
 * @param {HTMLElement} container
 * @param {{
 *   page: number,
 *   hasNext: boolean,
 *   total?: number,
 *   onPageChange: (page: number) => void,
 * }} props
 * @returns {{ update: (props: { page?: number, hasNext?: boolean, total?: number }) => void, destroy: () => void }}
 */
export function renderPagination(container, { page, hasNext, total, onPageChange }) {
  /** @type {{ page: number, hasNext: boolean, total?: number }} */
  let _props = { page, hasNext, total };

  function _render() {
    const { page, hasNext, total } = _props;
    container.innerHTML = '';

    const wrap = document.createElement('div');
    wrap.className = 'flex flex-wrap items-center justify-center gap-1.5 py-4';

    // ── Prev ──
    const prevBtn = _mkNavTile(iconChevronLeft, page <= 1, () => onPageChange(_props.page - 1));
    prevBtn.setAttribute('aria-label', 'Previous page');
    wrap.appendChild(prevBtn);

    // ── Pages ──
    if (total != null && total > 0) {
      for (const entry of _pageWindow(page, total)) {
        if (entry === null) {
          wrap.appendChild(_mkEllipsis());
        } else {
          const n = entry;
          const btn = n === page
            ? _mkActiveTile(String(n))
            : _mkNumberTile(String(n), () => onPageChange(n));
          btn.setAttribute('aria-label', `Page ${n}`);
          if (n === page) btn.setAttribute('aria-current', 'page');
          wrap.appendChild(btn);
        }
      }
    } else {
      // Fallback: non-interactive current-page tile
      const cur = _mkActiveTile(String(page));
      cur.setAttribute('aria-current', 'page');
      wrap.appendChild(cur);
    }

    // ── Next ──
    const nextBtn = _mkNavTile(iconChevronRight, !hasNext, () => onPageChange(_props.page + 1));
    nextBtn.setAttribute('aria-label', 'Next page');
    wrap.appendChild(nextBtn);

    container.appendChild(wrap);
  }

  _render();

  return {
    update(newProps) {
      _props = { ..._props, ...newProps };
      _render();
    },
    destroy() {
      container.innerHTML = '';
    },
  };
}

// ── Tile factories ────────────────────────────────────────────────────────────

const TILE_BASE = 'tile-btn';

/** Navigation arrow tile (prev / next). */
function _mkNavTile(iconHtml, disabled, onClick) {
  const btn = document.createElement('button');
  btn.type = 'button';
  btn.className = `${TILE_BASE} border-border bg-surface text-text-muted icon-sm transition-colors hover:bg-surface-2 disabled:opacity-35 disabled:cursor-not-allowed cursor-pointer`;
  btn.innerHTML = iconHtml;
  btn.disabled = disabled;
  if (!disabled) btn.addEventListener('click', onClick);
  return btn;
}

/** Numbered page tile (not current). */
function _mkNumberTile(label, onClick) {
  const btn = document.createElement('button');
  btn.type = 'button';
  btn.className = `${TILE_BASE} border-border bg-surface text-text-muted transition-colors hover:bg-surface-2 cursor-pointer`;
  btn.textContent = label;
  btn.addEventListener('click', onClick);
  return btn;
}

/** Current-page tile — accent fill, not interactive. */
function _mkActiveTile(label) {
  const span = document.createElement('span');
  span.className = `${TILE_BASE} border-accent bg-accent text-on-accent font-medium`;
  span.textContent = label;
  return span;
}

/** Ellipsis gap indicator. */
function _mkEllipsis() {
  const span = document.createElement('span');
  span.className = 'tile-btn border-0 text-text-muted select-none';
  span.setAttribute('aria-hidden', 'true');
  span.textContent = '…';
  return span;
}

// ── Windowing ─────────────────────────────────────────────────────────────────

/**
 * Returns the sequence of page numbers (and null for ellipsis gaps) to render.
 * Always pins: 1, total, current−1, current, current+1 (clipped to [1..total]).
 * Inserts null where consecutive entries differ by more than 1.
 * @param {number} current
 * @param {number} total
 * @returns {Array<number | null>}
 */
function _pageWindow(current, total) {
  const pinned = new Set(
    [1, total, current - 1, current, current + 1].filter(n => n >= 1 && n <= total),
  );
  const sorted = [...pinned].sort((a, b) => a - b);
  /** @type {Array<number | null>} */
  const result = [];
  for (let i = 0; i < sorted.length; i++) {
    if (i > 0 && sorted[i] - sorted[i - 1] > 1) result.push(null);
    result.push(sorted[i]);
  }
  return result;
}
