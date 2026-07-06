// @ts-check
// Manage tab — Download filter rules: add, edit, delete, reorder, live preview.

import * as api from '../../api.js';
import { t } from '../../i18n.js';
import { debounce, escapeHtml } from '../../utils.js';
import { showToast } from '../toast.js';
import { createEmptyState } from '../empty-state.js';
import { Combobox } from '../combobox.js';
import { h, render } from 'preact';
import htm from 'htm';
import { iconX, iconPencil, iconCheck } from '../../icons.js';
const html = htm.bind(h);

// ── Helpers ───────────────────────────────────────────────────────────────────

/** @param {any} kind */
function _ruleLabel(kind) {
  if (typeof kind === 'string') return kind;
  const [k, v] = Object.entries(kind)[0] ?? ['', ''];
  const labels = /** @type {Record<string,string>} */ ({
    LanguageInclude: `Language = ${v}`,
    LanguageExclude: `Language ≠ ${v}`,
    TitleContains: `Title contains "${v}"`,
    TitleExcludes: `Title excludes "${v}"`,
    ChapterNumberMin: `Chapter ≥ ${v}`,
    ChapterNumberMax: `Chapter ≤ ${v}`,
    MaxAgeDays: `Max age: ${v} days`,
    PublishedAfter: `Published after ${new Date(Number(v) * 1000).toLocaleDateString()}`,
  });
  return labels[k] ?? `${k}: ${v}`;
}

/** @param {string} type @param {any} v @returns {any} */
function _buildKind(type, v) {
  if (type === 'ExcludeFractional') return 'ExcludeFractional';
  if (['ChapterNumberMin', 'ChapterNumberMax', 'MaxAgeDays'].includes(type)) return { [type]: Number(v) };
  if (type === 'PublishedAfter') return { PublishedAfter: Math.floor(new Date(v).getTime() / 1000) };
  return { [type]: v };
}

/** Convert stored epoch back to YYYY-MM-DD for a date input. @param {number} epoch */
function _epochToDateInput(epoch) {
  return new Date(epoch * 1000).toISOString().slice(0, 10);
}

// ── Mount ─────────────────────────────────────────────────────────────────────

/**
 * @param {HTMLElement} bodyEl
 * @param {any[]} initialRules
 * @param {number} dbId
 */
