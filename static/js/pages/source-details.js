// @ts-check
// Source details page — browse / search manga within a single source.
// Desktop: sidebar + tabbed panel (Browse / Settings). Mobile: back button + panel.

import { h, render } from 'preact';
import { useState, useEffect } from 'preact/hooks';
import htm from 'htm';
import * as api from '../api.js';
import { hasPermission, updateState, subscribe } from '../state.js';
import { navigate } from '../router.js';
import { setLocal, getLocal, getLocalInt, debounce, hasNextPage, confirmDialog } from '../utils.js';
import { skeletonGrid } from '../components/skeletons.js';
import { renderPagination } from '../components/pagination.js';
import { createMangaCard } from '../components/manga-card.js';
import { Modal, mountIntoModalRoot } from '../components/modal.js';
import { SourcesSidebar, AddSourceModal, consumePendingSourceId } from '../components/sources-sidebar.js';
import { PreferenceRow, PreferenceDetailView } from '../components/preference-row.js';
import { Icon } from '../components/icon.js';
import { startLoading, finishLoading } from '../components/page-loading-bar.js';
import { setPageHeader, clearPageHeader } from '../components/app-header.js';
import { createErrorState } from '../components/error-state.js';
import { createEmptyState } from '../components/empty-state.js';
import { mountFilterModal } from '../components/filter-panel.js';
import { renderTabs } from '../components/tabs.js';
import { iconSearch, iconChevronDown, iconWarning } from '../icons.js';
const html = htm.bind(h);

// ── Module state ──────────────────────────────────────────────────────────────

let _sourceId = 0;
let _page = 1;
let _pageSize = 0;
let _query = '';
let _sourceName = '';
let _sourceEnabled = true;
let _activeTab = 'popular';
/** @type {AbortController | null} */
let _abort = null;
/** @type {AbortController | null} */
let _libAbort = null;
/** @type {(() => void) | null} */
let _destroyPaginationSearch = null;
/** @type {(() => void) | null} */
let _destroyPaginationPopular = null;
/** @type {(() => void) | null} */
let _destroyLibPagination = null;
/** @type {(() => void) | null} */
let _unsubSourcesInvalidation = null;
/** @type {IntersectionObserver | null} */
let _sentinelObserver = null;
/** @type {HTMLElement | null} */
let _settingsMountEl = null;
/** @type {HTMLElement | null} */
let _asideEl = null;
/** @type {HTMLButtonElement | null} */
let _addSourceBtn = null;
/** @type {HTMLElement | null} */
let _popularPanelEl = null;
/** @type {HTMLElement | null} */
let _searchPanelEl = null;
/** @type {((activeId: string) => void) | null} */
let _tabsUpdateFn = null;
let _settingsMounted = false;
let _libPage = 1;
let _libPageSize = 24;
/** @type {any[]} */
let _filterDefs = [];
/** @type {Record<string, string>} */
let _filters = {};
/** @type {(() => void) | null} */
let _filterModalDestroy = null;
/** @type {Record<string, string>} */
let _pendingFilterParams = {};

// ── Filter helpers ─────────────────────────────────────────────────────────────

/**
 * Normalize a filter state value from either adjacently-tagged ({kind, data}) or
 * externally-tagged ({Selection: data}) serde format to the adjacently-tagged form
 * expected by the filter panel: { kind: 'Selection'|'Checkbox'|'TextInput', data: ... }.
 * @param {any} raw
 * @returns {any}
 */
function _normalizeFilterState(raw) {
  if (!raw || typeof raw !== 'object') return raw;
  if (typeof raw.kind === 'string') return raw; // already adjacently-tagged
  // Externally-tagged: { Selection: {...} } → { kind: 'Selection', data: {...} }
  const entries = Object.entries(raw);
  if (entries.length === 1) return { kind: entries[0][0], data: entries[0][1] };
  return raw;
}

/**
 * Build an initial filter state from filter defaults.
 * @param {any[]} filterDefs
 * @returns {Record<string, any>}
 */
function _buildDefaultFilters(filterDefs) {
  /** @type {Record<string, any>} */
  const defaults = {};
  for (const f of filterDefs) {
    if (f.default_value) defaults[f.id] = _normalizeFilterState(f.default_value);
  }
  return defaults;
}

// ── Source settings page component ───────────────────────────────────────────

/**
 * Full-page settings panel for a single source, designed for the Settings tab.
 * On desktop, preferences are rendered inline. On mobile, they open in a modal.
 * @param {{
 *   source: any,
 *   activeIds: Set<number>,
 *   onDeleted: () => void,
 *   onEnabledChange?: (enabled: boolean) => void,
 * }} props
 */
