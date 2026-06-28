// @ts-check
// Settings — Storage: disk usage and library integrity check (admin only).

import * as api from '../../api.js';
import { t } from '../../i18n.js';
import { showApiError } from '../../components/toast.js';
import { mkSettingsGroup, mkSettingsGroupCard, mkSettingsRow } from './_shared.js';

/** @param {number} bytes */
function _fmt(bytes) {
  if (bytes == null) return '—';
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 ** 2) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 ** 3) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
  return `${(bytes / 1024 ** 3).toFixed(2)} GB`;
}

/** @param {HTMLElement} el */
export function mount(el) {
  el.innerHTML = '';
  _mountUsage(el);
  _mountIntegrity(el);
  _mountHistory(el);
  return { destroy() { el.innerHTML = ''; } };
}

/** @param {HTMLElement} el */
async function _mountUsage(el) {
  const group = mkSettingsGroup(t('storage.group.usage'));
  const card = mkSettingsGroupCard(group);
  el.appendChild(group);

  const placeholder = document.createElement('div');
  placeholder.className = 'px-4 py-3 text-sm text-text-muted';
  placeholder.textContent = 'Loading…';
  card.appendChild(placeholder);

  try {
    const stats = await api.getAdminStorageStats();
    placeholder.remove();

    /** @type {[string, any][]} */
    const rows = [
      [t('storage.stat.library'),       stats.library_used_bytes],
      [t('storage.stat.chapters'),      stats.chapter_used_bytes],
      [t('storage.stat.covers'),        stats.cover_used_bytes],
      [t('storage.stat.data'),          stats.data_used_bytes],
      [t('storage.stat.free'),          stats.library_free_bytes],
      [t('storage.stat.manga'),         stats.total_manga],
      [t('storage.stat.chapters_count'), stats.total_chapters],
    ];

    for (const [label, value] of rows) {
      const valueEl = document.createElement('span');
      valueEl.className = 'text-sm font-medium text-text';
      const isBytes = label !== t('storage.stat.manga') && label !== t('storage.stat.chapters_count');
      valueEl.textContent = isBytes ? _fmt(value) : (value ?? '—').toString();
      card.appendChild(mkSettingsRow({ label, control: valueEl }));
    }
  } catch (e) {
    placeholder.textContent = e.message ?? 'Failed to load storage stats.';
    placeholder.className = 'px-4 py-3 text-sm text-danger';
  }
}

/** @param {HTMLElement} el */
function _mountIntegrity(el) {
  const group = mkSettingsGroup(t('storage.group.integrity'));
  const card = mkSettingsGroupCard(group);
  el.appendChild(group);

  const desc = document.createElement('p');
  desc.className = 'text-xs text-text-muted px-4 py-2';
  desc.textContent = t('storage.integrity.desc');
  card.appendChild(desc);

  const btnRow = document.createElement('div');
  btnRow.className = 'flex items-center gap-2 px-4 py-3 border-t border-border-subtle';

  const runBtn = document.createElement('button');
  runBtn.type = 'button';
  runBtn.className = 'btn-secondary btn-sm';
  runBtn.textContent = t('storage.integrity.run');

  const fixBtn = document.createElement('button');
  fixBtn.type = 'button';
  fixBtn.className = 'btn-danger btn-sm';
  fixBtn.textContent = t('storage.integrity.fix');

  btnRow.appendChild(runBtn);
  btnRow.appendChild(fixBtn);
  card.appendChild(btnRow);

  const resultsEl = document.createElement('div');
  resultsEl.className = 'hidden flex-col gap-0 divide-y divide-border-subtle';
  card.appendChild(resultsEl);

  async function _run(fix) {
    runBtn.disabled = true;
    fixBtn.disabled = true;
    runBtn.textContent = t('storage.integrity.running');
    fixBtn.textContent = t('storage.integrity.running');
    resultsEl.innerHTML = '';
    resultsEl.classList.add('hidden');
    resultsEl.classList.remove('flex');
    try {
      const res = await api.runIntegrityCheck(fix);
      _renderResults(resultsEl, res, fix);
    } catch (e) {
      showApiError(e);
    } finally {
      runBtn.disabled = false;
      fixBtn.disabled = false;
      runBtn.textContent = t('storage.integrity.run');
      fixBtn.textContent = t('storage.integrity.fix');
    }
  }

  runBtn.addEventListener('click', () => _run(false));
  fixBtn.addEventListener('click', () => _run(true));
}

