// @ts-check
// History API (pushState) SPA router.
// Pages are lazy-imported modules that export init(container, params) and destroy(container).

/**
 * @typedef {{ init: (container: HTMLElement, params: Record<string, string>) => void,
 *             destroy?: (container: HTMLElement) => void }} PageModule
 */

/** @type {Array<{ re: RegExp, keys: string[], load: () => Promise<PageModule> }>} */
const _routes = [
  { path: '/login',                     load: () => import('./pages/login.js') },
  { path: '/sources',                   load: () => import('./pages/sources.js') },
  { path: '/source/:id/manga/:manga_id',load: () => import('./pages/manga-details.js') },
  { path: '/source/:id',                load: () => import('./pages/source-details.js') },
  { path: '/manga/:db_id',              load: () => import('./pages/manga-details.js') },
  { path: '/reader/:id',                load: () => import('./pages/reader.js') },
  { path: '/search',                    load: () => import('./pages/global-search.js') },
  { path: '/downloads',                 load: () => import('./pages/downloads.js') },
  { path: '/settings',                  load: () => import('./pages/settings.js') },
  { path: '/accounts',                  load: () => import('./pages/accounts.js') },
  { path: '/updates',                   load: () => import('./pages/recent-updates.js') },
  { path: '/',                          load: () => import('./pages/library.js') },
].map(({ path, load }) => ({ re: _pathToRegex(path), keys: _extractKeys(path), load }));

/** @type {HTMLElement | null} */
let _container = null;
/** @type {PageModule | null} */
let _activePage = null;
/** @type {Set<Function>} */
const _navCallbacks = new Set();
/** @type {Record<string, string>} */
let _currentParams = {};

/**
 * Initialises the router, performs the initial route match, and sets up
 * popstate + link-interception listeners.
 * @param {HTMLElement} container
 */
export function initRouter(container) {
  _container = container;
  window.addEventListener('popstate', () => _route(location.pathname));
  document.addEventListener('click', _interceptLink);
  _route(location.pathname);
}

/**
 * Navigates to `path`, updating the browser URL.
 * @param {string} path
 * @param {{ replace?: boolean }} [opts]
 */
export function navigate(path, opts = {}) {
  if (opts.replace) {
    history.replaceState(null, '', path);
  } else {
    history.pushState(null, '', path);
  }
  _route(path);
}

/**
 * Returns the params extracted from the current matched route.
 * @returns {Record<string, string>}
 */
export function getCurrentParams() {
  return { ..._currentParams };
}

/**
 * Registers a callback that fires on every navigation (used for nav highlight).
 * Returns an unregister function.
 * @param {(path: string) => void} callback
 * @returns {() => void}
 */
export function onNavigate(callback) {
  _navCallbacks.add(callback);
  return () => _navCallbacks.delete(callback);
}

// ── Internal ─────────────────────────────────────────────────────────────────

/** @param {string} path */
async function _route(path) {
  if (!_container) return;

  // Strip query string before matching — pages read location.search directly
  const pathname = path.split('?')[0];

  // Find matching route
  let matched = null;
  let params = {};
  for (const route of _routes) {
    const m = pathname.match(route.re);
    if (m) {
      for (let i = 0; i < route.keys.length; i++) {
        params[route.keys[i]] = decodeURIComponent(m[i + 1] ?? '');
      }
      matched = route;
      break;
    }
  }

  // Tear down current page — keep DOM visible while the new module loads to
  // avoid a blank-content flash during the async import.
  if (_activePage?.destroy) {
    try { _activePage.destroy(_container); } catch (e) { console.error('destroy error:', e); }
  }
  _activePage = null;
  _currentParams = params;

  if (!matched) {
    _container.innerHTML = '<div style="padding:2rem;text-align:center;color:var(--color-text-muted)">Page not found.</div>';
    document.title = 'Not Found - Kani';
  } else {
    try {
      const mod = await matched.load();
      // Clear old content only once the new module is ready to render
      _container.innerHTML = '';
      _activePage = mod;
      await mod.init(_container, params);
    } catch (e) {
      console.error('Page init error:', e);
      _container.innerHTML = '<div style="padding:2rem;color:var(--color-danger)">Failed to load page.</div>';
    }
  }

  window.scrollTo(0, 0);

  // Notify nav and other listeners
  for (const cb of _navCallbacks) {
    try { cb(pathname); } catch {}
  }
}

/** @param {MouseEvent} e */
function _interceptLink(e) {
  // Ignore modified clicks, non-left-button, and already-handled events
  if (e.metaKey || e.ctrlKey || e.shiftKey || e.altKey || e.button !== 0) return;
  if (e.defaultPrevented) return;

  const anchor = /** @type {HTMLElement} */ (e.target)?.closest('a');
  if (!anchor) return;

  const href = anchor.getAttribute('href');
  if (!href) return;

  // Only intercept relative paths that aren't API or external URLs
  if (href.startsWith('/rest/') || href.startsWith('http') || href.startsWith('//')) return;
  if (!href.startsWith('/')) return;

  e.preventDefault();
  navigate(href);
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/** Converts '/source/:id/manga/:manga_id' to a RegExp. */
function _pathToRegex(path) {
  const escaped = path
    .replace(/[.*+?^${}()|[\]\\]/g, '\\$&')  // escape special chars
    .replace(/:([a-zA-Z_][a-zA-Z0-9_]*)/g, '([^/]+)'); // :param → capture group
  return new RegExp(`^${escaped}/?$`);
}

/** Extracts param names from '/source/:id/manga/:manga_id' → ['id', 'manga_id']. */
function _extractKeys(path) {
  return [...path.matchAll(/:([a-zA-Z_][a-zA-Z0-9_]*)/g)].map(m => m[1]);
}
