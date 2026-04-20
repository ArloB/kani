// @ts-check
// App entry point. Bootstraps permissions, SSE, nav, and the SPA router.

import { initPermissions, getState, setState, subscribe, hasPermission } from './state.js';
import { connectSSE } from './sse.js';
import { initRouter, navigate, onNavigate } from './router.js';
import { getBootId, logout } from './api.js';
import { iconBookOpen, iconCube, iconSearch, iconBell, iconSettings, iconLogout, iconWarning, iconDownload } from './icons.js';
import { mountNotificationsPanel } from './components/notifications-panel.js';

// ── Bootstrap ────────────────────────────────────────────────────────────────

(async () => {
  if (location.pathname === '/login') {
    const appEl = document.getElementById('app');
    if (appEl) initRouter(appEl);
    _hideChrome();
    return;
  }

  await initPermissions();
  connectSSE();

  try {
    const { boot_id } = await getBootId();
    if (boot_id) setState('bootId', boot_id);
  } catch { /* non-fatal */ }

  const navEl    = document.getElementById('nav');
  const tabNavEl = document.getElementById('bottom-nav');

  if (navEl)    _renderDesktopNav(navEl);
  if (tabNavEl) _renderBottomNav(tabNavEl);

  const appEl = document.getElementById('app');
  if (appEl) initRouter(appEl);

  window.addEventListener('kani:server-restart', _handleServerRestart);
})();

// ── Desktop nav ───────────────────────────────────────────────────────────────

/** @param {HTMLElement} el */
function _renderDesktopNav(el) {
  el.className = 'hidden md:flex fixed top-0 inset-x-0 z-30 h-12 bg-surface border-b border-border items-center px-6 gap-4';
  el.setAttribute('role', 'banner');

  const links = _buildNavLinks();

  el.innerHTML = `
    <a href="/" class="text-base font-bold text-text hover:text-accent transition-colors shrink-0 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2 focus-visible:ring-offset-bg rounded-sm">Kani</a>
    <nav class="flex items-center gap-1 flex-1" role="navigation" aria-label="Main navigation" id="desktop-nav-links">
      ${links}
    </nav>
    <div class="flex items-center gap-1 shrink-0">
      <div id="notifications-mount"></div>
      <button
        class="btn-icon"
        id="nav-logout"
        aria-label="Sign out"
        title="Sign out"
      >${iconLogout}</button>
    </div>
  `;

  // Mount notifications panel
  const mount = el.querySelector('#notifications-mount');
  if (mount) mountNotificationsPanel(/** @type {HTMLElement} */ (mount));

  el.querySelector('#nav-logout')?.addEventListener('click', async () => {
    try { await logout(); } catch { /* ignore */ }
    navigate('/login');
  });

  _updateDesktopActive(el, location.pathname);

  onNavigate(path => {
    if (path === '/login') {
      _hideChrome();
      return;
    }
    _showChrome();
    _updateDesktopActive(el, path);
  });

  subscribe('permissions', () => {
    const linksEl = el.querySelector('#desktop-nav-links');
    if (linksEl) linksEl.innerHTML = _buildNavLinks();
    _updateDesktopActive(el, location.pathname);
  });
}

/** @returns {string} */
function _buildNavLinks() {
  const defs = [
    { href: '/',           label: 'Library',   perm: 'library:view' },
    { href: '/sources',    label: 'Sources',   perm: 'source:browse' },
    { href: '/search',     label: 'Search',    perm: 'source:browse' },
    { href: '/updates',    label: 'Updates',   perm: 'library:view' },
    { href: '/downloads',  label: 'Downloads', perm: 'chapter:download' },
    { href: '/settings',   label: 'Settings',  perm: 'settings:view' },
    { href: '/accounts',   label: 'Accounts',  perm: 'user:manage' },
  ];
  return defs
    .filter(d => !d.perm || hasPermission(d.perm))
    .map(d => `<a href="${d.href}" class="desktop-nav-link px-3 py-1 text-sm font-medium rounded-md text-text-muted hover:text-text hover:bg-surface-2 transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2 focus-visible:ring-offset-bg">${d.label}</a>`)
    .join('');
}

/**
 * @param {HTMLElement} el
 * @param {string} path
 */
