// @ts-check
// Settings — Library section (categories with drag-and-drop reordering + import/export).

import { h, render } from 'preact';
import { useState, useEffect } from 'preact/hooks';
import htm from 'htm';
import * as api from '../../api.js';
import { showToast, showApiError } from '../../components/toast.js';
import { iconPencil, iconX } from '../../icons.js';
import { Modal, mountIntoModalRoot, showConfirm } from '../../components/modal.js';
import { mkSettingsGroup, mkSettingsGroupCard, mkSettingsRow, mkToggleRow, mkNumberRow } from './_shared.js';
import { t } from '../../i18n.js';
import { mountSortableList } from '../../components/sortable-list.js';
import { createEmptyState } from '../../components/empty-state.js';
import { mkAddRow } from '../../components/editable-row.js';
import { escapeHtml } from '../../utils.js';
import { FolderPicker } from '../../components/folder-picker.js';

const html = htm.bind(h);

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

    const group = mkSettingsGroup(t('library.categories.group'));
    const card  = mkSettingsGroupCard(group);
    el.appendChild(group);

    const cardHead = document.createElement('div');
    cardHead.className = 'detail-card-head';
    cardHead.innerHTML = `<span>${t('library.categories.count', { count: cats.length, s: cats.length !== 1 ? 'ies' : 'y' })}</span>`;
    card.appendChild(cardHead);

    // Sortable list container
    const listContainer = document.createElement('div');
    listContainer.className = 'divide-y divide-border-subtle';
    card.appendChild(listContainer);

    if (cats.length === 0) {
      listContainer.appendChild(createEmptyState({
        title: t('library.categories.empty.title'),
        subtitle: t('library.categories.empty.subtitle'),
      }));
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
            showToast(/** @type {any} */(e)?.message ?? t('library.categories.error.reorder'), { type: 'error' });
          }
          _refreshHead(cardHead, cats.length);
        },
        className: 'flex flex-col divide-y divide-border-subtle',
      });
    }

    const addWrap = document.createElement('div');
    addWrap.className = 'border-t border-border-subtle';
    addWrap.appendChild(mkAddRow({
      label: t('library.categories.add'),
      renderForm: () => {
        const input = document.createElement('input');
        input.type = 'text';
        input.className = 'input text-sm w-full';
        input.placeholder = t('library.categories.name.placeholder');
        input.setAttribute('aria-label', t('library.categories.name.label'));
        return {
          el: input,
          focusEl: input,
          validate: () => {
            if (!input.value.trim()) { input.focus(); return false; }
            return true;
          },
          reset: () => { input.value = ''; },
          getValue: () => input.value.trim(),
        };
      },
      onAdd: async (name) => {
        await api.createCategory(name, cats.length);
        const updated = await api.getCategories();
        cats = Array.isArray(updated) ? updated : cats;
        _render();
      },
    }));
    card.appendChild(addWrap);

    _mountImportExport(el);
  }

  /** @param {any} cat */
  function _renderCatRow(cat) {
    const wrap = document.createElement('div');
    wrap.className = 'flex items-center gap-2 px-2 flex-1 min-w-0';

    const nameSpan = document.createElement('span');
    nameSpan.className = 'flex-1 text-sm text-text truncate js-cat-name';
    nameSpan.textContent = cat.name;

    const editInput = document.createElement('input');
    editInput.type = 'text';
    editInput.className = 'input flex-1 text-sm js-cat-edit hidden';
    editInput.value = cat.name;
    editInput.setAttribute('aria-label', t('library.categories.rename', { name: cat.name }));

    const editBtn = document.createElement('button');
    editBtn.type = 'button';
    editBtn.className = 'btn-icon btn-sm shrink-0';
    editBtn.setAttribute('aria-label', t('library.categories.rename', { name: cat.name }));
    editBtn.innerHTML = iconPencil;

    const delBtn = document.createElement('button');
    delBtn.type = 'button';
    delBtn.className = 'btn-icon btn-sm text-danger shrink-0';
    delBtn.setAttribute('aria-label', t('library.categories.delete', { name: cat.name }));
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
        showToast(/** @type {any} */(e)?.message ?? t('library.categories.error.rename'), { type: 'error' });
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
      if (!(await showConfirm(t('library.categories.confirm.delete.msg', { name: cat.name }), { title: t('library.categories.confirm.delete.title'), danger: true }))) return;
      delBtn.disabled = true;
      try {
        await api.deleteCategory(cat.id);
        cats = cats.filter(c => c.id !== cat.id);
        if (sortable) sortable.update(cats);
        if (cats.length === 0) _render();
      } catch (e) {
        showToast(/** @type {any} */(e)?.message ?? t('library.categories.error.delete'), { type: 'error' });
        delBtn.disabled = false;
      }
    });

    return wrap;
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
  if (span) span.textContent = t('library.categories.count', { count, s: count !== 1 ? 'ies' : 'y' });
}

