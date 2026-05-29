// @ts-check
// Settings page orchestrator — replaces the monolithic settings.js.
// Manages: section navigation, RestartTray, section mounting/teardown.

import * as api from '../../api.js';
import { hasPermission } from '../../state.js';
import { escapeHtml, deferredSkeleton } from '../../utils.js';
import { iconLock } from '../../icons.js';
import { skeletonSettingsCards } from '../../components/skeletons.js';
import { mountRestartTray } from '../../components/restart-tray.js';
import { setPageHeader, clearPageHeader } from '../../components/app-header.js';
import { setBeforeNavigate, clearBeforeNavigate } from '../../router.js';
import { showConfirm } from '../../components/modal.js';
import * as general           from './general.js';
import * as library           from './library.js';
import * as mangaManagement   from './manga-management.js';
import * as downloads         from './downloads.js';
import * as scan              from './scan.js';
import * as trackers          from './trackers.js';
import * as offline           from './offline.js';
import * as advanced          from './advanced.js';
import * as account           from './account.js';
import * as server            from './server.js';
import * as email             from './email.js';
import * as webhooks          from './webhooks.js';

/** @type {Array<() => void>} */
let _panelDestroys = [];
/** @type {string | null} */
let _activeSection = null;
/** @type {(() => boolean) | null} */
let _activeIsDirty = null;

