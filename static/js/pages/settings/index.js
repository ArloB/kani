// @ts-check
// Settings page orchestrator — replaces the monolithic settings.js.
// Manages: section navigation, RestartTray, section mounting/teardown.

import * as api from '../../api.js';
import { hasPermission } from '../../session.js';
import { escapeHtml, deferredSkeleton } from '../../utils.js';
import { iconLock } from '../../icons.js';
import { t } from '../../i18n.js';
import { buildSettingsSearchIndex } from '../../settings-search-index.js';
import { skeletonSettingsCards } from '../../components/skeletons.js';
import { createEmptyState } from '../../components/empty-state.js';
import { mountRestartTray } from '../../components/restart-tray.js';
import { setPageHeader, clearPageHeader } from '../../components/app-header.js';
import { setBeforeNavigate, clearBeforeNavigate } from '../../router.js';
import { showConfirm } from '../../components/modal.js';
import { showApiError } from '../../components/toast.js';
import { pushState as pushUrlState } from '../../url-params.js';
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
import * as security          from './security.js';
import * as collections       from './collections.js';
import * as trash             from './trash.js';
import * as storage           from './storage.js';
import * as maintenance       from './maintenance.js';

/** @type {Array<() => void>} */
let _panelDestroys = [];
/** @type {string | null} */
let _activeSection = null;
/** @type {(() => boolean) | null} */
let _activeIsDirty = null;
/** @type {ReturnType<typeof setInterval> | null} */
let _dirtyPollTimer = null;

function _stopDirtyPoll() {
  if (_dirtyPollTimer != null) clearInterval(_dirtyPollTimer);
  _dirtyPollTimer = null;
}

/** @param {string} message */
function _confirmDiscard(message) {
  return showConfirm(message, {
    title: t('settings.unsaved.title'),
    confirmLabel: t('settings.unsaved.leave'),
    cancelLabel: t('settings.unsaved.stay'),
  });
}

