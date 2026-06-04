// @ts-check
// REST API client. All functions return Promises and throw on non-2xx responses.
// A 401 response redirects to /login automatically.

/** @param {Response} res */
async function _parseBody(res) {
  const ct = res.headers.get('content-type') ?? '';
  if (ct.includes('application/json')) return res.json();
  if (res.status === 204 || res.headers.get('content-length') === '0') return null;
  return res.text();
}

/**
 * Internal fetch wrapper.
 * @param {string} method
 * @param {string} path
 * @param {{ body?: any, params?: Record<string, any>, signal?: AbortSignal, timeoutMs?: number }} [opts]
 */
async function _req(method, path, opts = {}) {
  let url = `/rest${path}`;

  if (opts.params) {
    const qs = new URLSearchParams();
    for (const [k, v] of Object.entries(opts.params)) {
      if (v != null && v !== '') {
        qs.set(k, typeof v === 'object' ? JSON.stringify(v) : String(v));
      }
    }
    const s = qs.toString();
    if (s) url += '?' + s;
  }

  /** @type {RequestInit} */
  const init = { method, credentials: 'include', headers: {} };
  if (opts.body != null) {
    // @ts-ignore
    init.headers['Content-Type'] = 'application/json';
    init.body = JSON.stringify(opts.body);
  }

  // Apply timeout via a local AbortController; if the caller also passed a signal,
  // chain it so either abort source cancels the request.
  let timer;
  if (opts.timeoutMs && opts.timeoutMs > 0) {
    const ctrl = new AbortController();
    timer = setTimeout(() => ctrl.abort(new DOMException('Request timed out', 'TimeoutError')), opts.timeoutMs);
    if (opts.signal) {
      if (opts.signal.aborted) ctrl.abort(opts.signal.reason);
      else opts.signal.addEventListener('abort', () => ctrl.abort(opts.signal?.reason), { once: true });
    }
    init.signal = ctrl.signal;
  } else if (opts.signal) {
    init.signal = opts.signal;
  }

  let res;
  try {
    res = await fetch(url, init);
  } finally {
    if (timer) clearTimeout(timer);
  }

  if (res.status === 401) {
    if (location.pathname !== '/login') window.location.href = '/login';
    throw Object.assign(new Error('Unauthorized'), { status: 401 });
  }

  if (!res.ok) {
    let body;
    try { body = await res.json(); } catch { body = { error: await res.text().catch(() => res.statusText) }; }
    throw Object.assign(new Error(body?.error || `HTTP ${res.status}`), {
      status: res.status,
      code: body?.code ?? null,
      hint: body?.hint ?? null,
      suggestions: body?.suggestions ?? null,
      body,
    });
  }

  return _parseBody(res);
}

// ── Auth ─────────────────────────────────────────────────────────────────────

/**
 * Returns the authenticated user's permission list as an array of strings.
 * @returns {Promise<string[]>}
 */
export async function getPermissions() {
  return _req('GET', '/auth/permissions');
}

export async function getMe() {
  return _req('GET', '/auth/me');
}

export async function getCurrentUser() {
  return _req('GET', '/auth/current_user');
}

/** @param {string} username @param {string} password */
export async function login(username, password) {
  return _req('POST', '/auth/login', { body: { username, password } });
}

export async function logout() {
  return _req('POST', '/auth/logout');
}

/** @param {string} currentPassword @param {string} newPassword */
export async function changePassword(currentPassword, newPassword) {
  return _req('POST', '/auth/change_password', {
    body: { current_password: currentPassword, new_password: newPassword },
  });
}

export async function logoutEverywhere() {
  return _req('POST', '/auth/logout_everywhere');
}

export async function getPasswordResetEnabled() {
  return _req('GET', '/auth/password-reset-enabled');
}

export async function getRegistrationEnabled() {
  return _req('GET', '/auth/registration-enabled');
}

/** Default timeout (ms) for auth-flow requests so a hanging server doesn't strand the submit button. */
const AUTH_TIMEOUT_MS = 15_000;

/** @param {string} email */
export async function requestPasswordReset(email) {
  return _req('POST', '/auth/password-reset/request', { body: { email }, timeoutMs: AUTH_TIMEOUT_MS });
}

/** @param {string} token */
export async function validateResetToken(token) {
  return _req('GET', '/auth/password-reset/validate', { params: { token }, timeoutMs: AUTH_TIMEOUT_MS });
}

/** @param {string} token @param {string} newPassword */
export async function confirmPasswordReset(token, newPassword) {
  return _req('POST', '/auth/password-reset/confirm', { body: { token, new_password: newPassword }, timeoutMs: AUTH_TIMEOUT_MS });
}

/** @param {string} token */
export async function verifyEmail(token) {
  return _req('POST', '/auth/verify-email', { body: { token }, timeoutMs: AUTH_TIMEOUT_MS });
}

export async function resendVerification() {
  return _req('POST', '/auth/resend-verification', { timeoutMs: AUTH_TIMEOUT_MS });
}

/** @param {string} to */
export async function sendTestEmail(to) {
  return _req('POST', '/admin/email/test', { body: { to } });
}

