// @ts-check
// Settings — Collections: manage smart collections.

import * as api from '../../api.js';
import { t } from '../../i18n.js';
import { showToast } from '../../components/toast.js';
import { showConfirm } from '../../components/modal.js';
import { createEmptyState } from '../../components/empty-state.js';
import { createErrorState } from '../../components/error-state.js';
import { mkSettingsGroup, mkSettingsGroupCard } from './_shared.js';
import { mkAddRow, mkEditableRow } from '../../components/editable-row.js';

const _STATUS_LABELS = ['Ongoing', 'Completed', 'Hiatus', 'Cancelled', 'Unknown'];

function _describeRule(ruleJson) {
  try {
    const r = JSON.parse(ruleJson);
    switch (r.op) {
      case 'has_unread': return t('collections.rule.has_unread');
      case 'status': return `${t('collections.rule.status')}: ${_STATUS_LABELS[r.value] ?? r.value}`;
      case 'tag': return `${t('collections.rule.tag')}: ${r.name}`;
      case 'chapter_count_gt': return t('collections.rule.chapter_count_gt_label', { count: r.value });
      case 'chapter_count_lt': return t('collections.rule.chapter_count_lt_label', { count: r.value });
      case 'and': return t('collections.rule.and', { count: r.rules?.length ?? 0 });
      case 'or':  return t('collections.rule.or',  { count: r.rules?.length ?? 0 });
      default: return r.op ?? '—';
    }
  } catch { return '—'; }
}

function _parseSimpleRule(ruleJson) {
  try {
    const r = JSON.parse(ruleJson);
    if (r.op === 'and' || r.op === 'or') return null;
    return r;
  } catch { return null; }
}

function _mkRuleBuilder(initial) {
  const wrap = document.createElement('div');
  wrap.className = 'flex items-center gap-2 flex-wrap';

  const typeSelect = document.createElement('select');
  typeSelect.className = 'input text-sm';

  const ruleTypes = [
    ['has_unread', t('collections.rule.has_unread')],
    ['status', t('collections.rule.status')],
    ['tag', t('collections.rule.tag')],
    ['chapter_count_gt', t('collections.rule.chapter_count_gt')],
    ['chapter_count_lt', t('collections.rule.chapter_count_lt')],
  ];
  for (const [val, label] of ruleTypes) {
    const opt = document.createElement('option');
    opt.value = val;
    opt.textContent = label;
    typeSelect.appendChild(opt);
  }
  if (initial?.op) typeSelect.value = initial.op;

  const valueWrap = document.createElement('div');

  function _syncValue() {
    valueWrap.innerHTML = '';
    const type = typeSelect.value;
    if (type === 'status') {
      const sel = document.createElement('select');
      sel.className = 'input text-sm';
      for (let i = 0; i < _STATUS_LABELS.length; i++) {
        const opt = document.createElement('option');
        opt.value = String(i);
        opt.textContent = _STATUS_LABELS[i];
        sel.appendChild(opt);
      }
      if (initial?.op === 'status') sel.value = String(initial.value ?? 0);
      valueWrap.appendChild(sel);
    } else if (type === 'tag') {
      const inp = document.createElement('input');
      inp.type = 'text';
      inp.className = 'input text-sm w-36';
      inp.placeholder = t('collections.tag.placeholder');
      if (initial?.op === 'tag') inp.value = initial.name ?? '';
      valueWrap.appendChild(inp);
    } else if (type === 'chapter_count_gt' || type === 'chapter_count_lt') {
      const inp = document.createElement('input');
      inp.type = 'number';
      inp.className = 'input text-sm w-20';
      inp.min = '0';
      inp.step = '1';
      inp.placeholder = '0';
      if (initial?.op === type) inp.value = String(initial.value ?? 0);
      valueWrap.appendChild(inp);
    }
  }

  typeSelect.addEventListener('change', () => {
    initial = null;
    _syncValue();
  });
  _syncValue();

  wrap.appendChild(typeSelect);
  wrap.appendChild(valueWrap);

  function getRule() {
    const type = typeSelect.value;
    const ctrl = /** @type {HTMLInputElement|HTMLSelectElement|null} */ (valueWrap.querySelector('select, input'));
    const v = ctrl?.value ?? '';
    switch (type) {
      case 'has_unread': return { op: 'has_unread' };
      case 'status': return { op: 'status', value: Number(v) };
      case 'tag': return { op: 'tag', name: v };
      case 'chapter_count_gt': return { op: 'chapter_count_gt', value: Number(v) };
      case 'chapter_count_lt': return { op: 'chapter_count_lt', value: Number(v) };
      default: return { op: 'has_unread' };
    }
  }

  return { el: wrap, getRule };
}

