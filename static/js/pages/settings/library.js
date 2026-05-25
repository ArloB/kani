// @ts-check
// Settings — Library section (categories with drag-and-drop reordering + import/export).

import { h } from 'preact';
import { useState, useEffect } from 'preact/hooks';
import htm from 'htm';
import * as api from '../../api.js';
import { openConfirm } from '../../utils.js';
import { showToast, showApiError } from '../../components/toast.js';
import { iconPencil, iconX } from '../../icons.js';
import { Modal, mountIntoModalRoot } from '../../components/modal.js';
import { mkSettingsGroup, mkSettingsGroupCard, mkSettingsRow } from './_shared.js';
import { mountSortableList } from '../../components/sortable-list.js';

const html = htm.bind(h);
const _DRAG_HANDLE_SVG = `<svg viewBox="0 0 24 24" fill="currentColor" xmlns="http://www.w3.org/2000/svg" aria-hidden="true"><circle cx="9" cy="6" r="1.5"/><circle cx="15" cy="6" r="1.5"/><circle cx="9" cy="12" r="1.5"/><circle cx="15" cy="12" r="1.5"/><circle cx="9" cy="18" r="1.5"/><circle cx="15" cy="18" r="1.5"/></svg>`;

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

    _mountImportExport(el);
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

    // Visually matches a real sortable row: drag handle + input + disabled action buttons
    const pendingRow = document.createElement('div');
    pendingRow.className = 'js-pending-cat flex items-center gap-3 py-2 px-2 border-t border-border-subtle';

    const grip = document.createElement('span');
    grip.className = 'text-text-muted/30 shrink-0 select-none icon-sm pointer-events-none';
    grip.innerHTML = _DRAG_HANDLE_SVG;
    pendingRow.appendChild(grip);

    const input = document.createElement('input');
    input.type = 'text';
    input.className = 'input flex-1 text-sm';
    input.placeholder = 'Category name';
    input.setAttribute('aria-label', 'New category name');
    pendingRow.appendChild(input);

    const editBtn = document.createElement('button');
    editBtn.type = 'button';
    editBtn.className = 'btn-icon shrink-0 opacity-30';
    editBtn.disabled = true;
    editBtn.innerHTML = iconPencil;
    pendingRow.appendChild(editBtn);

    const delBtn = document.createElement('button');
    delBtn.type = 'button';
    delBtn.className = 'btn-icon text-danger shrink-0 opacity-30';
    delBtn.disabled = true;
    delBtn.innerHTML = iconX;
    pendingRow.appendChild(delBtn);

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

// ── Import & Export ───────────────────────────────────────────────────────────