/** @param {number} userId */
export async function adminTriggerPasswordReset(userId) {
  return _req('POST', `/admin/users/${userId}/password-reset`);
}

// ── Boot / SSE ────────────────────────────────────────────────────────────────

/** @returns {Promise<{ boot_id: string }>} */
export async function getBootId() {
  return _req('GET', '/boot_id');
}

// ── Sources ───────────────────────────────────────────────────────────────────

export async function getSources() {
  return _req('GET', '/sources');
}

export async function getSourcesHealth() {
  return _req('GET', '/sources/health');
}

/** @param {string} name */
export async function createSource(name) {
  return _req('POST', '/sources', { body: { name } });
}

/** @param {number} id */
export async function getSource(id) {
  return _req('GET', `/sources/${id}`);
}

/** @param {number} id */
export async function deleteSource(id) {
  return _req('DELETE', `/sources/${id}`);
}

/** @param {number} id */
export async function getSourceMetadata(id) {
  return _req('GET', `/sources/${id}/metadata`);
}

/** @param {number} id */
export async function reloadSource(id) {
  return _req('POST', `/sources/${id}/reload`);
}

/**
 * Fetch and install a WASM extension from a URL.
 * @param {number} id @param {string} url
 */
export async function fetchWasm(id, url) {
  return _req('POST', `/sources/${id}/wasm/fetch`, { body: { url } });
}

/**
 * Upload a .wasm file to install an extension.
 * @param {number} id
 * @param {File} file
 */
export async function uploadWasm(id, file) {
  const body = new FormData();
  body.append('file', file);
  const res = await fetch(`/rest/sources/${id}/wasm`, {
    method: 'POST',
    credentials: 'include',
    body,
  });
  if (res.status === 401) {
    if (location.pathname !== '/login') window.location.href = '/login';
    throw Object.assign(new Error('Unauthorized'), { status: 401 });
  }
  if (!res.ok) {
    let body;
    try { body = await res.json(); } catch { body = { error: await res.text().catch(() => res.statusText) }; }
    throw Object.assign(new Error(body?.error || `HTTP ${res.status}`), {
      status: res.status,
      code: body?.code ?? null,
      hint: body?.hint ?? null,
      suggestions: body?.suggestions ?? null,
      body,
    });
  }
  return null;
}

/** @param {number} sid @param {number} page @param {number} size @param {string} [filters] @param {AbortSignal} [signal] */
export async function getPopularManga(sid, page, size, filters, signal) {
  const params = filters ? { filters } : undefined;
  return _req('GET', `/sources/${sid}/popular/${page}/${size}`, { params, signal });
}

/** @param {number} sid @param {string} query @param {number} page @param {number} size @param {string} [filters] @param {AbortSignal} [signal] */
export async function searchManga(sid, query, page, size, filters, signal) {
  const params = filters ? { query, filters } : { query };
  return _req('GET', `/sources/${sid}/search/${page}/${size}`, { params, signal });
}

/** @param {number} sid @param {AbortSignal} [signal] */
export async function getSourceFilters(sid, signal) {
  return _req('GET', `/sources/${sid}/filters`, { signal });
}

/** @param {number} sid @param {string} mangaId */
export async function getRemoteMangaDetails(sid, mangaId, signal) {
  return _req('GET', `/sources/${sid}/details/${encodeURIComponent(mangaId)}`, { signal });
}

/** @param {number} sid @param {string} mangaId */
export async function getSourceMangaUrl(sid, mangaId) {
  return _req('GET', `/sources/${sid}/url/${encodeURIComponent(mangaId)}`);
}

/** @param {number} sid @param {string} mangaId */
/** @param {number|string} sid @param {string} mangaId @param {boolean} [force] */
export async function saveToLibrary(sid, mangaId, force = false) {
  const qs = force ? '?force=true' : '';
  return _req('POST', `/sources/${sid}/save/${encodeURIComponent(mangaId)}${qs}`);
}

/** @param {number} sid @param {string} mangaId @param {number} page @param {number} size @param {AbortSignal|undefined} signal @param {string|null} [sort] */
export async function getRemoteChapters(sid, mangaId, page, size, signal, sort) {
  const qs = sort ? `?sort=${encodeURIComponent(sort)}` : '';
  return _req('GET', `/sources/${sid}/chapters/${encodeURIComponent(mangaId)}/${page}/${size}${qs}`, { signal });
}

/** @param {number} sid @param {string} mangaId */
export async function getRemoteChapterSorts(sid, mangaId) {
  return _req('GET', `/sources/${sid}/chapter-sorts/${encodeURIComponent(mangaId)}`);
}

/** @param {number} sid @param {string} mangaId @param {string} chapterId */
export async function getPages(sid, mangaId, chapterId) {
  return _req('GET', `/sources/${sid}/pages/${encodeURIComponent(mangaId)}/${encodeURIComponent(chapterId)}`);
}

