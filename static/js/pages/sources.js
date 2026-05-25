// @ts-check
// Sources page — sidebar list of sources (desktop), full list (mobile).

import * as api from '../api.js';
import { escapeHtml, deferredSkeleton } from '../utils.js';
import { hasPermission } from '../state.js';
import { skeletonSourceList } from '../components/skeletons.js';
import { createErrorState } from '../components/error-state.js';
import { createEmptyState } from '../components/empty-state.js';
import { iconCube, iconStarFilled, iconStarOutline } from '../icons.js';
import { h, render } from 'preact';
import htm from 'htm';
import { mountIntoModalRoot } from '../components/modal.js';
import { SourcesSidebar, AddSourceModal, consumePendingSourceId } from '../components/sources-sidebar.js';
import { setPageHeader, clearPageHeader } from '../components/app-header.js';
const html = htm.bind(h);

/** @type {HTMLElement | null} */
let _asideEl = null;

/** @param {HTMLElement} container */
export async function init(container) {
  document.title = 'Sources - Kani';

  const _headerActions = (() => {
    if (!hasPermission('source:install')) return undefined;
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'btn-primary btn-sm';
    btn.textContent = 'Add source';
    return btn;
  })();
  setPageHeader({ crumbs: [{ label: 'Sources' }], actions: _headerActions });

  if (!hasPermission('source:browse')) {
    container.innerHTML = '';
    container.appendChild(createErrorState({ message: 'You do not have permission to browse sources.' }));
    return;
  }

  container.innerHTML = `
    <div class="flex">

      <!-- Sidebar (lg+) — SourcesSidebar mounts here -->
      <aside
        class="hidden lg:flex flex-col w-72 shrink-0 border-r border-border-subtle sticky overflow-y-auto"
        style="top:var(--header-h);height:calc(100vh - var(--header-h));"
        aria-label="Sources"
      ></aside>

      <!-- Content -->
      <div class="flex-1 min-w-0">

        <!-- Mobile source list (hidden on lg+) -->
        <div class="lg:hidden max-w-page mx-auto px-4 md:px-6 py-4 md:pt-6 md:pb-0 flex flex-col gap-4">
          <input
            type="search"
            class="input input-sm w-full max-w-sm js-mobile-search"
            placeholder="Filter sources…"
            aria-label="Filter sources"
          />
          <div class="js-mobile-list flex flex-col divide-y divide-border-subtle" aria-live="polite"></div>
        </div>

        <!-- Desktop "select a source" prompt -->
        <div class="hidden lg:flex flex-col items-center justify-center min-h-96 gap-3 text-text-muted">
          <span class="icon-xl opacity-30" aria-hidden="true">${iconCube}</span>
          <p class="text-sm">Select a source from the sidebar to browse.</p>
        </div>

      </div>
    </div>
  `;

  _asideEl = /** @type {HTMLElement} */ (container.querySelector('aside'));
  const mobileList   = /** @type {HTMLElement} */ (container.querySelector('.js-mobile-list'));
  const mobileSearch = /** @type {HTMLInputElement} */ (container.querySelector('.js-mobile-search'));

  // ── Mobile list ──────────────────────────────────────────────────────────

  let allSources = /** @type {any[]} */ ([]);

  // Show skeleton only if sources take > 150 ms to load
  const cancelSkeleton = deferredSkeleton(() => { mobileList.innerHTML = skeletonSourceList(5); });

  /** @param {string} query */
  function _renderMobile(query) {
    mobileList.innerHTML = '';
    const filtered = query
      ? allSources.filter(s => s.name?.toLowerCase().includes(query.toLowerCase()))
      : allSources;

    if (filtered.length === 0) {
      mobileList.appendChild(createEmptyState({
        icon: iconCube,
        title: query ? 'No sources match your search.' : 'No sources yet.',
        subtitle: query ? undefined : 'Create a source to get started.',
      }));
      return;
    }

    for (const src of filtered) {
      const item = document.createElement('div');
      item.className = 'flex items-center gap-2 py-3';

      const a = document.createElement('a');
      a.href = `/source/${src.id}`;
      a.className = [
        'flex-1 flex items-center gap-3 min-w-0 text-sm transition-colors',
        'focus-visible:outline-none focus-visible:text-accent hover:text-accent',
        src.enabled ? 'text-text' : 'text-text-muted opacity-60',
      ].join(' ');
      a.innerHTML = `
        <span class="flex-1 font-medium truncate">${escapeHtml(src.name)}</span>
        <span class="text-xs text-text-muted shrink-0">
          v${escapeHtml(src.version ?? '?')}${src.language ? ' · ' + escapeHtml(src.language) : ''}${!src.enabled ? ' · Disabled' : ''}
        </span>
      `;

      let starred = src.favourited ?? false;
      const starBtn = document.createElement('button');
      starBtn.type = 'button';
      starBtn.className = [
        'shrink-0 p-1.5 rounded-md transition-colors',
        'focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent',
        'icon-sm',
        starred ? 'text-accent' : 'text-text-faint',
      ].join(' ');
      starBtn.setAttribute('aria-label', starred ? 'Unfavourite' : 'Favourite');
      starBtn.innerHTML = starred ? iconStarFilled : iconStarOutline;

      starBtn.addEventListener('click', async (e) => {
        e.preventDefault();
        e.stopPropagation();
        const newVal = !starred;
        starred = newVal;
        starBtn.innerHTML = newVal ? iconStarFilled : iconStarOutline;
        starBtn.setAttribute('aria-label', newVal ? 'Unfavourite' : 'Favourite');
        starBtn.classList.toggle('text-accent', newVal);
        starBtn.classList.toggle('text-text-faint', !newVal);
        try {
          await api.toggleSourceFavourite(src.id, newVal);
        } catch {
          starred = !newVal;
          starBtn.innerHTML = starred ? iconStarFilled : iconStarOutline;
          starBtn.setAttribute('aria-label', starred ? 'Unfavourite' : 'Favourite');
          starBtn.classList.toggle('text-accent', starred);
          starBtn.classList.toggle('text-text-faint', !starred);
        }
      });

      item.appendChild(a);
      item.appendChild(starBtn);
      mobileList.appendChild(item);
    }
  }

  // ── Sidebar component ────────────────────────────────────────────────────

  /** Mounts/updates the sidebar with the latest sources. */
  function _mountSidebar() {
    render(html`<${SourcesSidebar}
      sources=${allSources}
      onCreated=${_refresh}
    />`, _asideEl);
  }

  // ── Fetch ────────────────────────────────────────────────────────────────

  async function _refresh() {
    try {
      const updated = await api.getSources();
      if (Array.isArray(updated)) {
        allSources = updated;
        _renderMobile(mobileSearch?.value ?? '');
        _mountSidebar();
      }
    } catch { /* ignore refresh failures */ }
  }

  let sources;
  try {
    sources = await api.getSources();
  } catch {
    cancelSkeleton();
    mobileList.innerHTML = '';
    mobileList.appendChild(createErrorState({ message: 'Failed to load sources.', onRetry: () => init(container) }));
    return;
  }

  cancelSkeleton();
  allSources = Array.isArray(sources) ? sources : [];
  _renderMobile('');
  _mountSidebar();

  mobileSearch?.addEventListener('input', () => _renderMobile(mobileSearch.value));

  // ── Add source modal ─────────────────────────────────────────────────────

  if (hasPermission('source:install') && _headerActions) {
    let _modalOpen = false;
    const _setOpen = (open) => {
      _modalOpen = open;
      mountIntoModalRoot(html`
        <${AddSourceModal}
          open=${_modalOpen}
          onClose=${() => _setOpen(false)}
          onCreated=${() => { _setOpen(false); _refresh(); }}
        />
      `);
    };
    _headerActions.addEventListener('click', () => _setOpen(true));
  }
}

/** @param {HTMLElement} container */
export function destroy(container) {
  clearPageHeader();
  const pendingId = consumePendingSourceId();
  if (pendingId !== null) api.deleteSource(pendingId).catch(() => {});
  if (_asideEl) render(null, _asideEl);
  _asideEl = null;
  mountIntoModalRoot(null);
  container.innerHTML = '';
}
