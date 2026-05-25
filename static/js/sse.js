// @ts-check
// Server-Sent Events client for real-time download/refresh/scan notifications.
// Manages reconnection with exponential backoff and detects server restarts.

import { getState, setState, updateState } from './state.js';

const SSE_URL = '/rest/events';
const MAX_DELAY_MS = 30_000;

/** @type {EventSource | null} */
let _source = null;
let _retryCount = 0;
/** @type {ReturnType<typeof setTimeout> | null} */
let _retryTimer = null;

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
    try { data = JSON.parse(event.data); } catch { return; }
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
    return;
  }

  if (type === 'manga_refreshed') {
    updateState('refreshState', (s) => {
      if (s.type !== 'running') return s;
      return { ...s, completed: data.completed, total: data.total };
    });
    return;
  }

  if (type === 'completed') {
    setState('refreshState', { type: 'done', total: data.total, failed: data.failed });
    // Increment library invalidation so pages re-fetch
    updateState('libraryInvalidation', (n) => n + 1);
    // Return to idle after 5s
    setTimeout(() => setState('refreshState', { type: 'idle' }), 5000);
    return;
  }

  // ── New chapters ─────────────────────────────────────────────────────────
  if (type === 'new_chapters') {
    if (localStorage.getItem('kani_disable_notifications') === 'true') return;
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
