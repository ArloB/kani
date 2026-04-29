// @ts-check
// Settings — Library section (categories with drag-and-drop reordering).

import * as api from '../../api.js';
import { escapeHtml, openConfirm } from '../../utils.js';
import { showToast } from '../../components/toast.js';
import { iconPencil, iconX } from '../../icons.js';
import { mkSettingsGroup, mkSettingsGroupCard } from './_shared.js';
import { mountSortableList } from '../../components/sortable-list.js';

/**
 * @param {HTMLElement} el
 * @param {any[]} initialCategories
 */
export function mount(el, initialCategories) {
  let cats = [...initialCategories];
  /** @type {{ update: (items: any[]) => void, destroy: () => void } | null} */
  let sortable = null;

  function _render() {
    el.innerHTML = '';

    const group = mkSettingsGroup('Categories');
    const card  = mkSettingsGroupCard(group);
    el.appendChild(group);

    // Card header with Add button
    const cardHead = document.createElement('div');
    cardHead.className = 'detail-card-head';
    cardHead.innerHTML = `<span>${cats.length} categor${cats.length === 1 ? 'y' : 'ies'}</span>`;
    const addBtn = document.createElement('button');
    addBtn.type = 'button';
    addBtn.className = 'btn-primary btn-sm';
    addBtn.textContent = '+ Add category';
    cardHead.appendChild(addBtn);
    card.appendChild(cardHead);

    // Sortable list container
    const listContainer = document.createElement('div');
    listContainer.className = 'divide-y divide-border-subtle';
    card.appendChild(listContainer);

    if (cats.length === 0) {
      listContainer.innerHTML = '<p class="text-sm text-text-muted px-4 py-3">No categories yet.</p>';
    } else {
      sortable = mountSortableList(listContainer, {
        items: cats,
        getId: (cat) => cat.id,
        renderItem: (cat) => _renderCatRow(cat),
        onReorder: async (ids, newOrder) => {
          cats = newOrder;
          try {
            await api.reorderCategories(ids);
          } catch (e) {
            showToast(/** @type {any} */(e)?.message ?? 'Failed to reorder.', { type: 'error' });
          }
          _refreshHead(cardHead, cats.length);
        },
        className: 'flex flex-col divide-y divide-border-subtle',
      });
    }

    addBtn.addEventListener('click', () => _showInlineAdd(listContainer, addBtn));
  }

  /** @param {any} cat */
  function _renderCatRow(cat) {
    const wrap = document.createElement('div');
    wrap.className = 'flex items-center gap-2 px-4 py-2.5 flex-1 min-w-0';

    const nameSpan = document.createElement('span');
    nameSpan.className = 'flex-1 text-sm text-text truncate js-cat-name';
    nameSpan.textContent = cat.name;

    const editInput = document.createElement('input');
    editInput.type = 'text';
    editInput.className = 'input flex-1 text-sm js-cat-edit hidden';
    editInput.value = cat.name;
    editInput.setAttribute('aria-label', `Rename ${cat.name}`);

    const editBtn = document.createElement('button');
    editBtn.type = 'button';
    editBtn.className = 'btn-icon shrink-0';
    editBtn.setAttribute('aria-label', `Rename ${cat.name}`);
    editBtn.innerHTML = iconPencil;

    const delBtn = document.createElement('button');
    delBtn.type = 'button';
    delBtn.className = 'btn-icon text-danger shrink-0';
    delBtn.setAttribute('aria-label', `Delete ${cat.name}`);
    delBtn.innerHTML = iconX;

    wrap.appendChild(nameSpan);
    wrap.appendChild(editInput);
    wrap.appendChild(editBtn);
    wrap.appendChild(delBtn);

    editBtn.addEventListener('click', () => {
      nameSpan.classList.add('hidden');
      editInput.classList.remove('hidden');
      editInput.focus();
      editInput.select();
    });

    const _saveEdit = async () => {
      const newName = editInput.value.trim();
      if (!newName || newName === cat.name) {
        editInput.classList.add('hidden');
        nameSpan.classList.remove('hidden');
        return;
      }
      try {
        await api.renameCategory(cat.id, newName);
        cat.name = newName;
        nameSpan.textContent = newName;
      } catch (e) {
        showToast(/** @type {any} */(e)?.message ?? 'Failed to rename.', { type: 'error' });
      }
      editInput.classList.add('hidden');
      nameSpan.classList.remove('hidden');
    };

    editInput.addEventListener('blur', _saveEdit);
    editInput.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') { e.preventDefault(); _saveEdit(); }
      if (e.key === 'Escape') {
        editInput.value = cat.name;
        editInput.classList.add('hidden');
        nameSpan.classList.remove('hidden');
      }
    });

    delBtn.addEventListener('click', async () => {
      if (!(await openConfirm({ title: 'Delete category', message: `Delete category "${cat.name}"? This cannot be undone.`, danger: true }))) return;
      delBtn.disabled = true;
      try {
        await api.deleteCategory(cat.id);
        cats = cats.filter(c => c.id !== cat.id);
        if (sortable) sortable.update(cats);
        if (cats.length === 0) _render();
      } catch (e) {
        showToast(/** @type {any} */(e)?.message ?? 'Failed to delete.', { type: 'error' });
        delBtn.disabled = false;
      }
    });

    return wrap;
  }

  /**
   * Insert an inline text input at the bottom of the list.
   * On Enter/blur with a name: calls API and refreshes. On Escape/blur empty: discards.
   * @param {HTMLElement} listContainer
   * @param {HTMLButtonElement} addBtn
   */
  function _showInlineAdd(listContainer, addBtn) {
    // Prevent double-open
    if (listContainer.querySelector('.js-pending-cat')) return;
    addBtn.disabled = true;

    const pendingRow = document.createElement('div');
    pendingRow.className = 'js-pending-cat flex items-center gap-2 px-4 py-2.5 border-t border-border-subtle';

    const input = document.createElement('input');
    input.type = 'text';
    input.className = 'input flex-1 text-sm';
    input.placeholder = 'Category name';
    input.setAttribute('aria-label', 'New category name');
    pendingRow.appendChild(input);

    listContainer.appendChild(pendingRow);
    input.focus();

    let _committed = false;

    async function _commit() {
      if (_committed) return;
      const name = input.value.trim();
      if (!name) { _discard(); return; }
      _committed = true;
      input.disabled = true;
      try {
        await api.createCategory(name, cats.length);
        const updated = await api.getCategories();
        cats = Array.isArray(updated) ? updated : cats;
        _render();
      } catch (e) {
        showToast(/** @type {any} */(e)?.message ?? 'Failed to add category.', { type: 'error' });
        _discard();
        _committed = false;
      }
    }

    function _discard() {
      pendingRow.remove();
      addBtn.disabled = false;
    }

    input.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') { e.preventDefault(); _commit(); }
      if (e.key === 'Escape') { _discard(); }
    });
    input.addEventListener('blur', () => {
      if (!_committed) _commit();
    });
  }

  _render();
  return { destroy() { sortable?.destroy(); el.innerHTML = ''; } };
}

/**
 * @param {HTMLElement} headEl
 * @param {number} count
 */
function _refreshHead(headEl, count) {
  const span = headEl.querySelector('span');
  if (span) span.textContent = `${count} categor${count === 1 ? 'y' : 'ies'}`;
}