function _updateDesktopActive(el, path) {
  for (const a of /** @type {NodeListOf<HTMLAnchorElement>} */ (el.querySelectorAll('.desktop-nav-link'))) {
    const href = a.getAttribute('href') ?? '';
    const isActive = href === '/' ? path === '/' : path.startsWith(href);
    a.classList.toggle('!text-accent', isActive);
    a.classList.toggle('!bg-accent/5', isActive);
    a.setAttribute('aria-current', isActive ? 'page' : 'false');
  }
}

// ── Mobile bottom tab bar ────────────────────────────────────────────────────

/** @param {HTMLElement} el */
function _renderBottomNav(el) {
  el.className = 'md:hidden fixed bottom-0 inset-x-0 z-30 h-16 bg-surface border-t border-border pb-[env(safe-area-inset-bottom)]';

  const tabs = [
    { href: '/',         icon: iconBookOpen, label: 'Library',  perm: 'library:view' },
    { href: '/sources',  icon: iconCube,     label: 'Sources',  perm: 'source:browse', matchPrefix: '/source' },
    { href: '/search',   icon: iconSearch,   label: 'Search',   perm: 'source:browse' },
    { href: '/updates',  icon: iconBell,     label: 'Updates',  perm: 'library:view' },
    { href: '/settings', icon: iconSettings, label: 'Settings', perm: 'settings:view' },
  ].filter(t => !t.perm || hasPermission(t.perm));

  const inner = document.createElement('div');
  inner.className = 'flex h-full';
  inner.id = 'tab-bar-inner';

  for (const tab of tabs) {
    const a = document.createElement('a');
    a.href = tab.href;
    a.className = 'tab-link flex flex-col items-center justify-center flex-1 gap-0.5 text-xs text-text-muted transition-colors hover:text-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent';
    a.setAttribute('aria-label', tab.label);
    if (tab.matchPrefix) a.dataset.matchPrefix = tab.matchPrefix;
    a.innerHTML = `<span class="[&_svg]:w-5 [&_svg]:h-5">${tab.icon}</span><span>${tab.label}</span>`;
    inner.appendChild(a);
  }

  el.appendChild(inner);
  _updateTabActive(el, location.pathname);

  onNavigate(path => {
    if (path === '/login') { el.style.display = 'none'; return; }
    el.style.display = '';
    _updateTabActive(el, path);
  });
}

/**
 * @param {HTMLElement} el
 * @param {string} path
 */
function _updateTabActive(el, path) {
  for (const a of /** @type {NodeListOf<HTMLAnchorElement>} */ (el.querySelectorAll('.tab-link'))) {
    const href = a.getAttribute('href') ?? '';
    const matchPrefix = a.dataset.matchPrefix ?? href;
    const isActive = href === '/' ? path === '/' : path.startsWith(matchPrefix);
    a.classList.toggle('!text-accent', isActive);
    a.setAttribute('aria-current', isActive ? 'page' : 'false');
  }
}

// ── Chrome show/hide ──────────────────────────────────────────────────────────

function _hideChrome() {
  const nav    = document.getElementById('nav');
  const tabNav = document.getElementById('bottom-nav');
  if (nav)    nav.style.display = 'none';
  if (tabNav) tabNav.style.display = 'none';
  const app = document.getElementById('app');
  if (app) { app.classList.remove('md:pt-12', 'md:pt-[38px]', 'md:pt-14', 'pb-20', 'md:pb-6'); }
}

function _showChrome() {
  const nav    = document.getElementById('nav');
  const tabNav = document.getElementById('bottom-nav');
  if (nav)    nav.style.display = '';
  if (tabNav) tabNav.style.display = '';
  const app = document.getElementById('app');
  if (app) app.classList.add('md:pt-12', 'pb-20', 'md:pb-6');
}

// ── Server restart banner ─────────────────────────────────────────────────────

function _handleServerRestart() {
  if (document.getElementById('restart-banner')) return;

  const banner = document.createElement('div');
  banner.id = 'restart-banner';
  banner.className = 'fixed top-12 inset-x-0 z-20 flex items-center gap-3 px-6 py-3 bg-warn/10 border-b border-warn/30 text-sm text-warn';
  banner.innerHTML = `
    <span aria-hidden="true" class="shrink-0 [&_svg]:w-4 [&_svg]:h-4">${iconWarning}</span>
    <span class="flex-1">Server restarted. Reload to get the latest version.</span>
    <button class="btn-primary btn-sm ml-auto" onclick="location.reload()">Reload</button>
  `;
  document.body.prepend(banner);
}