/** @param {HTMLElement} container */
export async function init(container) {
  document.title = t('settings.page_title');
  _panelDestroys = [];
  _activeSection = null;
  _stopDirtyPoll();
  setPageHeader({ crumbs: [{ label: t('settings.crumb') }] });

  setBeforeNavigate(async () => {
    if (_activeIsDirty?.() && !(await _confirmDiscard(t('settings.unsaved.page.message')))) return false;
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
    { id: 'general',          label: t('settings.section.general.label'),          description: t('settings.section.general.desc'),          perm: null,                     mount: el => general.mount(el) },
    { id: 'library',          label: t('settings.section.library.label'),          description: t('settings.section.library.desc'),          perm: 'library:manage',          mount: el => library.mount(el, catList) },
    { id: 'collections',      label: t('settings.section.collections.label'),      description: t('settings.section.collections.desc'),      perm: 'library:manage',          mount: el => collections.mount(el) },
    { id: 'manga-management', label: t('settings.section.manga_management.label'), description: t('settings.section.manga_management.desc'), perm: 'library:manage',          mount: el => mangaManagement.mount(el) },
    { id: 'trash',            label: t('settings.section.trash.label'),            description: t('settings.section.trash.desc'),            perm: 'library:view',            mount: el => trash.mount(el) },
    { id: 'downloads',        label: t('settings.section.downloads.label'),        description: t('settings.section.downloads.desc'),        perm: 'settings:edit_download',  mount: el => downloads.mount(el, settings) },
    { id: 'offline',          label: t('settings.section.offline.label'),          description: t('settings.section.offline.desc'),          perm: null,                      mount: el => offline.mount(el) },
    { id: 'scan',             label: t('settings.section.scan.label'),             description: t('settings.section.scan.desc'),             perm: 'settings:edit_scan',      mount: el => scan.mount(el, settings) },
    { id: 'trackers',         label: t('settings.section.trackers.label'),         description: t('settings.section.trackers.desc'),         perm: null,                      mount: el => trackers.mount(el, settings) },
    { id: 'email',            label: t('settings.section.email.label'),            description: t('settings.section.email.desc'),            perm: 'settings:edit_advanced', group: t('settings.group.server'),  mount: el => email.mount(el, settings) },
    { id: 'webhooks',         label: t('settings.section.webhooks.label'),         description: t('settings.section.webhooks.desc'),         perm: 'settings:edit_advanced', group: t('settings.group.server'),  mount: el => webhooks.mount(el) },
    { id: 'advanced',         label: t('settings.section.advanced.label'),         description: t('settings.section.advanced.desc'),         perm: 'settings:edit_advanced', group: t('settings.group.server'),  mount: el => advanced.mount(el, settings, bootId) },
    { id: 'storage',          label: t('settings.section.storage.label'),          description: t('settings.section.storage.desc'),          perm: 'admin:manage',           group: t('settings.group.server'),  mount: el => storage.mount(el) },
    { id: 'maintenance',      label: t('settings.section.maintenance.label'),      description: t('settings.section.maintenance.desc'),      perm: 'settings:edit_advanced', group: t('settings.group.server'),  mount: el => maintenance.mount(el, settings) },
    { id: 'server',           label: t('settings.section.server.label'),           description: t('settings.section.server.desc'),           perm: 'server:manage',          group: t('settings.group.server'),  mount: el => server.mount(el) },
    { id: 'account',          label: t('settings.section.account.label'),          description: t('settings.section.account.desc'),          perm: null,                     group: t('settings.group.account'), mount: el => account.mount(el) },
    { id: 'security',         label: t('settings.section.security.label'),         description: t('settings.section.security.desc'),         perm: null,                     group: t('settings.group.account'), mount: el => security.mount(el) },
  ];

  const sections = allSections.filter(s => !s.perm || hasPermission(s.perm));

  container.innerHTML = `
    <div class="flex h-full min-h-0 flex-1">
      <aside
        class="hidden lg:flex flex-col w-52 shrink-0 border-r border-border-subtle overflow-y-auto"
        aria-label="Settings sections"
      >
        <div class="p-2 flex flex-col gap-0.5 pt-4">
          <div class="px-2 pb-1">
            <input
              id="settings-search"
              type="search"
              placeholder="${t('settings.search.placeholder')}"
              autocomplete="off"
              class="w-full text-xs bg-surface-2 border border-border-subtle rounded-lg px-2.5 py-1.5 outline-none focus:ring-1 focus:ring-accent/50 placeholder:text-text-faint text-text"
              aria-label="${t('settings.search.placeholder')}"
            />
          </div>
          <div class="nav-section">${t('settings.crumb')}</div>
          <div id="settings-nav-items"></div>
        </div>
      </aside>
      <div class="flex-1 min-w-0 flex flex-col overflow-y-auto">
        <div id="settings-restart-tray" class="px-4 md:px-8 pt-4"></div>
        <div class="js-mobile-list lg:hidden flex flex-col gap-0 px-0 py-2">
          <div class="px-4 pt-2 pb-1 lg:hidden">
            <input
              id="settings-search-mobile"
              type="search"
              placeholder="${t('settings.search.placeholder_mobile')}"
              autocomplete="off"
              class="w-full text-sm bg-surface-2 border border-border-subtle rounded-lg px-3 py-2 outline-none focus:ring-1 focus:ring-accent/50 placeholder:text-text-faint text-text"
              aria-label="${t('settings.search.placeholder_mobile')}"
            />
          </div>
          <div class="flex flex-col divide-y divide-border-subtle border-t border-border-subtle" id="mobile-nav-items"></div>
        </div>
        <button type="button" class="js-mobile-back lg:hidden hidden items-center gap-2 px-4 py-3 text-sm text-accent hover:text-accent/80 transition-colors">
          <span aria-hidden="true">‹</span> ${t('settings.mobile_back')}
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
  const searchEl       = /** @type {HTMLInputElement|null} */ (container.querySelector('#settings-search'));
  const searchMobileEl = /** @type {HTMLInputElement|null} */ (container.querySelector('#settings-search-mobile'));

  let _filteredSections = sections;
  /** @type {Map<string, string[]>} matched individual-setting labels per section */
  let _sectionHits = new Map();
  let _activeQuery = '';

  /** @type {(() => Promise<void>) | null} save() from the mounted section */
  let _activeSave = null;
  let _saving = false;

  const saveBarEl = document.createElement('div');
  saveBarEl.className = 'js-save-bar hidden sticky bottom-0 max-w-4xl w-full px-4 md:px-8 pb-4 pt-2';
  saveBarEl.innerHTML = `
    <div class="flex items-center gap-3 bg-surface border border-border rounded-xl px-4 py-3 shadow-lg">
      <span class="dirty-dot shrink-0" aria-hidden="true"></span>
      <span class="text-sm text-text flex-1">${t('settings.savebar.unsaved')}</span>
      <button type="button" class="btn-ghost btn-sm js-savebar-discard">${t('settings.savebar.discard')}</button>
      <button type="button" class="btn-primary btn-sm js-savebar-save">${t('common.save')}</button>
    </div>
  `;
  contentEl.after(saveBarEl);
  const saveBtn = /** @type {HTMLButtonElement} */ (saveBarEl.querySelector('.js-savebar-save'));
  const discardBtn = /** @type {HTMLButtonElement} */ (saveBarEl.querySelector('.js-savebar-discard'));

  saveBtn.addEventListener('click', async () => {
    if (!_activeSave || _saving) return;
    _saving = true;
    saveBtn.disabled = true;
    discardBtn.disabled = true;
    try {
      await _activeSave();
    } catch (e) {
      showApiError(e);
    } finally {
      _saving = false;
      saveBtn.disabled = false;
      discardBtn.disabled = false;
    }
  });

  discardBtn.addEventListener('click', () => {
    if (_saving || !_activeSection) return;
    _activeIsDirty = null;
    _showSection(_activeSection);
  });

  function _syncSaveBar(/** @type {boolean} */ dirty) {
    saveBarEl.classList.toggle('hidden', !(dirty && _activeSave));
  }

  /** @param {string | null} sectionId @param {boolean} dirty */
  function _setDirtyIndicator(sectionId, dirty) {
    for (const navEl of [desktopNavEl, mobileNavEl]) {
      for (const dot of navEl.querySelectorAll('.dirty-dot')) dot.remove();
      if (!sectionId || !dirty) continue;
      const btn = navEl.querySelector(`[data-section="${sectionId}"]`);
      if (!btn) continue;
      const dot = document.createElement('span');
      dot.className = 'dirty-dot ml-auto';
      dot.setAttribute('aria-label', t('settings.unsaved.title'));
      btn.appendChild(dot);
    }
  }

  function _startDirtyPoll() {
    _stopDirtyPoll();
    _dirtyPollTimer = setInterval(() => {
      const dirty = _activeIsDirty?.() ?? false;
      _setDirtyIndicator(_activeSection, dirty);
      _syncSaveBar(dirty);
    }, 400);
  }

  const _searchIndex = buildSettingsSearchIndex(sections);

  function _applySearch(/** @type {string} */ query) {
    const q = query.trim().toLowerCase();
    _activeQuery = q;
    _sectionHits = new Map();
    if (q) {
      _filteredSections = sections.filter(s => {
        const sectionMatch = (s.label + ' ' + s.description).toLowerCase().includes(q);
        const hits = (_searchIndex.get(s.id) ?? []).filter(it =>
          it.label.toLowerCase().includes(q) || it.desc.toLowerCase().includes(q));
        if (hits.length) _sectionHits.set(s.id, hits);
        return sectionMatch || hits.length > 0;
      });
    } else {
      _filteredSections = sections;
    }
    _buildDesktopNav();
    _buildMobileNav();
    if (q) {
      _showSearchResults();
    } else if (_activeSection) {
      _showSection(_activeSection);
    }
  }

  /** Small hit-count badge shown next to a nav entry while searching. */
  function _mkHitList(/** @type {string} */ sectionId) {
    const hits = _sectionHits.get(sectionId);
    if (!_activeQuery || !hits?.length) return null;
    const badge = document.createElement('span');
    badge.className = 'nav-badge ml-auto';
    badge.textContent = String(hits.length);
    return badge;
  }

  /** Cross-section results panel: every matching setting, grouped by section. */
  function _showSearchResults() {
    _stopDirtyPoll();
    _setDirtyIndicator(null, false);
    _activeIsDirty = null;
    _activeSave = null;
    _syncSaveBar(false);
    for (const d of _panelDestroys.slice(1)) d();
    _panelDestroys = [_panelDestroys[0]];
    contentEl.innerHTML = '';

    const headerEl = document.createElement('div');
    headerEl.className = 'section-card-header border border-border rounded-xl mb-2 bg-surface';
    headerEl.innerHTML = `<div><h2>${escapeHtml(t('settings.search.results.title'))}</h2></div>`;
    contentEl.appendChild(headerEl);

    if (_sectionHits.size === 0) {
      contentEl.appendChild(createEmptyState({
        title: t('settings.search.results.empty.title'),
        subtitle: t('settings.search.results.empty.subtitle'),
      }));
      return;
    }

    for (const s of sections) {
      const hits = _sectionHits.get(s.id);
      if (!hits?.length) continue;

      const group = document.createElement('div');
      group.className = 'flex flex-col gap-2';

      const heading = document.createElement('h3');
      heading.className = 'font-display text-base font-bold text-text px-1';
      heading.textContent = s.label;
      group.appendChild(heading);

      const card = document.createElement('div');
      card.className = 'bg-surface border border-border-subtle rounded-xl divide-y divide-border-subtle overflow-hidden';
      for (const hit of hits) {
        const row = document.createElement('button');
        row.type = 'button';
        row.className = 'w-full flex items-center justify-between gap-3 px-4 py-3 text-left hover:bg-surface-2 transition-colors';
        row.innerHTML = `
          <span class="flex flex-col gap-0.5 min-w-0">
            <span class="text-sm font-medium text-text truncate">${escapeHtml(hit.label)}</span>
            ${hit.desc ? `<span class="text-xs text-text-muted truncate">${escapeHtml(hit.desc)}</span>` : ''}
          </span>
          <span class="text-xs text-text-faint shrink-0">${escapeHtml(s.label)}</span>
        `;
        row.addEventListener('click', () => {
          const label = hit.label;
          _clearSearchInputs();
          _showSection(s.id, true).then(() => _scheduleHighlight(label.toLowerCase()));
        });
        card.appendChild(row);
      }
      group.appendChild(card);
      contentEl.appendChild(group);
    }
  }

  function _clearSearchInputs() {
    _activeQuery = '';
    _sectionHits = new Map();
    _filteredSections = sections;
    if (searchEl) searchEl.value = '';
    if (searchMobileEl) searchMobileEl.value = '';
    _buildDesktopNav();
    _buildMobileNav();
  }

  // Build desktop nav items
  function _buildDesktopNav() {
    desktopNavEl.innerHTML = '';
    let lastGroup = '';
    for (const s of _filteredSections) {
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
      const hitBadge = _mkHitList(s.id);
      if (hitBadge) btn.appendChild(hitBadge);
    }
    if (_filteredSections.length === 0) {
      const empty = document.createElement('div');
      empty.className = 'px-2 py-2 text-xs text-text-faint';
      empty.textContent = t('settings.search.empty');
      desktopNavEl.appendChild(empty);
    }
  }

  // Build mobile nav items
  function _buildMobileNav() {
    mobileNavEl.innerHTML = '';
    for (const s of _filteredSections) {
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.dataset.section = s.id;
      btn.className = 'w-full text-left px-4 py-3.5 text-sm text-text hover:bg-surface-2 transition-colors flex items-center justify-between';
      btn.innerHTML = `<span>${escapeHtml(s.label)}</span><span class="text-text-muted text-xs">›</span>`;
      btn.addEventListener('click', () => _showSection(s.id, true));
      const hitBadge = _mkHitList(s.id);
      if (hitBadge) btn.insertBefore(hitBadge, btn.lastElementChild);
      mobileNavEl.appendChild(btn);
    }
  }

  /**
   * Marks rendered rows whose text matches the query. Works on any section
   * markup: finds elements with a directly-matching text node and climbs to
   * the nearest row-like container.
   * @param {HTMLElement} root
   * @param {string} q
   * @returns {boolean} whether at least one row matched
   */
  /**
   * @param {HTMLElement} root
   * @param {string} q
   * @param {{ single?: boolean }} [opts] — single: highlight only the best
   *   match (used when jumping from a search result, so one click marks one
   *   row). Matches only resolve to real settings rows — never headings.
   */
  function _highlightMatches(root, q, opts = {}) {
    for (const el of root.querySelectorAll('.search-hit')) el.classList.remove('search-hit');
    if (!q) return false;
    /** @type {HTMLElement|null} */ let exact = null;
    /** @type {Set<HTMLElement>} */ const partial = new Set();
    for (const el of root.querySelectorAll('span, p, label, div')) {
      const ownText = Array.from(el.childNodes)
        .filter(n => n.nodeType === Node.TEXT_NODE)
        .map(n => n.textContent)
        .join('')
        .trim();
      if (!ownText.toLowerCase().includes(q)) continue;
      const row = /** @type {HTMLElement|null} */ (
        el.closest('[data-settings-row]') ?? el.closest('.pref-row'));
      if (!row) continue;
      if (ownText.toLowerCase() === q) exact ??= row;
      partial.add(row);
    }
    /** @type {HTMLElement[]} */
    let rows = [...partial];
    if (opts.single) rows = exact ? [exact] : rows.slice(0, 1);
    for (const row of rows) row.classList.add('search-hit');
    const first = exact ?? rows[0] ?? null;
    if (first) first.scrollIntoView({ block: 'center', behavior: 'smooth' });
    return first !== null;
  }

  /**
   * Re-runs highlighting as async section content streams in.
   * @param {string} [overrideQ] — highlight this text instead of the live query
   *   (used when jumping from a cross-section search result).
   */
  function _scheduleHighlight(overrideQ) {
    const q = overrideQ ?? _activeQuery;
    if (!q) return;
    const single = overrideQ != null;
    for (const delay of [50, 400, 1200]) {
      setTimeout(() => {
        if (contentEl.isConnected) _highlightMatches(contentEl, q, { single });
      }, delay);
    }
  }

  searchEl?.addEventListener('input', (e) => _applySearch(/** @type {HTMLInputElement} */ (e.target).value));
  searchMobileEl?.addEventListener('input', (e) => _applySearch(/** @type {HTMLInputElement} */ (e.target).value));

  _buildDesktopNav();
  _buildMobileNav();

  /**
   * @param {string} sectionId
   * @param {boolean} [pushState] - true when triggered by user action; false on init/restore
   */
  async function _showSection(sectionId, pushState = false) {
    if (_activeIsDirty?.() && !(await _confirmDiscard(t('settings.unsaved.section.message')))) return;

    _stopDirtyPoll();
    _setDirtyIndicator(null, false);
    _activeSection = sectionId;
    const section = sections.find(s => s.id === sectionId);
    if (!section) return;

    if (pushState) {
      pushUrlState({ section: sectionId });
    }
    setPageHeader({ crumbs: [{ label: t('settings.crumb'), href: '/settings' }, { label: section.label }] });

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
    _activeSave = result.save ?? null;
    _syncSaveBar(false);
    _panelDestroys.push(result.destroy);
    _scheduleHighlight();
    if (_activeIsDirty) _startDirtyPoll();
  }

  async function _showMobileList(pushState = false) {
    if (_activeIsDirty?.() && !(await _confirmDiscard(t('settings.unsaved.section.message')))) return;

    _stopDirtyPoll();
    _setDirtyIndicator(null, false);
    _activeSection = null;
    _activeIsDirty = null;
    _activeSave = null;
    _syncSaveBar(false);
    if (pushState) pushUrlState({ section: null });
    setPageHeader({ crumbs: [{ label: t('settings.crumb') }] });
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

  // Restore section (and optional search query, e.g. from the command
  // palette's per-setting deep links) from URL.
  const _urlParams = new URLSearchParams(location.search);
  const _urlSection = _urlParams.get('section');
  const _urlQuery = _urlParams.get('q');
  const _initialSectionId = sections.find(s => s.id === _urlSection)?.id
    ?? (window.innerWidth >= 1024 ? sections[0]?.id : null);
  if (_urlQuery) {
    if (searchEl) searchEl.value = _urlQuery;
    if (searchMobileEl) searchMobileEl.value = _urlQuery;
    _applySearch(_urlQuery);
  }
  if (_initialSectionId) {
    _showSection(_initialSectionId);
  }
}

/** @param {HTMLElement} container */
export function destroy(container) {
  clearPageHeader();
  clearBeforeNavigate();
  _stopDirtyPoll();
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
    <p class="text-base font-medium text-text">${t('settings.access_denied.title')}</p>
    <p class="text-sm">${t('settings.access_denied.desc')}</p>
  `;
  return el;
}
