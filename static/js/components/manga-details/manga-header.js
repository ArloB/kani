// @ts-check
// Manga details — hero (cover + meta + description) + CTA button group.
// Handles responsive layout swap between mobile and desktop.

import * as api from '../../api.js';
import { hasPermission, getState, subscribe } from '../../state.js';
import { navigate } from '../../router.js';
import { getLocal, setLocal, escapeHtml } from '../../utils.js';
import { createCoverImage } from '../cover-image.js';
import { showToast, showApiError } from '../toast.js';
import { iconSpinner } from '../../icons.js';

// ── Source filter URL builder ──────────────────────────────────────────────────

/** @type {Map<string|number, Promise<any[]>>} */
const _sourceFilterDefsCache = new Map();

/**
 * @param {string|number} sid
 * @param {string} name
 * @param {'Author'|'Artist'|'Tag'} semantic
 * @returns {Promise<string>}
 */
export async function buildSourceMetaUrl(sid, name, semantic) {
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

// ── External link warning ──────────────────────────────────────────────────────

/** @param {string} url */
function _showExternalLinkDialog(url) {
  const overlay = document.createElement('div');
  overlay.className = 'fixed inset-0 bg-scrim z-modal flex items-center justify-center p-4';

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
  dialog.querySelector('.js-cancel')?.addEventListener('click', close);
  dialog.querySelector('.js-continue')?.addEventListener('click', () => {
    if (/** @type {HTMLInputElement} */ (dialog.querySelector('.js-dont-ask')).checked) {
      setLocal('kani_skip_external_warning', 'true');
    }
    window.open(url, '_blank', 'noopener,noreferrer');
    close();
  });

  document.body.appendChild(overlay);
  /** @type {HTMLButtonElement|null} */ (dialog.querySelector('.js-continue'))?.focus();
}

// ── Hero mount ─────────────────────────────────────────────────────────────────

/**
 * @typedef {{
 *   isLocal: boolean,
 *   dbId: number,
 *   sid: number,
 *   mangaId: string,
 *   existingDbId: () => number|null,
 *   addedDbId: () => number|null,
 *   findNextPreferredChapter: () => any|null,
 *   getChapters: () => any[],
 *   onDownloadAll: () => Promise<void>,
 *   onCancelAll: () => Promise<void>,
 *   onScan: () => Promise<{new_chapters?: number}>,
 *   onAddedToLibrary: (newDbId: number) => void,
 *   onSwitchToChapters: () => void,
 * }} HeroCtx
 */

/**
 * Mounts the hero (cover + meta + description) and CTA button group into leftCol.
 * Returns { destroy, coverEl }.
 *
 * @param {HTMLElement} leftCol
 * @param {any} info        manga metadata
 * @param {any} source      source metadata (may be null for remote views)
 * @param {HeroCtx} ctx
 * @returns {{ destroy: () => void, coverEl: HTMLElement }}
 */
export function mountMangaHeader(leftCol, info, source, ctx) {
  const { isLocal, dbId, sid, mangaId } = ctx;

  const coverUrl = isLocal
    ? api.getMangaCoverUrl(dbId) + '?v=' + Date.now()
    : (info?.cover_url ?? info?.cover_image_url ?? null);

  const isDesktop = () => window.innerWidth >= 768;

  // ── Cover ──
  const coverInner = document.createElement('div');
  coverInner.className = 'aspect-[2/3] rounded-xl overflow-hidden bg-surface-2 shrink-0 cursor-pointer'; /* justified: manga cover aspect ratio */
  coverInner.appendChild(createCoverImage({ url: coverUrl, alt: info?.title ?? '' }));

  // Cover lightbox
  coverInner.addEventListener('click', () => {
    if (!coverUrl) return;
    const rect = coverInner.getBoundingClientRect();
    const overlay = document.createElement('div');
    overlay.className = 'fixed inset-0 z-top flex items-center justify-center';
    overlay.style.cssText = 'background:rgba(0,0,0,0);transition:background var(--motion-slow) ease';

    const img = document.createElement('img');
    img.src = coverUrl;
    img.alt = info?.title ?? '';
    img.className = 'shadow-2xl object-contain';
    img.style.cssText = `position:fixed;top:${rect.top}px;left:${rect.left}px;width:${rect.width}px;height:${rect.height}px;border-radius:0.75rem;object-fit:contain;`;
    overlay.appendChild(img);
    document.body.appendChild(overlay);

    img.getBoundingClientRect(); // force reflow
    img.style.transition = `top 280ms var(--motion-ease), left 280ms var(--motion-ease), width 280ms var(--motion-ease), height 280ms var(--motion-ease), border-radius 280ms ease`;
    overlay.style.background = 'rgba(0,0,0,0.6)';

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
      overlay.style.background = 'rgba(0,0,0,0)'; /* fades back to transparent */
      img.style.top = rect.top + 'px'; img.style.left = rect.left + 'px';
      img.style.width = rect.width + 'px'; img.style.height = rect.height + 'px';
      setTimeout(() => overlay.remove(), 280);
    };
    overlay.addEventListener('click', close);
  });

  // ── Meta rows ──
  const meta = document.createElement('div');
  meta.className = 'flex flex-col gap-1.5';

  if (isLocal && (source || info?.source_id || sid)) {
    const p = document.createElement('p');
    p.className = 'text-base md:text-sm flex items-center gap-2';
    const sname = escapeHtml(source?.name || info?.source_name || 'Source');
    const srcId = source?.id || info?.source_id || sid;
    p.innerHTML = `<span class="font-semibold text-text">Source:</span> <a href="/source/${srcId}" class="text-accent hover:underline focus-visible:outline-none focus-visible:underline">${sname}</a>`;
    p.querySelector('a')?.addEventListener('click', e => { e.preventDefault(); navigate(`/source/${srcId}`); });
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
    if (isLocal) {
      p.innerHTML = '<span class="font-semibold text-text">Authors:</span> ' + info.authors.map((a, i) =>
        `<a class="text-accent hover:underline focus-visible:outline-none focus-visible:underline" href="/?author_id=${a.id}" data-idx="${i}">${escapeHtml(a.name)}</a>`
      ).join(', ');
      p.querySelectorAll('a').forEach(el => {
        const id = info.authors[Number(/** @type {HTMLElement} */(el).dataset.idx)].id;
        el.addEventListener('click', e => { e.preventDefault(); navigate(`/?author_id=${id}`); });
      });
    } else {
      p.innerHTML = '<span class="font-semibold text-text">Authors:</span> ' + info.authors.map((a, i) =>
        `<a class="text-accent hover:underline focus-visible:outline-none focus-visible:underline" href="/source/${sid}?q=${encodeURIComponent(a.name)}" data-idx="${i}">${escapeHtml(a.name)}</a>`
      ).join(', ');
      p.querySelectorAll('a').forEach(el => {
        const name = info.authors[Number(/** @type {HTMLElement} */(el).dataset.idx)].name;
        el.addEventListener('click', e => { e.preventDefault(); buildSourceMetaUrl(sid, name, 'Author').then(url => navigate(url)); });
      });
    }
    meta.appendChild(p);
  }

  if (info?.artists?.length) {
    const p = document.createElement('p');
    p.className = 'text-base md:text-sm';
    if (isLocal) {
      p.innerHTML = '<span class="font-semibold text-text">Artists:</span> ' + info.artists.map((a, i) =>
        `<a class="text-accent hover:underline focus-visible:outline-none focus-visible:underline" href="/?artist_id=${a.id}" data-idx="${i}">${escapeHtml(a.name)}</a>`
      ).join(', ');
      p.querySelectorAll('a').forEach(el => {
        const id = info.artists[Number(/** @type {HTMLElement} */(el).dataset.idx)].id;
        el.addEventListener('click', e => { e.preventDefault(); navigate(`/?artist_id=${id}`); });
      });
    } else {
      p.innerHTML = '<span class="font-semibold text-text">Artists:</span> ' + info.artists.map((a, i) =>
        `<a class="text-accent hover:underline focus-visible:outline-none focus-visible:underline" href="/source/${sid}?q=${encodeURIComponent(a.name)}" data-idx="${i}">${escapeHtml(a.name)}</a>`
      ).join(', ');
      p.querySelectorAll('a').forEach(el => {
        const name = info.artists[Number(/** @type {HTMLElement} */(el).dataset.idx)].name;
        el.addEventListener('click', e => { e.preventDefault(); buildSourceMetaUrl(sid, name, 'Artist').then(url => navigate(url)); });
      });
    }
    meta.appendChild(p);
  }

  // ── Button group ──
  const btnGroupEl = document.createElement('div');
  btnGroupEl.className = 'flex flex-col gap-2';
  _renderBtnGroup(btnGroupEl, info, source, ctx);

  // ── Description ──
  /** @type {HTMLElement|null} */ let descWrap = null;
  /** @type {HTMLElement|null} */ let desc = null;
  let expanded = false;

  if (info?.description_html || info?.description) {
    descWrap = document.createElement('div');
    desc = document.createElement('div');
    desc.className = 'text-sm text-text-muted leading-relaxed line-clamp-3';
    desc.innerHTML = info.description_html ?? `<p>${escapeHtml(info.description)}</p>`;

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
    toggle.className = 'text-xs text-accent hover:underline text-left mt-1 self-start';
    toggle.textContent = 'Show more';
    toggle.style.display = 'none';
    descWrap.appendChild(toggle);

    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        if (desc && desc.scrollHeight > desc.offsetHeight + 2) toggle.style.display = '';
      });
    });

    toggle.addEventListener('click', () => {
      expanded = !expanded;
      toggle.textContent = expanded ? 'Show less' : 'Show more';
      if (!desc) return;

      if (isDesktop()) {
        const slideAmount = coverInner ? Math.round(coverInner.offsetHeight * 0.55) : 80;
        if (expanded) {
          const clampedHeight = desc.offsetHeight;
          desc.dataset.clampedHeight = String(clampedHeight);
          desc.classList.remove('line-clamp-3');
          desc.style.overflow = 'hidden';
          const fullHeight = desc.scrollHeight;
          desc.style.maxHeight = clampedHeight + 'px';
          desc.offsetHeight; // force reflow
          desc.style.transition = 'max-height 0.4s ease';
          desc.style.maxHeight = fullHeight + 'px';
          contentCard.style.marginTop = `-${slideAmount}px`;

          let settled = false;
          const expand = () => {
            if (settled) return; settled = true;
            contentCard.removeEventListener('transitionend', expand);
            clearTimeout(safety);
            if (!desc || !descWrap) return;
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
          if (descWrap) descWrap.style.overflow = 'hidden';
          desc.style.overflow = 'hidden';
          desc.style.overflowY = '';
          desc.style.scrollbarWidth = '';
          desc.style.maxHeight = currentHeight + 'px';
          desc.offsetHeight; // force reflow
          desc.style.transition = 'max-height 0.4s ease';
          desc.style.maxHeight = (desc.dataset.clampedHeight || '72') + 'px';
          contentCard.style.marginTop = '-0.5rem';

          let settled = false;
          const collapse = () => {
            if (settled) return; settled = true;
            contentCard.removeEventListener('transitionend', collapse);
            clearTimeout(safety);
            if (!desc) return;
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

  // ── Layout containers ──
  const titleMetaCard = document.createElement('div');
  titleMetaCard.className = 'flex flex-col gap-2 min-w-0';
  titleMetaCard.appendChild(meta);

  const contentCard = document.createElement('div');
  contentCard.className = 'flex flex-col gap-3 min-w-0';
  contentCard.style.position = 'relative';
  contentCard.style.zIndex = '1';

  const heroRow = document.createElement('div');
  leftCol.appendChild(heroRow);

  // ── Responsive layout ──
  function applyHeroLayout() {
    if (!isDesktop()) {
      heroRow.style.cssText = 'display:flex;flex-direction:row;align-items:flex-start;gap:0.75rem';
      coverInner.style.width = '35%';
      coverInner.style.marginLeft = '';
      coverInner.style.marginRight = '';
      if (!heroRow.contains(coverInner)) heroRow.insertBefore(coverInner, heroRow.firstChild);
      if (!heroRow.contains(titleMetaCard)) heroRow.appendChild(titleMetaCard);
      if (heroRow.contains(contentCard)) heroRow.removeChild(contentCard);
      titleMetaCard.style.flex = '1 1 0%';
      if (!titleMetaCard.contains(meta)) titleMetaCard.appendChild(meta);
      btnGroupEl.style.paddingTop = '0.5rem';
      btnGroupEl.style.paddingBottom = '0.5rem';
      if (!leftCol.contains(btnGroupEl)) leftCol.appendChild(btnGroupEl);
      if (descWrap && !leftCol.contains(descWrap)) leftCol.appendChild(descWrap);
      contentCard.style.backgroundImage = '';
      contentCard.style.paddingTop = '';
      contentCard.style.marginTop = '';
      contentCard.style.transition = '';
    } else {
      heroRow.style.cssText = '';
      btnGroupEl.style.paddingTop = '';
      btnGroupEl.style.paddingBottom = '';
      if (!heroRow.contains(coverInner)) heroRow.insertBefore(coverInner, heroRow.firstChild);
      if (!heroRow.contains(contentCard)) heroRow.appendChild(contentCard);
      if (heroRow.contains(titleMetaCard)) heroRow.removeChild(titleMetaCard);
      if (!contentCard.contains(meta)) contentCard.insertBefore(meta, contentCard.firstChild);
      if (!contentCard.contains(btnGroupEl)) {
        if (descWrap && contentCard.contains(descWrap)) contentCard.insertBefore(btnGroupEl, descWrap);
        else contentCard.appendChild(btnGroupEl);
      }
      if (descWrap && !contentCard.contains(descWrap)) contentCard.appendChild(descWrap);
      contentCard.style.backgroundImage = 'linear-gradient(to bottom, transparent, var(--color-bg) 3rem)';
      contentCard.style.paddingTop = '3rem';
      if (!expanded) contentCard.style.marginTop = '-0.5rem';
      contentCard.style.transition = 'margin-top 0.35s ease';
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

  applyHeroLayout();
  window.addEventListener('resize', applyHeroLayout);

  return {
    destroy() { window.removeEventListener('resize', applyHeroLayout); },
    coverEl: coverInner,
  };
}

// ── Button group renderer ──────────────────────────────────────────────────────

/**
 * @param {HTMLElement} btnGroupEl
 * @param {any} info
 * @param {any} source
 * @param {HeroCtx} ctx
 */
function _renderBtnGroup(btnGroupEl, info, source, ctx) {
  btnGroupEl.innerHTML = '';
  const { isLocal, dbId, sid, mangaId, onDownloadAll, onCancelAll, onScan, onAddedToLibrary, onSwitchToChapters, findNextPreferredChapter } = ctx;

  if (isLocal) {
    const readBtn = document.createElement('button');
    readBtn.type = 'button';
    readBtn.className = 'btn-primary w-full';
    readBtn.textContent = 'Read';
    readBtn.addEventListener('click', async () => {
      if (readBtn.disabled) return;
      readBtn.disabled = true;
      try {
        const info = await api.getContinueReading(dbId);
        if (info) {
          const href = info.last_page > 0 ? `/reader/${info.chapter_id}?page=${info.last_page}` : `/reader/${info.chapter_id}`;
          navigate(href);
          return;
        }
        const nextUnread = findNextPreferredChapter();
        if (!nextUnread) {
          readBtn.disabled = false;
          const hasAnyUnread = ctx.getChapters().some(ch => !ch.read);
          if (hasAnyUnread) showToast('No chapters match your scanlator preferences. Adjust them in the Manage tab.', { type: 'warning' });
          else showToast('All chapters are read.');
          return;
        }
        const originalText = readBtn.textContent;
        readBtn.innerHTML = `<span class="inline-block animate-spin icon-sm">${iconSpinner}</span> Downloading…`;
        try {
          await api.downloadChapter(nextUnread.id);
          await new Promise((resolve, reject) => {
            let timeout;
            if (getState('chaptersProgress').get(nextUnread.id)?.status === 'completed') { resolve(undefined); return; }
            const unsub = subscribe('chaptersProgress', () => {
              if (getState('chaptersProgress').get(nextUnread.id)?.status === 'completed') {
                clearTimeout(timeout); unsub(); resolve(undefined);
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
      } catch { readBtn.disabled = false; }
    });
    api.getMangaTracking(dbId).then(t => {
      if (t && t.chapters_read > 0) readBtn.textContent = 'Continue Reading';
      else readBtn.textContent = 'Start Reading';
    }).catch(() => {});
    btnGroupEl.appendChild(readBtn);

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
          await onDownloadAll();
          dlBtn.style.display = 'none';
          cancelBtn.style.display = '';
        } finally { dlBtn.disabled = false; }
      });
      cancelBtn.addEventListener('click', async () => {
        cancelBtn.disabled = true;
        try {
          await onCancelAll();
        } finally {
          cancelBtn.disabled = false;
          cancelBtn.style.display = 'none';
          dlBtn.style.display = '';
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
          const res = await onScan();
          const count = res?.new_chapters ?? 0;
          showToast(
            count > 0 ? `${count} new chapter${count !== 1 ? 's' : ''} found` : 'No new chapters',
            { type: count > 0 ? 'success' : 'info' },
          );
        } catch (e) {
          showApiError(e);
        } finally { scanBtn.disabled = false; }
      });
      actionRow.appendChild(scanBtn);
    }

    if (actionRow.children.length > 0) btnGroupEl.appendChild(actionRow);
  } else {
    const inLibrary = !!(ctx.existingDbId() || ctx.addedDbId());
    if (inLibrary) {
      const existId = ctx.existingDbId() ?? ctx.addedDbId();
      const goBtn = document.createElement('a');
      goBtn.className = 'btn-primary w-full text-center';
      goBtn.textContent = 'Go to Library Entry';
      goBtn.href = `/manga/${existId}`;
      goBtn.addEventListener('click', e => { e.preventDefault(); navigate(`/manga/${existId}`); });
      btnGroupEl.appendChild(goBtn);
    } else if (hasPermission('library:add')) {
      const addBtn = document.createElement('button');
      addBtn.type = 'button';
      addBtn.className = 'btn-primary w-full';
      addBtn.textContent = 'Add to Library';
      addBtn.addEventListener('click', async () => {
        addBtn.disabled = true;
        try {
          const res = await api.saveToLibrary(sid, mangaId);
          const newDbId = res?.db_id ?? res?.id ?? null;
          if (newDbId) {
            onAddedToLibrary(newDbId);
            _renderBtnGroup(btnGroupEl, info, source, ctx);
          }
        } catch (err) {
          if (err?.status === 409 && err?.suggestions) {
            _showDuplicateModal(err.suggestions, sid, mangaId, onAddedToLibrary, btnGroupEl, info, source, ctx);
          } else {
            addBtn.disabled = false;
          }
        }
      });
      btnGroupEl.appendChild(addBtn);
    }
  }
}

/**
 * Show an inline duplicate-warning dialog below the add button.
 * @param {Array<{id: number, name: string, similarity: number, author_match: boolean}>} suggestions
 */
function _showDuplicateModal(suggestions, sid, mangaId, onAddedToLibrary, btnGroupEl, info, source, ctx) {
  const overlay = document.createElement('div');
  overlay.className = 'mt-3 p-3 rounded-lg bg-surface-2 border border-border-subtle text-sm flex flex-col gap-2';

  const top = document.createElement('div');
  top.className = 'font-medium text-warn';
  top.textContent = 'Possible duplicate detected';
  overlay.appendChild(top);

  for (const s of suggestions.slice(0, 3)) {
    const row = document.createElement('div');
    row.className = 'flex items-center justify-between gap-2';
    const nameLink = document.createElement('a');
    nameLink.href = `/manga/${s.id}`;
    nameLink.className = 'text-accent hover:underline truncate';
    nameLink.textContent = s.name;
    nameLink.addEventListener('click', e => { e.preventDefault(); navigate(`/manga/${s.id}`); });
    const meta = document.createElement('span');
    meta.className = 'text-text-muted text-xs shrink-0';
    meta.textContent = `${Math.round(s.similarity * 100)}% match${s.author_match ? ' · author' : ''}`;
    row.appendChild(nameLink);
    row.appendChild(meta);
    overlay.appendChild(row);
  }

  const actions = document.createElement('div');
  actions.className = 'flex gap-2 pt-1';

  const forceBtn = document.createElement('button');
  forceBtn.type = 'button';
  forceBtn.className = 'btn-primary btn-sm';
  forceBtn.textContent = 'Add anyway';
  forceBtn.addEventListener('click', async () => {
    forceBtn.disabled = true;
    try {
      const res = await api.saveToLibrary(sid, mangaId, true);
      const newDbId = res?.db_id ?? res?.id ?? null;
      overlay.remove();
      if (newDbId) { onAddedToLibrary(newDbId); _renderBtnGroup(btnGroupEl, info, source, ctx); }
    } catch { forceBtn.disabled = false; }
  });

  const cancelBtn = document.createElement('button');
  cancelBtn.type = 'button';
  cancelBtn.className = 'btn-secondary btn-sm';
  cancelBtn.textContent = 'Cancel';
  cancelBtn.addEventListener('click', () => overlay.remove());

  actions.appendChild(forceBtn);
  actions.appendChild(cancelBtn);
  overlay.appendChild(actions);
  btnGroupEl.appendChild(overlay);
}