/**
 * Returns the page manifest for a locally downloaded chapter.
 * @param {number} chapterId
 * @returns {Promise<{
 *   chapter_id: number,
 *   chapter_title: string,
 *   manga_id: number,
 *   manga_title: string,
 *   page_count: number,
 *   pages: Array<{
 *     index: number,
 *     filename: string,
 *     double_page: boolean,
 *   }>,
 *   prev_chapter_id: number | null,
 *   next_chapter_id: number | null,
 *   last_page_read: number | null,
 *   spread_analysed: boolean,
 * }>}
 */
export async function getChapterPages(chapterId) {
  return _req('GET', `/chapter/${chapterId}/pages`);
}

/**
 * Returns the URL for a single page image from a downloaded chapter.
 * Use directly as an <img src> value — auth cookies are sent automatically.
 * @param {number} chapterId
 * @param {number} pageNum
 * @returns {string}
 */
export function getChapterPageUrl(chapterId, pageNum) {
  return `/rest/chapter/${chapterId}/page/${pageNum}`;
}

/** @param {number} sid @param {string} mangaId @returns {Promise<{ db_id: number | null }>} */
export async function checkInLibrary(sid, mangaId) {
  return _req('GET', `/sources/${sid}/in_library/${encodeURIComponent(mangaId)}`);
}

/** @param {number} sid @param {boolean} enabled */
export async function toggleSourceEnabled(sid, enabled) {
  return _req('PATCH', `/sources/${sid}/toggle_enabled`, { body: { enabled } });
}

/** @param {number} sid @param {boolean} favourited */
export async function toggleSourceFavourite(sid, favourited) {
  return _req('PATCH', `/sources/${sid}/toggle_favourite`, { body: { favourited } });
}

/** @returns {Promise<number[]>} */
export async function getActiveSourceIds() {
  return _req('GET', '/sources/active_ids');
}

// ── Source Preferences ────────────────────────────────────────────────────────

/** @param {number} sid */
export async function getPreferenceSchema(sid) {
  return _req('GET', `/sources/${sid}/preference_schema`);
}

/** @param {number} sid */
export async function getPreferences(sid) {
  return _req('GET', `/sources/${sid}/preferences`);
}

/** @param {number} sid @param {string} key @param {string} value */
export async function setPreference(sid, key, value) {
  return _req('PUT', `/sources/${sid}/preferences/${encodeURIComponent(key)}`, { body: { value } });
}

/** @param {number} sid @param {string} key @param {string} item */
export async function appendPreferenceItem(sid, key, item) {
  return _req('POST', `/sources/${sid}/preferences/${encodeURIComponent(key)}/append`, { body: { item } });
}

/** @param {number} sid @param {string} key @param {string} item */
export async function removePreferenceItem(sid, key, item) {
  return _req('POST', `/sources/${sid}/preferences/${encodeURIComponent(key)}/remove_item`, { body: { item } });
}

/** @param {number} sid @param {string} key @param {string} item @param {boolean} selected */
export async function togglePreferenceSelect(sid, key, item, selected) {
  return _req('POST', `/sources/${sid}/preferences/${encodeURIComponent(key)}/toggle_select`, { body: { item, selected } });
}

// ── Library ───────────────────────────────────────────────────────────────────

/**
 * @param {{ page: number, page_size: number, search?: string, status_filter?: number|null,
 *           tag_filter?: number|null, author_filter?: number|null, artist_filter?: number|null,
 *           category_filter?: number|null, sort_by?: string }} params
 * @param {AbortSignal} [signal]
 */
export async function getLibrary(params, signal) {
  return _req('GET', '/library', { params, signal });
}

/** @param {number} page @param {AbortSignal} [signal] */
export async function getRecentUpdates(page, signal) {
  return _req('GET', '/recent_updates', { params: { page }, signal });
}

/**
 * @param {string} query
 * @param {"FavouritedOnly"|"AllEnabled"|{Sources: number[]}} scope
 * @param {number} page
 * @param {number} pageSize
 * @param {AbortSignal} [signal]
 */
export async function globalSearch(query, scope, page, pageSize, signal) {
  // Unit variants pass as plain strings; Sources variant as JSON-encoded object.
  const scopeParam = typeof scope === 'string' ? scope : JSON.stringify(scope);
  return _req('GET', '/global_search', {
    params: { query, scope: scopeParam, page, page_size: pageSize },
    signal,
  });
}

// ── Manga ─────────────────────────────────────────────────────────────────────

/** @param {number} id */
export async function getManga(id) {
  return _req('GET', `/manga/${id}`);
}

/** @param {number} id */
export async function deleteManga(id) {
  return _req('DELETE', `/manga/${id}`);
}

/**
 * Returns a URL string suitable for use as an img src — not a fetch.
 * @param {number} id
 * @returns {string}
 */
export function getMangaCoverUrl(id) {
  return `/rest/manga/${id}/cover`;
}

/** @param {number} id @param {AbortSignal} [signal] */
export async function getMangaDetails(id, signal) {
  return _req('GET', `/manga/${id}/details`, { signal });
}