/** @param {HTMLElement} el */
function _mountImportExport(el) {
  const group = mkSettingsGroup('Import & Export');
  const card  = mkSettingsGroupCard(group);
  el.appendChild(group);

  // ── Export ────────────────────────────────────────────────────────────────
  let includeProgress = false;

  const exportCtrl = document.createElement('div');
  exportCtrl.className = 'flex items-center gap-2';

  const progressLabel = document.createElement('label');
  progressLabel.className = 'flex items-center gap-1.5 text-xs text-text-muted cursor-pointer select-none';
  const progressCheck = document.createElement('input');
  progressCheck.type = 'checkbox';
  progressCheck.className = 'rounded';
  progressCheck.addEventListener('change', () => { includeProgress = progressCheck.checked; });
  progressLabel.appendChild(progressCheck);
  progressLabel.appendChild(document.createTextNode('Include chapter progress'));

  const exportBtn = document.createElement('button');
  exportBtn.type = 'button';
  exportBtn.className = 'btn-primary btn-sm';
  exportBtn.textContent = 'Export';
  exportBtn.addEventListener('click', () => api.downloadBackup(includeProgress));

  exportCtrl.appendChild(progressLabel);
  exportCtrl.appendChild(exportBtn);
  card.appendChild(mkSettingsRow({ label: 'Export backup', description: 'Download a .zip backup of your library.', control: exportCtrl }));

  // ── Restore ───────────────────────────────────────────────────────────────
  const restoreCtrl = document.createElement('div');
  restoreCtrl.className = 'flex flex-col gap-2 items-end';

  const restoreBtn = document.createElement('button');
  restoreBtn.type = 'button';
  restoreBtn.className = 'btn-secondary btn-sm';
  restoreBtn.textContent = 'Choose file (.zip)';

  const restoreInput = document.createElement('input');
  restoreInput.type = 'file';
  restoreInput.accept = '.zip';
  restoreInput.className = 'hidden';
  restoreInput.addEventListener('change', async () => {
    const file = restoreInput.files?.[0];
    if (!file) return;
    restoreInput.value = '';
    try {
      const preview = await api.previewBackup(file);
      _showRestoreDialog(file, preview);
    } catch (e) {
      showToast(`Preview failed: ${e.message}`, 'error');
    }
  });

  restoreBtn.addEventListener('click', () => restoreInput.click());
  restoreCtrl.appendChild(restoreInput);
  restoreCtrl.appendChild(restoreBtn);
  card.appendChild(mkSettingsRow({ label: 'Restore backup', description: 'Restore from a Kani backup file.', control: restoreCtrl }));

  // ── Tachiyomi ─────────────────────────────────────────────────────────────
  const tachiCtrl = document.createElement('div');
  const tachiBtn = document.createElement('button');
  tachiBtn.type = 'button';
  tachiBtn.className = 'btn-secondary btn-sm';
  tachiBtn.textContent = 'Choose file (.tachibk)';

  const tachiInput = document.createElement('input');
  tachiInput.type = 'file';
  tachiInput.accept = '.tachibk,.proto.gz';
  tachiInput.className = 'hidden';
  tachiInput.addEventListener('change', async () => {
    const file = tachiInput.files?.[0];
    if (!file) return;
    tachiInput.value = '';
    try {
      const preview = await api.previewTachiyomiImport(file);
      _showTachiyomiDialog(file, preview);
    } catch (e) {
      showToast(`Preview failed: ${e.message}`, 'error');
    }
  });

  tachiBtn.addEventListener('click', () => tachiInput.click());
  tachiCtrl.appendChild(tachiInput);
  tachiCtrl.appendChild(tachiBtn);
  card.appendChild(mkSettingsRow({ label: 'Import from Tachiyomi / Mihon', description: 'Import a .tachibk backup file.', control: tachiCtrl }));
}

