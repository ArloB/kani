// @ts-check
// Virtual chapter list — windowed rendering for large chapter counts.

import { h } from 'preact';
import { useState, useEffect, useRef } from 'preact/hooks';
import htm from 'htm';
import { getState, subscribe, hasPermission } from '../state.js';
import { formatDate, isChapterDownloaded } from '../utils.js';
import { downloadChapter, deleteChapter, cancelDownload, setChapterReadStatus, markChaptersUpTo } from '../api.js';
import { iconCheck, iconDownload, iconCloud, iconCloudCheck } from '../icons.js';
import { Icon } from './icon.js';
import { ContextMenu } from './menu.js';
import { cacheChapter, evictChapter } from '../offline.js';
const html = htm.bind(h);

/** @typedef {import('../state.js').ChapterProgress} ChapterProgress */

const ROW_H = 56; // px — must match the rendered height of each chapter row
const OVERSCAN = 5;

/**
 * @typedef {{
 *   id: number,
 *   title: string,
 *   chapter_number?: number | null,
 *   source_chapter_id?: string | null,
 *   scanlator?: string | null,
 *   date_uploaded?: string | null,
 *   downloaded: boolean,
 *   read?: boolean,
 *   is_orphaned?: boolean,
 * }} VirtualChapter
 */

/**
 * @param {{
 *   chapter: VirtualChapter,
 *   readerHref: string,
 *   inLibrary: boolean,
 *   mangaId?: number | null,
 *   selectMode?: boolean,
 *   selected?: boolean,
 *   isCached?: boolean,
 *   kccAvailable?: boolean,
 *   onToggleRead?: (id: number, isRead: boolean) => void,
 *   onMarkUpTo?: (chapterNumber: number, isRead: boolean) => void,
 *   onToggleSelect?: (id: number) => void,
 *   onEnterSelectWithChapter?: (id: number) => void,
 *   onDelete?: (id: number) => void,
 *   onCacheChange?: (id: number, cached: boolean) => void,
 * }} props
 */