/** @param {number} id @param {number} page @param {number} pageSize @param {string} sortOrder @param {AbortSignal} [signal] */
/**
 * @param {number} id
 * @param {number} page
 * @param {number} pageSize
 * @param {string} sortOrder
 * @param {AbortSignal | undefined} signal
 * @param {{ filterDownloaded?: boolean|null, filterUnread?: boolean|null, filterScanlator?: string|null }} [filters]
 */
export async function getLocalChapters(id, page, pageSize, sortOrder, signal, filters = {}) {
  const { filterDownloaded, filterUnread, filterScanlator } = filters;
  return _req('GET', `/manga/${id}/chapters`, {
    params: {
      page,
      page_size: pageSize,
      sort_order: sortOrder,
      ...(filterDownloaded != null && { filter_downloaded: filterDownloaded }),
      ...(filterUnread != null && { filter_unread: filterUnread }),
      ...(filterScanlator != null && { filter_scanlator: filterScanlator }),
    },
    signal,
  });
}

/**
 * Returns all chapter IDs matching the given filters (no pagination).
 * @param {number} id
 * @param {{ filterDownloaded?: boolean|null, filterUnread?: boolean|null, filterScanlator?: string|null, preferredOnly?: boolean, sortOrder?: string }} [opts]
 * @returns {Promise<{ ids: number[] }>}
 */
export async function getChapterIds(id, opts = {}) {
  const { filterDownloaded, filterUnread, filterScanlator, preferredOnly, sortOrder } = opts;
  return _req('GET', `/manga/${id}/chapter_ids`, {
    params: {
      ...(filterDownloaded != null && { filter_downloaded: filterDownloaded }),
      ...(filterUnread != null && { filter_unread: filterUnread }),
      ...(filterScanlator != null && { filter_scanlator: filterScanlator }),
      ...(preferredOnly && { preferred_only: true }),
      ...(sortOrder && { sort_order: sortOrder }),
    },
  });
}

/** @param {number} id */
export async function downloadAll(id) {
  return _req('POST', `/manga/${id}/download_all`);
}

/** @param {number} id */
export async function cancelAllDownloads(id) {
  return _req('POST', `/manga/${id}/cancel_all`);
}

/** @param {number} id */
export async function refreshManga(id, opts) {
  return _req('POST', `/manga/${id}/refresh`, opts ? { body: opts } : undefined);
}

/** @param {number} id @returns {Promise<{ new_chapters: number }>} */
export async function scanManga(id) {
  return _req('POST', `/manga/${id}/scan`);
}

/** @returns {Promise<{ queued: number }>} */
export async function scanAllLibrary() {
  return _req('POST', '/library/scan-all');
}

/**
 * Unified scan: scan all library manga or a specific list of IDs.
 * Emits SSE Started/MangaRefreshed/Completed events identical to scan-all.
 * @param {number[] | 'all'} idsOrAll
 */
export async function scanMangaMultiple(idsOrAll) {
  return _req('POST', '/manga/scan', { body: { ids: idsOrAll } });
}

/** @param {number} id @param {boolean} enabled */
export async function toggleAutoDownload(id, enabled) {
  return _req('POST', `/manga/${id}/toggle_auto_download`, { body: { enabled } });
}

/** @param {number} id @param {boolean} enabled */
export async function toggleAutoScan(id, enabled) {
  return _req('POST', `/manga/${id}/toggle_auto_scan`, { body: { enabled } });
}

/** @param {number} id @param {string} notes */
export async function updateMangaNotes(id, notes) {
  return _req('PATCH', `/manga/${id}/notes`, { body: { notes } });
}

/**
 * @param {number} id
 * @param {{ local_name?: string|null, local_description?: string|null,
 *           local_status?: number|null, authors?: string[]|null,
 *           artists?: string[]|null, tags?: string[]|null }} data
 */
export async function updateLocalMetadata(id, data) {
  return _req('PATCH', `/manga/${id}/local_metadata`, { body: data });
}

/** @param {number} id @param {File} file */
export async function uploadMangaCover(id, file) {
  const body = new FormData();
  body.append('file', file);
  const res = await fetch(`/rest/manga/${id}/cover`, { method: 'POST', credentials: 'include', body });
  if (res.status === 401) {
    if (location.pathname !== '/login') window.location.href = '/login';
    throw Object.assign(new Error('Unauthorized'), { status: 401 });
  }
  if (!res.ok) {
    let b;
    try { b = await res.json(); } catch { b = { error: res.statusText }; }
    throw Object.assign(new Error(b?.error || `HTTP ${res.status}`), { status: res.status });
  }
  return res.status === 204 ? null : res.json().catch(() => null);
}

/** @param {number} id */
export async function clearMangaCoverOverride(id) {
  return _req('DELETE', `/manga/${id}/cover`);
}

/** @returns {Promise<Array<{id: number, name: string}>>} */
export async function getFilterAuthors() { return _req('GET', '/filters/authors'); }

/** @returns {Promise<Array<{id: number, name: string}>>} */
export async function getFilterArtists() { return _req('GET', '/filters/artists'); }

/** @returns {Promise<Array<{id: number, name: string}>>} */
export async function getFilterTags() { return _req('GET', '/filters/tags'); }

