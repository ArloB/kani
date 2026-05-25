// @ts-check
// Manage tab — Scanlator mode, priority drag-sort, block/unblock, add scanlator.

import { h, render } from 'preact';
import htm from 'htm';
import * as api from '../../api.js';
import { showToast } from '../toast.js';
import { createEmptyState } from '../empty-state.js';
import { Combobox } from '../combobox.js';
import { iconX } from '../../icons.js';
import { mountSortableList } from '../sortable-list.js';
const html = htm.bind(h);

/**
 * @param {HTMLElement} bodyEl  Card body element
 * @param {any[]} initialPrefs
 * @param {string} initialMode
 * @param {number} dbId
 * @param {(prefs: any[]) => void} onPrefsChange  Notifies parent of updated prefs
 */
export function mountScanlatorPrefsPanel(bodyEl, initialPrefs, initialMode, dbId, onPrefsChange) {
  let prefs = Array.isArray(initialPrefs) ? [...initialPrefs] : [];
  let mode = initialMode ?? 'priority';

  const wrap = document.createElement('div');
  wrap.className = 'flex flex-col gap-3';
  bodyEl.appendChild(wrap);

  let scOptions = /** @type {Array<{id:number,name:string}>} */ ([{ id: -1, name: '* (Any scanlator)' }]);
  let scCmbVal = '';
  /** @type {HTMLDivElement|null} */ let scCmbMount = null;

  const renderScCmb = () => {
    if (!scCmbMount) return;
    const used = new Set(prefs.map(p => p.scanlator === '' ? '* (Any scanlator)' : p.scanlator));
    const opts = scOptions.filter(o => !used.has(o.name));
    if (scCmbVal && !opts.some(o => o.name === scCmbVal)) scCmbVal = '';
    render(html`<${Combobox}
      options=${opts}
      value=${opts.find(o => o.name === scCmbVal)?.id ?? null}
      onChange=${(/** @type {any} */ id) => { scCmbVal = opts.find(o => o.id === id)?.name ?? ''; }}
      placeholder="Select scanlator…"
    />`, scCmbMount);
  };

  api.getChapterScanlators(dbId).then(scanlators => {
    scOptions = [
      { id: -1, name: '* (Any scanlator)' },
      ...(Array.isArray(scanlators) ? scanlators : []).map((s, i) => ({ id: i, name: s })),
    ];
    renderScCmb();
  }).catch(() => {});

  const rerender = () => {
    onPrefsChange([...prefs]);
    wrap.innerHTML = '';

    // Mode selector
    const modeRow = document.createElement('div');
    modeRow.className = 'flex items-center gap-2';
    modeRow.innerHTML = `
      <span class="text-sm font-medium text-text">Mode:</span>
      <button type="button" class="btn-sm js-mode-priority ${mode === 'priority' ? 'btn-primary' : 'btn-ghost'}">Priority</button>
      <button type="button" class="btn-sm js-mode-whitelist ${mode === 'whitelist' ? 'btn-primary' : 'btn-ghost'}">Whitelist</button>
    `;
    const modeDesc = document.createElement('p');
    modeDesc.className = 'text-xs text-text-muted';
    modeDesc.textContent = mode === 'priority'
      ? 'All scanlators accepted. Use priority to prefer, and block to exclude.'
      : 'Only listed scanlators are accepted.';
    modeRow.querySelector('.js-mode-priority')?.addEventListener('click', async () => {
      try { await api.setScanlatorMode(dbId, 'priority'); mode = 'priority'; rerender(); } catch { /* ignore */ }
    });
    modeRow.querySelector('.js-mode-whitelist')?.addEventListener('click', async () => {
      try { await api.setScanlatorMode(dbId, 'whitelist'); mode = 'whitelist'; rerender(); } catch { /* ignore */ }
    });
    wrap.appendChild(modeRow);
    wrap.appendChild(modeDesc);

    const sortedPrefs = [...prefs].sort((a, b) => (b.priority ?? 0) - (a.priority ?? 0));

    if (sortedPrefs.length > 0) {
      const listContainer = document.createElement('div');
      listContainer.className = 'flex flex-col';

      mountSortableList(listContainer, {
        items: sortedPrefs,
        getId: (pref) => pref.id,
        renderItem: (pref) => {
          const content = document.createElement('div');
          content.className = 'flex flex-1 items-center gap-3 min-w-0';

          const blockedClass = pref.blocked ? 'text-danger line-through' : 'text-text';
          const nameSpan = document.createElement('span');
          nameSpan.className = `flex-1 text-sm ${blockedClass}`;
          nameSpan.textContent = pref.scanlator || '* (Any scanlator)';
          content.appendChild(nameSpan);

          const btns = document.createElement('div');
          btns.className = 'flex items-center gap-2 shrink-0';

          if (mode === 'priority') {
            const blockBtn = document.createElement('button');
            blockBtn.type = 'button';
            blockBtn.className = `btn-sm ${pref.blocked ? 'btn-danger' : 'btn-ghost'}`;
            blockBtn.title = pref.blocked ? 'Unblock' : 'Block';
            blockBtn.textContent = pref.blocked ? 'Blocked' : 'Block';
            blockBtn.addEventListener('click', async () => {
              const newBlocked = !pref.blocked;
              try {
                await api.setScanlatorPref(dbId, pref.scanlator, pref.priority, newBlocked);
                pref.blocked = newBlocked;
                rerender();
              } catch { /* ignore */ }
            });
            btns.appendChild(blockBtn);
          }

          const rmBtn = document.createElement('button');
          rmBtn.type = 'button';
          rmBtn.className = 'btn-icon text-danger';
          rmBtn.setAttribute('aria-label', `Remove ${pref.scanlator || '* (Any scanlator)'}`);
          rmBtn.innerHTML = iconX;
          rmBtn.addEventListener('click', async () => {
            try {
              await api.deleteScanlatorPref(pref.id);
              prefs = prefs.filter(p => p.id !== pref.id);
              rerender();
            } catch { /* ignore */ }
          });
          btns.appendChild(rmBtn);
          content.appendChild(btns);
          return content;
        },
        onReorder: async (_ids, newOrder) => {
          for (let i = 0; i < newOrder.length; i++) {
            const pref = newOrder[i];
            const newPriority = newOrder.length - i;
            if (pref.priority !== newPriority) {
              pref.priority = newPriority;
              api.setScanlatorPref(dbId, pref.scanlator, newPriority, pref.blocked).catch(() => {});
            }
          }
          for (const sp of newOrder) {
            const p = prefs.find(p => p.id === sp.id);
            if (p) p.priority = sp.priority;
          }
          onPrefsChange([...prefs]);
        },
        className: 'flex flex-col divide-y divide-border-subtle',
      });

      if (mode === 'priority') {
        const fallbackRow = document.createElement('div');
        fallbackRow.className = 'flex items-center gap-3 py-2 opacity-50 border-t border-border-subtle';
        fallbackRow.title = 'Always present as the lowest-priority fallback — cannot be removed';
        fallbackRow.innerHTML = `
          <span class="shrink-0 icon-sm text-transparent" aria-hidden="true">
            <svg viewBox="0 0 24 24" fill="currentColor"><circle cx="9" cy="12" r="1.5"/></svg>
          </span>
          <span class="flex-1 text-sm text-text-muted italic">All scanlators (fallback)</span>
        `;
        listContainer.appendChild(fallbackRow);
      }
      wrap.appendChild(listContainer);
    } else {
      const emptyWrap = document.createElement('div');
      emptyWrap.className = 'flex flex-col gap-0';
      emptyWrap.appendChild(createEmptyState({ title: mode === 'priority' ? 'No preferences set — all scanlators accepted equally.' : 'No whitelisted scanlators.' }));
      if (mode === 'priority') {
        const fallbackLi = document.createElement('div');
        fallbackLi.className = 'flex items-center gap-3 py-2 opacity-50';
        fallbackLi.title = 'Always present as the lowest-priority fallback — cannot be removed';
        fallbackLi.innerHTML = `
          <span class="shrink-0 icon-sm text-transparent" aria-hidden="true">
            <svg viewBox="0 0 24 24" fill="currentColor"><circle cx="9" cy="12" r="1.5"/></svg>
          </span>
          <span class="flex-1 text-sm text-text-muted italic">All scanlators (fallback)</span>
        `;
        emptyWrap.appendChild(fallbackLi);
      }
      wrap.appendChild(emptyWrap);
    }

    const form = document.createElement('div');
    form.className = 'flex flex-wrap items-center gap-2 mt-2';
    form.innerHTML = `
      <div class="js-sc-cmb-wrap flex-1 min-w-48"></div>
      <button type="button" class="btn-ghost btn-sm js-sc-add">Add</button>
    `;
    scCmbMount = /** @type {HTMLDivElement} */ (form.querySelector('.js-sc-cmb-wrap'));

    form.querySelector('.js-sc-add')?.addEventListener('click', async () => {
      const name = scCmbVal.trim();
      if (!name) return;
      const priority = prefs.length + 1;
      try {
        const finalName = name === '* (Any scanlator)' ? '' : name;
        await api.setScanlatorPref(dbId, finalName, priority, false);
        const existing = prefs.find(p => p.scanlator === finalName);
        if (existing) { existing.priority = priority; existing.blocked = false; }
        else prefs.push({ id: Date.now(), manga_id: dbId, scanlator: finalName, priority, blocked: false });
        scCmbVal = '';
        rerender();
      } catch (e) {
        showToast(/** @type {any} */(e)?.hint ?? /** @type {any} */(e)?.message ?? 'Failed to add preference', { type: 'error' });
      }
    });

    wrap.appendChild(form);
    renderScCmb();
  };

  rerender();
}