function ChapterRow({ chapter, readerHref, inLibrary, mangaId, selectMode, selected, isCached, kccAvailable, onToggleRead, onMarkUpTo, onToggleSelect, onEnterSelectWithChapter, onDelete, onCacheChange }) {
  const [progress, setProgress] = useState(/** @type {ChapterProgress|null} */(null));
  const [isRead, setIsRead] = useState(!!chapter.read);
  const [menuOpen, setMenuOpen] = useState(false);
  const btnRef = useRef(/** @type {HTMLButtonElement|null} */(null));
  const longPressTimer = useRef(/** @type {ReturnType<typeof setTimeout>|null} */ (null));
  // One-shot flag: absorbs the click from pointer-up that fires immediately after the
  // long-press timer calls onEnterSelectWithChapter and the row re-renders in select mode.
  const longPressFiredRef = useRef(false);

  useEffect(() => { setIsRead(!!chapter.read); }, [chapter.read]);

  function _startLongPress(/** @type {PointerEvent} */ e) {
    if (selectMode || !onEnterSelectWithChapter) return;
    // Only primary pointer (touch or left mouse)
    if (e.pointerType === 'mouse' && e.button !== 0) return;
    longPressTimer.current = setTimeout(() => {
      longPressTimer.current = null;
      longPressFiredRef.current = true;
      // Safety reset: if click never fires (mobile OS cancels after contextmenu), clear flag
      setTimeout(() => { longPressFiredRef.current = false; }, 300);
      onEnterSelectWithChapter(chapter.id);
    }, 400);
  }

  function _cancelLongPress() {
    if (longPressTimer.current != null) {
      clearTimeout(longPressTimer.current);
      longPressTimer.current = null;
    }
  }

  useEffect(() => {
    function sync() {
      /** @type {Map<number, ChapterProgress>} */
      const map = getState('chaptersProgress');
      setProgress(map.get(chapter.id) ?? null);
    }
    sync();
    return subscribe('chaptersProgress', sync);
  }, [chapter.id]);


  const isActive = progress?.status === 'in_progress';
  const isFailed = progress?.status === 'failed';
  const isCancelled = progress?.status === 'cancelled';
  const downloaded = isChapterDownloaded(chapter, progress);

  const canDownload = hasPermission('chapter:download');
  const canDelete = hasPermission('chapter:delete');

  // Download state button — still needed for in-progress / failed visual in context menu trigger area
  // We render a small status indicator inline with the title row instead of a full button
  let statusIndicator = null;
  if (inLibrary) {
    if (isActive) {
      const pct = progress && progress.totalPages > 0
        ? Math.round((progress.completedPages / progress.totalPages) * 100)
        : 0;
      const spinning = pct === 0;
      const circ = 75.4;
      const ring = html`
        <svg class=${'w-4 h-4 ' + (spinning ? 'dl-ring-spin' : '-rotate-90')} viewBox="0 0 32 32" aria-hidden="true">
          <circle cx="16" cy="16" r="12" fill="none" stroke="currentColor" stroke-width="2.5" opacity="0.25" />
          <circle cx="16" cy="16" r="12" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"
            stroke-dasharray=${spinning ? '56.5 18.9' : String(circ)}
            stroke-dashoffset=${spinning ? undefined : String(circ - circ * pct / 100)}
            style=${{ transition: 'stroke-dashoffset 0.3s ease' }}
          />
        </svg>
      `;
      statusIndicator = html`<span class="text-accent shrink-0" aria-label=${'Downloading' + (pct > 0 ? ` (${pct}%)` : '')}>${ring}</span>`;
    } else if (isFailed) {
      statusIndicator = html`<span class="text-danger text-xs shrink-0 font-medium" aria-label="Download failed">!</span>`;
    } else if (downloaded && !isCancelled) {
      statusIndicator = html`<span class="text-success shrink-0 icon-xs" aria-label="Downloaded"><${Icon} svg=${iconCheck} /></span>`;
    } else if (isRead) {
      // Read but not downloaded — show a muted checkmark so the read state is clearly visible
      statusIndicator = html`<span class="text-text-faint shrink-0 icon-xs" aria-label="Read, not downloaded"><${Icon} svg=${iconCheck} /></span>`;
    } else {
      statusIndicator = html`<span class="text-text-faint shrink-0 icon-xs" aria-label="Not downloaded"><${Icon} svg=${iconDownload} /></span>`;
    }
  }

  async function handleToggleRead() {
    const newRead = !isRead;
    setIsRead(newRead);
    try {
      await setChapterReadStatus([chapter.id], newRead);
      if (onToggleRead) onToggleRead(chapter.id, newRead);
    } catch (err) {
      setIsRead(!newRead);
      console.error('toggle read failed:', err);
    }
  }

  async function handleMarkUpTo(markRead) {
    if (!mangaId || chapter.chapter_number == null) return;
    try {
      await markChaptersUpTo(mangaId, chapter.chapter_number, markRead);
      if (onMarkUpTo) onMarkUpTo(chapter.chapter_number, markRead);
    } catch (err) {
      console.error('mark up to failed:', err);
    }
  }

  async function handleDownload() {
    try { await downloadChapter(chapter.id); } catch (err) { console.error('download failed:', err); }
  }

  async function handleDelete() {
    try {
      await deleteChapter(chapter.id);
      onDelete?.(chapter.id);
    } catch (err) { console.error('delete failed:', err); }
  }

  async function handleCacheToggle() {
    if (isCached) {
      evictChapter(chapter.id);
      onCacheChange?.(chapter.id, false);
    } else {
      const pageCount = Number(/** @type {any} */ (chapter).page_count ?? 0);
      cacheChapter(chapter.id, pageCount);
      onCacheChange?.(chapter.id, true);
    }
  }

  function handleExportDownload(url) {
    const a = document.createElement('a');
    a.href = url;
    a.download = '';
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    setMenuOpen(false);
  }

  async function handleCancel() {
    try { await cancelDownload(chapter.id); } catch (err) { console.error('cancel failed:', err); }
  }

  async function handleRetry() {
    try { await downloadChapter(chapter.id); } catch (err) { console.error('retry failed:', err); }
  }

  /** @type {import('./menu.js').MenuItem[]} */
  const menuItems = inLibrary ? [
    { label: 'Select', action: () => { if (onEnterSelectWithChapter) onEnterSelectWithChapter(chapter.id); } },
    { divider: true },
    { label: isRead ? 'Mark as unread' : 'Mark as read', action: handleToggleRead },
    ...(chapter.chapter_number != null ? [
      { label: 'Mark as read up to here', action: () => handleMarkUpTo(true) },
      { label: 'Mark as unread from here', action: () => handleMarkUpTo(false) },
    ] : []),
    ...((canDownload || canDelete) ? [{ divider: /** @type {true} */ (true) }] : []),
    ...(isActive && canDownload ? [{ label: 'Cancel download', action: handleCancel }] : []),
    ...(isFailed && canDownload ? [{ label: 'Retry download', action: handleRetry }] : []),
    ...(!isActive && !isFailed && !downloaded && canDownload ? [{ label: 'Download', action: handleDownload }] : []),
    ...(!isActive && downloaded && !isCancelled && canDelete ? [{ label: 'Delete download', action: handleDelete, danger: true }] : []),
    // ── Offline caching ───────────────────────────────────────────────────
    ...(!isActive && downloaded && !isCancelled && ('caches' in window) ? [
      { divider: /** @type {true} */ (true) },
      ...(isCached
        ? [{ label: 'Remove from offline cache', action: handleCacheToggle }]
        : [{ label: 'Save for offline', action: handleCacheToggle }]),
    ] : []),
    // ── Export ────────────────────────────────────────────────────────────
    ...(!isActive && downloaded && !isCancelled ? [
      { divider: /** @type {true} */ (true) },
      { label: 'Export as EPUB', action: () => handleExportDownload(`/rest/chapters/${chapter.id}/export/epub`) },
      { label: 'Export as EPUB (Kindle)', action: () => handleExportDownload(`/rest/chapters/${chapter.id}/export/epub?profile=kindle-pw`) },
      { label: 'Export as KEPUB (Kobo)', action: () => handleExportDownload(`/rest/chapters/${chapter.id}/export/kepub?profile=kobo-libra`) },
      ...(kccAvailable ? [{ label: 'Export as MOBI (Kindle)', action: () => handleExportDownload(`/rest/chapters/${chapter.id}/export/kcc?format=MOBI&profile=KPW5&manga=true`) }] : []),
    ] : []),
  ] : [];

  const menuBtn = inLibrary ? html`
    <div class="relative">
      <button
        ref=${btnRef}
        class="inline-flex items-center justify-center w-9 h-9 text-text-muted hover:text-text rounded-md cursor-pointer select-none transition-colors"
        aria-label="More actions"
        aria-expanded=${menuOpen}
        onClick=${(e) => { e.preventDefault(); e.stopPropagation(); setMenuOpen(o => !o); }}
      >⋮</button>
      ${menuOpen && html`<${ContextMenu} items=${menuItems} trigger=${btnRef} onClose=${() => setMenuOpen(false)} />`}
    </div>
  ` : null;

  if (selectMode) {
    let selectRowBorder = 'border-l-2 border-l-transparent ';
    if (downloaded) {
      selectRowBorder = 'border-l-2 border-l-success/60 ';
    } else if (isRead) {
      selectRowBorder = 'border-l-2 border-l-text-faint/30 ';
    }
    return html`
      <div
        class=${selectRowBorder + 'flex items-center gap-3 px-3 py-2.5 border-b border-border-subtle cursor-pointer select-none' + (chapter.is_orphaned ? ' opacity-60' : '') + (selected ? ' bg-accent/10' : '')}
        onClick=${() => {
          // Absorb the click from pointer-up that fires right after a long-press entered select mode
          if (longPressFiredRef.current) { longPressFiredRef.current = false; return; }
          onToggleSelect && onToggleSelect(chapter.id);
        }}
      >
        <input type="checkbox" class="shrink-0 accent-accent" checked=${!!selected} readOnly />
        <div class="flex-1 min-w-0 flex flex-col gap-0.5">
          <span class=${'text-sm truncate ' + (isRead ? 'text-text-faint' : 'text-text')}>${chapter.title}</span>
          <div class="flex items-center gap-3 text-xs text-text-muted">
            ${chapter.scanlator && html`<span>${chapter.scanlator}</span>`}
            ${chapter.date_uploaded && html`<span>${formatDate(chapter.date_uploaded)}</span>`}
          </div>
        </div>
      </div>
    `;
  }

  // For local chapters, only fully downloaded chapters are clickable (not while downloading)
  const isClickable = inLibrary && downloaded && !isActive;
  let nonClickableClass = '', nonClickableTitle = '';
  if (!isClickable) {
    nonClickableClass = ' cursor-default';
    nonClickableTitle = !inLibrary ? 'Add to library to read' : isActive ? 'Downloading…' : 'Download to read';
  }

  let rowBorderClass = 'border-l-2 border-l-transparent ';
  if (downloaded) {
    rowBorderClass = 'border-l-2 border-l-success/60 ';
  } else if (isRead) {
    rowBorderClass = 'border-l-2 border-l-text-faint/30 ';
  }

  return html`
    <div
      class=${rowBorderClass + 'flex items-center gap-3 px-3 py-2.5 border-b border-border-subtle' + (chapter.is_orphaned ? ' opacity-60' : '') + (menuOpen ? ' relative' : '')}
      style=${menuOpen ? 'z-index: 50' : undefined}
      onPointerDown=${_startLongPress}
      onPointerUp=${_cancelLongPress}
      onPointerCancel=${_cancelLongPress}
      onContextMenu=${_cancelLongPress}
    >
      <div class="flex-1 min-w-0 flex flex-col gap-0.5">
        <div class="flex items-center gap-2">
          ${chapter.is_orphaned && html`
            <span class="inline-flex items-center px-1.5 py-0.5 text-xs font-medium rounded-sm bg-warn/20 text-warn">Orphaned</span>
          `}
          ${statusIndicator}
          ${isClickable
      ? html`<a class=${'text-sm truncate hover:text-accent transition-colors ' + (isRead ? 'text-text-faint' : 'text-text')} href=${readerHref}>${chapter.title}</a>`
      : html`<span class=${'text-sm truncate' + nonClickableClass + (isRead ? ' text-text-faint' : ' text-text-muted')} title=${nonClickableTitle || undefined}>${chapter.title}</span>`
    }
        </div>
        <div class="flex items-center gap-3 text-xs text-text-muted">
          ${chapter.scanlator && html`<span>${chapter.scanlator}</span>`}
          ${chapter.date_uploaded && html`<span>${formatDate(chapter.date_uploaded)}</span>`}
        </div>
      </div>
      <div class="flex items-center gap-1 shrink-0">
        ${downloaded && !isActive && !isCancelled && ('caches' in window) && html`
          <span
            class=${'icon-xs ' + (isCached ? 'text-accent' : 'text-text-faint')}
            title=${isCached ? 'Cached for offline' : 'Not cached'}
            aria-label=${isCached ? 'Cached for offline' : 'Not cached'}
          >
            <${Icon} svg=${isCached ? iconCloudCheck : iconCloud} />
          </span>
        `}
        ${menuBtn}
      </div>
    </div>
  `;
}