/** @param {{ file: File, preview: any, onClose: () => void }} props */
function RestoreModal({ file, preview, onClose }) {
  const [opts, setOpts] = useState({
    merge: false,
    import_manga: true,
    import_categories: !!preview.category_count,
    import_download_rules: !!preview.download_rule_count,
    import_tracking: !!preview.has_tracking,
    import_chapter_progress: false,
    import_settings: !!preview.has_settings,
  });
  const [loading, setLoading] = useState(false);
  const [progress, setProgress] = useState(/** @type {{ completed: number, total: number, title: string } | null} */ (null));

  useEffect(() => {
    if (!loading) return;
    /** @param {Event} e */
    function onSse(e) {
      const d = /** @type {any} */ (e).detail;
      if (d?.origin !== 'kani_backup') return;
      if (d.type === 'import_progress') {
        setProgress({ completed: d.completed, total: d.total, title: d.title });
      } else if (d.type === 'import_completed') {
        setProgress(null);
      }
    }
    window.addEventListener('kani:sse', onSse);
    return () => window.removeEventListener('kani:sse', onSse);
  }, [loading]);

  /** @type {Array<[string, string, boolean]>} */
  const checkDefs = [
    [`Import manga (${preview.manga_count})`, 'import_manga', true],
    ['Import categories', 'import_categories', !!preview.category_count],
    ['Import download rules', 'import_download_rules', !!preview.download_rule_count],
    ['Import reading status', 'import_tracking', !!preview.has_tracking],
    ['Import chapter progress', 'import_chapter_progress', !!preview.has_chapter_progress],
    ['Import settings', 'import_settings', !!preview.has_settings],
  ];

  async function doImport() {
    setLoading(true);
    setProgress(null);
    try {
      const r = await api.restoreBackup(file, opts);
      showToast(`Backup restored: ${r.imported_manga} manga imported.`, { type: 'success' });
      onClose();
    } catch (e) {
      showApiError(e);
    } finally {
      setLoading(false);
    }
  }

  return html`
    <${Modal} open=${true} title="Restore Backup" onClose=${onClose} footer=${html`
      <button type="button" class="btn-ghost btn-sm" onClick=${onClose}>Cancel</button>
      <button type="button" class="btn-primary btn-sm" disabled=${loading} onClick=${doImport}>
        ${loading ? 'Importing…' : 'Import'}
      </button>
    `}>
      <div class="flex flex-col gap-4">
        <p class="text-sm text-text-muted">
          Kani Backup · ${preview.exported_at?.slice(0, 10) ?? 'unknown date'} · ${preview.manga_count} manga, ${preview.category_count} categories
        </p>
        ${preview.sources?.length ? html`
          <div class="text-xs flex flex-col gap-1 bg-surface-3 rounded p-2">
            ${preview.sources.map(s => html`
              <div class="flex items-center justify-between" key=${s.source_name}>
                <span>${s.source_name} (${s.manga_count})</span>
                <span class=${s.found ? 'text-success' : 'text-danger'}>${s.found ? '✓ available' : '✗ not installed'}</span>
              </div>
            `)}
          </div>
        ` : null}
        <div class="flex flex-col gap-1.5 text-sm">
          ${checkDefs.map(([label, key, available]) => html`
            <label class=${'flex items-center gap-2 ' + (available ? 'cursor-pointer' : 'opacity-40 pointer-events-none')} key=${key}>
              <input type="checkbox" checked=${opts[key]} disabled=${!available}
                onChange=${e => setOpts(o => ({ ...o, [key]: e.target.checked }))} />
              ${label}
            </label>
          `)}
          <label class="flex items-center gap-2 cursor-pointer mt-1">
            <input type="checkbox" checked=${opts.merge}
              onChange=${e => setOpts(o => ({ ...o, merge: e.target.checked }))} />
            Merge (keep existing data, add new)
          </label>
        </div>
        ${loading && progress ? html`
          <div>
            <div class="flex justify-between text-xs text-text-muted mb-1">
              <span class="truncate min-w-0 mr-2">${progress.title}</span>
              <span class="shrink-0">${progress.completed} / ${progress.total}</span>
            </div>
            <div class="w-full bg-surface-3 rounded-full h-1.5">
              <div class="bg-accent h-1.5 rounded-full transition-all"
                style=${{ width: progress.total > 0 ? `${Math.round(progress.completed / progress.total * 100)}%` : '0%' }}></div>
            </div>
          </div>
        ` : null}
      </div>
    </${Modal}>
  `;
}

