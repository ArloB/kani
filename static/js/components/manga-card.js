// @ts-check
// Manga card component — cover, title, optional metadata badge.

import { createCoverImage } from './cover-image.js';
import { getMangaCoverUrl } from '../api.js';
import { iconEllipsisVertical } from '../icons.js';

/**
 * @typedef {{ id: number, title: string, source_id?: number | null, cover_image_url?: string | null, new_chapter_count?: number, is_orphaned?: boolean }} MangaCardData
 */

/**
 * Creates a manga card element.
 * @param {{
 *   manga: MangaCardData,
 *   href: string,
 *   badge?: string | null,
 *   extraClass?: string,
 *   onCardClick?: ((manga: MangaCardData) => void) | null,
 *   onMenuClick?: ((manga: MangaCardData, btnEl: HTMLElement) => void) | null,
 * }} props
 * @returns {HTMLElement}
 */
export function createMangaCard({ manga, href, badge = null, extraClass = '', onCardClick = null, onMenuClick = null }) {
  const card = document.createElement('div');
  card.className = ['manga-card', extraClass].filter(Boolean).join(' ');
  card.dataset.mangaId = String(manga.id);

  const coverUrl = manga.cover_image_url ?? (manga.source_id != null
    ? getMangaCoverUrl(manga.id)
    : null);
  const link = document.createElement('a');
  link.href = href;

  const coverWrap = document.createElement('div');
  coverWrap.className = 'relative w-full overflow-hidden rounded-sm bg-surface-2';
  coverWrap.style.aspectRatio = '2/3';
  coverWrap.appendChild(createCoverImage({ url: coverUrl, alt: manga.title }));

  const titleEl = document.createElement('p');
  titleEl.className = 'title';
  const titleSpan = document.createElement('span');
  titleSpan.textContent = manga.title;
  titleEl.appendChild(titleSpan);

  if (manga.new_chapter_count && manga.new_chapter_count > 0) {
    const newBadge = document.createElement('span');
    newBadge.className = 'absolute top-1 left-1 bg-accent text-white text-xs font-bold px-1.5 py-0.5 rounded-full leading-none z-10';
    newBadge.textContent = manga.new_chapter_count > 99 ? '99+' : String(manga.new_chapter_count);
    newBadge.setAttribute('aria-label', `${manga.new_chapter_count} new chapter${manga.new_chapter_count !== 1 ? 's' : ''}`);
    coverWrap.appendChild(newBadge);
  }

  if (manga.is_orphaned) {
    const orphanBadge = document.createElement('span');
    orphanBadge.className = 'absolute top-1 right-1 bg-warn/20 text-warn text-xs font-semibold px-1.5 py-0.5 rounded leading-none z-10';
    orphanBadge.textContent = 'Orphaned';
    orphanBadge.setAttribute('aria-label', 'Source deleted — manga is orphaned');
    coverWrap.appendChild(orphanBadge);
  }

  coverWrap.appendChild(titleEl);
  link.appendChild(coverWrap);
  if (onCardClick) {
    link.addEventListener('click', (e) => { e.preventDefault(); onCardClick(manga); });
  }
  card.appendChild(link);

  if (onMenuClick) {
    const menuBtn = document.createElement('button');
    menuBtn.type = 'button';
    menuBtn.className = 'manga-card__menu-btn';
    menuBtn.setAttribute('aria-label', 'More options');
    menuBtn.innerHTML = iconEllipsisVertical;
    menuBtn.addEventListener('click', (e) => {
      e.preventDefault();
      e.stopPropagation();
      onMenuClick(manga, menuBtn);
    });
    coverWrap.appendChild(menuBtn);
  }

  if (badge) {
    const badgeEl = document.createElement('span');
    badgeEl.className = 'manga-card__badge';
    badgeEl.textContent = badge;
    card.appendChild(badgeEl);
  }

  return card;
}

/**
 * Adds or removes a scan-pending spinner overlay on a card within `root`.
 * @param {number} mangaId
 * @param {boolean} isScanning
 * @param {ParentNode} [root]
 */