/** @param {number} id */
export async function markMangaSeen(id) {
  return _req('PATCH', `/manga/${id}/seen`);
}

/** @param {number} id @param {boolean} enabled */
export async function toggleDownloadAllPreferred(id, enabled) {
  return _req('POST', `/manga/${id}/toggle_download_all_preferred`, { body: { enabled } });
}

/** @param {number} id @param {number} targetSourceId @param {string} targetMangaId */
export async function previewMigration(id, targetSourceId, targetMangaId) {
  return _req('POST', `/manga/${id}/preview_migration`, {
    body: { target_source_id: targetSourceId, target_source_manga_id: targetMangaId },
  });
}

/** @param {number} id @param {number} targetSourceId @param {string} targetMangaId @param {boolean} keepOrphaned */
export async function migrateManga(id, targetSourceId, targetMangaId, keepOrphaned) {
  return _req('POST', `/manga/${id}/migrate`, {
    body: {
      target_source_id: targetSourceId,
      target_source_manga_id: targetMangaId,
      keep_orphaned_downloads: keepOrphaned,
    },
  });
}

// ── Download Rules ────────────────────────────────────────────────────────────

/** @param {number} mangaId */
export async function getDownloadRules(mangaId) {
  return _req('GET', `/manga/${mangaId}/download_rules`);
}

/**
 * @param {number} mangaId
 * @param {{ LanguageInclude: string }|{ LanguageExclude: string }|
 *          { TitleContains: string }|{ TitleExcludes: string }|
 *          { ChapterNumberMin: number }|{ ChapterNumberMax: number }|
 *          'ExcludeFractional'|{ MaxAgeDays: number }|{ PublishedAfter: number }} kind
 */
export async function addDownloadRule(mangaId, kind) {
  return _req('POST', `/manga/${mangaId}/download_rules`, { body: { kind } });
}

/** @param {number} ruleId */
export async function deleteDownloadRule(ruleId) {
  return _req('DELETE', `/download_rules/${ruleId}`);
}

/** @param {number} ruleId @param {any} kind */
export async function updateDownloadRule(ruleId, kind) {
  return _req('PATCH', `/download_rules/${ruleId}`, { body: { kind } });
}

/**
 * @param {number} mangaId
 * @param {number[]} orderedIds
 */
export async function reorderDownloadRules(mangaId, orderedIds) {
  return _req('PUT', `/manga/${mangaId}/download_rules/order`, { body: { ordered_ids: orderedIds } });
}

/**
 * @param {number} mangaId
 * @param {any[]} kinds
 * @returns {Promise<{matching: number, total: number}>}
 */
export async function previewDownloadRules(mangaId, kinds) {
  return _req('POST', `/manga/${mangaId}/download_rules/preview`, { body: { kinds } });
}

// ── Scanlator Preferences ─────────────────────────────────────────────────────

/** @param {number} mangaId */
export async function getScanlatorPrefs(mangaId) {
  return _req('GET', `/manga/${mangaId}/scanlator_preferences`);
}

/** @param {number} mangaId @param {string} scanlator @param {number} priority @param {boolean} [blocked] */
export async function setScanlatorPref(mangaId, scanlator, priority, blocked = false) {
  return _req('POST', `/manga/${mangaId}/scanlator_preferences`, { body: { scanlator, priority, blocked } });
}

/** @param {number} id */
export async function deleteScanlatorPref(id) {
  return _req('DELETE', `/scanlator_preferences/${id}`);
}

/** @param {number} mangaId @param {'priority'|'whitelist'} mode */
export async function setScanlatorMode(mangaId, mode) {
  return _req('PATCH', `/manga/${mangaId}/scanlator_mode`, { body: { mode } });
}

/** @param {number} mangaId @returns {Promise<string[]>} */
export async function getChapterScanlators(mangaId) {
  return _req('GET', `/manga/${mangaId}/scanlators`);
}

/** @param {number} mangaId @returns {Promise<string[]>} */
export async function getChapterLanguages(mangaId) {
  return _req('GET', `/manga/${mangaId}/languages`);
}

// ── Chapters ──────────────────────────────────────────────────────────────────

/** @param {number} id */
export async function downloadChapter(id) {
  return _req('POST', `/chapter/${id}/download`);
}

/** @param {number} id */
export async function deleteChapter(id) {
  return _req('DELETE', `/chapter/${id}/delete`);
}

/** @param {number} id */
export async function cancelDownload(id) {
  return _req('POST', `/chapter/${id}/cancel`);
}

// ── Progress Tracking ────────────────────────────────────────────────────

/** @param {number} chapterId @param {number} page */
export async function setChapterProgress(chapterId, page) {
  return _req('PUT', `/chapter/${chapterId}/progress`, { body: { page } });
}

/** @param {number[]} chapterIds @param {boolean} isRead */
export async function setChapterReadStatus(chapterIds, isRead) {
  return _req('PUT', '/chapters/read_status', { body: { chapter_ids: chapterIds, is_read: isRead } });
}

/** @param {number} mangaId */
export async function getMangaTracking(mangaId) {
  return _req('GET', `/manga/${mangaId}/tracking`);
}