function SourceSettingsPage({ source, activeIds, onDeleted, onEnabledChange }) {
  const sid = source.id;
  const isActive = activeIds.has(sid);

  const [enabled, setEnabled] = useState(source.enabled ?? false);
  const [confirming, setConfirming] = useState(false);



  const [schema, setSchema] = useState(/** @type {any[]} */ ([]));
  const [liveValues, setLiveValues] = useState(/** @type {Record<string,any>} */ ({}));
  const [prefsLoading, setPrefsLoading] = useState(true);
  const [modalOpen, setModalOpen] = useState(false);
  const [activeDescriptor, setActiveDescriptor] = useState(/** @type {any} */ (null));
  const [collapsedGroups, setCollapsedGroups] = useState(/** @type {Set<string>} */ (new Set()));

  // Load preferences on mount
  useEffect(() => {
    Promise.all([api.getPreferenceSchema(sid), api.getPreferences(sid)])
        .then(([schemaRes, prefsRes]) => {
            setSchema(Array.isArray(schemaRes) ? schemaRes : []);

            if (Array.isArray(prefsRes)) {
                const liveObject = Object.fromEntries(
                    prefsRes.map(p => [p.key, p.value])
                );
                setLiveValues(liveObject);
            } else {
                setLiveValues({});
            }
        })
        .catch(e => console.error('Failed to load prefs:', e))
        .finally(() => setPrefsLoading(false));
  }, [sid]);

  /** @type {Map<string, any[]>} */
  const groups = new Map();
  for (const d of schema) {
    const g = d.group ?? '';
    if (!groups.has(g)) groups.set(g, []);
    groups.get(g).push(d);
  }

  async function toggleEnabled(val) {
    if (val && source.unrestricted_http && !confirming) {
      setConfirming(true);
      return;
    }
    setConfirming(false);
    try {
      await api.toggleSourceEnabled(sid, val);
      setEnabled(val);
      onEnabledChange?.(val);
    } catch { /* revert on error */ }
  }

  async function handleDelete() {
    const confirmed = await confirmDialog({
      title: 'Delete source?',
      message: 'This will permanently delete this source extension. This cannot be undone.',
      confirmLabel: 'Delete',
      danger: true,
    });
    if (!confirmed) return;
    try {
      await api.deleteSource(sid);
      onDeleted();
    } catch (e) {
      console.error('Delete failed:', e);
    }
  }



  const _toggleGroup = (group) => setCollapsedGroups(prev => {
    const next = new Set(prev);
    next.has(group) ? next.delete(group) : next.add(group);
    return next;
  });

  const prefContent = prefsLoading
    ? html`<p class="text-sm text-text-muted py-2">Loading preferences…</p>`
    : activeDescriptor
      ? html`<${PreferenceDetailView}
          sourceId=${sid}
          descriptor=${activeDescriptor}
          currentValue=${liveValues[activeDescriptor.key]}
          liveValues=${liveValues}
          onValueChange=${(key, val) => setLiveValues(prev => ({ ...prev, [key]: val }))}
          onBack=${() => setActiveDescriptor(null)}
        />`
      : schema.length === 0
        ? html`<p class="text-sm text-text-muted py-4">No preferences available.</p>`
        : html`
          <div class="flex flex-col">
            ${[...groups.entries()].map(([group, descriptors]) => {
              const isCollapsed = collapsedGroups.has(group);
              return html`
                <div key=${group} class="flex flex-col">
                  ${group && html`
                    <button
                      class="flex items-center justify-between gap-2 py-2 w-full text-left text-xs font-semibold uppercase tracking-wider text-text-muted hover:text-text transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent rounded"
                      onClick=${() => _toggleGroup(group)}
                      aria-expanded=${!isCollapsed}
                    >
                      ${group}
                      <span class=${'icon-xs transition-transform ' + (isCollapsed ? '' : 'rotate-180')}>
                        <${Icon} svg=${iconChevronDown} />
                      </span>
                    </button>
                  `}
                  ${!isCollapsed && descriptors.map(d => html`
                    <${PreferenceRow}
                      key=${d.key}
                      sourceId=${sid}
                      descriptor=${d}
                      currentValue=${liveValues[d.key]}
                      liveValues=${liveValues}
                      onValueChange=${(key, val) => setLiveValues(prev => ({ ...prev, [key]: val }))}
                      onOpenDetail=${(desc) => setActiveDescriptor(desc)}
                    />
                  `)}
                </div>
              `;
            })}
          </div>
        `;

  /** @param {string} title @param {string} subtitle */
  const mkSectionHdr = (title, subtitle) => html`
    <div class="flex flex-col gap-0.5 pb-2 border-b border-border-subtle">
      <h2 class="text-sm font-semibold text-text">${title}</h2>
      <p class="text-xs text-text-muted">${subtitle}</p>
    </div>
  `;

  return html`
    <div class="flex flex-col gap-8">

      <!-- 1. General -->
      <div class="flex flex-col gap-3">
        ${mkSectionHdr('General', 'Configure basic source behaviour and status.')}
        <div class=${'bg-surface border rounded-xl px-4 md:px-6 py-1 ' + (source.unrestricted_http ? 'border-warn/50' : 'border-border')}>

          <div class="py-4 first:pt-3 last:pb-3 border-b border-border-subtle last:border-b-0">
            <div class="flex items-center justify-between gap-4">
              <div>
                <p class="text-sm font-medium text-text">Runtime status</p>
                <p class="text-xs text-text-muted mt-0.5">Whether the extension is currently loaded in memory</p>
              </div>
              <span class=${'shrink-0 inline-flex items-center px-2 py-0.5 text-xs font-medium rounded-full ' + (isActive ? 'bg-success/20 text-success' : 'bg-surface-2 text-text-muted')}>
                ${isActive ? 'Loaded' : 'Unloaded'}
              </span>
            </div>
          </div>

          <div class="py-4 first:pt-3 last:pb-3 border-b border-border-subtle last:border-b-0">
            <div class="flex items-center justify-between gap-4">
              <div>
                <p class="text-sm font-medium text-text">Enabled</p>
                <p class="text-xs text-text-muted mt-0.5">Allow this source to be used for browsing and searching</p>
              </div>
              <label class="kani-toggle shrink-0 cursor-pointer">
                <input
                  type="checkbox"
                  class="kani-toggle__input"
                  checked=${enabled}
                  aria-label=${enabled ? 'Disable source' : 'Enable source'}
                  onChange=${(e) => toggleEnabled(/** @type {HTMLInputElement} */ (e.target).checked)}
                />
                <span class="kani-toggle__track"></span>
              </label>
            </div>
            ${confirming && html`
              <div class="mt-3 rounded-lg bg-warn/10 border border-warn/30 p-3 flex flex-col gap-2">
                <p class="text-sm text-warn flex items-center gap-1.5">
                  <${Icon} svg=${iconWarning} />
                  This extension uses unrestricted HTTP. Only enable it if you trust the source.
                </p>
                <div class="flex items-center gap-2 justify-end">
                  <button class="btn-ghost btn-sm" onClick=${() => setConfirming(false)}>Cancel</button>
                  <button class="btn-danger btn-sm" onClick=${() => toggleEnabled(true)}>Enable Anyway</button>
                </div>
              </div>
            `}
          </div>

          ${source.unrestricted_http && html`
            <div class="py-4 first:pt-3 last:pb-3 border-b border-border-subtle last:border-b-0">
              <div class="flex items-center gap-1.5 text-warn icon-sm">
                <${Icon} svg=${iconWarning} />
                <p class="text-sm font-medium">Unrestricted HTTP</p>
              </div>
              <p class="text-xs text-text-muted mt-0.5">This extension can make arbitrary network requests. Only use it if you trust the source.</p>
            </div>
          `}

          <div class="py-4 first:pt-3 last:pb-3 border-b border-border-subtle last:border-b-0">
            <div class="flex items-center justify-between gap-4">
              <p class="text-sm font-medium text-text">Version</p>
              <span class="text-sm text-text-muted shrink-0">v${source.version ?? '?'}</span>
            </div>
          </div>

        </div>
      </div>

      <!-- 2. Preferences -->
    <div class="flex flex-col gap-3">
        ${mkSectionHdr('Preferences', 'Extension-specific settings and configuration options.')}

        <!-- Desktop: inline in card -->
        <div class="hidden md:block bg-surface border border-border rounded-xl px-4 md:px-6 py-1">
          ${prefContent}
        </div>

        <!-- Mobile: configure button + modal -->
        <div class="md:hidden">
            <div class="bg-surface border border-border rounded-xl px-4 md:px-6 py-1 flex flex-col divide-y divide-border-subtle">
                <div class="py-4 first:pt-3 last:pb-3">
                    <div class="flex items-start justify-between gap-4 flex-wrap">
                        <div>
                            <p class="text-sm font-medium text-text">Configure Extension Preferences</p>
                            <p class="text-xs text-text-muted mt-0.5">Configure the internal preferences exposed by the extension.</p>
                        </div>
                        <button class="btn-ghost" onClick=${() => setModalOpen(true)}>Configure preferences</button>
                    </div>
                    <${Modal} open=${modalOpen} onClose=${() => setModalOpen(false)} title="Extension Preferences">
                        <div class="flex flex-col divide-y divide-border-subtle">
                            ${prefContent}
                        </div>
                    </${Modal}>
                </div>
            </div>
        </div>
    </div>


      <!-- 4. Danger Zone -->
      <div class="flex flex-col gap-3">
        ${mkSectionHdr('Danger Zone', 'These actions are difficult or impossible to reverse. Proceed with care.')}
        <div class="bg-surface border border-border rounded-xl px-4 md:px-6 py-1">
          <div class="py-4 first:pt-3 last:pb-3 border-b border-border-subtle last:border-b-0">
            <div class="flex items-center justify-between gap-4">
              <div>
                <p class="text-sm font-medium text-text">Delete source</p>
                <p class="text-xs text-text-muted mt-0.5">Permanently removes this source extension</p>
              </div>
              <button class="btn-danger btn-sm shrink-0" onClick=${handleDelete}>Delete</button>
            </div>
          </div>
        </div>
      </div>

    </div>
  `;
}

