// @ts-check
// Sources page — sidebar list of sources (desktop), full list (mobile).

import * as api from '../api.js';
import { deferredSkeleton } from '../utils.js';
import { hasPermission } from '../session.js';
import { skeletonSourceList } from '../components/skeletons.js';
import { createErrorState } from '../components/error-state.js';
import { iconCube } from '../icons.js';
import { h, render } from 'preact';
import htm from 'htm';
import { mountIntoModalRoot } from '../components/modal.js';
import { SourcesSidebar, AddSourceModal, consumePendingSourceId } from '../components/sources-sidebar.js';
import { setPageHeader, clearPageHeader } from '../components/app-header.js';
import { mountRepoManager } from '../components/repo-manager.js';
import { t } from '../i18n.js';
const html = htm.bind(h);

/** @type {HTMLElement | null} */
let _asideEl = null;
/** @type {HTMLElement | null} */
let _mobileEl = null;
/** @type {{ destroy: () => void } | null} */
let _repoManager = null;

/** @param {HTMLElement} container */
export async function init(container) {
  document.title = 'Sources - Kani';

  if (!hasPermission('source:browse')) {
    setPageHeader({ crumbs: [{ label: t('sources.crumb') }] });
    container.innerHTML = '';
    container.appendChild(createErrorState({ message: 'You do not have permission to browse sources.' }));
    return;
  }

  const canInstall = hasPermission('source:install');

  const _tabsEl = document.createElement('div');
  _tabsEl.className = 'flex gap-1';

  const _sourcesTabBtn = document.createElement('button');
  _sourcesTabBtn.type = 'button';
  _sourcesTabBtn.className = 'btn-ghost btn-sm';
  _sourcesTabBtn.textContent = t('sources.tab.extensions');

  const _reposTabBtn = document.createElement('button');
  _reposTabBtn.type = 'button';
  _reposTabBtn.className = 'btn-ghost btn-sm';
  _reposTabBtn.textContent = t('repo.tab');

  _tabsEl.appendChild(_sourcesTabBtn);
  _tabsEl.appendChild(_reposTabBtn);

  const _addSourceBtn = (() => {
    if (!canInstall) return undefined;
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'btn-primary btn-sm';
    btn.textContent = t('source.add.title');
    return btn;
  })();

  // Passed as separate actions (not one wrapper) so the header can collapse the
  // tab switcher into its kebab on narrow screens and keep "Add source" visible.
  const _actions = /** @type {HTMLElement[]} */ ([_tabsEl]);
  if (_addSourceBtn) _actions.push(_addSourceBtn);

  setPageHeader({ crumbs: [{ label: t('sources.crumb') }], actions: _actions });

  container.innerHTML = `
    <!-- Sources view -->
    <div class="js-sources-view flex">

      <!-- Sidebar (lg+) — SourcesSidebar mounts here -->
      <aside
        class="hidden lg:flex flex-col w-72 shrink-0 border-r border-border-subtle sticky overflow-y-auto"
        style="top:var(--header-h);height:calc(100dvh - var(--header-h));"
        aria-label="${t('sources.crumb')}"
      ></aside>

      <!-- Content -->
      <div class="flex-1 min-w-0">

        <!-- Mobile source list (hidden on lg+) — same component as the sidebar -->
        <div class="js-mobile-sources lg:hidden max-w-page mx-auto w-full px-2 sm:px-4 md:px-6 py-2 md:pt-4" aria-live="polite"></div>

        <!-- Desktop "select a source" prompt -->
        <div class="hidden lg:flex flex-col items-center justify-center min-h-96 gap-3 text-text-muted">
          <span class="icon-xl opacity-30" aria-hidden="true">${iconCube}</span>
          <p class="text-sm">${t('source.select_prompt')}</p>
        </div>

      </div>
    </div>

    <!-- Repos view (hidden initially). Flex column with a bounded height so the
         repo-manager master-detail stretches its list pane to the bottom. -->
    <div class="js-repos-view hidden flex flex-col h-full min-h-0"></div>
  `;

  const sourcesView = /** @type {HTMLElement} */ (container.querySelector('.js-sources-view'));
  const reposView = /** @type {HTMLElement} */ (container.querySelector('.js-repos-view'));

  function _switchTab(tab) {
    _sourcesTabBtn.classList.toggle('bg-surface-2', tab === 'extensions');
    _reposTabBtn.classList.toggle('bg-surface-2', tab === 'repos');

    if (tab === 'repos') {
      _addSourceBtn?.classList.add('hidden');
      sourcesView.classList.add('hidden');
      reposView.classList.remove('hidden');
      if (!_repoManager) {
        _repoManager = mountRepoManager(reposView);
      }
    } else {
      _addSourceBtn?.classList.remove('hidden');
      reposView.classList.add('hidden');
      sourcesView.classList.remove('hidden');
    }
  }

  _sourcesTabBtn.addEventListener('click', () => _switchTab('extensions'));
  _reposTabBtn.addEventListener('click', () => _switchTab('repos'));
  _switchTab('extensions');

  _asideEl = /** @type {HTMLElement} */ (container.querySelector('aside'));
  _mobileEl = /** @type {HTMLElement} */ (container.querySelector('.js-mobile-sources'));

  let allSources = /** @type {any[]} */ ([]);

  // Show skeleton only if sources take > 150 ms to load
  const cancelSkeleton = deferredSkeleton(() => { if (_mobileEl) _mobileEl.innerHTML = skeletonSourceList(5); });


  /** Mounts/updates the source list into both the desktop aside and the mobile slot. */
  function _mountSourceList() {
    const vnode = html`<${SourcesSidebar} sources=${allSources} onCreated=${_refresh} />`;
    if (_asideEl)  render(vnode, _asideEl);
    if (_mobileEl) render(html`<${SourcesSidebar} sources=${allSources} onCreated=${_refresh} />`, _mobileEl);
  }


  async function _refresh() {
    try {
      const updated = await api.getSources();
      if (Array.isArray(updated)) {
        allSources = updated;
        _mountSourceList();
      }
    } catch { }
  }

  let sources;
  try {
    sources = await api.getSources();
  } catch {
    cancelSkeleton();
    if (_mobileEl) {
      _mobileEl.innerHTML = '';
      _mobileEl.appendChild(createErrorState({ message: t('sources.error.load'), onRetry: () => init(container) }));
    }
    return;
  }

  cancelSkeleton();
  allSources = Array.isArray(sources) ? sources : [];
  _mountSourceList();


  // Mount once per click and close via the returned cleanup — mountIntoModalRoot
  // gives each call its own container, so re-mounting with open=false would stack
  // an empty modal instead of closing the open one.
  if (canInstall && _addSourceBtn) {
    _addSourceBtn.addEventListener('click', () => {
      let cleanup = () => {};
      cleanup = mountIntoModalRoot(html`
        <${AddSourceModal}
          open=${true}
          onClose=${() => cleanup()}
          onCreated=${() => { cleanup(); _refresh(); }}
        />
      `);
    });
  }
}

/** @param {HTMLElement} container */
export function destroy(container) {
  clearPageHeader();
  const pendingId = consumePendingSourceId();
  if (pendingId !== null) api.deleteSource(pendingId).catch(() => {});
  if (_asideEl)  render(null, _asideEl);
  if (_mobileEl) render(null, _mobileEl);
  _asideEl = null;
  _mobileEl = null;
  _repoManager?.destroy();
  _repoManager = null;
  mountIntoModalRoot(null);
  container.innerHTML = '';
}