/**
 * @param {number} mangaId
 * @param {{ status?: string, score?: number }} data
 */
export async function setMangaTracking(mangaId, data) {
  return _req('PUT', `/manga/${mangaId}/tracking`, { body: data });
}

// ── Filters ───────────────────────────────────────────────────────────────────

export async function getTags() {
  return _req('GET', '/filters/tags');
}

export async function getAuthors() {
  return _req('GET', '/filters/authors');
}

export async function getArtists() {
  return _req('GET', '/filters/artists');
}

// ── Categories ────────────────────────────────────────────────────────────────

export async function getCategories() {
  return _req('GET', '/categories');
}

/** @param {string} name @param {number} sortOrder */
export async function createCategory(name, sortOrder) {
  return _req('POST', '/categories', { body: { name, sort_order: sortOrder } });
}

/** @param {number[]} orderedIds */
export async function reorderCategories(orderedIds) {
  return _req('PUT', '/categories/reorder', { body: { ordered_ids: orderedIds } });
}

/** @param {number} id @param {string} name */
export async function renameCategory(id, name) {
  return _req('PATCH', `/categories/${id}`, { body: { name } });
}

/** @param {number} id */
export async function deleteCategory(id) {
  return _req('DELETE', `/categories/${id}`);
}

/** @param {number} mangaId */
export async function getMangaCategories(mangaId) {
  return _req('GET', `/manga/${mangaId}/categories`);
}

/** @param {number} mangaId @param {number[]} categoryIds */
export async function setMangaCategories(mangaId, categoryIds) {
  return _req('PUT', `/manga/${mangaId}/categories`, { body: { category_ids: categoryIds } });
}

// ── Settings ──────────────────────────────────────────────────────────────────

export async function getSettings() {
  return _req('GET', '/settings');
}

/**
 * @param {{ Download: object } | { Scan: object } | { Advanced: object }} payload
 */
export async function updateSettings(payload) {
  return _req('PATCH', '/settings', { body: payload });
}

export async function getRefreshStatus() {
  return _req('GET', '/refresh/status');
}

export async function startRefreshAll() {
  return _req('POST', '/refresh/start');
}

export async function serverStop() {
  return _req('POST', '/server/stop');
}

export async function serverRestart() {
  return _req('POST', '/server/restart');
}

export async function runMaintenance() {
  return _req('POST', '/admin/maintenance');
}

export async function clearCache() {
  return _req('POST', '/admin/cache/clear');
}

export async function getCredentialEncryptionStatus() {
  return _req('GET', '/admin/credentials/status');
}

export async function migrateCredentialsToEncrypted() {
  return _req('POST', '/admin/credentials/encrypt');
}

export async function stopScan() {
  return _req('POST', '/admin/scan/stop');
}

export async function cancelAllGlobalDownloads() {
  return _req('DELETE', '/downloads/active');
}

// ── External Trackers ────────────────────────────────────────────────────

export async function getTrackers() {
  return _req('GET', '/trackers');
}

/** @param {number} trackerId @param {string} redirectUri */
export async function getTrackerAuthUrl(trackerId, redirectUri) {
  return _req('GET', `/trackers/${trackerId}/auth_url`, { params: { redirect_uri: redirectUri } });
}

/** @param {number} trackerId */
export async function getTrackerConfig(trackerId) {
  return _req('GET', `/trackers/${trackerId}/config`);
}

/**
 * @param {number} trackerId
 * @param {{ client_id: string, client_secret?: string }} config
 */
export async function setTrackerConfig(trackerId, config) {
  return _req('PUT', `/trackers/${trackerId}/config`, { body: config });
}

/** @param {number} trackerId */
export async function deleteTrackerConfig(trackerId) {
  return _req('DELETE', `/trackers/${trackerId}/config`);
}

/** @param {number} trackerId */
export async function unlinkTracker(trackerId) {
  return _req('POST', `/trackers/${trackerId}/unlink`);
}

/** @param {number} trackerId @param {string} query */
export async function searchTrackerManga(trackerId, query) {
  return _req('GET', `/trackers/${trackerId}/search`, { params: { query } });
}

/** @param {number} mangaId */
export async function getTrackerMappings(mangaId) {
  return _req('GET', `/manga/${mangaId}/tracker_mappings`);
}

/** @param {number} mangaId @param {number} trackerId @param {string} trackerMangaId */
export async function setTrackerMapping(mangaId, trackerId, trackerMangaId) {
  return _req('PUT', `/manga/${mangaId}/tracker_mappings`, {
    body: { tracker_id: trackerId, tracker_manga_id: trackerMangaId },
  });
}

/** @param {number} mangaId @param {number} trackerId */
export async function deleteTrackerMapping(mangaId, trackerId) {
  return _req('DELETE', `/manga/${mangaId}/tracker_mappings/${trackerId}`);
}

export async function syncAllTrackers() {
  return _req('POST', '/trackers/sync');
}

/** @param {number} mangaId */
export async function syncMangaTrackers(mangaId) {
  return _req('POST', `/manga/${mangaId}/sync`);
}

