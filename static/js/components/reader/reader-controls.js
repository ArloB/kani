// @ts-check
// Compact DOM builder helpers for the reader side panel.
// All helpers produce vanilla DOM to match the one-shot panel policy.

const BTN_ACTIVE   = 'bg-surface-2 text-text';
const BTN_INACTIVE = 'text-muted hover:bg-surface-2 hover:text-text';

/**
 * Creates a labeled panel section with a bottom border.
 * @param {string} title  — section heading (pass '' to omit the label)
 * @param {...HTMLElement} children
 * @returns {HTMLElement}
 */
export function mkReaderSection(title, ...children) {
  const section = document.createElement('div');
  section.className = 'px-3 py-4 flex flex-col gap-3 border-b border-border';
  if (title) {
    const label = document.createElement('p');
    label.className = 'text-xs font-medium text-muted uppercase tracking-wide';
    label.textContent = title;
    section.appendChild(label);
  }
  for (const child of children) section.appendChild(child);
  return section;
}

/**
 * Creates a collapsible accordion section. Open/closed state is remembered in
 * localStorage keyed by the section title.
 * @param {string} title
 * @param {{ defaultOpen?: boolean }} [opts]
 * @param {...HTMLElement} children
 * @returns {HTMLElement}
 */
export function mkAccordionSection(title, opts = {}, ...children) {
  const key = `kani_reader_acc_${title.toLowerCase().replace(/\s+/g, '_')}`;
  const stored = localStorage.getItem(key);
  const isOpen = stored !== null ? stored === 'true' : (opts.defaultOpen ?? false);

  const section = document.createElement('div');
  section.className = 'border-b border-border';

  const header = document.createElement('button');
  header.className = 'w-full flex items-center justify-between px-3 py-3 text-xs font-medium text-muted uppercase tracking-wide hover:text-text transition-colors';
  header.type = 'button';

  const titleEl = document.createElement('span');
  titleEl.textContent = title;

  const caret = document.createElement('span');
  caret.className = 'transition-transform duration-150 text-base leading-none';
  caret.textContent = '▾';

  header.appendChild(titleEl);
  header.appendChild(caret);

  const body = document.createElement('div');
  body.className = 'flex flex-col gap-3 px-3 pb-4 overflow-hidden';

  const _setOpen = (/** @type {boolean} */ open) => {
    body.style.display = open ? '' : 'none';
    caret.style.transform = open ? '' : 'rotate(-90deg)';
    localStorage.setItem(key, String(open));
  };
  _setOpen(isOpen);

  header.addEventListener('click', () => {
    const nowOpen = body.style.display === 'none';
    _setOpen(nowOpen);
  });

  for (const child of children) body.appendChild(child);
  section.appendChild(header);
  section.appendChild(body);
  return section;
}

/**
 * Creates a toggle row matching the existing reader panel style.
 * @param {{ label: string, checked: boolean, onChange: (v: boolean) => void }} opts
 * @returns {{ row: HTMLElement, input: HTMLInputElement }}
 */
export function mkToggleRow({ label, checked, onChange }) {
  const row = document.createElement('label');
  row.className = 'flex items-center justify-between gap-3 cursor-pointer';

  const labelEl = document.createElement('span');
  labelEl.className = 'text-sm text-text';
  labelEl.textContent = label;

  const toggleLabel = document.createElement('label');
  toggleLabel.className = 'kani-toggle';
  toggleLabel.setAttribute('aria-label', label);

  const input = document.createElement('input');
  input.type = 'checkbox';
  input.className = 'kani-toggle__input';
  input.checked = checked;
  input.addEventListener('change', () => onChange(input.checked));

  const track = document.createElement('span');
  track.className = 'kani-toggle__track';

  toggleLabel.appendChild(input);
  toggleLabel.appendChild(track);
  row.appendChild(labelEl);
  row.appendChild(toggleLabel);

  return { row, input };
}

/**
 * Creates a labelled range slider row with an editable numeric input for fine control.
 * @param {{ label: string, min: number, max: number, value: number, step?: number, onChange: (v: number) => void, unit?: string }} opts
 * @returns {{ row: HTMLElement, input: HTMLInputElement, valueEl: HTMLInputElement }}
 */
