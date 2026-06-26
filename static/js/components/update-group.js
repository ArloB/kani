// @ts-check
// Update group — recent-updates manga group with cover, title, and chapter list.

import { createCoverImage } from './cover-image.js';
import { getMangaCoverUrl } from '../api.js';
import { escapeHtml, formatDate } from '../utils.js';

/**
 * @typedef {{
 *   manga_id: number,
 *   manga_title: string,
 *   chapters: Array<{
 *     id: number,
 *     title: string,
 *     date_uploaded?: string | null,
 *   }>,
 * }} UpdateGroupData
 */

/**
 * Creates a recent-updates group element.
 * @param {{ group: UpdateGroupData }} props
 * @returns {HTMLElement}
 */
export function createUpdateGroup({ group }) {
  const el = document.createElement('div');
  el.className = 'flex flex-col gap-2 py-3 border-b border-border last:border-b-0';

  const mangaHref = `/manga/${group.manga_id}`;
  const coverUrl = getMangaCoverUrl(group.manga_id, 'sm');

  // Header: cover thumb + manga title link
  const header = document.createElement('div');
  header.className = 'flex items-center gap-3';

  const thumbWrap = document.createElement('a');
  thumbWrap.href = mangaHref;
  thumbWrap.className = 'w-10 h-10 rounded-md overflow-hidden shrink-0 bg-surface-2 block';
  const cover = createCoverImage({ url: coverUrl, alt: group.manga_title });
  thumbWrap.appendChild(cover);

  const titleLink = document.createElement('a');
  titleLink.href = mangaHref;
  titleLink.className = 'text-sm font-medium text-text hover:text-accent transition-colors flex-1 truncate';
  titleLink.textContent = group.manga_title;

  header.appendChild(thumbWrap);
  header.appendChild(titleLink);
  el.appendChild(header);

  // Chapter list
  const list = document.createElement('ul');
  list.className = 'flex flex-col gap-0.5 pl-13'; /* justified: aligns with thumbnail width (48px) + gap */

  for (const ch of group.chapters) {
    const item = document.createElement('li');
    item.className = 'flex items-center justify-between gap-2 py-0.5';

    const chHref = `/manga/${group.manga_id}`;
    item.dataset.chapterId = String(ch.id);
    item.innerHTML = `
      <a class="text-sm text-text-muted hover:text-text transition-colors flex-1 truncate" href="${escapeHtml(chHref)}">${escapeHtml(ch.title)}</a>
      ${ch.date_uploaded
        ? `<span class="text-xs text-text-faint shrink-0">${escapeHtml(formatDate(ch.date_uploaded))}</span>`
        : ''}
    `;
    list.appendChild(item);
  }

  el.appendChild(list);
  return el;
}
