// @ts-check
// Notifications panel — dropdown showing scan notifications and download activity.

import { h, render } from 'preact';
import { useState, useEffect, useRef } from 'preact/hooks';
import htm from 'htm';
import { getState, subscribe, setState, updateState } from '../state.js';
import { navigate } from '../router.js';
import { iconBell, iconX, iconDownload, iconCheck } from '../icons.js';
import { Icon } from './icon.js';
const html = htm.bind(h);

/** @typedef {import('../state.js').ScanNotification} ScanNotification */
/** @typedef {import('../state.js').ChapterProgress} ChapterProgress */

function NotificationsPanel() {
  const [open, setOpen] = useState(false);
  const [notifications, setNotifications] = useState(/** @type {ScanNotification[]} */ ([]));
  const [activeDownloads, setActiveDownloads] = useState(0);
  const [failedDownloads, setFailedDownloads] = useState(0);
  const [completedDownloads, setCompletedDownloads] = useState(/** @type {ChapterProgress[]} */ ([]));
  const wrapRef = useRef(/** @type {HTMLDivElement | null} */ (null));

  useEffect(() => {
    setNotifications(getState('scanNotifications'));
    return subscribe('scanNotifications', (notifs) => setNotifications([...notifs]));
  }, []);

  useEffect(() => {
    function syncDownloads() {
      /** @type {Map<number, ChapterProgress>} */
      const map = getState('chaptersProgress');
      let active = 0, failed = 0;
      /** @type {ChapterProgress[]} */
      const completed = [];
      for (const e of map.values()) {
        if (e.status === 'in_progress') active++;
        else if (e.status === 'failed') failed++;
        else if (e.status === 'completed') completed.push(e);
      }
      setActiveDownloads(active);
      setFailedDownloads(failed);
      setCompletedDownloads(completed);
    }
    syncDownloads();
    return subscribe('chaptersProgress', syncDownloads);
  }, []);

  // Close panel on outside click
  useEffect(() => {
    if (!open) return;
    /** @param {MouseEvent} e */
    const handler = (e) => {
      if (!wrapRef.current?.contains(/** @type {Node} */ (e.target))) setOpen(false);
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, [open]);

  const chapterCount = notifications.reduce((sum, n) => sum + n.count, 0);
  const badgeCount = chapterCount + failedDownloads + completedDownloads.length;

  function dismissAll() {
    setState('scanNotifications', []);
    updateState('chaptersProgress', (map) => {
      const next = new Map(map);
      for (const [k, v] of next) {
        if (v.status === 'completed') next.set(k, { ...v, status: 'completed_hidden' });
      }
      return next;
    });
    setOpen(false);
  }

  function dismiss(/** @type {number} */ mangaId) {
    setState('scanNotifications', notifications.filter(n => n.mangaId !== mangaId));
  }

  function dismissCompletedDownload(/** @type {number} */ chapterId) {
    updateState('chaptersProgress', (map) => {
      const next = new Map(map);
      const entry = next.get(chapterId);
      if (entry) next.set(chapterId, { ...entry, status: 'completed_hidden' });
      return next;
    });
  }

  const hasAnyDismissable = notifications.length > 0 || completedDownloads.length > 0;

  // Merged feed: completed downloads first, then scan notifications
  /** @type {Array<{ type: 'download', dl: ChapterProgress } | { type: 'scan', n: ScanNotification }>} */
  const feed = [
    ...completedDownloads.map(dl => ({ type: /** @type {'download'} */ ('download'), dl })),
    ...notifications.map(n => ({ type: /** @type {'scan'} */ ('scan'), n })),
  ];

  return html`
    <div class="relative" ref=${wrapRef}>
      <button
        type="button"
        class=${'relative inline-flex items-center justify-center w-9 h-9 rounded-full transition-colors focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2 focus-visible:ring-offset-bg focus-visible:outline-none ' + (badgeCount === 0 && activeDownloads === 0 ? 'text-text-muted hover:bg-surface-2' : 'text-accent hover:bg-accent/10')}
        aria-label=${'Notifications' + (badgeCount > 0 ? ': ' + badgeCount + ' unread' : '')}
        aria-expanded=${open}
        onClick=${() => setOpen(v => !v)}
      >
        <${Icon} svg=${iconBell} />
        ${badgeCount > 0 && html`
          <span class="absolute -top-1 -right-1 min-w-5 h-5 px-1 flex items-center justify-center text-2xs font-bold bg-accent text-on-accent rounded-full">
            ${badgeCount}
          </span>
        `}
        ${badgeCount === 0 && activeDownloads > 0 && html`
          <span class="absolute -top-0.5 -right-0.5 w-2.5 h-2.5 rounded-full bg-accent animate-pulse"></span>
        `}
      </button>

      ${open && html`
        <div class="absolute top-full right-0 mt-2 w-80 bg-surface border border-border rounded-xl shadow-lg z-50 overflow-hidden flex flex-col">

          <!-- Active downloads row -->
          ${activeDownloads > 0 && html`
            <a
              href="/downloads"
              class="flex items-center gap-3 px-4 py-3 border-b border-border hover:bg-surface-2 transition-colors shrink-0"
              onClick=${() => setOpen(false)}
            >
              <span class="shrink-0 icon-sm text-accent"><${Icon} svg=${iconDownload} /></span>
              <span class="flex-1 text-sm text-text">
                <strong>${activeDownloads}</strong> chapter${activeDownloads !== 1 ? 's' : ''} downloading
              </span>
              <span class="text-xs text-accent shrink-0">View →</span>
            </a>
          `}

          <!-- Failed downloads row -->
          ${failedDownloads > 0 && html`
            <a
              href="/downloads"
              class="flex items-center gap-3 px-4 py-3 border-b border-border hover:bg-surface-2 transition-colors shrink-0"
              onClick=${() => setOpen(false)}
            >
              <span class="shrink-0 icon-sm text-danger"><${Icon} svg=${iconDownload} /></span>
              <span class="flex-1 text-sm text-text">
                <strong class="text-danger">${failedDownloads}</strong> download${failedDownloads !== 1 ? 's' : ''} failed
              </span>
              <span class="text-xs text-accent shrink-0">View →</span>
            </a>
          `}

          <!-- Merged notification feed -->
          ${feed.length === 0
            ? html`<p class="p-4 text-sm text-text-muted">No new notifications.</p>`
            : html`
              <ul class="max-h-80 overflow-y-auto divide-y divide-border-subtle">
                ${feed.map(item => {
                  if (item.type === 'download') {
                    const dl = item.dl;
                    return html`
                      <li key=${'dl-' + dl.id} class="flex items-center gap-2 px-4 py-2.5">
                        <span class="shrink-0 text-success icon-xs"><${Icon} svg=${iconCheck} /></span>
                        <div class="flex-1 min-w-0">
                          <p class="text-2xs font-semibold uppercase tracking-wide text-text-muted mb-0.5">Chapter Downloaded</p>
                          ${dl.mangaId > 0 && html`
                            <a
                              href=${'/manga/' + dl.mangaId}
                              class="text-xs text-text-muted truncate block hover:text-accent transition-colors"
                              onClick=${(/** @type {MouseEvent} */ e) => { e.preventDefault(); setOpen(false); navigate('/manga/' + dl.mangaId); }}
                            >${dl.mangaTitle || 'Manga'}</a>
                          `}
                          <a
                            href=${'/reader/' + dl.id}
                            class="text-sm text-text truncate block hover:text-accent transition-colors"
                            onClick=${(/** @type {MouseEvent} */ e) => { e.preventDefault(); setOpen(false); navigate('/reader/' + dl.id); }}
                          >${dl.name}</a>
                        </div>
                        <button
                          type="button"
                          class="btn-icon w-6 h-6 shrink-0"
                          aria-label="Dismiss"
                          onClick=${() => dismissCompletedDownload(dl.id)}
                        ><${Icon} svg=${iconX} /></button>
                      </li>
                    `;
                  } else {
                    const n = item.n;
                    return html`
                      <li key=${'scan-' + n.mangaId} class="flex flex-col gap-1 px-4 py-3">
                        <div class="flex items-center gap-3">
                          <span class="shrink-0 text-accent icon-xs"><${Icon} svg=${iconBell} /></span>
                          <div class="flex-1 min-w-0">
                            <p class="text-2xs font-semibold uppercase tracking-wide text-text-muted mb-0.5">New Chapter${n.count !== 1 ? 's' : ''}</p>
                            <a
                              href=${'/manga/' + n.mangaId}
                              class="text-sm font-medium text-text truncate block hover:text-accent transition-colors"
                              onClick=${() => setOpen(false)}
                            >${n.mangaName}</a>
                          </div>
                          <span class="text-xs text-text-muted whitespace-nowrap shrink-0">+${n.count} new</span>
                          <button type="button" class="btn-icon w-7 h-7 shrink-0" aria-label="Dismiss" onClick=${() => dismiss(n.mangaId)}><${Icon} svg=${iconX} /></button>
                        </div>
                        ${n.chapterNames?.length > 0 && html`
                          <div class="flex flex-col gap-0.5 pl-6">
                            ${n.chapterNames.slice(0, 3).map(name => html`
                              <span class="text-xs text-text-muted truncate">${name}</span>
                            `)}
                            ${n.chapterNames.length > 3 && html`
                              <span class="text-xs text-text-faint">+${n.chapterNames.length - 3} more</span>
                            `}
                          </div>
                        `}
                      </li>
                    `;
                  }
                })}
              </ul>
            `
          }

          <!-- Single dismiss-all button at bottom -->
          ${hasAnyDismissable && html`
            <div class="px-4 py-2.5 border-t border-border-subtle shrink-0">
              <button type="button" class="btn-ghost btn-sm w-full" onClick=${dismissAll}>
                Dismiss all
              </button>
            </div>
          `}
        </div>
      `}
    </div>
  `;
}

/**
 * Mount the notifications panel into a container.
 * @param {HTMLElement} container
 */
export function mountNotificationsPanel(container) {
  render(html`<${NotificationsPanel} />`, container);
}
