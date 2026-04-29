// @ts-check
// App entry point. Bootstraps permissions, SSE, nav, and the SPA router.

import { initPermissions, getState, setState, subscribe, hasPermission } from './state.js';
import { connectSSE } from './sse.js';
import { initRouter, navigate, onNavigate } from './router.js';
import { getBootId, logout } from './api.js';
import { iconSettings, iconLogout, iconWarning, iconBell, iconLibrary, iconSources, iconSearch, iconUpdates, iconDownloads, iconAccounts, iconBookOpen, iconCube } from './icons.js';
import { mountNotificationsPanel } from './components/notifications-panel.js';
import { mountAppHeader } from './components/app-header.js';

// ── Bootstrap ────────────────────────────────────────────────────────────────

(async () => {
  if (location.pathname === '/login' || location.pathname === '/register') {
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
  if (appEl) {
    // Mount the global header at the top of shell-main; pages go in a sub-container.
    const { notificationsMount } = mountAppHeader(appEl);
    mountNotificationsPanel(notificationsMount);

    const pageContent = document.createElement('div');
    pageContent.id = 'page-content';
    pageContent.className = 'flex flex-col flex-1';
    appEl.appendChild(pageContent);

    initRouter(pageContent);
  }

  window.addEventListener('kani:server-restart', _handleServerRestart);
})();

// ── Sidebar ───────────────────────────────────────────────────────────────────

/** @param {HTMLElement} el */
function _renderDesktopNav(el) {
  el.className = 'sidebar';

  el.innerHTML = `
    <div class="sidebar-brand">
      <span class="sidebar-mark" aria-hidden="true">K</span>
      <span class="sidebar-title">Kani</span>
    </div>
    <nav class="sidebar-nav" id="sidebar-nav-links" aria-label="Main navigation"></nav>
    <div class="sidebar-footer" id="sidebar-footer">
      <span class="avatar" id="sidebar-avatar" aria-hidden="true">U</span>
      <span class="flex flex-col min-w-0 flex-1">
        <span class="text-sm font-medium text-text truncate" id="sidebar-username">User</span>
        <span class="meta truncate" id="sidebar-role"></span>
      </span>
      <button class="btn-icon shrink-0" id="nav-logout" aria-label="Sign out" title="Sign out">${iconLogout}</button>
    </div>
  `;

  el.querySelector('#nav-logout')?.addEventListener('click', async () => {
    try { await logout(); } catch { /* ignore */ }
    navigate('/login');
  });

  _rebuildSidebarLinks(el);
  _updateSidebarUser(el);
  _updateDesktopActive(el, location.pathname);

  onNavigate(path => {
    if (path === '/login' || path === '/register') { _hideChrome(); return; }
    _showChrome();
    _updateDesktopActive(el, path);
  });

  subscribe('permissions', () => {
    _rebuildSidebarLinks(el);
    _updateDesktopActive(el, location.pathname);
  });

  subscribe('user', () => _updateSidebarUser(el));
}

/** @param {HTMLElement} sidebar */
function _rebuildSidebarLinks(sidebar) {
  const nav = sidebar.querySelector('#sidebar-nav-links');
  if (!nav) return;
  nav.innerHTML = _buildNavLinks();
}

/** @param {HTMLElement} sidebar */
function _updateSidebarUser(sidebar) {
  const user = getState('user');
  if (!user) return;
  const avatarEl = sidebar.querySelector('#sidebar-avatar');
  const nameEl   = sidebar.querySelector('#sidebar-username');
  const roleEl   = sidebar.querySelector('#sidebar-role');
  if (avatarEl) avatarEl.textContent = (user.username ?? 'U')[0].toUpperCase();
  if (nameEl)   nameEl.textContent   = user.username ?? 'User';
  if (roleEl)   roleEl.textContent   = user.role ?? '';
}

/** @returns {string} */
function _buildNavLinks() {
  /** @type {{ href: string, label: string, icon: string, perm?: string, section?: string, matchPrefix?: string, matchPaths?: string[] }[]} */
  const defs = [
    { href: '/',          label: 'Library',   icon: iconLibrary,   perm: 'library:view',  matchPaths: ['/', '/manga'] },
    { href: '/sources',   label: 'Sources',   icon: iconSources,   perm: 'source:browse', matchPrefix: '/source' },
    { href: '/search',    label: 'Search',    icon: iconSearch,    perm: 'source:browse' },
    { href: '/updates',   label: 'Updates',   icon: iconUpdates,   perm: 'library:view' },
    { href: '/downloads', label: 'Downloads', icon: iconDownloads, perm: 'chapter:download' },
    { href: '/settings',  label: 'Settings',  icon: iconSettings,  perm: 'settings:view',  section: 'Admin' },
    { href: '/accounts',  label: 'Accounts',  icon: iconAccounts,  perm: 'user:manage' },
  ];
  const visible = defs.filter(d => !d.perm || hasPermission(d.perm));
  let html = '';
  let lastSection = '';
  for (const d of visible) {
    if (d.section && d.section !== lastSection) {
      html += `<div class="nav-section">${d.section}</div>`;
      lastSection = d.section;
    }
    const prefix = d.matchPrefix ? ` data-match-prefix="${d.matchPrefix}"` : '';
    const paths = d.matchPaths ? ` data-match-paths="${d.matchPaths.join(',')}"` : '';
    html += `<a href="${d.href}" class="nav-item" data-href="${d.href}"${prefix}${paths}>${d.icon}<span>${d.label}</span></a>`;
  }
  return html;
}

/**
 * @param {HTMLElement} el
 * @param {string} path
 */
function _updateDesktopActive(el, path) {
  for (const a of /** @type {NodeListOf<HTMLAnchorElement>} */ (el.querySelectorAll('.nav-item[data-href]'))) {
    const href = a.dataset.href ?? '';
    const matchPrefix = a.dataset.matchPrefix;
    const matchPaths = a.dataset.matchPaths?.split(',');
    let isActive;
    if (matchPaths) {
      isActive = matchPaths.some(p => p === '/' ? path === '/' : path.startsWith(p));
    } else if (matchPrefix) {
      isActive = path.startsWith(matchPrefix);
    } else {
      isActive = href === '/' ? path === '/' : path.startsWith(href);
    }
    a.classList.toggle('active', isActive);
    a.setAttribute('aria-current', isActive ? 'page' : 'false');
  }
}

// ── Mobile bottom tab bar ────────────────────────────────────────────────────

/** @param {HTMLElement} el */
function _renderBottomNav(el) {
  el.className = 'md:hidden fixed bottom-0 inset-x-0 z-30 h-16 bg-surface border-t border-border pb-safe';

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
    a.innerHTML = `<span class="icon-md">${tab.icon}</span><span>${tab.label}</span>`;
    inner.appendChild(a);
  }

  el.appendChild(inner);
  _updateTabActive(el, location.pathname);

  onNavigate(path => {
    if (path === '/login' || path === '/register') { el.style.display = 'none'; return; }
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
  const hdr    = document.querySelector('.app-header');
  if (nav)    nav.style.display = 'none';
  if (tabNav) tabNav.style.display = 'none';
  if (hdr)    /** @type {HTMLElement} */ (hdr).style.display = 'none';
  const app = document.getElementById('app');
  if (app) app.classList.remove('shell-main');
}

function _showChrome() {
  const nav    = document.getElementById('nav');
  const tabNav = document.getElementById('bottom-nav');
  const hdr    = document.querySelector('.app-header');
  if (nav)    nav.style.display = '';
  if (tabNav) tabNav.style.display = '';
  if (hdr)    /** @type {HTMLElement} */ (hdr).style.display = '';
  const app = document.getElementById('app');
  if (app) app.classList.add('shell-main');
}

// ── Server restart banner ─────────────────────────────────────────────────────

function _handleServerRestart() {
  if (document.getElementById('restart-banner')) return;

  const banner = document.createElement('div');
  banner.id = 'restart-banner';
  banner.className = 'fixed top-0 right-0 left-0 md:left-sidebar z-20 flex items-center gap-3 px-6 py-3 bg-warn/10 border-b border-warn/30 text-sm text-warn';
  banner.innerHTML = `
    <span aria-hidden="true" class="shrink-0 icon-sm">${iconWarning}</span>
    <span class="flex-1">Server restarted. Reload to get the latest version.</span>
    <button class="btn-primary btn-sm ml-auto" onclick="location.reload()">Reload</button>
  `;
  document.body.prepend(banner);
}