// ── Import & Export ───────────────────────────────────────────────────────────

/**
 * Action-card builder for the import/export grid: title, description, then
 * the action's own controls stacked in reading order.
 * @param {string} title
 * @param {string} desc
 * @returns {{ card: HTMLElement, body: HTMLElement }}
 */
function _mkActionCard(title, desc) {
  const card = document.createElement('div');
  card.className = 'bg-surface border border-border-subtle rounded-xl p-4 flex flex-col gap-3 min-w-0';
  const head = document.createElement('div');
  head.innerHTML = `
    <p class="text-sm font-medium text-text">${escapeHtml(title)}</p>
    <p class="text-xs text-text-muted mt-0.5">${escapeHtml(desc)}</p>
  `;
  card.appendChild(head);
  const body = document.createElement('div');
  body.className = 'flex flex-col gap-2';
  card.appendChild(body);
  return { card, body };
}

/** @param {HTMLElement} el */
function _mountImportExport(el) {
  const group = mkSettingsGroup(t('library.import_export.group'));
  const oldCard = mkSettingsGroupCard(group);
  const grid = document.createElement('div');
  grid.className = 'grid sm:grid-cols-2 gap-3';
  group.replaceChild(grid, oldCard);
  el.appendChild(group);

  // ── Export card ───────────────────────────────────────────────────────────
  let includeProgress = false;
  const { card: exportCard, body: exportBody } = _mkActionCard(t('library.export.label'), t('library.export.desc'));

  const progressLabel = document.createElement('label');
  progressLabel.className = 'flex items-center gap-2 text-sm text-text cursor-pointer select-none';
  const progressCheck = document.createElement('input');
  progressCheck.type = 'checkbox';
  progressCheck.addEventListener('change', () => { includeProgress = progressCheck.checked; });
  progressLabel.appendChild(progressCheck);
  progressLabel.appendChild(document.createTextNode(t('library.export.include_progress')));
  exportBody.appendChild(progressLabel);

  const exportPassphrase = document.createElement('input');
  exportPassphrase.type = 'password';
  exportPassphrase.className = 'input text-sm w-full';
  exportPassphrase.placeholder = t('backup.export.passphrase');
  exportPassphrase.autocomplete = 'new-password';
  exportPassphrase.title = t('backup.export.passphrase.desc');
  exportBody.appendChild(exportPassphrase);

  const exportBtn = document.createElement('button');
  exportBtn.type = 'button';
  exportBtn.className = 'btn-secondary btn-sm w-full';
  exportBtn.textContent = t('library.export.btn');
  exportBtn.addEventListener('click', () => api.downloadBackupEncrypted(includeProgress, exportPassphrase.value));
  exportBody.appendChild(exportBtn);

  grid.appendChild(exportCard);

  // ── Import card (restore + Tachiyomi) ─────────────────────────────────────
  const { card: importCard, body: importBody } = _mkActionCard(t('library.restore.label'), t('library.restore.desc'));

  const restorePassphrase = document.createElement('input');
  restorePassphrase.type = 'password';
  restorePassphrase.className = 'input text-sm w-full';
  restorePassphrase.placeholder = t('backup.restore.passphrase');
  restorePassphrase.autocomplete = 'current-password';
  importBody.appendChild(restorePassphrase);

  const restoreInput = document.createElement('input');
  restoreInput.type = 'file';
  restoreInput.accept = '.zip';
  restoreInput.className = 'hidden';
  restoreInput.addEventListener('change', async () => {
    const file = restoreInput.files?.[0];
    if (!file) return;
    restoreInput.value = '';
    try {
      const preview = await api.previewBackupEncrypted(file, restorePassphrase.value);
      _showRestoreDialog(file, preview, restorePassphrase.value);
    } catch (e) {
      showToast(t('library.restore.preview_failed', { msg: e.message }), 'error');
    }
  });
  importBody.appendChild(restoreInput);

  const restoreBtn = document.createElement('button');
  restoreBtn.type = 'button';
  restoreBtn.className = 'btn-secondary btn-sm w-full';
  restoreBtn.textContent = t('library.restore.choose_file');
  restoreBtn.addEventListener('click', () => restoreInput.click());
  importBody.appendChild(restoreBtn);

  const divider = document.createElement('div');
  divider.className = 'border-t border-border-subtle my-1';
  importBody.appendChild(divider);

  const tachiHead = document.createElement('div');
  tachiHead.innerHTML = `
    <p class="text-sm font-medium text-text">${escapeHtml(t('library.tachiyomi.label'))}</p>
    <p class="text-xs text-text-muted mt-0.5">${escapeHtml(t('library.tachiyomi.desc'))}</p>
  `;
  importBody.appendChild(tachiHead);

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
      showToast(t('library.tachiyomi.preview_failed', { msg: e.message }), 'error');
    }
  });
  importBody.appendChild(tachiInput);

  const tachiBtn = document.createElement('button');
  tachiBtn.type = 'button';
  tachiBtn.className = 'btn-secondary btn-sm w-full';
  tachiBtn.textContent = t('library.tachiyomi.choose_file');
  tachiBtn.addEventListener('click', () => tachiInput.click());
  importBody.appendChild(tachiBtn);

  grid.appendChild(importCard);

  _mountScheduledBackup(el);
}

