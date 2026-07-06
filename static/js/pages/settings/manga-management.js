// @ts-check
// Settings — Manga Management: Pending Imports | Duplicates | Orphaned Manga

import * as api from '../../api.js';
import { escapeHtml } from '../../utils.js';
import { showToast, showApiError } from '../../components/toast.js';
import { showConfirm } from '../../components/modal.js';
import { navigate } from '../../router.js';
import { skeletonSettingsCards } from '../../components/skeletons.js';
import { createEmptyState } from '../../components/empty-state.js';
import { createErrorState } from '../../components/error-state.js';
import { t } from '../../i18n.js';

/** @param {HTMLElement} el */
export function mount(el) {
  el.innerHTML = '';

  // ── Tab bar ───────────────────────────────────────────────────────────────
  const tabs = [
    { id: 'pending',  label: t('settings.manga_mgmt.tab.pending') },
    { id: 'dupes',    label: t('settings.manga_mgmt.tab.dupes') },
    { id: 'orphaned', label: t('settings.manga_mgmt.tab.orphaned') },
  ];

  const tabBar = document.createElement('div');
  tabBar.className = 'flex border-b border-border-subtle mb-4 gap-0';

  const panels = /** @type {Record<string, HTMLElement>} */ ({});
  /** @type {string} */
  let activeTab = 'pending';

  tabs.forEach(tab => {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.dataset.tab = tab.id;
    btn.className = 'px-4 py-2 text-sm font-medium border-b-2 -mb-px transition-colors';
    btn.textContent = tab.label;
    btn.addEventListener('click', () => _switchTab(tab.id));
    tabBar.appendChild(btn);

    const panel = document.createElement('div');
    panel.id = `tab-${tab.id}`;
    panel.className = 'hidden';
    panels[tab.id] = panel;
    el.appendChild(panel);
  });

  el.insertBefore(tabBar, el.firstChild);

  function _switchTab(id) {
    activeTab = id;
    tabBar.querySelectorAll('button').forEach(b => {
      const active = b.dataset.tab === id;
      b.className = `px-4 py-2 text-sm font-medium border-b-2 -mb-px transition-colors ${active ? 'border-accent text-accent' : 'border-transparent text-text-muted hover:text-text'}`;
    });
    Object.values(panels).forEach(p => p.classList.add('hidden'));
    panels[id]?.classList.remove('hidden');
  }

  _switchTab('pending');

  // ── Pending Imports ───────────────────────────────────────────────────────
  _loadPendingImports(panels['pending']);

  // ── Duplicates ────────────────────────────────────────────────────────────
  _mountDuplicatesTab(panels['dupes']);

  // ── Orphaned Manga ────────────────────────────────────────────────────────
  _loadOrphanedManga(panels['orphaned']);

  return { destroy() { el.innerHTML = ''; } };
}

// ── Pending Imports ───────────────────────────────────────────────────────────

/** @param {HTMLElement} el */
async function _loadPendingImports(el) {
  el.innerHTML = skeletonSettingsCards(3);
  try {
    const items = await api.getPendingImports();
    _renderPendingImports(el, items);
  } catch (e) {
    el.innerHTML = '';
    el.appendChild(createErrorState({ message: t('settings.manga_mgmt.load_failed', { msg: e?.message ?? '' }), onRetry: () => _loadPendingImports(el) }));
  }
}