export function mkSliderRow({ label, min, max, value, step = 1, onChange, unit = '' }) {
  const row = document.createElement('div');
  row.className = 'flex flex-col gap-1.5';

  const header = document.createElement('div');
  header.className = 'flex items-center justify-between gap-2';

  const labelEl = document.createElement('span');
  labelEl.className = 'text-sm text-text min-w-0 truncate';
  labelEl.textContent = label;

  const valueEl = document.createElement('input');
  valueEl.type = 'number';
  valueEl.min = String(min);
  valueEl.max = String(max);
  valueEl.step = String(step);
  valueEl.value = String(value);
  valueEl.className = 'no-spinners text-xs text-muted tabular-nums text-right w-14 bg-transparent border border-border rounded px-1 py-0.5 shrink-0 focus:outline-none focus:border-accent';
  if (unit) valueEl.title = unit;

  header.appendChild(labelEl);
  header.appendChild(valueEl);

  const input = document.createElement('input');
  input.type = 'range';
  input.min = String(min);
  input.max = String(max);
  input.step = String(step);
  input.value = String(value);
  input.className = 'w-full accent-accent h-1.5 cursor-pointer';

  input.addEventListener('input', () => {
    valueEl.value = input.value;
    onChange(Number(input.value));
  });

  valueEl.addEventListener('change', () => {
    const clamped = Math.max(min, Math.min(max, Number(valueEl.value)));
    valueEl.value = String(clamped);
    input.value   = String(clamped);
    onChange(clamped);
  });
  // Prevent stray keypresses (arrows etc.) from triggering reader navigation while typing.
  valueEl.addEventListener('keydown', (e) => e.stopPropagation());

  row.appendChild(header);
  row.appendChild(input);
  return { row, input, valueEl };
}

/**
 * Creates a segmented control row (label above optional, buttons below).
 * @param {{ label?: string, options: { value: string, label: string }[], selected: string, onSelect: (v: string) => void }} opts
 * @returns {{ row: HTMLElement, update: (v: string) => void }}
 */
export function mkSegmentedRow({ label, options, selected, onSelect }) {
  const row = document.createElement('div');
  row.className = 'flex flex-col gap-1.5';

  if (label) {
    const labelEl = document.createElement('p');
    labelEl.className = 'text-xs text-muted';
    labelEl.textContent = label;
    row.appendChild(labelEl);
  }

  const btns = document.createElement('div');
  btns.className = 'flex gap-2';

  let _current = selected;

  const buttonEls = options.map(opt => {
    const btn = document.createElement('button');
    btn.className = `flex-1 text-sm px-2 py-1.5 rounded-md transition-colors ${opt.value === _current ? BTN_ACTIVE : BTN_INACTIVE}`;
    btn.setAttribute('aria-pressed', String(opt.value === _current));
    btn.textContent = opt.label;
    btn.addEventListener('click', () => {
      _current = opt.value;
      for (const [i, b] of buttonEls.entries()) {
        const active = options[i].value === _current;
        b.className = `flex-1 text-sm px-2 py-1.5 rounded-md transition-colors ${active ? BTN_ACTIVE : BTN_INACTIVE}`;
        b.setAttribute('aria-pressed', String(active));
      }
      onSelect(_current);
    });
    btns.appendChild(btn);
    return btn;
  });

  const update = (v) => {
    _current = v;
    for (const [i, b] of buttonEls.entries()) {
      const active = options[i].value === v;
      b.className = `flex-1 text-sm px-2 py-1.5 rounded-md transition-colors ${active ? BTN_ACTIVE : BTN_INACTIVE}`;
      b.setAttribute('aria-pressed', String(active));
    }
  };

  row.appendChild(btns);
  return { row, update };
}

/**
 * Creates a native select row matching the panel's compact style.
 * The `<select>` uses the existing `input` CSS class so it inherits theme colours.
 * @param {{ label?: string, options: { value: string, label: string }[], selected: string, onChange: (v: string) => void, disabled?: boolean }} opts
 * @returns {{ row: HTMLElement, select: HTMLSelectElement }}
 */
export function mkSelectRow({ label, options, selected, onChange, disabled = false }) {
  const row = document.createElement('div');
  row.className = 'flex flex-col gap-1.5';

  if (label) {
    const labelEl = document.createElement('p');
    labelEl.className = 'text-xs text-muted';
    labelEl.textContent = label;
    row.appendChild(labelEl);
  }

  const select = document.createElement('select');
  select.className = 'input w-full text-sm';
  select.disabled = disabled;

  for (const opt of options) {
    const el = document.createElement('option');
    el.value = opt.value;
    el.textContent = opt.label;
    if (opt.value === selected) el.selected = true;
    select.appendChild(el);
  }

  select.addEventListener('change', () => onChange(select.value));
  row.appendChild(select);
  return { row, select };
}

/**
 * Creates a full-width action button row.
 * @param {{ label: string, onClick: () => void }} opts
 * @returns {HTMLButtonElement}
 */
export function mkActionBtn({ label, onClick }) {
  const btn = document.createElement('button');
  btn.className = 'btn-ghost w-full flex items-center justify-center gap-1 text-sm';
  btn.textContent = label;
  btn.addEventListener('click', onClick);
  return btn;
}
