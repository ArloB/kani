// @ts-check
// Server-Sent Events client for real-time download/refresh/scan notifications.
// Manages reconnection with exponential backoff and detects server restarts.

import { getState, setState, updateState } from './state.js';
import { postToServiceWorker, cacheChapter } from './offline.js';

const SSE_URL = '/rest/events';
const MAX_DELAY_MS = 30_000;

/** @type {EventSource | null} */
let _source = null;
let _retryCount = 0;
/** @type {ReturnType<typeof setTimeout> | null} */
let _retryTimer = null;

/** Accumulates per-manga new chapter counts during a scan run. Reset on each 'started' event. */
let _scanNewChapters = /** @type {Map<number, number>} */ (new Map());

/**
 * Opens the SSE connection to /rest/events.
 * Call once at app startup. Returns a `disconnect()` function.
 * @returns {{ disconnect: () => void }}
 */
export function connectSSE() {
  _connect();
  return { disconnect: _disconnect };
}

function _connect() {
  if (_source) { _source.close(); _source = null; }

  _source = new EventSource(SSE_URL, { withCredentials: true });

  _source.addEventListener('open', () => {
    _retryCount = 0;
  });

  _source.addEventListener('message', (event) => {
    let data;
    try {
      data = JSON.parse(event.data);
    } catch (err) {
      console.warn('[SSE] Failed to parse event payload, dropping:', err, event.data);
      return;
    }
    _handleEvent(data);
    // Broadcast to any page-level listeners (e.g. manga-details watching its chapters).
    window.dispatchEvent(new CustomEvent('kani:sse', { detail: data }));
  });

  // The server sends a named 'close' event when the stream lags too much.
  _source.addEventListener('close', () => {
    _scheduleReconnect();
  });

  _source.addEventListener('error', () => {
    _source?.close();
    _source = null;
    _scheduleReconnect();
  });
}

function _disconnect() {
  if (_retryTimer) { clearTimeout(_retryTimer); _retryTimer = null; }
  if (_source) { _source.close(); _source = null; }
}

function _scheduleReconnect() {
  if (_retryTimer) return;
  const delay = Math.min(1000 * Math.pow(2, _retryCount), MAX_DELAY_MS);
  _retryCount++;
  _retryTimer = setTimeout(() => {
    _retryTimer = null;
    _connect();
  }, delay);
}