/**
 * Chapter list with optional windowed rendering and infinite-scroll load-more.
 *
 * When `height` is provided, renders only the rows visible inside a fixed-height
 * scroll container. `onLoadMore` is triggered by the scroll handler when the user
 * approaches the bottom.
 *
 * Without `height`, all rows are rendered in normal document flow and an
 * IntersectionObserver sentinel triggers `onLoadMore` when it enters the viewport.
 *
 * @param {{
 *   chapters: VirtualChapter[],
 *   readerHrefFn: (ch: VirtualChapter) => string,
 *   inLibrary: boolean,
 *   mangaId?: number | null,
 *   height?: number,
 *   hasMore?: boolean,
 *   loading?: boolean,
 *   selectMode?: boolean,
 *   selected?: Set<number>,
 *   canDownload?: boolean,
 *   canDelete?: boolean,
 *   onLoadMore?: () => void,
 *   onToggleRead?: (id: number, isRead: boolean) => void,
 *   onMarkUpTo?: (chapterNumber: number, isRead: boolean) => void,
 *   onToggleSelect?: (id: number) => void,
 *   onSelectAll?: () => void,
 *   onFlipSelection?: () => void,
 *   onSelectUndownloaded?: () => void,
 *   onSelectUnread?: () => void,
 *   onBulkRead?: (isRead: boolean) => void,
 *   onBulkDownload?: () => void,
 *   onBulkDelete?: () => void,
 *   onExitSelect?: () => void,
 *   onEnterSelectWithChapter?: (id: number) => void,
 *   onDelete?: (id: number) => void,
 *   cachedChapterIds?: Set<number>,
 *   kccAvailable?: boolean,
 *   onCacheChange?: (id: number, cached: boolean) => void,
 * }} props
 */
