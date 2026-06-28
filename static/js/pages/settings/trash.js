// @ts-check
// Settings — Trash: list trashed manga, restore, or permanently purge.

import * as api from '../../api.js';
import { t } from '../../i18n.js';
import { showToast, showApiError } from '../../components/toast.js';
import { showConfirm } from '../../components/modal.js';
import { createEmptyState } from '../../components/empty-state.js';
import { createErrorState } from '../../components/error-state.js';
import { mkSettingsGroup, mkSettingsGroupCard } from './_shared.js';

/** @param {HTMLElement} el */
export function mount(el) {
  el.innerHTML = '';
  _load(el);
  return { destroy() { el.innerHTML = ''; } };
}

/** @param {HTMLElement} el */
async function _load(el) {
  el.innerHTML = '<div class="text-sm text-text-muted px-1 py-4">Loading…</div>';
  try {
    const items = await api.listTrash();
    _render(el, Array.isArray(items) ? items : []);
  } catch (e) {
    el.innerHTML = '';
    el.appendChild(createErrorState({ message: e.message ?? 'Failed to load trash.' }));
  }
}

/** @param {HTMLElement} el @param {any[]} items */
function _render(el, items) {
  el.innerHTML = '';

  if (items.length === 0) {
    el.appendChild(createEmptyState({
      title: t('trash.empty.title'),
      subtitle: t('trash.empty.desc'),
    }));
    return;
  }

  const group = mkSettingsGroup();
  const card = mkSettingsGroupCard(group);

  const head = document.createElement('div');
  head.className = 'detail-card-head';
  head.innerHTML = `<span>${items.length} item${items.length === 1 ? '' : 's'}</span>`;

  const emptyBtn = document.createElement('button');
  emptyBtn.type = 'button';
  emptyBtn.className = 'btn-danger btn-sm';
  emptyBtn.textContent = t('trash.action.empty');
  emptyBtn.addEventListener('click', async () => {
    if (!(await showConfirm(t('trash.confirm.empty'), { title: t('trash.action.empty'), confirmLabel: t('trash.action.purge') }))) return;
    emptyBtn.disabled = true;
    try {
      const res = await api.purgeTrashAll();
      showToast(t('trash.toast.emptied', { count: res?.purged ?? 0 }), { type: 'success' });
      await _load(el);
    } catch (e) {
      showApiError(e);
      emptyBtn.disabled = false;
    }
  });
  head.appendChild(emptyBtn);
  card.appendChild(head);

  const list = document.createElement('div');
  list.className = 'divide-y divide-border-subtle';
  for (const item of items) {
    list.appendChild(_mkRow(item, el));
  }
  card.appendChild(list);
  el.appendChild(group);
}

/** @param {any} manga @param {HTMLElement} el */
function _mkRow(manga, el) {
  const row = document.createElement('div');
  row.className = 'flex items-center gap-3 px-4 py-3';

  const title = document.createElement('span');
  title.className = 'flex-1 text-sm text-text truncate';
  title.textContent = manga.name ?? 'Unknown';

  const deletedAt = manga.deleted_at
    ? new Date(manga.deleted_at).toLocaleDateString()
    : '';
  const meta = document.createElement('span');
  meta.className = 'text-xs text-text-muted shrink-0';
  meta.textContent = deletedAt;

  const restoreBtn = document.createElement('button');
  restoreBtn.type = 'button';
  restoreBtn.className = 'btn-secondary btn-sm shrink-0';
  restoreBtn.textContent = t('trash.action.restore');
  restoreBtn.addEventListener('click', async () => {
    restoreBtn.disabled = true;
    try {
      await api.untrashManga(manga.id);
      showToast(t('trash.toast.restored', { title: manga.name ?? '' }), { type: 'success' });
      row.remove();
    } catch (e) {
      showApiError(e);
      restoreBtn.disabled = false;
    }
  });

  const purgeBtn = document.createElement('button');
  purgeBtn.type = 'button';
  purgeBtn.className = 'btn-icon text-danger shrink-0';
  purgeBtn.setAttribute('aria-label', t('trash.action.purge'));
  purgeBtn.innerHTML = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="icon-sm"><polyline points="3 6 5 6 21 6"/><path d="M19 6l-1 14H6L5 6"/><path d="M10 11v6M14 11v6"/><path d="M9 6V4h6v2"/></svg>';
  purgeBtn.addEventListener('click', async () => {
    if (!(await showConfirm(t('trash.confirm.purge', { title: manga.name ?? '' }), { confirmLabel: t('trash.action.purge') }))) return;
    purgeBtn.disabled = true;
    try {
      await api.purgeTrashOne(manga.id);
      showToast(t('trash.toast.purged', { title: manga.name ?? '' }), { type: 'success' });
      row.remove();
      const remaining = /** @type {HTMLElement|null} */ (el.querySelector('.divide-y'))?.children.length ?? 0;
      if (remaining === 0) _load(el);
    } catch (e) {
      showApiError(e);
      purgeBtn.disabled = false;
    }
  });

  row.appendChild(title);
  row.appendChild(meta);
  row.appendChild(restoreBtn);
  row.appendChild(purgeBtn);
  return row;
}