/**
 * @param {HTMLElement} el
 * @param {any} res
 * @param {boolean} fix
 */
function _renderResults(el, res, fix) {
  el.innerHTML = '';
  el.classList.remove('hidden');
  el.classList.add('flex');

  if (fix) {
    _addStat(el, t('storage.integrity.removed'), res.removed_count ?? 0);
    _addStat(el, t('storage.integrity.failed'),  res.failed_count ?? 0);
    return;
  }

  const orphaned = res.orphaned_files?.length ?? 0;
  const missing  = res.missing_files?.length ?? 0;
  const covers   = res.cover_mismatches?.length ?? 0;

  if (orphaned === 0 && missing === 0 && covers === 0) {
    const ok = document.createElement('p');
    ok.className = 'px-4 py-3 text-sm text-success';
    ok.textContent = t('storage.integrity.ok');
    el.appendChild(ok);
    return;
  }

  _addStat(el, t('storage.integrity.orphaned'),       orphaned);
  _addStat(el, t('storage.integrity.missing'),        missing);
  _addStat(el, t('storage.integrity.cover_mismatches'), covers);
  _addStat(el, t('storage.integrity.chapter_count'),  res.db_chapter_count ?? '—');
  _addStat(el, t('storage.integrity.disk_count'),     res.disk_file_count ?? '—');
}

/** @param {HTMLElement} el */
async function _mountHistory(el) {
  const group = mkSettingsGroup(t('storage.history.group'));
  const card = mkSettingsGroupCard(group);
  el.appendChild(group);

  const placeholder = document.createElement('p');
  placeholder.className = 'px-4 py-3 text-sm text-text-muted';
  placeholder.textContent = 'Loading…';
  card.appendChild(placeholder);

  try {
    const rows = await api.getAdminStorageStatsHistory();
    placeholder.remove();

    if (!Array.isArray(rows) || rows.length === 0) {
      const empty = document.createElement('p');
      empty.className = 'px-4 py-3 text-sm text-text-muted';
      empty.textContent = t('storage.history.empty');
      card.appendChild(empty);
      return;
    }

    const table = document.createElement('table');
    table.className = 'w-full text-xs';

    const thead = document.createElement('thead');
    thead.className = 'text-left border-b border-border-subtle';
    thead.innerHTML = `<tr>
      <th class="px-4 py-2 font-medium text-text-muted">${t('storage.history.date')}</th>
      <th class="px-4 py-2 font-medium text-text-muted text-right">${t('storage.history.chapters')}</th>
      <th class="px-4 py-2 font-medium text-text-muted text-right">${t('storage.history.covers')}</th>
      <th class="px-4 py-2 font-medium text-text-muted text-right">${t('storage.history.free')}</th>
    </tr>`;

    const tbody = document.createElement('tbody');
    tbody.className = 'divide-y divide-border-subtle';
    for (const r of rows.slice(0, 30)) {
      const tr = document.createElement('tr');
      const date = r.captured_at ? new Date(r.captured_at).toLocaleDateString() : '—';
      tr.innerHTML = `
        <td class="px-4 py-2 text-text-muted">${date}</td>
        <td class="px-4 py-2 text-right font-medium text-text">${_fmt(r.chapter_used_bytes)}</td>
        <td class="px-4 py-2 text-right text-text">${_fmt(r.cover_used_bytes)}</td>
        <td class="px-4 py-2 text-right text-text">${_fmt(r.free_bytes)}</td>
      `;
      tbody.appendChild(tr);
    }

    table.appendChild(thead);
    table.appendChild(tbody);
    const wrap = document.createElement('div');
    wrap.className = 'overflow-x-auto';
    wrap.appendChild(table);
    card.appendChild(wrap);
  } catch (e) {
    placeholder.textContent = e.message ?? 'Failed to load history.';
    placeholder.className = 'px-4 py-3 text-sm text-danger';
  }
}

/** @param {HTMLElement} el @param {string} label @param {string|number} value */
function _addStat(el, label, value) {
  const row = document.createElement('div');
  row.className = 'flex items-center justify-between gap-4 px-4 py-2.5 text-sm';
  const lbl = document.createElement('span');
  lbl.className = 'text-text-muted';
  lbl.textContent = label;
  const val = document.createElement('span');
  val.className = 'font-medium text-text';
  val.textContent = String(value);
  row.appendChild(lbl);
  row.appendChild(val);
  el.appendChild(row);
}