export function setMangaCardScanning(mangaId, isScanning, root = document) {
  const card = root.querySelector(`[data-manga-id="${mangaId}"]`);
  if (!card) return;
  const coverWrap = card.querySelector('.relative');
  if (!coverWrap) return;
  const existing = coverWrap.querySelector('.js-scan-spinner');
  if (isScanning && !existing) {
    const overlay = document.createElement('div');
    overlay.className = 'js-scan-spinner absolute inset-0 flex items-center justify-center bg-black/40 rounded-md z-10';
    const spinner = document.createElement('div');
    spinner.className = 'w-10 h-10 border-[3px] border-white/30 border-t-white rounded-full animate-spin';
    overlay.appendChild(spinner);
    coverWrap.appendChild(overlay);
  } else if (!isScanning && existing) {
    existing.remove();
  }
}

/**
 * Sets or removes a "new chapters" badge count on a card.
 * Pass `count = 0` to remove the badge.
 * @param {number} mangaId
 * @param {number} count
 * @param {ParentNode} [root]
 */
export function setNewChapterCount(mangaId, count, root = document) {
  const card = root.querySelector(`[data-manga-id="${mangaId}"]`);
  if (!card) return;
  const coverWrap = card.querySelector('.relative');
  if (!coverWrap) return;
  let badge = /** @type {HTMLElement | null} */ (coverWrap.querySelector('.js-new-ch-badge'));
  if (count <= 0) {
    badge?.remove();
    return;
  }
  if (!badge) {
    badge = document.createElement('div');
    badge.className = 'js-new-ch-badge absolute top-1 left-1 z-10 min-w-[1.25rem] h-5 px-1 flex items-center justify-center rounded-full text-[10px] font-bold bg-accent text-white leading-none';
    coverWrap.appendChild(badge);
  }
  badge.textContent = count > 99 ? '99+' : String(count);
}

/**
 * Updates the download progress bar overlay on a card within `root`.
 * Pass `pct = null` to remove the bar.
 * @param {number} mangaId
 * @param {number | null} pct  0–100
 * @param {ParentNode} [root]
 */
export function setMangaCardDownloadProgress(mangaId, pct, root = document) {
  const card = root.querySelector(`[data-manga-id="${mangaId}"]`);
  if (!card) return;
  const coverWrap = card.querySelector('.relative');
  if (!coverWrap) return;
  let bar = /** @type {HTMLElement | null} */ (coverWrap.querySelector('.js-dl-bar'));
  if (pct === null) {
    bar?.remove();
    return;
  }
  if (!bar) {
    bar = document.createElement('div');
    bar.className = 'js-dl-bar absolute bottom-0 left-0 right-0 h-1 bg-black/30 z-10';
    const fill = document.createElement('div');
    fill.className = 'js-dl-bar-fill h-full bg-accent transition-[width] duration-300';
    bar.appendChild(fill);
    coverWrap.appendChild(bar);
  }
  const fill = /** @type {HTMLElement} */ (bar.querySelector('.js-dl-bar-fill'));
  fill.style.width = `${pct}%`;
}

/**
 * Renders a grid of manga cards into a container.
 * @param {HTMLElement} container
 * @param {{
 *   items: MangaCardData[],
 *   getHref: (manga: MangaCardData) => string,
 *   getBadge?: (manga: MangaCardData) => string | null,
 *   large?: boolean,
 *   onCardClick?: ((manga: MangaCardData) => void) | null,
 *   onMenuClick?: ((manga: MangaCardData, btnEl: HTMLElement) => void) | null,
 * }} props
 */
export function renderMangaGrid(container, { items, getHref, getBadge, large = false, onCardClick = null, onMenuClick = null }) {
  const grid = document.createElement('div');
  grid.className = large ? 'manga-grid manga-grid--large' : 'manga-grid';

  for (const manga of items) {
    const card = createMangaCard({
      manga,
      href: getHref(manga),
      badge: getBadge ? getBadge(manga) : null,
      onCardClick: onCardClick ? () => onCardClick(manga) : null,
      onMenuClick: onMenuClick ? (m, btn) => onMenuClick(m, btn) : null,
    });
    grid.appendChild(card);
  }

  container.appendChild(grid);
  return grid;
}
