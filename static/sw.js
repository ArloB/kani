// @ts-check
// Kani service worker — shell caching + page-image caching.

const SHELL_CACHE  = 'kani-shell-v1';
const PAGE_CACHE   = 'kani-pages-v1';
const KNOWN_CACHES = [SHELL_CACHE, PAGE_CACHE];

const SHELL_URLS = [
  '/',
  '/css/main.css',
  '/js/app.js',
  '/js/dist/app.js',
  '/js/vendor/preact.module.js',
  '/js/vendor/preact-hooks.module.js',
  '/js/vendor/htm.module.js',
  '/manifest.webmanifest',
];

// ── Install ───────────────────────────────────────────────────────────────────

self.addEventListener('install', e => {
  e.waitUntil(
    caches.open(SHELL_CACHE).then(cache =>
      cache.addAll(SHELL_URLS).catch(() => {})
    )
  );
  self.skipWaiting();
});

// ── Activate ──────────────────────────────────────────────────────────────────

self.addEventListener('activate', e => {
  e.waitUntil(
    caches.keys().then(keys =>
      Promise.all(
        keys
          .filter(k => !KNOWN_CACHES.includes(k))
          .map(k => caches.delete(k))
      )
    )
  );
  self.clients.claim();
});

// ── Fetch ─────────────────────────────────────────────────────────────────────

self.addEventListener('fetch', e => {
  const { request } = e;
  const url = new URL(request.url);

  // Only handle same-origin GET requests.
  if (request.method !== 'GET' || url.origin !== self.location.origin) return;

  const path = url.pathname;

  // Page images — cache-first, persistent.
  if (path.match(/^\/rest\/chapter\/\d+\/page\/\d+/)) {
    e.respondWith(_cacheFirst(PAGE_CACHE, request));
    return;
  }

  // Shell assets — cache-first.
  if (SHELL_URLS.includes(path) || path.startsWith('/js/') || path.startsWith('/css/') || path.startsWith('/icons/')) {
    e.respondWith(_cacheFirst(SHELL_CACHE, request));
    return;
  }

  // Everything else (API, auth, etc.) — network-first, no caching.
});

// ── Message ───────────────────────────────────────────────────────────────────

self.addEventListener('message', e => {
  if (!e.data) return;

  if (e.data.type === 'SKIP_WAITING') {
    self.skipWaiting();
    return;
  }

  if (e.data.type === 'CACHE_CHAPTER') {
    const { chapterId, pageCount, maxBytes } = e.data;
    e.waitUntil(_cacheChapter(chapterId, pageCount, maxBytes));
    return;
  }

  if (e.data.type === 'EVICT_CHAPTER') {
    const { chapterId } = e.data;
    e.waitUntil(_evictChapter(chapterId));
    return;
  }
});

// ── Helpers ───────────────────────────────────────────────────────────────────

async function _cacheFirst(cacheName, request) {
  const cache = await caches.open(cacheName);
  const cached = await cache.match(request);
  if (cached) return cached;
  try {
    const response = await fetch(request);
    if (response.ok) cache.put(request, response.clone());
    return response;
  } catch {
    return new Response('Offline', { status: 503 });
  }
}

async function _cacheChapter(chapterId, pageCount, maxBytes) {
  const cache = await caches.open(PAGE_CACHE);

  if (maxBytes) {
    const est = await navigator.storage?.estimate?.();
    if (est?.usage != null && est.usage >= maxBytes) return;
  }

  for (let i = 0; i < pageCount; i++) {
    const url = `/rest/chapter/${chapterId}/page/${i}`;
    if (await cache.match(url)) continue;
    try {
      const resp = await fetch(url);
      if (resp.ok) await cache.put(url, resp);
    } catch { /* ignore individual page failures */ }
  }

  const clients = await self.clients.matchAll();
  for (const client of clients) {
    client.postMessage({ type: 'CHAPTER_CACHED', chapterId });
  }
}

async function _evictChapter(chapterId) {
  const cache = await caches.open(PAGE_CACHE);
  const keys = await cache.keys();
  await Promise.all(
    keys
      .filter(r => r.url.includes(`/chapter/${chapterId}/page/`))
      .map(r => cache.delete(r))
  );
}
