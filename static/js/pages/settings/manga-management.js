// @ts-check
// Settings — Manga Management: Pending Imports | Duplicates | Orphaned Manga

import * as api from '../../api.js';
import { escapeHtml } from '../../utils.js';
import { showToast } from '../../components/toast.js';
import { showConfirm } from '../../components/modal.js';
import { navigate } from '../../router.js';

/** @param {HTMLElement} el */
export function mount(el) {
  el.innerHTML = '';

  // ── Tab bar ───────────────────────────────────────────────────────────────
  const tabs = [
    { id: 'pending',  label: 'Pending Imports' },
    { id: 'dupes',    label: 'Duplicates' },
    { id: 'orphaned', label: 'Orphaned Manga' },
  ];

  const tabBar = document.createElement('div');
  tabBar.className = 'flex border-b border-border-subtle mb-4 gap-0';

  const panels = /** @type {Record<string, HTMLElement>} */ ({});
  /** @type {string} */
  let activeTab = 'pending';

  tabs.forEach(t => {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.dataset.tab = t.id;
    btn.className = 'px-4 py-2 text-sm font-medium border-b-2 -mb-px transition-colors';
    btn.textContent = t.label;
    btn.addEventListener('click', () => _switchTab(t.id));
    tabBar.appendChild(btn);

    const panel = document.createElement('div');
    panel.id = `tab-${t.id}`;
    panel.className = 'hidden';
    panels[t.id] = panel;
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
  el.innerHTML = '<p class="text-sm text-text-muted py-4">Loading…</p>';
  try {
    const items = await api.getPendingImports();
    _renderPendingImports(el, items);
  } catch (e) {
    el.innerHTML = `<p class="text-sm text-danger py-4">Failed to load: ${escapeHtml(e.message)}</p>`;
  }
}

function _renderPendingImports(el, items) {
  el.innerHTML = '';
  if (!items.length) {
    el.innerHTML = '<p class="text-sm text-text-muted py-4">No pending imports.</p>';
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
      dup.innerHTML = `Possible duplicate of <a href="/manga/${item.possible_duplicate_of}" class="underline text-accent" data-dupid="${item.possible_duplicate_of}">${escapeHtml(item.possible_duplicate_title ?? '#' + item.possible_duplicate_of)}</a>${item.duplicate_similarity ? ` (${Math.round(item.duplicate_similarity * 100)}% match)` : ''}`;
      dup.querySelector('a')?.addEventListener('click', e => { e.preventDefault(); navigate(`/manga/${item.possible_duplicate_of}`); });
      left.appendChild(dup);
    }

    header.appendChild(left);

    const actions = document.createElement('div');
    actions.className = 'flex gap-2 shrink-0';

    const findBtn = document.createElement('a');
    findBtn.href = `/sources?search=${encodeURIComponent(item.title)}`;
    findBtn.className = 'btn-primary btn-sm';
    findBtn.textContent = 'Find & Import';
    findBtn.addEventListener('click', e => { e.preventDefault(); navigate(`/sources?search=${encodeURIComponent(item.title)}`); });
    actions.appendChild(findBtn);

    const dismissBtn = document.createElement('button');
    dismissBtn.type = 'button';
    dismissBtn.className = 'btn-secondary btn-sm';
    dismissBtn.textContent = 'Dismiss';
    dismissBtn.addEventListener('click', async () => {
      dismissBtn.disabled = true;
      try {
        await api.deletePendingImport(item.id);
        card.remove();
        if (!list.children.length) {
          el.innerHTML = '<p class="text-sm text-text-muted py-4">No pending imports.</p>';
        }
      } catch (e) {
        showToast(`Error: ${e.message}`, 'error');
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
  el.innerHTML = '<p class="text-sm text-text-muted py-4">Loading…</p>';
  try {
    const pairs = await api.getDuplicates();
    el.innerHTML = '';
    _renderDuplicates(el, pairs);
  } catch (e) {
    el.innerHTML = `<p class="text-sm text-danger py-4">Failed to load: ${escapeHtml(e.message)}</p>`;
  }
}

function _renderDuplicates(el, pairs) {
  if (!pairs.length) {
    el.innerHTML = '<p class="text-sm text-text-muted py-4">No duplicates detected. New ones will appear here automatically when manga are added.</p>';
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
        if (!await showConfirm(`Keep "${keep.name}" and permanently delete "${discard.name}"?`, { title: 'Merge manga', confirmLabel: 'Merge' })) return;
        btn.disabled = true;
        try {
          await api.mergeDuplicate(keep.id, discard.id);
          card.remove();
          if (!list.children.length) _renderDuplicates(el, []);
          showToast(`Merged: kept "${keep.name}".`, 'success');
        } catch (e) {
          showToast(`Error: ${e.message}`, 'error');
          btn.disabled = false;
        }
      });
      return btn;
    };

    actions.appendChild(mkMergeBtn(pair.manga_a, pair.manga_b, `Keep "${pair.manga_a.name}"`));
    actions.appendChild(mkMergeBtn(pair.manga_b, pair.manga_a, `Keep "${pair.manga_b.name}"`));

    const notDupBtn = document.createElement('button');
    notDupBtn.type = 'button';
    notDupBtn.className = 'btn-secondary btn-sm';
    notDupBtn.textContent = 'Not a duplicate';
    notDupBtn.addEventListener('click', async () => {
      notDupBtn.disabled = true;
      try {
        await api.dismissDuplicate(pair.manga_a.id, pair.manga_b.id);
        card.remove();
        if (!list.children.length) _renderDuplicates(el, []);
      } catch (e) {
        showToast(`Error: ${e.message}`, 'error');
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
  el.innerHTML = '<p class="text-sm text-text-muted py-4">Loading…</p>';
  try {
    const items = await api.getOrphanedManga();
    _renderOrphanedManga(el, items);
  } catch (e) {
    el.innerHTML = `<p class="text-sm text-danger py-4">Failed to load: ${escapeHtml(e.message)}</p>`;
  }
}

function _renderOrphanedManga(el, items) {
  el.innerHTML = '';
  if (!items.length) {
    el.innerHTML = '<p class="text-sm text-text-muted py-4">No orphaned manga.</p>';
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
    srcEl.textContent = `Orphaned from: ${item.source_name}`;
    left.appendChild(nameEl);
    left.appendChild(srcEl);

    const actions = document.createElement('div');
    actions.className = 'flex gap-2 shrink-0';

    const migrateBtn = document.createElement('a');
    migrateBtn.href = `/manga/${item.id}`;
    migrateBtn.className = 'btn-primary btn-sm';
    migrateBtn.textContent = 'Migrate';
    migrateBtn.addEventListener('click', e => { e.preventDefault(); navigate(`/manga/${item.id}`); });
    actions.appendChild(migrateBtn);

    const deleteBtn = document.createElement('button');
    deleteBtn.type = 'button';
    deleteBtn.className = 'btn-danger btn-sm';
    deleteBtn.textContent = 'Delete';
    deleteBtn.addEventListener('click', async () => {
      if (!await showConfirm(`Permanently delete "${item.name}"?`, { title: 'Delete manga', confirmLabel: 'Delete' })) return;
      deleteBtn.disabled = true;
      try {
        await api.deleteManga(item.id);
        card.remove();
        if (!list.children.length) {
          el.innerHTML = '<p class="text-sm text-text-muted py-4">No orphaned manga.</p>';
        }
      } catch (e) {
        showToast(`Error: ${e.message}`, 'error');
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
