// @ts-check
// Manga card component — cover, title, optional metadata badge.

import { createCoverImage } from './cover-image.js';
import { getMangaCoverUrl } from '../api.js';
import { iconEllipsisVertical } from '../icons.js';

/**
 * @typedef {{ id: number, title: string, source_id?: number | null, cover_image_url?: string | null }} MangaCardData
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
