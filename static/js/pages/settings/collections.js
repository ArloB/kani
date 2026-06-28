// @ts-check
// Settings — Collections: manage smart collections.

import * as api from '../../api.js';
import { t } from '../../i18n.js';
import { showToast, showApiError } from '../../components/toast.js';
import { showConfirm } from '../../components/modal.js';
import { createEmptyState } from '../../components/empty-state.js';
import { createErrorState } from '../../components/error-state.js';
import { mkSettingsGroup, mkSettingsGroupCard } from './_shared.js';

const _STATUS_LABELS = ['Ongoing', 'Completed', 'Hiatus', 'Cancelled', 'Unknown'];

function _describeRule(ruleJson) {
  try {
    const r = JSON.parse(ruleJson);
    switch (r.op) {
      case 'has_unread': return t('collections.rule.has_unread');
      case 'status': return `${t('collections.rule.status')}: ${_STATUS_LABELS[r.value] ?? r.value}`;
      case 'tag': return `${t('collections.rule.tag')}: ${r.name}`;
      case 'chapter_count_gt': return `> ${r.value} chapters`;
      case 'chapter_count_lt': return `< ${r.value} chapters`;
      case 'and': return `All of ${r.rules?.length ?? 0} conditions`;
      case 'or': return `Any of ${r.rules?.length ?? 0} conditions`;
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
      inp.placeholder = 'Tag name';
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
  el.innerHTML = '<div class="text-sm text-text-muted px-1 py-4">Loading…</div>';
  try {
    const cols = await api.listCollections();
    _render(el, Array.isArray(cols) ? cols : []);
  } catch (e) {
    el.innerHTML = '';
    el.appendChild(createErrorState({ message: e.message ?? 'Failed to load collections.' }));
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

  const row = document.createElement('div');
  row.className = 'px-4 py-3';

  const viewRow = document.createElement('div');
  viewRow.className = 'flex items-center gap-3';

  const nameEl = document.createElement('span');
  nameEl.className = 'flex-1 text-sm font-medium text-text truncate';
  nameEl.textContent = col.name;

  const descEl = document.createElement('span');
  descEl.className = 'text-xs text-text-muted shrink-0 max-w-[10rem] truncate';
  descEl.textContent = _describeRule(col.rule_json);

  const editBtn = document.createElement('button');
  editBtn.type = 'button';
  editBtn.className = 'btn-icon text-text-muted shrink-0' + (simple ? '' : ' hidden');
  editBtn.setAttribute('aria-label', 'Edit');
  editBtn.innerHTML = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="icon-sm"><path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"/><path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"/></svg>';

  const delBtn = document.createElement('button');
  delBtn.type = 'button';
  delBtn.className = 'btn-icon text-danger shrink-0';
  delBtn.setAttribute('aria-label', 'Delete');
  delBtn.innerHTML = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="icon-sm"><polyline points="3 6 5 6 21 6"/><path d="M19 6l-1 14H6L5 6"/><path d="M10 11v6M14 11v6"/><path d="M9 6V4h6v2"/></svg>';

  viewRow.append(nameEl, descEl, editBtn, delBtn);
  row.appendChild(viewRow);

  const editForm = document.createElement('div');
  editForm.className = 'hidden flex-col gap-2 mt-3';

  const nameInput = document.createElement('input');
  nameInput.type = 'text';
  nameInput.className = 'input text-sm w-full';
  nameInput.value = col.name;

  const { el: builderEl, getRule } = _mkRuleBuilder(simple);

  const editActions = document.createElement('div');
  editActions.className = 'flex gap-2 justify-end';

  const cancelBtn = document.createElement('button');
  cancelBtn.type = 'button';
  cancelBtn.className = 'btn-ghost btn-sm';
  cancelBtn.textContent = 'Cancel';

  const saveBtn = document.createElement('button');
  saveBtn.type = 'button';
  saveBtn.className = 'btn-primary btn-sm';
  saveBtn.textContent = 'Save';

  editActions.append(cancelBtn, saveBtn);
  editForm.append(nameInput, builderEl, editActions);
  row.appendChild(editForm);

  const _showEdit = () => {
    editForm.classList.remove('hidden');
    editForm.classList.add('flex');
    editBtn.classList.add('hidden');
    delBtn.classList.add('hidden');
    nameInput.focus();
  };

  const _hideEdit = () => {
    editForm.classList.add('hidden');
    editForm.classList.remove('flex');
    editBtn.classList.remove('hidden');
    delBtn.classList.remove('hidden');
    nameInput.value = col.name;
  };

  editBtn.addEventListener('click', _showEdit);
  cancelBtn.addEventListener('click', _hideEdit);

  saveBtn.addEventListener('click', async () => {
    const name = nameInput.value.trim();
    if (!name) { nameInput.focus(); return; }
    saveBtn.disabled = true;
    try {
      await api.updateCollection(col.id, { name, rule: getRule(), sort_order: col.sort_order });
      showToast(t('collections.toast.updated'), { type: 'success' });
      _load(containerEl);
    } catch (e) {
      showApiError(e);
      saveBtn.disabled = false;
    }
  });

  delBtn.addEventListener('click', async () => {
    if (!(await showConfirm(t('collections.delete.confirm', { name: col.name }), { confirmLabel: 'Delete' }))) return;
    delBtn.disabled = true;
    try {
      await api.deleteCollection(col.id);
      showToast(t('collections.toast.deleted'), { type: 'success' });
      _load(containerEl);
    } catch (e) {
      showApiError(e);
      delBtn.disabled = false;
    }
  });

  return row;
}

/** @param {HTMLElement} containerEl */
function _mkAddForm(containerEl) {
  const wrap = document.createElement('div');
  wrap.className = 'border-t border-border-subtle';

  const trigger = document.createElement('div');
  trigger.className = 'flex items-center justify-between px-4 py-3';

  const triggerLabel = document.createElement('span');
  triggerLabel.className = 'text-sm font-medium text-text';
  triggerLabel.textContent = t('collections.add');

  const addBtn = document.createElement('button');
  addBtn.type = 'button';
  addBtn.className = 'btn-primary btn-sm';
  addBtn.textContent = '+';
  addBtn.setAttribute('aria-label', t('collections.add'));
  addBtn.setAttribute('aria-expanded', 'false');

  trigger.append(triggerLabel, addBtn);
  wrap.appendChild(trigger);

  const form = document.createElement('div');
  form.className = 'hidden flex-col gap-2 px-4 pb-4';

  const nameInput = document.createElement('input');
  nameInput.type = 'text';
  nameInput.className = 'input text-sm w-full';
  nameInput.placeholder = t('collections.name.placeholder');

  const { el: builderEl, getRule } = _mkRuleBuilder(null);

  const btnRow = document.createElement('div');
  btnRow.className = 'flex gap-2 justify-end';

  const cancelBtn = document.createElement('button');
  cancelBtn.type = 'button';
  cancelBtn.className = 'btn-ghost btn-sm';
  cancelBtn.textContent = 'Cancel';

  const submitBtn = document.createElement('button');
  submitBtn.type = 'button';
  submitBtn.className = 'btn-primary btn-sm';
  submitBtn.textContent = t('collections.add');

  btnRow.append(cancelBtn, submitBtn);
  form.append(nameInput, builderEl, btnRow);
  wrap.appendChild(form);

  const _showForm = () => {
    form.classList.remove('hidden');
    form.classList.add('flex');
    addBtn.setAttribute('aria-expanded', 'true');
    nameInput.focus();
  };

  const _hideForm = () => {
    form.classList.add('hidden');
    form.classList.remove('flex');
    addBtn.setAttribute('aria-expanded', 'false');
    nameInput.value = '';
  };

  addBtn.addEventListener('click', _showForm);
  cancelBtn.addEventListener('click', _hideForm);

  submitBtn.addEventListener('click', async () => {
    const name = nameInput.value.trim();
    if (!name) { nameInput.focus(); return; }
    submitBtn.disabled = true;
    try {
      await api.createCollection({ name, rule: getRule(), sort_order: 0 });
      showToast(t('collections.toast.created'), { type: 'success' });
      _load(containerEl);
    } catch (e) {
      showApiError(e);
      submitBtn.disabled = false;
    }
  });

  return wrap;
}