/** @param {HTMLElement} container */
export async function init(container) {
  document.title = 'Settings - Kani';
  _panelDestroys = [];
  _activeSection = null;
  setPageHeader({ crumbs: [{ label: 'Settings' }] });

  setBeforeNavigate(async () => {
    if (_activeIsDirty?.() && !(await showConfirm('You have unsaved changes. Leave this page anyway?', { title: 'Unsaved changes', confirmLabel: 'Leave', cancelLabel: 'Stay' }))) return false;
    return true;
  });

  if (!hasPermission('settings:view')) {
    container.innerHTML = '';
    container.appendChild(_createAccessDenied());
    return;
  }

  const cancelSkeleton = deferredSkeleton(() => {
    container.innerHTML = `<div class="max-w-page mx-auto px-4 md:px-6 py-8">${skeletonSettingsCards(5)}</div>`;
  });

  const [settings, categories, bootData] = await Promise.allSettled([
    api.getSettings(),
    api.getCategories(),
    api.getBootId(),
  ]).then(r => r.map(s => s.status === 'fulfilled' ? s.value : null));

  cancelSkeleton();

  const bootId  = bootData?.boot_id ?? bootData ?? '';
  const catList = Array.isArray(categories) ? categories : [];

  /** @type {Array<{ id: string, label: string, description: string, perm: string|null, group?: string, mount: (el: HTMLElement) => { destroy: () => void } }>} */
  const allSections = [
    { id: 'general',          label: 'General',          description: 'Display preferences, reading behaviour, and notifications.',            perm: null,                     mount: el => general.mount(el) },
    { id: 'library',          label: 'Library',          description: 'Manage categories and import/export your manga collection.',             perm: 'library:manage',          mount: el => library.mount(el, catList) },
    { id: 'manga-management', label: 'Manga Management', description: 'Pending imports, duplicate detection, and orphaned manga.',              perm: 'library:manage',          mount: el => mangaManagement.mount(el) },
    { id: 'downloads',        label: 'Downloads',        description: 'Control download concurrency, queue size, and reading-ahead behaviour.', perm: 'settings:edit_download',  mount: el => downloads.mount(el, settings) },
    { id: 'offline',          label: 'Offline',          description: 'Configure offline reading, page cache, and the OPDS catalog server.',      perm: null,                      mount: el => offline.mount(el) },
    { id: 'scan',      label: 'Scan',       description: 'Configure automatic scanning for new chapters.',                         perm: 'settings:edit_scan',      mount: el => scan.mount(el, settings) },
    { id: 'trackers',  label: 'Trackers',   description: 'Link external tracking services like AniList and MyAnimeList.',          perm: null,                      mount: el => trackers.mount(el, settings) },
    { id: 'email',           label: 'Email / SMTP',    description: 'Configure outbound email for password reset and notifications.', perm: 'settings:edit_advanced', group: 'Server',  mount: el => email.mount(el, settings) },
    { id: 'webhooks',        label: 'Webhooks',        description: 'Send HTTP POST notifications to external services when events occur.', perm: 'settings:edit_advanced', group: 'Server',  mount: el => webhooks.mount(el) },
    { id: 'advanced',        label: 'Advanced',        description: 'FlareSolverr, library path, and other low-level options.',  perm: 'settings:edit_advanced', group: 'Server',  mount: el => advanced.mount(el, settings, bootId) },
    { id: 'server',          label: 'Lifecycle',       description: 'Stop or restart the server process.',                         perm: 'server:manage',          group: 'Server',  mount: el => server.mount(el) },
    { id: 'account',         label: 'My Account',      description: 'Change your password and manage active sessions.',            perm: null,                     group: 'Account', mount: el => account.mount(el) },
  ];

  const sections = allSections.filter(s => !s.perm || hasPermission(s.perm));

  container.innerHTML = `
    <div class="flex min-h-full">
      <aside
        class="hidden lg:flex flex-col w-52 shrink-0 border-r border-border-subtle sticky overflow-y-auto"
        style="top:0;height:100vh;"
        aria-label="Settings sections"
      >
        <div class="p-2 flex flex-col gap-0.5 pt-4">
          <div class="nav-section">Settings</div>
          <div id="settings-nav-items"></div>
        </div>
      </aside>
      <div class="flex-1 min-w-0 flex flex-col">
        <div id="settings-restart-tray" class="px-4 md:px-8 pt-4"></div>
        <div class="js-mobile-list lg:hidden flex flex-col gap-0 px-0 py-2">
          <div class="flex flex-col divide-y divide-border-subtle border-t border-border-subtle" id="mobile-nav-items"></div>
        </div>
        <button type="button" class="js-mobile-back lg:hidden hidden items-center gap-2 px-4 py-3 text-sm text-accent hover:text-accent/80 transition-colors">
          <span aria-hidden="true">‹</span> Back
        </button>
        <div class="js-content max-w-4xl w-full px-4 md:px-8 py-4 md:py-6 flex flex-col gap-6"></div>
      </div>
    </div>
  `;

  // Mount RestartTray
  const restartTrayEl = /** @type {HTMLElement} */ (container.querySelector('#settings-restart-tray'));
  const { unmount: unmountTray } = mountRestartTray(restartTrayEl, {
    currentBootId: bootId,
    onRestart: async () => {
      try { await api.serverRestart(); } catch { /* handled in server section */ }
    },
  });
  _panelDestroys.push(unmountTray);

  const contentEl      = /** @type {HTMLElement} */ (container.querySelector('.js-content'));
  const mobileListEl   = /** @type {HTMLElement} */ (container.querySelector('.js-mobile-list'));
  const mobileBackBtn  = /** @type {HTMLButtonElement} */ (container.querySelector('.js-mobile-back'));
  const desktopNavEl   = /** @type {HTMLElement} */ (container.querySelector('#settings-nav-items'));
  const mobileNavEl    = /** @type {HTMLElement} */ (container.querySelector('#mobile-nav-items'));

  // Build desktop nav items
  function _buildDesktopNav() {
    desktopNavEl.innerHTML = '';
    let lastGroup = '';
    for (const s of sections) {
      if (s.group && s.group !== lastGroup) {
        const sep = document.createElement('div');
        sep.className = 'nav-section';
        sep.textContent = s.group;
        desktopNavEl.appendChild(sep);
        lastGroup = s.group;
      }
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.className = 'nav-item w-full text-left';
      btn.dataset.section = s.id;
      btn.textContent = s.label;
      btn.addEventListener('click', () => _showSection(s.id, true));
      desktopNavEl.appendChild(btn);
    }
  }

  // Build mobile nav items
  function _buildMobileNav() {
    mobileNavEl.innerHTML = '';
    for (const s of sections) {
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.dataset.section = s.id;
      btn.className = 'w-full text-left px-4 py-3.5 text-sm text-text hover:bg-surface-2 transition-colors flex items-center justify-between';
      btn.innerHTML = `<span>${escapeHtml(s.label)}</span><span class="text-text-muted text-xs">›</span>`;
      btn.addEventListener('click', () => _showSection(s.id, true));
      mobileNavEl.appendChild(btn);
    }
  }

  _buildDesktopNav();
  _buildMobileNav();

  /**
   * @param {string} sectionId
   * @param {boolean} [pushState] - true when triggered by user action; false on init/restore
   */
  async function _showSection(sectionId, pushState = false) {
    if (_activeIsDirty?.() && !(await showConfirm('You have unsaved changes. Leave this section anyway?', { title: 'Unsaved changes', confirmLabel: 'Leave', cancelLabel: 'Stay' }))) return;

    _activeSection = sectionId;
    const section = sections.find(s => s.id === sectionId);
    if (!section) return;

    if (pushState) {
      history.pushState(null, '', '/settings?section=' + encodeURIComponent(sectionId));
    }
    setPageHeader({ crumbs: [{ label: 'Settings', href: '/settings' }, { label: section.label }] });

    // Update desktop active state
    for (const btn of desktopNavEl.querySelectorAll('[data-section]')) {
      const isActive = /** @type {HTMLElement} */ (btn).dataset.section === sectionId;
      btn.classList.toggle('active', isActive);
      btn.setAttribute('aria-current', isActive ? 'page' : 'false');
    }

    mobileListEl.classList.add('hidden');
    mobileBackBtn.classList.remove('hidden');
    mobileBackBtn.classList.add('flex');

    for (const d of _panelDestroys.slice(1)) d(); // keep tray destroy at [0]
    _panelDestroys = [_panelDestroys[0]];
    _activeIsDirty = null;
    contentEl.innerHTML = '';

    const headerEl = document.createElement('div');
    headerEl.className = 'section-card-header border border-border rounded-xl mb-2 bg-surface';
    headerEl.innerHTML = `
      <div>
        <h2>${escapeHtml(section.label)}</h2>
        ${section.description ? `<p>${escapeHtml(section.description)}</p>` : ''}
      </div>
    `;
    contentEl.appendChild(headerEl);

    const bodyEl = document.createElement('div');
    bodyEl.className = 'flex flex-col gap-5';
    contentEl.appendChild(bodyEl);

    const result = section.mount(bodyEl);
    _activeIsDirty = result.isDirty ?? null;
    _panelDestroys.push(result.destroy);
  }

  async function _showMobileList(pushState = false) {
    if (_activeIsDirty?.() && !(await showConfirm('You have unsaved changes. Leave this section anyway?', { title: 'Unsaved changes', confirmLabel: 'Leave', cancelLabel: 'Stay' }))) return;

    _activeSection = null;
    _activeIsDirty = null;
    if (pushState) history.pushState(null, '', '/settings');
    setPageHeader({ crumbs: [{ label: 'Settings' }] });
    mobileListEl.classList.remove('hidden');
    mobileBackBtn.classList.add('hidden');
    mobileBackBtn.classList.remove('flex');
    const [trayDestroy, ...rest] = _panelDestroys;
    for (const d of rest) d();
    _panelDestroys = [trayDestroy];
    contentEl.innerHTML = '';
    for (const btn of desktopNavEl.querySelectorAll('[data-section]')) {
      btn.classList.remove('active');
      btn.setAttribute('aria-current', 'false');
    }
  }

  mobileBackBtn.addEventListener('click', () => _showMobileList(true));

  // Restore section from URL (or default to first on desktop)
  const _urlSection = new URLSearchParams(location.search).get('section');
  const _initialSectionId = sections.find(s => s.id === _urlSection)?.id
    ?? (window.innerWidth >= 1024 ? sections[0]?.id : null);
  if (_initialSectionId) {
    _showSection(_initialSectionId);
  }
}

/** @param {HTMLElement} container */
export function destroy(container) {
  clearPageHeader();
  clearBeforeNavigate();
  for (const d of _panelDestroys) d();
  _panelDestroys = [];
  _activeSection = null;
  _activeIsDirty = null;
  container.innerHTML = '';
}

function _createAccessDenied() {
  const el = document.createElement('div');
  el.className = 'flex flex-col items-center justify-center gap-3 py-20 text-text-muted';
  el.innerHTML = `
    <span class="icon-xl opacity-40" aria-hidden="true">${iconLock}</span>
    <p class="text-base font-medium text-text">Access denied</p>
    <p class="text-sm">You do not have permission to view settings.</p>
  `;
  return el;
}