// ── Continue reading ──────────────────────────────────────────────────────────

/** @param {number} mangaId */
export async function getContinueReading(mangaId) {
  return _req('GET', `/manga/${mangaId}/continue_reading`);
}

/** @param {number} [limit] */
export async function getContinueReadingShelf(limit = 12) {
  return _req('GET', '/library/continue_reading', { params: { limit } });
}

/**
 * @param {number} mangaId
 * @param {number} chapterNumber
 * @param {boolean} isRead
 */
export async function markChaptersUpTo(mangaId, chapterNumber, isRead) {
  return _req('POST', `/manga/${mangaId}/chapters/mark_up_to`, {
    body: { chapter_number: chapterNumber, is_read: isRead },
  });
}

// ── Admin — user management ───────────────────────────────────────────────────

export async function adminListUsers() {
  return _req('GET', '/admin/users');
}

/**
 * @param {{ username: string, email: string, password: string, roles?: string[] }} body
 */
export async function adminCreateUser(body) {
  return _req('POST', '/admin/users', { body });
}

/**
 * @param {number} userId
 * @param {{ username?: string, email?: string, is_active?: boolean, password?: string }} body
 */
export async function adminUpdateUser(userId, body) {
  return _req('PATCH', `/admin/users/${userId}`, { body });
}

/** @param {number} userId */
export async function adminDeleteUser(userId) {
  return _req('DELETE', `/admin/users/${userId}`);
}

/**
 * @param {number} userId
 * @param {string} roleSlug
 */
export async function adminGrantRole(userId, roleSlug) {
  return _req('POST', `/admin/users/${userId}/roles`, { body: { role_slug: roleSlug } });
}

/**
 * @param {number} userId
 * @param {string} roleSlug
 */
export async function adminRevokeRole(userId, roleSlug) {
  return _req('DELETE', `/admin/users/${userId}/roles/${roleSlug}`);
}

export async function adminListRoles() {
  return _req('GET', '/admin/roles');
}

/**
 * @param {{ slug: string, parent?: string, description?: string, permissions?: string[] }} body
 */
export async function adminCreateRole(body) {
  return _req('POST', '/admin/roles', { body });
}

/**
 * @param {string} slug
 * @param {{ description?: string, permissions?: string[] }} body
 */
export async function adminUpdateRole(slug, body) {
  return _req('PATCH', `/admin/roles/${slug}`, { body });
}

/** @param {string} slug */
export async function adminDeleteRole(slug) {
  return _req('DELETE', `/admin/roles/${slug}`);
}

/**
 * @param {number} userId
 * @param {{ before?: string, limit?: number }} [opts]
 */
export async function getUserActivity(userId, opts = {}) {
  const params = {};
  if (opts.before) params.before = opts.before;
  if (opts.limit)  params.limit  = opts.limit;
  return _req('GET', `/admin/users/${userId}/activity`, Object.keys(params).length ? { params } : {});
}

/** @param {number} [limit] */
export async function getDownloadHistory(limit) {
  return _req('GET', '/downloads/history', limit ? { params: { limit } } : {});
}

// ── Admin logs ────────────────────────────────────────────────────────────────

/**
 * @param {{ level?: string, source?: string, from?: string, to?: string,
 *           search?: string, page?: number, page_size?: number }} [params]
 */
export async function getAdminLogs(params = {}) {
  return _req('GET', '/admin/logs', { params });
}

/**
 * @param {{ user_id?: number, action?: string, from?: string, to?: string,
 *           search?: string, page?: number, page_size?: number }} [params]
 */
export async function getAdminAuditLog(params = {}) {
  return _req('GET', '/admin/audit-log', { params });
}

// ── Reading stats ─────────────────────────────────────────────────────────────

/** @param {number} [period] Number of days for the activity window (default 90). */
export async function getReadingStats(period) {
  return _req('GET', '/stats', period ? { params: { period } } : {});
}

// ── Backup / Restore ──────────────────────────────────────────────────────────

/** Fetch as a File download (navigates browser). */
export function downloadBackup(includeChapterProgress = false) {
  const qs = includeChapterProgress ? '?include_chapter_progress=true' : '';
  window.location.href = `/rest/library/backup${qs}`;
}

/** @param {File} file */
export async function previewBackup(file) {
  const body = new FormData();
  body.append('file', file);
  const res = await fetch('/rest/library/backup/preview', { method: 'POST', credentials: 'include', body });
  if (!res.ok) { let b; try { b = await res.json(); } catch { b = {}; } throw Object.assign(new Error(b?.error || `HTTP ${res.status}`), { status: res.status }); }
  return res.json();
}

/**
 * @param {File} file
 * @param {{ merge?: boolean, import_manga?: boolean, import_categories?: boolean,
 *            import_download_rules?: boolean, import_tracking?: boolean,
 *            import_chapter_progress?: boolean, import_settings?: boolean }} [opts]
 */