/** @param {any} data */
function _handleEvent(data) {
  const type = data.type;

  // ── Initial state snapshot ───────────────────────────────────────────────
  if (type === 'state_snapshot') {
    const prev = getState('bootId');
    if (data.boot_id && prev && data.boot_id !== prev) {
      window.dispatchEvent(new CustomEvent('kani:server-restart'));
    }
    if (data.boot_id) setState('bootId', data.boot_id);

    // Repopulate chapter progress from snapshot
    /** @type {Map<number, import('./state.js').ChapterProgress>} */
    const map = new Map();
    for (const ch of (data.chapters ?? [])) {
      map.set(Number(ch.chapter_id), {
        id: Number(ch.chapter_id),
        name: ch.chapter_name ?? '',
        mangaId: Number(ch.manga_id ?? 0),
        mangaTitle: ch.manga_title ?? '',
        totalPages: ch.total_pages ?? 0,
        completedPages: ch.completed_pages ?? 0,
        status: _normaliseStatus(ch.status),
      });
    }
    setState('chaptersProgress', map);

    if (data.is_refreshing) {
      setState('refreshState', { type: 'running', completed: 0, total: 0 });
    }
    return;
  }

  // ── Download events ──────────────────────────────────────────────────────
  if (type === 'chapter_started') {
    updateState('chaptersProgress', (map) => {
      const m = new Map(map);
      m.set(Number(data.chapter_id), {
        id: Number(data.chapter_id),
        name: data.chapter_name,
        mangaId: Number(data.manga_id ?? 0),
        mangaTitle: data.manga_title ?? '',
        totalPages: data.total_pages,
        completedPages: 0,
        status: 'in_progress',
      });
      return m;
    });
    return;
  }

  if (type === 'page_completed') {
    updateState('chaptersProgress', (map) => {
      const m = new Map(map);
      const id = Number(data.chapter_id);
      const entry = m.get(id);
      if (entry) {
        // Only advance — out-of-order events must not move the bar backward.
        const next = data.page_index + 1;
        if (next > entry.completedPages) {
          m.set(id, { ...entry, completedPages: next });
        }
      }
      return m;
    });
    return;
  }

  if (type === 'chapter_completed') {
    updateState('chaptersProgress', (map) => {
      const m = new Map(map);
      const id = Number(data.chapter_id);
      const entry = m.get(id);
      if (entry) m.set(id, { ...entry, status: 'completed', completedPages: entry.totalPages });
      return m;
    });
    _maybeAutoCache(Number(data.chapter_id), Number(data.successful_pages ?? 0));
    return;
  }

  if (type === 'chapter_failed') {
    updateState('chaptersProgress', (map) => {
      const m = new Map(map);
      const id = Number(data.chapter_id);
      const entry = m.get(id);
      if (entry) m.set(id, { ...entry, status: 'failed' });
      return m;
    });
    return;
  }

  if (type === 'chapter_cancelled' || type === 'chapter_deferred') {
    updateState('chaptersProgress', (map) => {
      const m = new Map(map);
      const id = Number(data.chapter_id);
      const entry = m.get(id);
      if (entry) m.set(id, { ...entry, status: 'cancelled' });
      return m;
    });
    return;
  }

  // ── Refresh events ───────────────────────────────────────────────────────
  if (type === 'started') {
    setState('refreshState', { type: 'running', completed: 0, total: data.total });
    // Mark every manga that will be scanned as "pending" so covers show a spinner.
    setState('scanningMangaIds', new Set((data.manga_ids ?? []).map(Number)));
    // Reset per-manga chapter accumulator for the new run.
    _scanNewChapters = new Map();
    return;
  }

  if (type === 'manga_refreshed') {
    updateState('refreshState', (s) => {
      if (s.type !== 'running') return s;
      return { ...s, completed: data.completed, total: data.total };
    });
    // Remove from pending set — this manga is done.
    updateState('scanningMangaIds', (s) => { const n = new Set(s); n.delete(Number(data.manga_id)); return n; });
    // Accumulate new chapter counts (non-zero only for scan operations).
    const nc = Number(data.new_chapters ?? 0);
    if (nc > 0) _scanNewChapters.set(Number(data.manga_id), nc);
    return;
  }

  if (type === 'completed') {
    const totalNew = [..._scanNewChapters.values()].reduce((a, b) => a + b, 0);
    setState('scanResult', {
      total: Number(data.total),
      failed: Number(data.failed),
      newChapters: totalNew,
      perManga: new Map(_scanNewChapters),
    });
    _scanNewChapters = new Map();
    setState('refreshState', { type: 'done', total: data.total, failed: data.failed });
    setState('scanningMangaIds', new Set());
    // Increment library invalidation so pages re-fetch
    updateState('libraryInvalidation', (n) => n + 1);
    // Return to idle after 5s
    setTimeout(() => setState('refreshState', { type: 'idle' }), 5000);
    return;
  }

  // ── New chapters ─────────────────────────────────────────────────────────
  if (type === 'new_chapters') {
    // In-app badge notifications
    if (localStorage.getItem('kani_disable_notifications') !== 'true') {
      const incomingNames = /** @type {string[]} */ (data.chapter_names ?? []);
      updateState('scanNotifications', (list) => {
        const existing = list.findIndex((/** @type {{ mangaId: number; }} */ n) => n.mangaId === Number(data.manga_id));
        if (existing >= 0) {
          const copy = [...list];
          copy[existing] = {
            ...copy[existing],
            count: copy[existing].count + data.count,
            chapterNames: [...(copy[existing].chapterNames ?? []), ...incomingNames],
          };
          return copy;
        }
        return [...list, {
          mangaId: Number(data.manga_id),
          mangaName: data.manga_name,
          count: data.count,
          chapterNames: incomingNames,
        }];
      });
    }

    // Browser push notifications
    const mangaId = Number(data.manga_id);
    const notifyPrefs = getState('mangaNotifyPrefs');
    const notifyAllowed = notifyPrefs instanceof Map
      ? (notifyPrefs.has(mangaId) ? notifyPrefs.get(mangaId) : true)
      : true;
    if (
      notifyAllowed &&
      localStorage.getItem('kani_browser_notifications') === 'true' &&
      'Notification' in window &&
      Notification.permission === 'granted'
    ) {
      const count = data.count ?? 1;
      const body = count === 1
        ? (data.chapter_names?.[0] ?? 'New chapter available')
        : `${count} new chapters`;
      try {
        new Notification(data.manga_name ?? 'New chapters', { body, tag: `kani-manga-${mangaId}` });
      } catch { /* ignore — browsers may restrict notifications in some contexts */ }
    }
  }
}

/**
 * Normalises a status string from the snapshot into our internal status enum.
 * @param {string} raw
 * @returns {'in_progress'|'completed'|'failed'|'cancelled'}
 */
function _normaliseStatus(raw) {
  if (raw === 'in_progress') return 'in_progress';
  if (raw === 'completed')   return 'completed';
  if (raw === 'failed')      return 'failed';
  return 'cancelled';
}

/**
 * Cache a just-downloaded chapter if offline auto-mode is active.
 * @param {number} chapterId
 * @param {number} pageCount
 */
function _maybeAutoCache(chapterId, pageCount) {
  if (localStorage.getItem('kani_offline_mode') !== 'auto') return;
  if (!navigator.serviceWorker?.controller) return;
  if (pageCount <= 0) return;
  cacheChapter(chapterId, pageCount);
}