function _renderPendingImports(el, items) {
  el.innerHTML = '';
  if (!items.length) {
    el.appendChild(createEmptyState({
      title: t('settings.manga_mgmt.pending.empty.title'),
      subtitle: t('settings.manga_mgmt.pending.empty.desc'),
    }));
    return;
  }
  const list = document.createElement('div');
  list.className = 'flex flex-col gap-2';

  for (const item of items) {
    const card = document.createElement('div');
    card.className = 'bg-surface-2 rounded-xl p-4 flex flex-col gap-2';

    const header = document.createElement('div');
    header.className = 'flex items-start justify-between gap-2';

    const left = document.createElement('div');
    left.className = 'flex flex-col gap-0.5 min-w-0';

    const nameEl = document.createElement('p');
    nameEl.className = 'font-medium text-sm truncate';
    nameEl.textContent = item.title;
    left.appendChild(nameEl);

    const meta = document.createElement('p');
    meta.className = 'text-xs text-text-muted';
    const originBadge = item.origin === 'tachiyomi' ? 'Tachiyomi' : 'Kani Backup';
    meta.textContent = `${originBadge}${item.source_hint ? ' · ' + item.source_hint : ''}`;
    left.appendChild(meta);

    if (item.possible_duplicate_of) {
      const dup = document.createElement('p');
      dup.className = 'text-xs text-warn';
      dup.innerHTML = `${t('settings.manga_mgmt.pending.dup.prefix')} <a href="/manga/${item.possible_duplicate_of}" class="underline text-accent" data-dupid="${item.possible_duplicate_of}">${escapeHtml(item.possible_duplicate_title ?? '#' + item.possible_duplicate_of)}</a>${item.duplicate_similarity ? ' ' + t('settings.manga_mgmt.pending.dup.match', { pct: Math.round(item.duplicate_similarity * 100) }) : ''}`;
      dup.querySelector('a')?.addEventListener('click', e => { e.preventDefault(); navigate(`/manga/${item.possible_duplicate_of}`); });
      left.appendChild(dup);
    }

    header.appendChild(left);

    const actions = document.createElement('div');
    actions.className = 'flex gap-2 shrink-0';

    const findBtn = document.createElement('a');
    findBtn.href = `/sources?search=${encodeURIComponent(item.title)}`;
    findBtn.className = 'btn-primary btn-sm';
    findBtn.textContent = t('settings.manga_mgmt.pending.find_btn');
    findBtn.addEventListener('click', e => { e.preventDefault(); navigate(`/sources?search=${encodeURIComponent(item.title)}`); });
    actions.appendChild(findBtn);

    const dismissBtn = document.createElement('button');
    dismissBtn.type = 'button';
    dismissBtn.className = 'btn-secondary btn-sm';
    dismissBtn.textContent = t('settings.manga_mgmt.pending.dismiss_btn');
    dismissBtn.addEventListener('click', async () => {
      dismissBtn.disabled = true;
      try {
        await api.deletePendingImport(item.id);
        card.remove();
        if (!list.children.length) {
          _renderPendingImports(el, []);
        }
      } catch (e) {
        showApiError(e);
        dismissBtn.disabled = false;
      }
    });
    actions.appendChild(dismissBtn);

    header.appendChild(actions);
    card.appendChild(header);
    list.appendChild(card);
  }
  el.appendChild(list);
}

// ── Duplicates ────────────────────────────────────────────────────────────────

/** @param {HTMLElement} el */
async function _mountDuplicatesTab(el) {
  el.innerHTML = skeletonSettingsCards(3);
  try {
    const pairs = await api.getDuplicates();
    el.innerHTML = '';
    _renderDuplicates(el, pairs);
  } catch (e) {
    el.innerHTML = '';
    el.appendChild(createErrorState({ message: t('settings.manga_mgmt.load_failed', { msg: e?.message ?? '' }), onRetry: () => _mountDuplicatesTab(el) }));
  }
}

function _renderDuplicates(el, pairs) {
  el.innerHTML = '';
  if (!pairs.length) {
    el.appendChild(createEmptyState({
      title: t('settings.manga_mgmt.dupes.empty.title'),
      subtitle: t('settings.manga_mgmt.dupes.empty.desc'),
    }));
    return;
  }

  const list = document.createElement('div');
  list.className = 'flex flex-col gap-2';

  for (const pair of pairs) {
    const card = document.createElement('div');
    card.className = 'bg-surface-2 rounded-xl p-4 flex flex-col gap-2';

    const row = document.createElement('div');
    row.className = 'flex items-start gap-4';

    const mkMangaCol = (m) => {
      const col = document.createElement('div');
      col.className = 'flex-1 min-w-0';
      const link = document.createElement('a');
      link.href = `/manga/${m.id}`;
      link.className = 'font-medium text-sm text-accent hover:underline truncate block';
      link.textContent = m.name;
      link.addEventListener('click', e => { e.preventDefault(); navigate(`/manga/${m.id}`); });
      col.appendChild(link);
      return col;
    };

    row.appendChild(mkMangaCol(pair.manga_a));

    const vsEl = document.createElement('div');
    vsEl.className = 'text-xs text-text-muted shrink-0 pt-0.5';
    vsEl.textContent = `${Math.round(pair.similarity * 100)}%${pair.author_match ? ' · author' : ''}`;
    row.appendChild(vsEl);

    row.appendChild(mkMangaCol(pair.manga_b));
    card.appendChild(row);

    const actions = document.createElement('div');
    actions.className = 'flex gap-2 flex-wrap';

    const mkMergeBtn = (keep, discard, label) => {
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.className = 'btn-primary btn-sm';
      btn.textContent = label;
      btn.addEventListener('click', async () => {
        if (!await showConfirm(t('settings.manga_mgmt.dupes.merge.confirm', { keep: keep.name, discard: discard.name }), { title: t('settings.manga_mgmt.dupes.merge.title'), confirmLabel: t('settings.manga_mgmt.dupes.merge.btn') })) return;
        btn.disabled = true;
        try {
          await api.mergeDuplicate(keep.id, discard.id);
          card.remove();
          if (!list.children.length) _renderDuplicates(el, []);
          showToast(t('settings.manga_mgmt.dupes.merge.success', { name: keep.name }), { type: 'success' });
        } catch (e) {
          showApiError(e);
          btn.disabled = false;
        }
      });
      return btn;
    };

    actions.appendChild(mkMergeBtn(pair.manga_a, pair.manga_b, t('settings.manga_mgmt.dupes.keep_btn', { name: pair.manga_a.name })));
    actions.appendChild(mkMergeBtn(pair.manga_b, pair.manga_a, t('settings.manga_mgmt.dupes.keep_btn', { name: pair.manga_b.name })));

    const notDupBtn = document.createElement('button');
    notDupBtn.type = 'button';
    notDupBtn.className = 'btn-secondary btn-sm';
    notDupBtn.textContent = t('settings.manga_mgmt.dupes.not_dup');
    notDupBtn.addEventListener('click', async () => {
      notDupBtn.disabled = true;
      try {
        await api.dismissDuplicate(pair.manga_a.id, pair.manga_b.id);
        card.remove();
        if (!list.children.length) _renderDuplicates(el, []);
      } catch (e) {
        showApiError(e);
        notDupBtn.disabled = false;
      }
    });
    actions.appendChild(notDupBtn);

    card.appendChild(actions);
    list.appendChild(card);
  }

  el.appendChild(list);
}