// ── URL state ─────────────────────────────────────────────────────────────────

function _updateUrl() {
  const params = new URLSearchParams();
  params.set('tab', _activeTab);
  if (_page > 1) params.set('page', String(_page));
  if (_query) params.set('q', _query);
  for (const [filterId, state] of Object.entries(_filters)) {
    params.set('f_' + filterId, JSON.stringify(state));
  }
  const qs = params.toString();
  history.replaceState(null, '', location.pathname + (qs ? '?' + qs : ''));
}

// ── Breadcrumb ────────────────────────────────────────────────────────────────

function _updateBreadcrumb() {
  const crumbs = [{ label: 'Sources', href: '/sources' }];
  if (_query) {
    crumbs.push({ label: _sourceName || 'Source', href: `/source/${_sourceId}` });
    crumbs.push({ label: `Search: ${_query}` });
  } else {
    crumbs.push({ label: _sourceName || 'Source' });
  }
  setPageHeader({ crumbs, actions: _addSourceBtn ?? null });
}

// ── Init ──────────────────────────────────────────────────────────────────────

/**
 * @param {HTMLElement} container
 * @param {{ id: string }} params
 */
export async function init(container, { id }) {
  _sourceId = Number(id);
  _page = 1;
  const _urlParams = new URLSearchParams(location.search);
  _query = _urlParams.get('q') ?? '';
  _pageSize = getLocalInt('kani_source_page_size', 18);
  _sourceName = '';
  _sourceEnabled = true;
  _settingsMountEl = null;
  _popularPanelEl = null;
  _searchPanelEl = null;
  _tabsUpdateFn = null;
  _settingsMounted = false;
  _filterDefs = [];
  _filterModalDestroy?.();
  _filterModalDestroy = null;

  // If a query or filter arrives from a semantic link, open directly to Search tab
  const _preFilterName  = _urlParams.get('filter_name');
  const _preFilterValue = _urlParams.get('filter_value');
  _filters = (_preFilterName && _preFilterValue)
    ? { [_preFilterName]: _preFilterValue }
    : {};

  // Restore tab from URL (takes precedence over query-based heuristic)
  const _tabParam = _urlParams.get('tab');
  if (_tabParam && ['popular', 'search', 'library', 'settings'].includes(_tabParam)) {
    _activeTab = _tabParam;
  } else {
    _activeTab = (_query || _preFilterName) ? 'search' : 'popular';
  }

  // Restore page number from URL
  const _pageParam = _urlParams.get('page');
  if (_pageParam) _page = Math.max(1, parseInt(_pageParam, 10) || 1);

  // Stash f_* filter params for async restoration after filter defs load
  _pendingFilterParams = {};
  for (const [key, value] of _urlParams.entries()) {
    if (key.startsWith('f_')) _pendingFilterParams[key.slice(2)] = value;
  }

  if (!hasPermission('source:browse')) {
    container.innerHTML = '';
    container.appendChild(createErrorState({ message: 'You do not have permission to browse sources.' }));
    return;
  }

  // Add source button (shown in header for consistency with sources page)
  if (hasPermission('source:install')) {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'btn-primary btn-sm';
    btn.textContent = 'Add source';
    _addSourceBtn = btn;
  } else {
    _addSourceBtn = null;
  }

  container.innerHTML = `
    <div class="flex">

      <!-- Sidebar (lg+) — SourcesSidebar mounts here -->
      <aside
        class="hidden lg:flex flex-col w-72 shrink-0 border-r border-border-subtle sticky overflow-y-auto"
        style="top:var(--header-h);height:calc(100vh - var(--header-h));"
        aria-label="Sources"
      ></aside>

      <!-- Main panel -->
      <div class="flex-1 min-w-0 flex flex-col">

        <div class="flex-1 max-w-page w-full px-4 md:px-6 py-4 md:pt-6 md:pb-0 flex flex-col gap-4">
          <!-- Tab bar -->
          <div class="js-tabs"></div>

          <!-- Popular panel -->
          <div class="js-panel" data-panel="popular">
            <div class="flex flex-col gap-4">
              <div class="flex items-end justify-end gap-2">
                <select class="input w-20 js-popular-page-size" aria-label="Page size">
                  ${[9, 18, 27].map(n => `<option value="${n}"${n === _pageSize ? ' selected' : ''}>${n}</option>`).join('')}
                </select>
              </div>
              <div class="js-popular-grid" aria-live="polite" aria-busy="false"></div>
              <div class="js-popular-pagination"></div>
            </div>
          </div>

          <!-- Search panel (hidden initially) -->
          <div class="js-panel hidden" data-panel="search">
            <div class="flex flex-col gap-4">
              <div class="flex items-center gap-3 flex-wrap">
                <div class="relative flex-1 min-w-48 max-w-sm">
                  <span class="absolute left-3 top-1/2 -translate-y-1/2 text-text-muted pointer-events-none icon-sm" aria-hidden="true">${iconSearch}</span>
                  <input
                    type="search"
                    class="input w-full pl-9 js-search"
                    placeholder="Search manga…"
                    aria-label="Search manga in this source"
                  />
                </div>
                <select class="input w-20 js-page-size" aria-label="Page size">
                  ${[9, 18, 27].map(n => `<option value="${n}"${n === _pageSize ? ' selected' : ''}>${n}</option>`).join('')}
                </select>
                <button type="button" class="js-filter-btn btn-ghost btn-sm flex items-center gap-1.5" aria-label="Open filters" style="display:none">Filters</button>
              </div>
              <div class="js-search-grid" aria-live="polite" aria-busy="false"></div>
              <div class="js-search-pagination"></div>
            </div>
          </div>

          <!-- Library panel (hidden initially) -->
          <div class="js-panel hidden" data-panel="library">
            <div class="flex flex-col gap-4">
              <div class="js-lib-grid" aria-live="polite" aria-busy="false"></div>
              <div class="js-lib-pagination"></div>
            </div>
          </div>

          <!-- Settings panel (hidden initially) -->
          <div class="js-panel hidden" data-panel="settings">
            <div class="js-settings-mount flex flex-col gap-4"></div>
          </div>
        </div>
      </div>
    </div>
  `;

  _asideEl = /** @type {HTMLElement} */ (container.querySelector('aside'));
  _settingsMountEl = /** @type {HTMLElement} */ (container.querySelector('.js-settings-mount'));
  _popularPanelEl  = /** @type {HTMLElement} */ (container.querySelector('[data-panel="popular"]'));
  _searchPanelEl   = /** @type {HTMLElement} */ (container.querySelector('[data-panel="search"]'));

  const popularGridEl  = /** @type {HTMLElement} */ (container.querySelector('.js-popular-grid'));
  const popularPaginEl = /** @type {HTMLElement} */ (container.querySelector('.js-popular-pagination'));
  const popularSizeEl  = /** @type {HTMLSelectElement} */ (container.querySelector('.js-popular-page-size'));
  const searchGridEl   = /** @type {HTMLElement} */ (container.querySelector('.js-search-grid'));
  const searchPaginEl  = /** @type {HTMLElement} */ (container.querySelector('.js-search-pagination'));
  const filterBtnEl    = /** @type {HTMLButtonElement} */ (container.querySelector('.js-filter-btn'));
  const searchEl       = /** @type {HTMLInputElement} */ (container.querySelector('.js-search'));
  const searchSizeEl   = /** @type {HTMLSelectElement} */ (container.querySelector('.js-page-size'));
  const libGridEl      = /** @type {HTMLElement} */ (container.querySelector('.js-lib-grid'));
  const libPaginEl     = /** @type {HTMLElement} */ (container.querySelector('.js-lib-pagination'));

  // ── Breadcrumb + header ──
  _updateBreadcrumb();

  // Wire add source button
  if (_addSourceBtn) {
    let _addSourceModalOpen = false;
    const _setAddOpen = (open) => {
      _addSourceModalOpen = open;
      mountIntoModalRoot(html`
        <${AddSourceModal}
          open=${_addSourceModalOpen}
          onClose=${() => _setAddOpen(false)}
          onCreated=${() => { _setAddOpen(false); _refreshSidebar(); }}
        />
      `);
    };
    _addSourceBtn.addEventListener('click', () => _setAddOpen(true));
  }

  if (_query) searchEl.value = _query;

  // ── Disabled subpage helper ──
  /** Shows a "disabled" placeholder in a panel, replacing its content. */
  function _showDisabledPanel(panelEl) {
    const inner = panelEl.querySelector('.flex.flex-col');
    if (!inner) return;
    inner.innerHTML = `
      <div class="flex flex-col items-center justify-center py-20 gap-4 text-center">
        <span class="icon-xl text-warn">${iconWarning}</span>
        <div>
          <p class="text-sm font-medium text-text">This extension is disabled</p>
          <p class="text-xs text-text-muted mt-1">Enable it in the Settings tab to browse manga.</p>
        </div>
      </div>
    `;
  }

  // ── Tab switching ──
  const panels = /** @type {NodeListOf<HTMLElement>} */ (container.querySelectorAll('.js-panel'));

  let _popularFetched = false;

  const _switchTab = (/** @type {string} */ tab) => {
    _activeTab = tab;
    _tabsUpdateFn?.(tab);
    for (const panel of panels) {
      panel.classList.toggle('hidden', panel.dataset.panel !== tab);
    }
    if (tab === 'settings') _mountSettings();
    if (tab === 'library') _fetchLibrary(libGridEl, libPaginEl);
    if (tab === 'popular' && !_popularFetched) {
      _popularFetched = true;
      if (!_sourceEnabled) {
        _showDisabledPanel(/** @type {HTMLElement} */ (container.querySelector('[data-panel="popular"]')));
      } else {
        _fetch(popularGridEl, popularPaginEl, false);
      }
    }
    if (tab === 'search' && !_sourceEnabled) {
      _showDisabledPanel(/** @type {HTMLElement} */ (container.querySelector('[data-panel="search"]')));
    }
  };

  // ── Tab bar ──
  const tabsEl = /** @type {HTMLElement} */ (container.querySelector('.js-tabs'));
  const { update: tabsUpdate } = renderTabs(tabsEl, {
    tabs: [
      { id: 'popular', name: 'Popular' },
      { id: 'search', name: 'Search' },
      { id: 'library', name: 'Library' },
      { id: 'settings', name: 'Settings' },
    ],
    activeId: _activeTab,
    onSelect: (tab) => { _switchTab(tab); _updateUrl(); },
  });
  _tabsUpdateFn = tabsUpdate;

  _switchTab(_activeTab);

  // If search tab is initial tab (arriving from URL with query/filter), fetch immediately
  if (_activeTab === 'search') {
    _fetch(searchGridEl, searchPaginEl, true);
  }

  // ── Search tab events ──
  searchEl.addEventListener('input', debounce(() => {
    _query = searchEl.value.trim();
    _page = 1;
    _updateBreadcrumb();
    _updateUrl();
    _fetch(searchGridEl, searchPaginEl, true);
  }, 600));

  searchSizeEl.addEventListener('change', () => {
    _pageSize = Number(searchSizeEl.value);
    setLocal('kani_source_page_size', String(_pageSize));
    _page = 1;
    _updateUrl();
    _fetch(searchGridEl, searchPaginEl, true);
  });

  // ── Popular tab page size ──
  popularSizeEl.addEventListener('change', () => {
    _pageSize = Number(popularSizeEl.value);
    setLocal('kani_source_page_size', String(_pageSize));
    _page = 1;
    _popularFetched = false; // force re-fetch
    _updateUrl();
    _fetch(popularGridEl, popularPaginEl, false);
    _popularFetched = true;
  });

  // ── Filter modal — shown in Search tab, hidden until filter defs load ──
  _filterDefs = [];
  _filterModalDestroy?.();
  _filterModalDestroy = null;
  api.getSourceFilters(_sourceId).then(fl => {
    const defs = Array.isArray(fl?.filters) ? fl.filters : [];
    if (defs.length === 0) return;
    _filterDefs = defs;

    // Apply pending URL filter params (from back/forward navigation)
    if (Object.keys(_pendingFilterParams).length > 0) {
      const merged = _buildDefaultFilters(_filterDefs);
      for (const [filterId, value] of Object.entries(_pendingFilterParams)) {
        try { merged[filterId] = JSON.parse(value); } catch { /* skip malformed */ }
      }
      _filters = merged;
      _pendingFilterParams = {};
      // Re-fetch on search tab with the restored filter state
      if (_activeTab === 'search') _fetch(searchGridEl, searchPaginEl, true);
    }

    filterBtnEl.style.display = '';
    _filterModalDestroy = mountFilterModal(filterBtnEl, document.body, {
      filterDefs: _filterDefs,
      activeFilters: _filters,
      onChange: (updated) => {
        _filters = updated;
        _page = 1;
        _updateUrl();
        _fetch(searchGridEl, searchPaginEl, true);
      },
    });
  }).catch(() => {});

  // ── Source name + title ──
  document.title = 'Source - Kani';
  api.getSource(_sourceId).then(src => {
    if (src?.name) {
      _sourceName = src.name;
      document.title = src.name + ' - Kani';
      _updateBreadcrumb();
    }
    if (src && src.enabled === false) {
      _sourceEnabled = false;
      // If currently on popular or search panel, replace with disabled state
      if (_activeTab === 'popular') {
        _popularFetched = false;
        _showDisabledPanel(/** @type {HTMLElement} */ (container.querySelector('[data-panel="popular"]')));
        _popularFetched = true;
      } else if (_activeTab === 'search') {
        _showDisabledPanel(/** @type {HTMLElement} */ (container.querySelector('[data-panel="search"]')));
      }
    }
  }).catch(() => {});

  // ── Sidebar component ──
  let _sidebarSources = /** @type {any[]} */ ([]);

  function _mountSidebar() {
    render(html`<${SourcesSidebar}
      sources=${_sidebarSources}
      activeSourceId=${_sourceId}
      canInstall=${hasPermission('source:install')}
      onCreated=${_refreshSidebar}
    />`, _asideEl);
  }

  async function _refreshSidebar() {
    try {
      const updated = await api.getSources();
      if (Array.isArray(updated)) {
        _sidebarSources = updated;
        _mountSidebar();
      }
    } catch { /* ignore */ }
  }

  api.getSources().then(sources => {
    _sidebarSources = Array.isArray(sources) ? sources : [];
    _mountSidebar();
  }).catch(() => { _mountSidebar(); });

  // Re-fetch sidebar when any source is enabled/disabled (e.g. from Settings tab)
  _unsubSourcesInvalidation = subscribe('sourcesInvalidation', _refreshSidebar);
}

