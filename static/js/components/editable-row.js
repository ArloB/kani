// @ts-check
// Editable row — view content with edit/delete icon buttons, toggling to an inline
// form (save/cancel) below. Caller owns all API calls and DOM content; onSave/onDelete
// are expected to trigger the caller's own list reload on success.

import { t } from '../i18n.js';
import { iconPencil, iconPlus, iconTrash } from '../icons.js';
import { showApiError } from './toast.js';

/**
 * @param {{
 *   renderView: () => HTMLElement,
 *   renderForm: () => {
 *     el: HTMLElement,
 *     getValue: () => any,
 *     focusEl?: HTMLElement,
 *     validate?: () => boolean,
 *     reset?: () => void,
 *   },
 *   onSave: (value: any) => Promise<void>,
 *   onDelete?: () => Promise<void>,
 *   canEdit?: boolean,
 *   editLabel?: string,
 *   deleteLabel?: string,
 * }} opts
 * @returns {HTMLElement}
 */
export function mkEditableRow({ renderView, renderForm, onSave, onDelete, canEdit = true, editLabel, deleteLabel }) {
  const row = document.createElement('div');
  row.className = 'px-4 py-3';

  const viewRow = document.createElement('div');
  viewRow.className = 'flex items-center gap-3';
  viewRow.appendChild(renderView());

  const editBtn = document.createElement('button');
  editBtn.type = 'button';
  editBtn.className = 'btn-icon text-text-muted shrink-0' + (canEdit ? '' : ' hidden');
  editBtn.setAttribute('aria-label', editLabel ?? t('common.edit'));
  editBtn.innerHTML = iconPencil;
  viewRow.appendChild(editBtn);

  let delBtn = null;
  if (onDelete) {
    delBtn = document.createElement('button');
    delBtn.type = 'button';
    delBtn.className = 'btn-icon text-danger shrink-0';
    delBtn.setAttribute('aria-label', deleteLabel ?? t('common.delete'));
    delBtn.innerHTML = iconTrash;
    viewRow.appendChild(delBtn);
  }

  row.appendChild(viewRow);

  const { el: formEl, getValue, focusEl, validate, reset } = renderForm();

  const editForm = document.createElement('div');
  editForm.className = 'hidden flex-col gap-2 mt-3';

  const editActions = document.createElement('div');
  editActions.className = 'flex gap-2 justify-end';

  const cancelBtn = document.createElement('button');
  cancelBtn.type = 'button';
  cancelBtn.className = 'btn-ghost btn-sm';
  cancelBtn.textContent = t('common.cancel');

  const saveBtn = document.createElement('button');
  saveBtn.type = 'button';
  saveBtn.className = 'btn-primary btn-sm';
  saveBtn.textContent = t('common.save');

  editActions.append(cancelBtn, saveBtn);
  editForm.append(formEl, editActions);
  row.appendChild(editForm);

  const _showEdit = () => {
    editForm.classList.remove('hidden');
    editForm.classList.add('flex');
    editBtn.classList.add('hidden');
    delBtn?.classList.add('hidden');
    focusEl?.focus();
  };

  const _hideEdit = () => {
    editForm.classList.add('hidden');
    editForm.classList.remove('flex');
    editBtn.classList.remove('hidden');
    delBtn?.classList.remove('hidden');
    reset?.();
  };

  editBtn.addEventListener('click', _showEdit);
  cancelBtn.addEventListener('click', _hideEdit);

  saveBtn.addEventListener('click', async () => {
    if (validate && !validate()) return;
    saveBtn.disabled = true;
    try {
      await onSave(getValue());
    } catch (e) {
      showApiError(e);
    } finally {
      saveBtn.disabled = false;
    }
  });

  if (delBtn) {
    delBtn.addEventListener('click', async () => {
      delBtn.disabled = true;
      try {
        await onDelete();
      } catch (e) {
        showApiError(e);
      } finally {
        delBtn.disabled = false;
      }
    });
  }

  return row;
}

/**
 * Trailing "add" row for editable lists: a ghost trigger row that expands into
 * an inline form with explicit Confirm/Cancel buttons (Enter confirms, Escape
 * cancels). The one add idiom across categories, collections, and webhooks.
 * @param {{
 *   label: string,
 *   confirmLabel?: string,
 *   renderForm: () => {
 *     el: HTMLElement,
 *     getValue: () => any,
 *     focusEl?: HTMLElement,
 *     validate?: () => boolean,
 *     reset?: () => void,
 *   },
 *   onAdd: (value: any) => Promise<void>,
 * }} opts
 * @returns {HTMLElement}
 */
export function mkAddRow({ label, confirmLabel, renderForm, onAdd }) {
  const wrap = document.createElement('div');

  const trigger = document.createElement('button');
  trigger.type = 'button';
  trigger.className = 'w-full flex items-center gap-2 px-4 py-3 text-sm font-medium text-text-muted hover:text-text hover:bg-surface-2 transition-colors text-left';
  trigger.setAttribute('aria-expanded', 'false');
  const plus = document.createElement('span');
  plus.className = 'icon-sm shrink-0';
  plus.setAttribute('aria-hidden', 'true');
  plus.innerHTML = iconPlus;
  const labelEl = document.createElement('span');
  labelEl.textContent = label;
  trigger.append(plus, labelEl);
  wrap.appendChild(trigger);

  const { el: formEl, getValue, focusEl, validate, reset } = renderForm();

  const form = document.createElement('div');
  form.className = 'hidden flex-col gap-2 px-4 pb-4 pt-1';

  const actions = document.createElement('div');
  actions.className = 'flex gap-2 justify-end';

  const cancelBtn = document.createElement('button');
  cancelBtn.type = 'button';
  cancelBtn.className = 'btn-ghost btn-sm';
  cancelBtn.textContent = t('common.cancel');

  const confirmBtn = document.createElement('button');
  confirmBtn.type = 'button';
  confirmBtn.className = 'btn-primary btn-sm';
  confirmBtn.textContent = confirmLabel ?? t('common.add');

  actions.append(cancelBtn, confirmBtn);
  form.append(formEl, actions);
  wrap.appendChild(form);

  const _open = () => {
    form.classList.remove('hidden');
    form.classList.add('flex');
    trigger.classList.add('hidden');
    trigger.setAttribute('aria-expanded', 'true');
    focusEl?.focus();
  };

  const _close = () => {
    form.classList.add('hidden');
    form.classList.remove('flex');
    trigger.classList.remove('hidden');
    trigger.setAttribute('aria-expanded', 'false');
    reset?.();
  };

  const _confirm = async () => {
    if (validate && !validate()) return;
    confirmBtn.disabled = true;
    try {
      await onAdd(getValue());
      _close();
    } catch (e) {
      showApiError(e);
    } finally {
      confirmBtn.disabled = false;
    }
  };

  trigger.addEventListener('click', _open);
  cancelBtn.addEventListener('click', _close);
  confirmBtn.addEventListener('click', _confirm);
  form.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && !(e.target instanceof HTMLTextAreaElement)) {
      e.preventDefault();
      _confirm();
    } else if (e.key === 'Escape') {
      _close();
    }
  });

  return wrap;
}