export function mountDownloadRulesPanel(bodyEl, initialRules, dbId) {
  let rules = Array.isArray(initialRules) ? [...initialRules] : [];
  /** @type {number|null} */
  let _dragFromIdx = null;

  const wrap = document.createElement('div');
  wrap.className = 'flex flex-col gap-3';
  bodyEl.appendChild(wrap);

  // ── Language combobox state ───────────────────────────────────────────────

  let langOptions = /** @type {Array<{id:number,name:string}>} */ ([]);
  let langCmbVal = '';
  /** @type {HTMLDivElement|null} */ let langCmbMount = null;
  let editLangCmbVal = '';
  /** @type {HTMLDivElement|null} */ let editLangCmbMount = null;

  const _renderLangCmb = (mount, getValue, setValue) => {
    if (!mount) return;
    const used = new Set(
      rules
        .filter(r => typeof r.kind === 'object' && ('LanguageInclude' in r.kind || 'LanguageExclude' in r.kind))
        .map(r => /** @type {string} */ (Object.values(r.kind)[0]))
    );
    const opts = langOptions.filter(o => !used.has(o.name));
    const curVal = getValue();
    if (curVal && !opts.some(o => o.name === curVal)) setValue('');
    render(html`<${Combobox}
      options=${opts}
      value=${opts.find(o => o.name === curVal)?.id ?? null}
      onChange=${(/** @type {any} */ id) => { setValue(opts.find(o => o.id === id)?.name ?? ''); }}
      placeholder=${t('manga.rules.lang_placeholder')}
    />`, mount);
  };

  api.getChapterLanguages(dbId).then(langs => {
    langOptions = (Array.isArray(langs) ? langs : []).map((l, i) => ({ id: i, name: l }));
    _renderLangCmb(langCmbMount, () => langCmbVal, v => { langCmbVal = v; });
    _renderLangCmb(editLangCmbMount, () => editLangCmbVal, v => { editLangCmbVal = v; });
  }).catch(() => {});

  // ── Preview ───────────────────────────────────────────────────────────────

  const previewEl = document.createElement('p');
  previewEl.className = 'text-sm text-text-muted';

  const _refreshPreview = debounce(async () => {
    if (rules.length === 0) { previewEl.textContent = t('manga.rules.preview.all'); return; }
    previewEl.textContent = t('manga.rules.preview.calculating');
    try {
      const res = await api.previewDownloadRules(dbId, rules.map(r => r.kind));
      previewEl.textContent = t('manga.rules.preview.result', { matching: res.matching, total: res.total });
    } catch { previewEl.textContent = ''; }
  }, 400);

  // ── Rule type select builder ───────────────────────────────────────────────

  /**
   * Build a rule-type select + value input group.
   * @param {{ type?: string, value?: string, cmbMount?: (el: HTMLDivElement) => void,
   *           getCmbVal?: () => string, setCmbVal?: (v: string) => void }} opts
   * @returns {{ typeEl: HTMLSelectElement, valEl: HTMLInputElement, cmbWrap: HTMLDivElement, formEl: HTMLDivElement }}
   */
  function _buildRuleForm(opts = {}) {
    const formEl = document.createElement('div');
    formEl.className = 'flex flex-wrap items-center gap-2 mt-2';
    formEl.innerHTML = `
      <select class="input w-auto text-sm js-rule-type">
        <optgroup label="${t('manga.rules.group.language')}">
          <option value="LanguageInclude">${t('manga.rules.type.lang_include')}</option>
          <option value="LanguageExclude">${t('manga.rules.type.lang_exclude')}</option>
        </optgroup>
        <optgroup label="${t('manga.rules.group.title')}">
          <option value="TitleContains">${t('manga.rules.type.title_contains')}</option>
          <option value="TitleExcludes">${t('manga.rules.type.title_excludes')}</option>
        </optgroup>
        <optgroup label="${t('manga.rules.group.chapter_number')}">
          <option value="ChapterNumberMin">${t('manga.rules.type.chapter_min')}</option>
          <option value="ChapterNumberMax">${t('manga.rules.type.chapter_max')}</option>
        </optgroup>
        <optgroup label="${t('manga.rules.group.other')}">
          <option value="ExcludeFractional">${t('manga.rules.type.exclude_fractional')}</option>
          <option value="MaxAgeDays">${t('manga.rules.type.max_age')}</option>
          <option value="PublishedAfter">${t('manga.rules.type.published_after')}</option>
        </optgroup>
      </select>
      <div class="js-rule-cmb-wrap flex-1 min-w-36" style="display:none"></div>
      <input type="text" class="input flex-1 min-w-24 text-sm js-rule-val" placeholder="${t('manga.rules.value_placeholder')}" />
    `;
    const typeEl = /** @type {HTMLSelectElement} */ (formEl.querySelector('.js-rule-type'));
    const valEl  = /** @type {HTMLInputElement} */  (formEl.querySelector('.js-rule-val'));
    const cmbWrap = /** @type {HTMLDivElement} */   (formEl.querySelector('.js-rule-cmb-wrap'));

    if (opts.type) typeEl.value = opts.type;
    if (opts.value && opts.type !== 'ExcludeFractional' && opts.type !== 'LanguageInclude' && opts.type !== 'LanguageExclude') {
      valEl.value = opts.value;
    }
    if (opts.cmbMount) opts.cmbMount(cmbWrap);

    const _syncVisibility = () => {
      const type = typeEl.value;
      const isLang = type === 'LanguageInclude' || type === 'LanguageExclude';
      valEl.style.display = (type === 'ExcludeFractional' || isLang) ? 'none' : '';
      if (type === 'PublishedAfter') valEl.type = 'date';
      else { valEl.type = 'text'; if (!['ChapterNumberMin','ChapterNumberMax','MaxAgeDays'].includes(type)) valEl.placeholder = t('manga.rules.value_placeholder'); else valEl.placeholder = t('manga.rules.number_placeholder'); }
      cmbWrap.style.display = isLang ? '' : 'none';
    };
    typeEl.addEventListener('change', _syncVisibility);
    _syncVisibility();

    return { typeEl, valEl, cmbWrap, formEl };
  }

  // ── Rerender ──────────────────────────────────────────────────────────────

  /** @param {number|null} editingId */
  const rerender = (editingId = null) => {
    wrap.innerHTML = '';
    langCmbMount = null;
    editLangCmbMount = null;
    editLangCmbVal = '';

    if (rules.length > 0) {
      const ul = document.createElement('ul');
      ul.className = 'flex flex-col divide-y divide-border-subtle';

      for (let idx = 0; idx < rules.length; idx++) {
        const rule = rules[idx];
        const li = document.createElement('li');
        li.className = 'flex flex-col gap-2 py-2';
        li.draggable = true;
        li.dataset.idx = String(idx);

        if (editingId === rule.id) {
          // ── Edit mode row ──────────────────────────────────────────────
          const ruleType = typeof rule.kind === 'string' ? rule.kind : Object.keys(rule.kind)[0];
          const ruleVal = typeof rule.kind === 'string' ? '' : String(Object.values(rule.kind)[0]);

          const { typeEl, valEl, cmbWrap, formEl } = _buildRuleForm({
            type: ruleType,
            value: ruleType === 'PublishedAfter' ? _epochToDateInput(Number(ruleVal)) : ruleVal,
            cmbMount: (el) => {
              editLangCmbMount = el;
              if (ruleType === 'LanguageInclude' || ruleType === 'LanguageExclude') {
                editLangCmbVal = ruleVal;
              }
              _renderLangCmb(editLangCmbMount, () => editLangCmbVal, v => { editLangCmbVal = v; });
            },
          });

          const btnRow = document.createElement('div');
          btnRow.className = 'flex gap-2 items-center';
          const confirmBtn = document.createElement('button');
          confirmBtn.type = 'button';
          confirmBtn.className = 'btn-ghost btn-sm flex items-center gap-1';
          confirmBtn.innerHTML = `<span class="icon-xs">${iconCheck}</span> ${t('common.save')}`;
          const cancelBtn = document.createElement('button');
          cancelBtn.type = 'button';
          cancelBtn.className = 'btn-ghost btn-sm text-text-muted';
          cancelBtn.textContent = t('common.cancel');
          btnRow.appendChild(formEl);
          btnRow.appendChild(confirmBtn);
          btnRow.appendChild(cancelBtn);
          li.appendChild(btnRow);

          confirmBtn.addEventListener('click', async () => {
            const type = typeEl.value;
            const isLang = type === 'LanguageInclude' || type === 'LanguageExclude';
            const rawVal = isLang ? editLangCmbVal : valEl.value.trim();
            if (type !== 'ExcludeFractional' && !rawVal) return;
            const kind = _buildKind(type, rawVal);
            try {
              await api.updateDownloadRule(rule.id, kind);
              rules[idx] = { ...rule, kind };
              rerender();
              _refreshPreview();
            } catch (e) {
              showToast(/** @type {any} */(e)?.hint ?? t('manga.rules.update_failed'), { type: 'error' });
            }
          });
          cancelBtn.addEventListener('click', () => rerender());
        } else {
          // ── Display mode row ───────────────────────────────────────────
          const row = document.createElement('div');
          row.className = 'flex items-center justify-between gap-2';
          row.innerHTML = `
            <span class="cursor-grab text-text-faint select-none shrink-0 js-drag-handle" title="${t('manga.rules.drag_reorder')}">⠿</span>
            <span class="text-sm text-text flex-1">${escapeHtml(_ruleLabel(rule.kind))}</span>
            <button class="btn-icon js-edit" data-id="${rule.id}" aria-label="${t('manga.rules.edit_rule')}">${iconPencil}</button>
            <button class="btn-icon text-danger js-rm" data-id="${rule.id}" aria-label="${t('manga.rules.remove_rule')}">${iconX}</button>
          `;
          li.appendChild(row);

          row.querySelector('.js-edit')?.addEventListener('click', () => rerender(rule.id));
          row.querySelector('.js-rm')?.addEventListener('click', async (e) => {
            const id = Number(/** @type {HTMLElement} */(e.currentTarget).dataset.id);
            try {
              await api.deleteDownloadRule(id);
              rules = rules.filter(r => r.id !== id);
              rerender();
              _refreshPreview();
            } catch { /* ignore */ }
          });
        }

        // ── Drag and drop ────────────────────────────────────────────────
        li.addEventListener('dragstart', (e) => {
          _dragFromIdx = idx;
          li.classList.add('opacity-50');
          e.dataTransfer?.setData('text/plain', String(idx));
        });
        li.addEventListener('dragend', () => {
          _dragFromIdx = null;
          li.classList.remove('opacity-50');
        });
        li.addEventListener('dragover', (e) => {
          e.preventDefault();
          if (_dragFromIdx === null || _dragFromIdx === idx) return;
          const moved = rules.splice(_dragFromIdx, 1)[0];
          rules.splice(idx, 0, moved);
          _dragFromIdx = idx;
          rerender();
        });
        li.addEventListener('drop', async (e) => {
          e.preventDefault();
          try {
            await api.reorderDownloadRules(dbId, rules.map(r => r.id));
          } catch { /* best-effort */ }
        });

        ul.appendChild(li);
      }
      wrap.appendChild(ul);
    } else {
      wrap.appendChild(createEmptyState({ title: t('manga.rules.empty') }));
    }

    // ── Add rule form ──────────────────────────────────────────────────────
    const { typeEl, valEl, cmbWrap, formEl } = _buildRuleForm({
      cmbMount: (el) => {
        langCmbMount = el;
        _renderLangCmb(langCmbMount, () => langCmbVal, v => { langCmbVal = v; });
      },
    });

    const addBtn = document.createElement('button');
    addBtn.type = 'button';
    addBtn.className = 'btn-ghost btn-sm';
    addBtn.textContent = t('common.add');

    const addRow = document.createElement('div');
    addRow.className = 'flex flex-wrap items-center gap-2 mt-2';
    addRow.appendChild(formEl);
    addRow.appendChild(addBtn);
    wrap.appendChild(addRow);

    addBtn.addEventListener('click', async () => {
      const type = typeEl.value;
      const isLang = type === 'LanguageInclude' || type === 'LanguageExclude';
      const rawVal = isLang ? langCmbVal : valEl.value.trim();
      if (type !== 'ExcludeFractional' && !rawVal) return;
      const kind = _buildKind(type, rawVal);
      try {
        const newRule = await api.addDownloadRule(dbId, kind);
        if (newRule?.id) rules.push({ id: newRule.id, manga_id: dbId, kind });
        valEl.value = '';
        langCmbVal = '';
        rerender();
        _refreshPreview();
      } catch (e) {
        showToast(/** @type {any} */(e)?.hint ?? /** @type {any} */(e)?.message ?? t('manga.rules.add_failed'), { type: 'error' });
      }
    });

    // ── Preview result ─────────────────────────────────────────────────────
    wrap.appendChild(previewEl);
    if (langOptions.length > 0) {
      _renderLangCmb(langCmbMount, () => langCmbVal, v => { langCmbVal = v; });
      _renderLangCmb(editLangCmbMount, () => editLangCmbVal, v => { editLangCmbVal = v; });
    }
  };

  rerender();
  _refreshPreview();
}