// ── Settings tab ──────────────────────────────────────────────────────────────

async function _mountSettings() {
  if (_settingsMounted || !_settingsMountEl) return;
  _settingsMounted = true;

  const [sourceRes, activeIdsRes] = await Promise.allSettled([
    api.getSource(_sourceId),
    api.getActiveSourceIds(),
  ]);

  const source    = sourceRes.status === 'fulfilled' ? sourceRes.value : null;
  const activeIds = new Set(activeIdsRes.status === 'fulfilled' && Array.isArray(activeIdsRes.value)
    ? activeIdsRes.value
    : []);

  if (!source) {
    _settingsMountEl.appendChild(createErrorState({ message: 'Failed to load source settings.' }));
    return;
  }

  render(
    html`<${SourceSettingsPage}
      source=${source}
      activeIds=${activeIds}
      onDeleted=${() => navigate('/sources')}
      onEnabledChange=${(enabled) => {
        _sourceEnabled = enabled;
        // Notify sidebar to re-fetch so the "Off" badge updates immediately
        updateState('sourcesInvalidation', n => n + 1);
        // Refresh popular/search panels immediately
        if (_popularPanelEl) {
          const inner = _popularPanelEl.querySelector('.flex.flex-col');
          if (inner) {
            if (enabled) {
              inner.innerHTML = '';
              const gridEl = document.createElement('div');
              gridEl.className = 'js-popular-grid-dyn';
              gridEl.setAttribute('aria-live', 'polite');
              gridEl.setAttribute('aria-busy', 'false');
              const paginEl = document.createElement('div');
              paginEl.className = 'js-popular-pagination-dyn';
              inner.appendChild(gridEl);
              inner.appendChild(paginEl);
              _fetch(gridEl, paginEl, false);
            } else {
              inner.innerHTML = `
                <div class="flex flex-col items-center justify-center py-20 gap-4 text-center">
                  <span class="icon-xl text-warn">${iconWarning}</span>
                  <div>
                    <p class="text-sm font-medium text-text">This extension is disabled</p>
                    <p class="text-xs text-text-muted mt-1">Enable it in the Settings tab to browse manga.</p>
                  </div>
                </div>
              `;
            }
          }
        }
        if (_searchPanelEl) {
          const inner = _searchPanelEl.querySelector('.flex.flex-col');
          if (inner && !enabled) {
            // Clear grid content but keep search bar; show disabled message in grid area
            const gridEl = inner.querySelector('.js-search-grid');
            if (gridEl) {
              gridEl.innerHTML = `
                <div class="flex flex-col items-center justify-center py-20 gap-4 text-center">
                  <span class="icon-xl text-warn">${iconWarning}</span>
                  <div>
                    <p class="text-sm font-medium text-text">This extension is disabled</p>
                    <p class="text-xs text-text-muted mt-1">Enable it in the Settings tab to browse manga.</p>
                  </div>
                </div>
              `;
            }
          }
        }
      }}
    />`,
    _settingsMountEl,
  );
}

