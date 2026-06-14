// @ts-check
// Shared layout helpers for settings section modules.

/**
 * Creates a titled card group.
 * @param {string} [groupLabel]
 * @returns {HTMLElement}
 */
export function mkSettingsGroup(groupLabel) {
  const wrap = document.createElement('div');
  wrap.className = 'flex flex-col gap-1.5';
  if (groupLabel) {
    const lbl = document.createElement('p');
    lbl.className = 'text-xs font-semibold uppercase tracking-wide text-text-muted px-1';
    lbl.textContent = groupLabel;
    wrap.appendChild(lbl);
  }
  const card = document.createElement('div');
  card.className = 'bg-surface-2 rounded-xl divide-y divide-border-subtle overflow-hidden';
  wrap.appendChild(card);
  return wrap;
}

/** Returns the inner card from a group created with `mkSettingsGroup`. */
export function mkSettingsGroupCard(groupEl) {
  return /** @type {HTMLElement} */ (groupEl.lastElementChild);
}

/**
 * Creates a row: label + optional description left, control right.
 * @param {{ label: string, description?: string, badge?: string, tooltip?: string, control: HTMLElement }} opts
 * @returns {HTMLElement}
 */
export function mkSettingsRow({ label, description, badge, tooltip, control }) {
  const row = document.createElement('div');
  row.className = 'flex items-center justify-between gap-4 px-4 py-3.5';
  if (tooltip) row.setAttribute('data-tooltip', tooltip);
  const left = document.createElement('div');
  left.className = 'flex flex-col gap-0.5 min-w-0';
  const labelEl = document.createElement('div');
  labelEl.className = 'flex items-center gap-2';
  const labelText = document.createElement('span');
  labelText.className = 'text-sm font-medium text-text';
  labelText.textContent = label;
  labelEl.appendChild(labelText);
  if (badge) {
    const badgeEl = document.createElement('span');
    badgeEl.className = 'text-xs px-1.5 py-0.5 rounded bg-warn/20 text-warn font-medium';
    badgeEl.textContent = badge;
    labelEl.appendChild(badgeEl);
  }
  left.appendChild(labelEl);
  if (description) {
    const desc = document.createElement('span');
    desc.className = 'text-xs text-text-muted';
    desc.textContent = description;
    left.appendChild(desc);
  }
  row.appendChild(left);
  control.classList.add('shrink-0');
  row.appendChild(control);
  return row;
}

/**
 * Creates a toggle row.
 * @param {{ label: string, description?: string, tooltip?: string, checked: boolean, onChange: (v: boolean) => void }} opts
 * @returns {HTMLElement}
 */
export function mkToggleRow({ label, description, tooltip, checked, onChange }) {
  const toggleLabel = document.createElement('label');
  toggleLabel.className = 'kani-toggle';
  const input = document.createElement('input');
  input.type = 'checkbox';
  input.className = 'kani-toggle__input';
  input.checked = checked;
  input.addEventListener('change', () => onChange(input.checked));
  const track = document.createElement('span');
  track.className = 'kani-toggle__track';
  toggleLabel.appendChild(input);
  toggleLabel.appendChild(track);
  return mkSettingsRow({ label, description, tooltip, control: toggleLabel });
}

/**
 * Creates a number input row.
 * @param {{ label: string, description?: string, badge?: string, tooltip?: string, id: string, value: any, min?: number, max?: number, onChange: (v: number) => void }} opts
 * @returns {HTMLElement}
 */
export function mkNumberRow({ label, description, badge, tooltip, id, value, min, max, onChange }) {
  const input = document.createElement('input');
  input.type = 'number';
  input.inputMode = 'numeric';
  input.id = id;
  input.className = 'input w-24 text-sm';
  if (value != null) input.value = String(value);
  if (min != null) input.min = String(min);
  if (max != null) input.max = String(max);
  input.addEventListener('change', () => onChange(Number(input.value)));
  return mkSettingsRow({ label, description, badge, tooltip, control: input });
}