/** @param {HTMLElement} el */
async function _mountScheduledBackup(el) {
  const group = mkSettingsGroup(t('backup.group.schedule'));
  const card  = mkSettingsGroupCard(group);
  el.appendChild(group);

  let cfg = {
    enabled: false,
    frequency: { type: 'daily', hour: 2 },
    retain_n: 7,
    destination: { type: 'local', path: '/backups' },
    passphrase: null,
  };

  try {
    const loaded = await api.getBackupSchedule();
    if (loaded) cfg = loaded;
  } catch { /* admin may not be available; silently skip */ }

  let dirty = false;

  const enabledRow = mkToggleRow({
    label: t('backup.schedule.enabled'),
    checked: !!cfg.enabled,
    onChange: v => { cfg = { ...cfg, enabled: v }; dirty = true; _updateDisabled(); },
  });
  card.appendChild(enabledRow);

  const freqCtrl = document.createElement('select');
  freqCtrl.className = 'input text-sm w-28';
  freqCtrl.innerHTML = `<option value="daily">${t('backup.schedule.daily')}</option><option value="weekly">${t('backup.schedule.weekly')}</option>`;
  freqCtrl.value = cfg.frequency?.type ?? 'daily';
  freqCtrl.addEventListener('change', () => {
    const freq = { ...cfg.frequency, type: freqCtrl.value };
    if (freqCtrl.value === 'weekly' && freq.weekday == null) {
      freq.weekday = Number(weekdayCtrl.value);
    }
    cfg = { ...cfg, frequency: freq };
    dirty = true;
    _updateWeekdayRow();
  });
  const freqRow = mkSettingsRow({ label: t('backup.schedule.frequency'), control: freqCtrl });
  card.appendChild(freqRow);

  const hourRow = mkNumberRow({
    label: t('backup.schedule.hour'), tooltip: t('backup.schedule.hour.tooltip'), id: 'backup-hour',
    value: cfg.frequency?.hour ?? 2, min: 0, max: 23,
    onChange: v => { cfg = { ...cfg, frequency: { ...cfg.frequency, hour: v } }; dirty = true; },
  });
  card.appendChild(hourRow);

  const weekdayCtrl = document.createElement('select');
  weekdayCtrl.className = 'input text-sm';
  const days = ['Sunday','Monday','Tuesday','Wednesday','Thursday','Friday','Saturday'];
  weekdayCtrl.innerHTML = days.map((d, i) => `<option value="${i}">${d}</option>`).join('');
  weekdayCtrl.value = String(cfg.frequency?.weekday ?? 0);
  weekdayCtrl.addEventListener('change', () => { cfg = { ...cfg, frequency: { ...cfg.frequency, weekday: Number(weekdayCtrl.value) } }; dirty = true; });
  const weekdayRow = mkSettingsRow({ label: t('backup.schedule.weekday'), control: weekdayCtrl });
  card.appendChild(weekdayRow);

  const retainRow = mkNumberRow({
    label: t('backup.schedule.retain'), tooltip: t('backup.schedule.retain.tooltip'), id: 'backup-retain',
    value: cfg.retain_n ?? 7, min: 1, max: 365, stepper: true,
    onChange: v => { cfg = { ...cfg, retain_n: v }; dirty = true; },
  });
  card.appendChild(retainRow);

  const pathCtrl = document.createElement('div');
  pathCtrl.className = 'flex items-center gap-2';
  const pathText = document.createElement('span');
  pathText.className = 'font-mono text-sm text-text truncate max-w-56';
  pathText.textContent = cfg.destination?.path ?? '/backups';
  const browseBtn = document.createElement('button');
  browseBtn.type = 'button';
  browseBtn.className = 'btn-secondary btn-sm shrink-0';
  browseBtn.textContent = t('backup.schedule.path.browse');
  pathCtrl.append(pathText, browseBtn);

  const pickerRoot = document.createElement('div');
  el.appendChild(pickerRoot);
  let pickerOpen = false;
  const _renderPicker = () => {
    render(html`<${FolderPicker}
      open=${pickerOpen}
      initialPath=${cfg.destination?.path ?? '/backups'}
      onClose=${() => { pickerOpen = false; _renderPicker(); }}
      onSelect=${(/** @type {string} */ path) => {
        pickerOpen = false;
        _renderPicker();
        pathText.textContent = path;
        cfg = { ...cfg, destination: { type: 'local', path } };
        dirty = true;
      }}
    />`, pickerRoot);
  };
  _renderPicker();
  browseBtn.addEventListener('click', () => { pickerOpen = true; _renderPicker(); });

  card.appendChild(mkSettingsRow({ label: t('backup.schedule.path'), tooltip: t('backup.schedule.path.tooltip'), control: pathCtrl }));

  const passphraseCtrl = document.createElement('input');
  passphraseCtrl.type = 'password';
  passphraseCtrl.className = 'input text-sm w-44';
  passphraseCtrl.placeholder = cfg.passphrase === '***' ? '••••••••' : '';
  passphraseCtrl.autocomplete = 'new-password';
  passphraseCtrl.addEventListener('change', () => { cfg = { ...cfg, passphrase: passphraseCtrl.value || null }; dirty = true; });
  card.appendChild(mkSettingsRow({ label: t('backup.schedule.passphrase'), description: t('backup.schedule.passphrase.desc'), control: passphraseCtrl }));

  const footerRow = document.createElement('div');
  footerRow.className = 'flex items-center gap-2 px-4 py-3 border-t border-border-subtle';

  const saveBtn = document.createElement('button');
  saveBtn.type = 'button';
  saveBtn.className = 'btn-primary btn-sm';
  saveBtn.textContent = t('backup.schedule.save');
  saveBtn.addEventListener('click', async () => {
    saveBtn.disabled = true;
    saveBtn.textContent = t('backup.schedule.saving');
    try {
      await api.setBackupSchedule(cfg);
      showToast(t('backup.schedule.saved'), { type: 'success' });
      dirty = false;
    } catch (e) {
      showApiError(e);
    } finally {
      saveBtn.disabled = false;
      saveBtn.textContent = t('backup.schedule.save');
    }
  });

  const runNowBtn = document.createElement('button');
  runNowBtn.type = 'button';
  runNowBtn.className = 'btn-secondary btn-sm';
  runNowBtn.textContent = t('backup.schedule.run_now');
  runNowBtn.setAttribute('data-tooltip', t('backup.schedule.run_now.desc'));
  runNowBtn.addEventListener('click', async () => {
    runNowBtn.disabled = true;
    try {
      await api.runBackupNow();
      showToast(t('backup.schedule.job_submitted'), { type: 'success', action: { label: t('backup.schedule.job_view'), href: '/admin/jobs' } });
    } catch (e) {
      showApiError(e);
    } finally {
      runNowBtn.disabled = false;
    }
  });

  footerRow.appendChild(saveBtn);
  footerRow.appendChild(runNowBtn);
  card.appendChild(footerRow);

  function _updateDisabled() {
    const disabled = !cfg.enabled;
    for (const el of [freqRow, hourRow, weekdayRow, retainRow]) {
      el.style.opacity = disabled ? '0.5' : '';
      for (const input of el.querySelectorAll('input, select')) {
        /** @type {HTMLInputElement} */ (input).disabled = disabled;
      }
    }
  }

  function _updateWeekdayRow() {
    weekdayRow.style.display = cfg.frequency?.type === 'weekly' ? '' : 'none';
  }

  _updateDisabled();
  _updateWeekdayRow();
}