// ── Library tab ───────────────────────────────────────────────────────────────

let _libLoaded = false;

/** @param {HTMLElement} gridEl @param {HTMLElement} paginEl */
async function _fetchLibrary(gridEl, paginEl) {
  if (_libLoaded) return; // don't re-fetch on re-switch to library tab (page changes clear this flag)
  _libLoaded = true;

  _libAbort?.abort();
  _libAbort = new AbortController();
  _destroyLibPagination?.();
  _destroyLibPagination = null;
  paginEl.innerHTML = '';
  gridEl.innerHTML = skeletonGrid(_libPageSize);
  gridEl.setAttribute('aria-busy', 'true');
  startLoading();

  let result;
  try {
    result = await api.getLibrary({
      page: _libPage,
      page_size: _libPageSize,
      source_id: _sourceId,
    }, _libAbort.signal);
  } catch (e) {
    if (e?.name === 'AbortError') return;
    gridEl.innerHTML = '';
    gridEl.setAttribute('aria-busy', 'false');
    finishLoading();
    gridEl.appendChild(createErrorState({ message: 'Failed to load library.' }));
    return;
  }

  finishLoading();
  gridEl.innerHTML = '';
  gridEl.setAttribute('aria-busy', 'false');

  const items = Array.isArray(result?.items) ? result.items
    : Array.isArray(result?.manga)            ? result.manga
    : Array.isArray(result)                   ? result
    : [];

  if (items.length === 0) {
    gridEl.appendChild(createEmptyState({
      icon: iconSearch,
      title: 'No library manga from this source.',
    }));
    return;
  }

  const grid = document.createElement('div');
  grid.className = 'manga-grid';
  for (const m of items) {
    grid.appendChild(createMangaCard({
      manga: { id: m.id, title: m.title, cover_image_url: m.cover_url ?? null },
      href: `/manga/${m.id}?from_source=${_sourceId}`,
    }));
  }
  gridEl.appendChild(grid);

  const hasNext = hasNextPage(result, items.length, _libPageSize);
  if (_libPage > 1 || hasNext) {
    const { destroy } = renderPagination(paginEl, {
      page: _libPage,
      hasNext,
      total: result?.total_pages ?? undefined,
      onPageChange: (p) => { _libPage = p; _libLoaded = false; _fetchLibrary(gridEl, paginEl); window.scrollTo(0, 0); },
    });
    _destroyLibPagination = destroy;
  }
}