// ── Orphaned Manga ────────────────────────────────────────────────────────────

/** @param {HTMLElement} el */
async function _loadOrphanedManga(el) {
  el.innerHTML = skeletonSettingsCards(3);
  try {
    const items = await api.getOrphanedManga();
    el.innerHTML = '';
    _renderOrphanedManga(el, items);
  } catch (e) {
    el.innerHTML = '';
    el.appendChild(createErrorState({ message: `Failed to load: ${e?.message ?? 'Unknown error'}`, onRetry: () => _loadOrphanedManga(el) }));
  }
}

function _renderOrphanedManga(el, items) {
  el.innerHTML = '';
  if (!items.length) {
    el.appendChild(createEmptyState({
      title: t('settings.manga_mgmt.orphaned.empty.title'),
      subtitle: t('settings.manga_mgmt.orphaned.empty.desc'),
    }));
    return;
  }
  const list = document.createElement('div');
  list.className = 'flex flex-col gap-2';

  for (const item of items) {
    const card = document.createElement('div');
    card.className = 'bg-surface-2 rounded-xl p-4 flex items-center justify-between gap-4';

    const left = document.createElement('div');
    left.className = 'flex flex-col gap-0.5 min-w-0';
    const nameEl = document.createElement('p');
    nameEl.className = 'font-medium text-sm truncate';
    nameEl.textContent = item.name;
    const srcEl = document.createElement('p');
    srcEl.className = 'text-xs text-text-muted';
    srcEl.textContent = t('settings.manga_mgmt.orphaned.from', { source: item.source_name });
    left.appendChild(nameEl);
    left.appendChild(srcEl);

    const actions = document.createElement('div');
    actions.className = 'flex gap-2 shrink-0';

    const migrateBtn = document.createElement('a');
    migrateBtn.href = `/manga/${item.id}`;
    migrateBtn.className = 'btn-primary btn-sm';
    migrateBtn.textContent = t('settings.manga_mgmt.orphaned.migrate');
    migrateBtn.addEventListener('click', e => { e.preventDefault(); navigate(`/manga/${item.id}`); });
    actions.appendChild(migrateBtn);

    const deleteBtn = document.createElement('button');
    deleteBtn.type = 'button';
    deleteBtn.className = 'btn-danger btn-sm';
    deleteBtn.textContent = t('common.delete');
    deleteBtn.addEventListener('click', async () => {
      if (!await showConfirm(t('settings.manga_mgmt.orphaned.delete.confirm', { name: item.name }), { title: t('settings.manga_mgmt.orphaned.delete.title'), confirmLabel: t('common.delete') })) return;
      deleteBtn.disabled = true;
      try {
        await api.deleteManga(item.id);
        card.remove();
        if (!list.children.length) {
          _renderOrphanedManga(el, []);
        }
      } catch (e) {
        showApiError(e);
        deleteBtn.disabled = false;
      }
    });
    actions.appendChild(deleteBtn);

    card.appendChild(left);
    card.appendChild(actions);
    list.appendChild(card);
  }
  el.appendChild(list);
}
