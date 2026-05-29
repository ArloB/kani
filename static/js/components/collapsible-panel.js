// @ts-check
// Collapsible panel component — toggle-able body section.

import { iconChevronRight } from '../icons.js';

/**
 * @param {HTMLElement} container
 * @param {{
 *   label: string,
 *   open?: boolean,
 *   variant?: 'panel' | 'section',
 *   hint?: string,
 *   renderBody: (bodyEl: HTMLElement) => void
 * }} props
 * @returns {{ toggle: () => void, isOpen: () => boolean, destroy: () => void }}
 */
export function renderCollapsiblePanel(container, { label, open = false, variant = 'panel', hint, renderBody }) {
  let _open = open;

  const isSection = variant === 'section';
  const _uid = Math.random().toString(36).slice(2, 8);
  const _bodyId = 'cp-body-' + _uid;
  const _labelId = 'cp-label-' + _uid;

  const wrap = document.createElement('div');
  wrap.className = isSection
    ? 'border-b border-border last:border-b-0'
    : 'rounded-lg border border-border overflow-hidden';
  container.appendChild(wrap);

  const toggleBtn = document.createElement('button');
  toggleBtn.type = 'button';
  toggleBtn.className = isSection
    ? 'w-full flex items-center justify-between gap-3 py-4 text-left hover:text-accent transition-colors focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2 focus-visible:ring-offset-bg focus-visible:outline-none rounded'
    : 'w-full flex items-center justify-between gap-3 px-4 py-3 text-left hover:bg-surface-2 transition-colors focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2 focus-visible:ring-offset-bg focus-visible:outline-none';
  toggleBtn.setAttribute('aria-controls', _bodyId);
  wrap.appendChild(toggleBtn);

  const labelEl = document.createElement('span');
  labelEl.id = _labelId;
  labelEl.className = isSection
    ? 'text-base font-semibold text-text'
    : 'text-sm font-medium text-text';
  labelEl.textContent = label;
  toggleBtn.appendChild(labelEl);

  const chevron = document.createElement('span');
  chevron.className = 'text-text-muted transition-transform duration-150 shrink-0 icon-sm';
  chevron.innerHTML = iconChevronRight;
  toggleBtn.appendChild(chevron);

  const body = document.createElement('div');
  body.id = _bodyId;
  body.setAttribute('role', 'region');
  body.setAttribute('aria-labelledby', _labelId);
  body.className = isSection
    ? 'pb-4'
    : 'border-t border-border-subtle px-4 py-3';
  wrap.appendChild(body);

  if (hint) {
    const hintEl = document.createElement('p');
    hintEl.className = 'text-xs text-text-muted mb-3';
    hintEl.textContent = hint;
    body.appendChild(hintEl);
  }

  let _bodyRendered = false;

  function _applyState() {
    toggleBtn.setAttribute('aria-expanded', String(_open));
    body.style.display = _open ? '' : 'none';
    chevron.style.transform = _open ? 'rotate(90deg)' : '';
    if (_open && !_bodyRendered) {
      renderBody(body);
      _bodyRendered = true;
    }
  }

  toggleBtn.addEventListener('click', () => {
    _open = !_open;
    _applyState();
  });

  _applyState();

  return {
    toggle() {
      _open = !_open;
      _applyState();
    },
    isOpen() { return _open; },
    destroy() {
      wrap.remove();
    },
  };
}