// ── Fetch ─────────────────────────────────────────────────────────────────────

/**
 * @param {HTMLElement} gridEl
 * @param {HTMLElement} paginEl
 * @param {boolean} isSearch - true = search_manga (query+filters), false = get_popular_manga
 */
async function _fetch(gridEl, paginEl, isSearch) {
  const infinite = getLocal('kani_source_pagination') === 'infinite';
  const isAppend = infinite && _page > 1;

  _abort?.abort();
  _abort = new AbortController();

  _sentinelObserver?.disconnect();
  _sentinelObserver = null;
  if (isSearch) {
    _destroyPaginationSearch?.();
    _destroyPaginationSearch = null;
  } else {
    _destroyPaginationPopular?.();
    _destroyPaginationPopular = null;
  }
  paginEl.innerHTML = '';

  if (isAppend) {
    paginEl.innerHTML = '<div class="h-14 mx-3 my-2 skeleton rounded-lg"></div>';
  } else {
    gridEl.innerHTML = skeletonGrid(_pageSize);
    gridEl.setAttribute('aria-busy', 'true');
    gridEl.classList.add('opacity-50', 'pointer-events-none');
  }
  startLoading();

  const filtersJson = Object.keys(_filters).length > 0
    ? JSON.stringify(
        Object.entries(_filters).map(([id, stateObj]) => ({
        filter_name: id,
        state: stateObj
        }))
    )
    : undefined;

  let result;
  try {
    result = isSearch
      ? await api.searchManga(_sourceId, _query, _page, _pageSize, filtersJson, _abort.signal)
      : await api.getPopularManga(_sourceId, _page, _pageSize, undefined, _abort.signal);
  } catch (e) {
    if (e?.name === 'AbortError') return;
    paginEl.innerHTML = '';
    if (!isAppend) {
      gridEl.innerHTML = '';
      gridEl.setAttribute('aria-busy', 'false');
      gridEl.classList.remove('opacity-50', 'pointer-events-none');
      finishLoading();
      gridEl.appendChild(createErrorState({ message: 'Failed to load manga.' }));
    }
    return;
  }

  finishLoading();
  paginEl.innerHTML = '';
  if (!isAppend) {
    gridEl.innerHTML = '';
    gridEl.setAttribute('aria-busy', 'false');
    gridEl.classList.remove('opacity-50', 'pointer-events-none');
  }

  const items = Array.isArray(result?.manga) ? result.manga
    : Array.isArray(result)                  ? result
    : [];

  if (items.length === 0 && !isAppend) {
    gridEl.appendChild(createEmptyState({
      icon: iconSearch,
      title: _query ? 'No results found.' : 'No manga available.',
    }));
  } else if (items.length > 0) {
    if (infinite) {
      let grid = /** @type {HTMLElement|null} */ (gridEl.querySelector('.manga-grid'));
      if (!grid) {
        grid = document.createElement('div');
        grid.className = 'manga-grid';
        gridEl.appendChild(grid);
      }
      for (const m of items) {
        grid.appendChild(createMangaCard({
          manga: { id: m.db_id ?? m.id, title: m.title, cover_image_url: m.cover_url ?? null },
          href: `/source/${_sourceId}/manga/${encodeURIComponent(m.source_manga_id ?? m.id)}`,
        }));
      }
    } else {
      const grid = document.createElement('div');
      grid.className = 'manga-grid';
      for (const m of items) {
        grid.appendChild(createMangaCard({
          manga: { id: m.db_id ?? m.id, title: m.title, cover_image_url: m.cover_url ?? null },
          href: `/source/${_sourceId}/manga/${encodeURIComponent(m.source_manga_id ?? m.id)}`,
        }));
      }
      gridEl.appendChild(grid);
    }
  }

  const hasNext = hasNextPage(result, items.length, _pageSize);
  if (infinite) {
    _setupSourceSentinel(gridEl, paginEl, hasNext, isSearch);
  } else if (_page > 1 || hasNext) {
    const { destroy } = renderPagination(paginEl, {
      page: _page,
      hasNext,
      total: result?.total_pages ?? undefined,
      onPageChange: (p) => { _page = p; _updateUrl(); _fetch(gridEl, paginEl, isSearch); window.scrollTo(0, 0); },
    });
    if (isSearch) {
      _destroyPaginationSearch = destroy;
    } else {
      _destroyPaginationPopular = destroy;
    }
  }
}