/** @param {{ file: File, preview: any, passphrase?: string, onClose: () => void }} props */
function RestoreModal({ file, preview, passphrase = '', onClose }) {
  const [opts, setOpts] = useState({
    merge: false,
    import_manga: true,
    import_categories: !!preview.category_count,
    import_download_rules: !!preview.download_rule_count,
    import_tracking: !!preview.has_tracking,
    import_chapter_progress: false,
    import_settings: !!preview.has_settings,
    import_repos: !!preview.repo_count,
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
    [t('library.restore.import_manga', { count: preview.manga_count }), 'import_manga', true],
    [t('library.restore.import_categories'), 'import_categories', !!preview.category_count],
    [t('library.restore.import_download_rules'), 'import_download_rules', !!preview.download_rule_count],
    [t('library.restore.import_tracking'), 'import_tracking', !!preview.has_tracking],
    [t('library.restore.import_chapter_progress'), 'import_chapter_progress', !!preview.has_chapter_progress],
    [t('library.restore.import_settings'), 'import_settings', !!preview.has_settings],
    [t('library.restore.import_repos', { count: preview.repo_count || 0 }), 'import_repos', !!preview.repo_count],
  ];

  async function doImport() {
    setLoading(true);
    setProgress(null);
    try {
      const r = await api.restoreBackupEncrypted(file, opts, passphrase);
      showToast(t('library.restore.success', { count: r.imported_manga }), { type: 'success' });
      onClose();
    } catch (e) {
      showApiError(e);
    } finally {
      setLoading(false);
    }
  }

  return html`
    <${Modal} open=${true} title=${t('library.restore.modal.title')} onClose=${onClose} footer=${html`
      <button type="button" class="btn-ghost btn-sm" onClick=${onClose}>${t('common.cancel')}</button>
      <button type="button" class="btn-primary btn-sm" disabled=${loading} onClick=${doImport}>
        ${loading ? t('library.restore.importing') : t('library.restore.import_btn')}
      </button>
    `}>
      <div class="flex flex-col gap-4">
        <p class="text-sm text-text-muted">
          ${t('library.restore.meta', { date: preview.exported_at?.slice(0, 10) ?? t('common.unknown'), manga: preview.manga_count, categories: preview.category_count })}
        </p>
        ${preview.sources?.length ? html`
          <div class="text-xs flex flex-col gap-1 bg-surface-3 rounded p-2">
            ${preview.sources.map(s => html`
              <div class="flex items-center justify-between" key=${s.source_name}>
                <span>${s.source_name} (${s.manga_count})</span>
                <span class=${s.found ? 'text-success' : 'text-danger'}>${s.found ? t('library.restore.source_available') : t('library.restore.source_missing')}</span>
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
            ${t('library.restore.merge_option')}
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
    [t('library.tachiyomi.import_manga'), 'import_manga', true],
    [t('library.restore.import_categories'), 'import_categories', !!preview.category_count],
    [t('library.restore.import_tracking'), 'import_tracking', !!preview.has_tracking],
    [t('library.restore.import_chapter_progress'), 'import_chapter_progress', !!preview.has_chapter_progress],
  ];

  async function doImport() {
    setLoading(true);
    setProgress(null);
    try {
      const r = await api.importTachiyomiBackup(file, opts);
      showToast(t('library.tachiyomi.success', { count: r.imported_manga }), { type: 'success' });
      onClose();
    } catch (e) {
      showApiError(e);
    } finally {
      setLoading(false);
    }
  }

  const pendingNote = preview.pending_import_estimate > 0
    ? ` · ${t('library.tachiyomi.pending_note', { count: preview.pending_import_estimate })}`
    : '';

  return html`
    <${Modal} open=${true} title=${t('library.tachiyomi.modal.title')} onClose=${onClose} footer=${html`
      <button type="button" class="btn-ghost btn-sm" onClick=${onClose}>${t('common.cancel')}</button>
      <button type="button" class="btn-primary btn-sm" disabled=${loading} onClick=${doImport}>
        ${loading ? t('library.restore.importing') : t('library.restore.import_btn')}
      </button>
    `}>
      <div class="flex flex-col gap-4">
        <p class="text-sm text-text-muted">
          ${t('library.tachiyomi.meta', { manga: preview.total_manga, categories: preview.category_count })}${pendingNote}
        </p>
        ${preview.sources?.length ? html`
          <div class="text-xs flex flex-col gap-1 bg-surface-3 rounded p-2">
            ${preview.sources.map(s => html`
              <div class="flex items-center justify-between" key=${s.source_id}>
                <span>${s.source_name} (${s.manga_count})</span>
                <span class=${s.found ? 'text-success' : 'text-danger'}>${s.found ? t('library.tachiyomi.source_matched') : t('library.tachiyomi.source_unmatched')}</span>
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

/** @param {File} file @param {any} preview @param {string} [passphrase] */
function _showRestoreDialog(file, preview, passphrase = '') {
  let cleanup = () => {};
  cleanup = mountIntoModalRoot(html`<${RestoreModal} file=${file} preview=${preview} passphrase=${passphrase} onClose=${() => cleanup()} />`);
}

/** @param {File} file @param {any} preview */
function _showTachiyomiDialog(file, preview) {
  let cleanup = () => {};
  cleanup = mountIntoModalRoot(html`<${TachiyomiImportModal} file=${file} preview=${preview} onClose=${() => cleanup()} />`);
}