/** @param {{ file: File, preview: any, onClose: () => void }} props */
function TachiyomiImportModal({ file, preview, onClose }) {
  const [opts, setOpts] = useState({
    import_manga: true,
    import_categories: !!preview.category_count,
    import_tracking: !!preview.has_tracking,
    import_chapter_progress: false,
  });
  const [loading, setLoading] = useState(false);
  const [progress, setProgress] = useState(/** @type {{ completed: number, total: number, title: string } | null} */ (null));

  useEffect(() => {
    if (!loading) return;
    /** @param {Event} e */
    function onSse(e) {
      const d = /** @type {any} */ (e).detail;
      if (d?.origin !== 'tachiyomi') return;
      if (d.type === 'import_progress') {
        setProgress({ completed: d.completed, total: d.total, title: d.title });
      } else if (d.type === 'import_completed') {
        setProgress(null);
      }
    }
    window.addEventListener('kani:sse', onSse);
    return () => window.removeEventListener('kani:sse', onSse);
  }, [loading]);

  /** @type {Array<[string, string, boolean]>} */
  const checkDefs = [
    ['Import manga', 'import_manga', true],
    ['Import categories', 'import_categories', !!preview.category_count],
    ['Import reading status', 'import_tracking', !!preview.has_tracking],
    ['Import chapter progress', 'import_chapter_progress', !!preview.has_chapter_progress],
  ];

  async function doImport() {
    setLoading(true);
    setProgress(null);
    try {
      const r = await api.importTachiyomiBackup(file, opts);
      showToast(`Import complete: ${r.imported_manga} manga added.`, { type: 'success' });
      onClose();
    } catch (e) {
      showApiError(e);
    } finally {
      setLoading(false);
    }
  }

  const pendingNote = preview.pending_import_estimate > 0
    ? ` · ~${preview.pending_import_estimate} will go to Pending Imports`
    : '';

  return html`
    <${Modal} open=${true} title="Import from Tachiyomi / Mihon" onClose=${onClose} footer=${html`
      <button type="button" class="btn-ghost btn-sm" onClick=${onClose}>Cancel</button>
      <button type="button" class="btn-primary btn-sm" disabled=${loading} onClick=${doImport}>
        ${loading ? 'Importing…' : 'Import'}
      </button>
    `}>
      <div class="flex flex-col gap-4">
        <p class="text-sm text-text-muted">
          ${preview.total_manga} manga · ${preview.category_count} categories${pendingNote}
        </p>
        ${preview.sources?.length ? html`
          <div class="text-xs flex flex-col gap-1 bg-surface-3 rounded p-2">
            ${preview.sources.map(s => html`
              <div class="flex items-center justify-between" key=${s.source_id}>
                <span>${s.source_name} (${s.manga_count})</span>
                <span class=${s.found ? 'text-success' : 'text-danger'}>${s.found ? '✓ matched' : '✗ unmatched'}</span>
              </div>
            `)}
          </div>
        ` : null}
        <div class="flex flex-col gap-1.5 text-sm">
          ${checkDefs.map(([label, key, available]) => html`
            <label class=${'flex items-center gap-2 ' + (available ? 'cursor-pointer' : 'opacity-40 pointer-events-none')} key=${key}>
              <input type="checkbox" checked=${opts[key]} disabled=${!available}
                onChange=${e => setOpts(o => ({ ...o, [key]: e.target.checked }))} />
              ${label}
            </label>
          `)}
        </div>
        ${loading && progress ? html`
          <div>
            <div class="flex justify-between text-xs text-text-muted mb-1">
              <span class="truncate min-w-0 mr-2">${progress.title}</span>
              <span class="shrink-0">${progress.completed} / ${progress.total}</span>
            </div>
            <div class="w-full bg-surface-3 rounded-full h-1.5">
              <div class="bg-accent h-1.5 rounded-full transition-all"
                style=${{ width: progress.total > 0 ? `${Math.round(progress.completed / progress.total * 100)}%` : '0%' }}></div>
            </div>
          </div>
        ` : null}
      </div>
    </${Modal}>
  `;
}

/** @param {File} file @param {any} preview */
function _showRestoreDialog(file, preview) {
  let cleanup = () => {};
  cleanup = mountIntoModalRoot(html`<${RestoreModal} file=${file} preview=${preview} onClose=${() => cleanup()} />`);
}

/** @param {File} file @param {any} preview */
function _showTachiyomiDialog(file, preview) {
  let cleanup = () => {};
  cleanup = mountIntoModalRoot(html`<${TachiyomiImportModal} file=${file} preview=${preview} onClose=${() => cleanup()} />`);
}