/** Sets up (or clears) the IntersectionObserver sentinel for source-details infinite scroll. */
function _setupSourceSentinel(gridEl, paginEl, hasNext, isSearch) {
  _sentinelObserver?.disconnect();
  _sentinelObserver = null;
  if (!hasNext) return;

  const sentinel = document.createElement('div');
  sentinel.className = 'js-sentinel h-px';
  paginEl.appendChild(sentinel);

  _sentinelObserver = new IntersectionObserver((entries) => {
    if (entries[0].isIntersecting) {
      _sentinelObserver?.disconnect();
      _sentinelObserver = null;
      _page++;
      _updateUrl();
      _fetch(gridEl, paginEl, isSearch);
    }
  }, { rootMargin: '200px' });
  _sentinelObserver.observe(sentinel);
}

// ── Destroy ───────────────────────────────────────────────────────────────────

/** @param {HTMLElement} container */
export function destroy(container) {
  _abort?.abort();
  _abort = null;
  _libAbort?.abort();
  _libAbort = null;
  _destroyPaginationSearch?.();
  _destroyPaginationSearch = null;
  _destroyPaginationPopular?.();
  _destroyPaginationPopular = null;
  _destroyLibPagination?.();
  _destroyLibPagination = null;
  _sentinelObserver?.disconnect();
  _sentinelObserver = null;
  if (_settingsMountEl) render(null, _settingsMountEl);
  _settingsMountEl = null;
  _settingsMounted = false;
  _libLoaded = false;
  _libPage = 1;
  _popularPanelEl = null;
  _searchPanelEl = null;
  _tabsUpdateFn = null;
  if (_asideEl) render(null, _asideEl);
  _asideEl = null;
  _filterModalDestroy?.();
  _filterModalDestroy = null;
  _unsubSourcesInvalidation?.();
  _unsubSourcesInvalidation = null;
  mountIntoModalRoot(null);
  _addSourceBtn = null;
  clearPageHeader();
  const pendingId = consumePendingSourceId();
  if (pendingId !== null) api.deleteSource(pendingId).catch(() => {});
  container.innerHTML = '';
}