/** @param {HTMLElement} el */
export function mount(el) {
  el.innerHTML = '';
  _load(el);
  return { destroy() { el.innerHTML = ''; } };
}

/** @param {HTMLElement} el */
async function _load(el) {
  el.innerHTML = `<div class="text-sm text-text-muted px-1 py-4">${t('common.loading')}</div>`;
  try {
    const cols = await api.listCollections();
    _render(el, Array.isArray(cols) ? cols : []);
  } catch (e) {
    el.innerHTML = '';
    el.appendChild(createErrorState({ message: e.message ?? t('collections.error.load') }));
  }
}

/** @param {HTMLElement} el @param {any[]} collections */
function _render(el, collections) {
  el.innerHTML = '';
  const group = mkSettingsGroup();
  const card = mkSettingsGroupCard(group);

  if (collections.length === 0) {
    card.appendChild(createEmptyState({
      title: t('collections.empty.title'),
      subtitle: t('collections.empty.desc'),
    }));
  } else {
    const list = document.createElement('div');
    list.className = 'divide-y divide-border-subtle';
    for (const col of collections) list.appendChild(_mkRow(col, el));
    card.appendChild(list);
  }

  card.appendChild(_mkAddForm(el));
  el.appendChild(group);
}

/**
 * @param {any} col
 * @param {HTMLElement} containerEl
 */
function _mkRow(col, containerEl) {
  const simple = _parseSimpleRule(col.rule_json);

  return mkEditableRow({
    canEdit: !!simple,
    renderView: () => {
      const frag = document.createElement('div');
      frag.className = 'flex-1 flex items-center gap-3 min-w-0';
      const nameEl = document.createElement('span');
      nameEl.className = 'flex-1 text-sm font-medium text-text truncate';
      nameEl.textContent = col.name;
      const descEl = document.createElement('span');
      descEl.className = 'text-xs text-text-muted shrink-0 max-w-[10rem] truncate';
      descEl.textContent = _describeRule(col.rule_json);
      frag.append(nameEl, descEl);
      return frag;
    },
    renderForm: () => {
      const nameInput = document.createElement('input');
      nameInput.type = 'text';
      nameInput.className = 'input text-sm w-full';
      nameInput.value = col.name;

      const { el: builderEl, getRule } = _mkRuleBuilder(simple);

      const wrap = document.createElement('div');
      wrap.className = 'flex flex-col gap-2';
      wrap.append(nameInput, builderEl);

      return {
        el: wrap,
        focusEl: nameInput,
        validate: () => {
          if (!nameInput.value.trim()) { nameInput.focus(); return false; }
          return true;
        },
        reset: () => { nameInput.value = col.name; },
        getValue: () => ({ name: nameInput.value.trim(), rule: getRule() }),
      };
    },
    onSave: async ({ name, rule }) => {
      await api.updateCollection(col.id, { name, rule, sort_order: col.sort_order });
      showToast(t('collections.toast.updated'), { type: 'success' });
      _load(containerEl);
    },
    onDelete: async () => {
      if (!(await showConfirm(t('collections.delete.confirm', { name: col.name }), { confirmLabel: t('common.delete') }))) return;
      await api.deleteCollection(col.id);
      showToast(t('collections.toast.deleted'), { type: 'success' });
      _load(containerEl);
    },
  });
}

/** @param {HTMLElement} containerEl */
function _mkAddForm(containerEl) {
  const wrap = document.createElement('div');
  wrap.className = 'border-t border-border-subtle';

  wrap.appendChild(mkAddRow({
    label: t('collections.add'),
    confirmLabel: t('collections.add'),
    renderForm: () => {
      const nameInput = document.createElement('input');
      nameInput.type = 'text';
      nameInput.className = 'input text-sm w-full';
      nameInput.placeholder = t('collections.name.placeholder');

      const { el: builderEl, getRule } = _mkRuleBuilder(null);

      const formBody = document.createElement('div');
      formBody.className = 'flex flex-col gap-2';
      formBody.append(nameInput, builderEl);

      return {
        el: formBody,
        focusEl: nameInput,
        validate: () => {
          if (!nameInput.value.trim()) { nameInput.focus(); return false; }
          return true;
        },
        reset: () => { nameInput.value = ''; },
        getValue: () => ({ name: nameInput.value.trim(), rule: getRule() }),
      };
    },
    onAdd: async ({ name, rule }) => {
      await api.createCollection({ name, rule, sort_order: 0 });
      showToast(t('collections.toast.created'), { type: 'success' });
      _load(containerEl);
    },
  }));

  return wrap;
}
