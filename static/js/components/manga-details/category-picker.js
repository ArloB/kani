// @ts-check
// Manage tab — Categories chip picker.

import * as api from '../../api.js';
import { createEmptyState } from '../empty-state.js';
import { showApiError } from '../toast.js';

/**
 * @param {HTMLElement} bodyEl  Card body element (already mounted by caller)
 * @param {any[]} allCats
 * @param {any[]} mangaCats
 * @param {number} dbId
 */
export function mountCategoryPicker(bodyEl, allCats, mangaCats, dbId) {
  const memberIds = new Set((Array.isArray(mangaCats) ? mangaCats : []).map(c => c.id ?? c));
  const all = Array.isArray(allCats) ? allCats : [];

  if (all.length === 0) {
    bodyEl.appendChild(createEmptyState({ title: 'No categories. Create some in Settings.' }));
    return;
  }

  const chips = document.createElement('div');
  chips.className = 'flex flex-wrap gap-2 p-1';

  const rerender = () => {
    chips.innerHTML = '';
    for (const cat of all) {
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.className = memberIds.has(cat.id) ? 'chip chip-active' : 'chip';
      btn.textContent = cat.name;
      btn.setAttribute('aria-pressed', String(memberIds.has(cat.id)));
      btn.addEventListener('click', async () => {
        const wasIn = memberIds.has(cat.id);
        if (wasIn) memberIds.delete(cat.id); else memberIds.add(cat.id);
        rerender();
        try {
          await api.setMangaCategories(dbId, [...memberIds]);
        } catch (err) {
          if (wasIn) memberIds.add(cat.id); else memberIds.delete(cat.id);
          rerender();
          showApiError(err);
        }
      });
      chips.appendChild(btn);
    }
  };
  rerender();
  bodyEl.appendChild(chips);
}