export async function restoreBackup(file, opts = {}) {
  const body = new FormData();
  body.append('file', file);
  for (const [k, v] of Object.entries(opts)) body.append(k, String(v));
  const res = await fetch('/rest/library/restore', { method: 'POST', credentials: 'include', body });
  if (!res.ok) { let b; try { b = await res.json(); } catch { b = {}; } throw Object.assign(new Error(b?.error || `HTTP ${res.status}`), { status: res.status }); }
  return res.json();
}

/** @param {File} file */
export async function previewTachiyomiImport(file) {
  const body = new FormData();
  body.append('file', file);
  const res = await fetch('/rest/library/import/tachiyomi/preview', { method: 'POST', credentials: 'include', body });
  if (!res.ok) { let b; try { b = await res.json(); } catch { b = {}; } throw Object.assign(new Error(b?.error || `HTTP ${res.status}`), { status: res.status }); }
  return res.json();
}

/**
 * @param {File} file
 * @param {{ import_manga?: boolean, import_categories?: boolean,
 *            import_tracking?: boolean, import_chapter_progress?: boolean }} [opts]
 */
export async function importTachiyomiBackup(file, opts = {}) {
  const body = new FormData();
  body.append('file', file);
  for (const [k, v] of Object.entries(opts)) body.append(k, String(v));
  const res = await fetch('/rest/library/import/tachiyomi', { method: 'POST', credentials: 'include', body });
  if (!res.ok) { let b; try { b = await res.json(); } catch { b = {}; } throw Object.assign(new Error(b?.error || `HTTP ${res.status}`), { status: res.status }); }
  return res.json();
}

// ── Pending imports ───────────────────────────────────────────────────────────

export async function getPendingImports() {
  return _req('GET', '/library/pending-imports');
}

/** @param {number} id */
export async function deletePendingImport(id) {
  return _req('DELETE', `/library/pending-imports/${id}`);
}

/** @param {number} id @param {number} sourceId @param {string} sourceMangaId */
export async function resolvePendingImport(id, sourceId, sourceMangaId) {
  return _req('POST', `/library/pending-imports/${id}/resolve`, { body: { source_id: sourceId, source_manga_id: sourceMangaId } });
}

// ── Orphaned manga ────────────────────────────────────────────────────────────

export async function getOrphanedManga() {
  return _req('GET', '/library/orphaned');
}

// ── Duplicates ────────────────────────────────────────────────────────────────

export async function getDuplicates() {
  return _req('GET', '/library/duplicates');
}

/** Trigger a full-library rescan and persist any new pairs found. */
export async function rescanDuplicates() {
  return _req('POST', '/library/duplicates/scan');
}

/** @param {number} aId @param {number} bId */
export async function dismissDuplicate(aId, bId) {
  return _req('POST', `/library/duplicates/${aId}/${bId}/dismiss`);
}

/** @param {number} keepId @param {number} discardId */
export async function mergeDuplicate(keepId, discardId) {
  return _req('POST', '/library/duplicates/merge', { body: { keep_id: keepId, discard_id: discardId } });
}

// ── Filesystem browser ────────────────────────────────────────────────────────

/** @param {string} path */
export async function fsBrowse(path) {
  return _req('GET', '/admin/fs/browse', { params: { path } });
}

/** @param {string} path @param {string} name */
export async function fsMkdir(path, name) {
  return _req('POST', '/admin/fs/mkdir', { body: { path, name } });
}

// ── Path migration ────────────────────────────────────────────────────────────

/** @param {'library_path'|'wasm_storage_path'} field @param {string} newPath */
export async function estimatePathMigration(field, newPath) {
  return _req('POST', '/admin/path/estimate', { body: { field, new_path: newPath } });
}

/** @param {'library_path'|'wasm_storage_path'} field @param {string} newPath */
export async function startPathMigration(field, newPath) {
  return _req('POST', '/admin/path/migrate', { body: { field, new_path: newPath } });
}

// ── Webhooks ──────────────────────────────────────────────────────────────────

export async function listWebhooks() {
  return _req('GET', '/webhooks');
}

/** @param {{ url: string, secret?: string, events?: string }} body */
export async function createWebhook(body) {
  return _req('POST', '/webhooks', { body });
}

/** @param {number} id @param {{ url?: string, secret?: string, events?: string, enabled?: boolean }} body */
export async function updateWebhook(id, body) {
  return _req('PATCH', `/webhooks/${id}`, { body });
}

/** @param {number} id */
export async function deleteWebhook(id) {
  return _req('DELETE', `/webhooks/${id}`);
}

/** @param {number} id */
export async function testWebhook(id) {
  return _req('POST', `/webhooks/${id}/test`);
}

/** @param {number} id */
export async function listWebhookDeliveries(id) {
  return _req('GET', `/webhooks/${id}/deliveries`);
}

/** @param {number} mangaId */
export async function getMangaWebhookNotify(mangaId) {
  return _req('GET', `/manga/${mangaId}/webhook-notify`);
}

/** @param {number} mangaId @param {boolean} enabled */
export async function setMangaWebhookNotify(mangaId, enabled) {
  return _req('PUT', `/manga/${mangaId}/webhook-notify`, { body: { enabled } });
}