export function VirtualChapterList({ chapters, readerHrefFn, inLibrary, mangaId, height, hasMore, loading, selectMode, selected, canDownload, canDelete, allSelectedProp, onLoadMore, onToggleRead, onMarkUpTo, onToggleSelect, onSelectAll, onFlipSelection, onSelectUndownloaded, onSelectUnread, onBulkRead, onBulkDownload, onBulkDelete, onExitSelect, onEnterSelectWithChapter, onDelete, cachedChapterIds, kccAvailable, onCacheChange }) {
  const [scrollTop, setScrollTop] = useState(0);
  const sentinelRef = useRef(/** @type {HTMLDivElement | null} */(null));

  // IntersectionObserver for the non-windowed sentinel
  useEffect(() => {
    if (height || !hasMore || !onLoadMore || !sentinelRef.current) return;
    const observer = new IntersectionObserver((entries) => {
      if (entries[0].isIntersecting) {
        observer.disconnect();
        onLoadMore();
      }
    }, { rootMargin: '200px' });
    observer.observe(sentinelRef.current);
    return () => observer.disconnect();
  }, [height, hasMore, onLoadMore, chapters.length]);

  const skeletonRow = html`<div class="h-14 mx-3 my-1 skeleton rounded-lg" />`;

  const selectedCount = selected ? selected.size : 0;
  const allSelected = allSelectedProp !== undefined
    ? allSelectedProp
    : (selectedCount === chapters.length && chapters.length > 0);

  // Count downloaded/undownloaded among selected for smart bulk-bar feedback
  const selectedDownloadedCount = selected
    ? chapters.filter(ch => selected.has(ch.id) && ch.downloaded).length
    : 0;
  const selectedUndownloadedCount = selectedCount - selectedDownloadedCount;

  const bulkBar = selectMode ? html`
    <div class="sticky bottom-0 z-20 flex flex-col bg-surface border-t border-border">
      <div class="flex items-center gap-2 flex-wrap px-3 py-2">
      <div class="flex flex-col">
        <span class="text-sm text-text-muted">${selectedCount} selected</span>
        ${selectedCount > 0 && html`
          <span class="text-xs text-text-faint">
            ${selectedDownloadedCount > 0 ? `${selectedDownloadedCount} downloaded` : ''}${selectedDownloadedCount > 0 && selectedUndownloadedCount > 0 ? ', ' : ''}${selectedUndownloadedCount > 0 ? `${selectedUndownloadedCount} not downloaded` : ''}
          </span>
        `}
      </div>
      <div class="flex items-center gap-1.5 flex-wrap flex-1">
        <button class="btn-ghost btn-sm" onClick=${() => onSelectAll && onSelectAll()}>
          ${allSelected ? 'Deselect all' : 'Select all'}
        </button>
        ${onFlipSelection && html`
          <button class="btn-ghost btn-sm" onClick=${() => onFlipSelection()}>Flip</button>
        `}
        ${onSelectUndownloaded && html`
          <button class="btn-ghost btn-sm" onClick=${() => onSelectUndownloaded()}>Undownloaded</button>
        `}
        ${onSelectUnread && html`
          <button class="btn-ghost btn-sm" onClick=${() => onSelectUnread()}>Unread</button>
        `}
      </div>
      <div class="flex items-center gap-1.5 flex-wrap">
        <button class="btn-primary btn-sm" disabled=${selectedCount === 0} onClick=${() => onBulkRead && onBulkRead(true)}>Mark read</button>
        <button class="btn-ghost btn-sm" disabled=${selectedCount === 0} onClick=${() => onBulkRead && onBulkRead(false)}>Mark unread</button>
        ${canDownload && onBulkDownload && html`
          <button class="btn-ghost btn-sm" disabled=${selectedUndownloadedCount === 0} onClick=${() => onBulkDownload()}>Download</button>
        `}
        ${canDelete && onBulkDelete && html`
          <button class="btn-ghost btn-sm" disabled=${selectedDownloadedCount === 0} onClick=${() => onBulkDelete()}>Delete</button>
        `}
        <button class="btn-ghost btn-sm" onClick=${() => onExitSelect && onExitSelect()}>Cancel</button>
      </div>
      </div>
    </div>
  ` : null;

  if (!height) {
    return html`
      <div class="flex flex-col">
        <div class="flex flex-col divide-y divide-border-subtle">
          ${chapters.map(ch => html`
            <${ChapterRow}
              key=${ch.id}
              chapter=${ch}
              readerHref=${readerHrefFn(ch)}
              inLibrary=${inLibrary}
              mangaId=${mangaId}
              selectMode=${!!selectMode}
              selected=${selected ? selected.has(ch.id) : false}
              onToggleRead=${onToggleRead}
              onMarkUpTo=${onMarkUpTo}
              onToggleSelect=${onToggleSelect}
              onEnterSelectWithChapter=${onEnterSelectWithChapter}
              onDelete=${onDelete}
              isCached=${cachedChapterIds ? cachedChapterIds.has(ch.id) : false}
              kccAvailable=${!!kccAvailable}
              onCacheChange=${onCacheChange}
            />
          `)}
          ${loading ? skeletonRow : (hasMore && html`<div ref=${sentinelRef} class="h-px" />`)}
        </div>
        ${bulkBar}
      </div>
    `;
  }

  // Windowed rendering for fixed-height scroll containers
  const totalH = chapters.length * ROW_H;
  const visibleCount = Math.ceil(height / ROW_H);
  const startIdx = Math.max(0, Math.floor(scrollTop / ROW_H) - OVERSCAN);
  const endIdx = Math.min(chapters.length, startIdx + visibleCount + OVERSCAN * 2);
  const visible = chapters.slice(startIdx, endIdx);

  function handleScroll(e) {
    const el = /** @type {HTMLElement} */ (e.currentTarget);
    setScrollTop(el.scrollTop);
    if (hasMore && onLoadMore && !loading &&
      el.scrollTop + el.clientHeight >= totalH - ROW_H * (OVERSCAN + 3)) {
      onLoadMore();
    }
  }

  return html`
    <div class="flex flex-col">
      <div
        class="overflow-y-auto"
        style=${{ height: (selectMode ? Math.max(100, height - 60) : height) + 'px', scrollbarWidth: 'none' }}
        onScroll=${handleScroll}
      >
        <div style=${{ height: totalH + 'px', position: 'relative' }}>
          ${visible.map((ch, i) => html`
            <div
              key=${ch.id}
              style=${{ position: 'absolute', top: (startIdx + i) * ROW_H + 'px', width: '100%' }}
            >
              <${ChapterRow}
                chapter=${ch}
                readerHref=${readerHrefFn(ch)}
                inLibrary=${inLibrary}
                mangaId=${mangaId}
                selectMode=${!!selectMode}
                selected=${selected ? selected.has(ch.id) : false}
                onToggleRead=${onToggleRead}
                onMarkUpTo=${onMarkUpTo}
                onToggleSelect=${onToggleSelect}
                onEnterSelectWithChapter=${onEnterSelectWithChapter}
                onDelete=${onDelete}
              />
            </div>
          `)}
        </div>
        ${loading && html`<div class="px-3 py-2">${skeletonRow}</div>`}
      </div>
      ${bulkBar}
    </div>
  `;
}
