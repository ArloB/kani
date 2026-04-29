// @ts-check
// Filter panel component — renders extension-defined filters inside a modal.
// Exported: mountFilterModal(triggerBtn, modalRoot, props)

import { h, render } from 'preact';
import htm from 'htm';
import { Combobox } from './combobox.js';
import { escapeHtml } from '../utils.js';
const html = htm.bind(h);

/**
 * @typedef { { kind: 'Selection', data: { name: string, value: string } }
 * | { kind: 'Checkbox', data: boolean }
 * | { kind: 'TextInput', data: string }
 * | { kind: 'Multiselect', data: string[] } } FilterState
 */

/**
 * @typedef {{
 * id: string,
 * name: string,
 * tag?: 'select'|'checkbox'|'text-input'|'sort'|'multiselect',
 * filter_type?: 'Select'|'Checkbox'|'TextInput'|'Sort'|'Multiselect',
 * options: {filter_name?: string, name: string, value: string}[],
 * default_value?: FilterState | null,
 * semantic?: 'Author'|'Artist'|'Tag'|null,
 * }} FilterDef
 */

/**
 * Mounts a Filters button that opens a modal with an explicit Apply action.
 * Returns a destroy function.
 *
 * @param {HTMLElement} triggerBtn  - The button element to use as the trigger (caller creates it)
 * @param {HTMLElement} modalRoot   - Where to insert the modal overlay (usually document.body)
 * @param {{
 * filterDefs: FilterDef[],
 * activeFilters: Record<string, FilterState>,
 * onChange: (filters: Record<string, FilterState>) => void,
 * }} props
 * @returns {() => void} destroy function
 */
export function mountFilterModal(triggerBtn, modalRoot, { filterDefs, activeFilters, onChange }) {
  // Committed = what has been applied; draft = what's in the open modal
  /** @type {Record<string, FilterState>} */
  let committed = { ...activeFilters };
  /** @type {Record<string, FilterState>} */
  let draft = {};

  /** @type {HTMLElement|null} */
  let overlayEl = null;

  function _updateBadge() {
    const count = Object.keys(committed).length;
    let badge = /** @type {HTMLElement|null} */ (triggerBtn.querySelector('.js-filter-badge'));
    if (count > 0) {
      if (!badge) {
        badge = document.createElement('span');
        badge.className = 'js-filter-badge inline-flex items-center justify-center w-4 h-4 text-2xs font-bold rounded-full bg-accent text-white ml-1.5';
        triggerBtn.appendChild(badge);
      }
      badge.textContent = String(count);
    } else {
      badge?.remove();
    }
  }

  function _openModal() {
    // Snapshot committed → draft for this session
    draft = { ...committed };

    const overlay = document.createElement('div');
    overlay.className = 'fixed inset-0 bg-scrim z-modal flex items-end sm:items-center justify-center p-0 sm:p-4';
    overlayEl = overlay;

    const dialog = document.createElement('div');
    dialog.className = 'bg-surface rounded-t-2xl sm:rounded-xl w-full sm:max-w-lg max-h-sheet flex flex-col shadow-xl overflow-hidden';
    dialog.setAttribute('role', 'dialog');
    dialog.setAttribute('aria-modal', 'true');
    dialog.setAttribute('aria-label', 'Filters');

    // Header
    const header = document.createElement('div');
    header.className = 'flex items-center justify-between px-4 py-3 border-b border-border-subtle shrink-0';
    header.innerHTML = `
      <h2 class="text-sm font-semibold text-text">Filters</h2>
      <button type="button" class="js-close btn-ghost btn-sm px-2! text-text-muted" aria-label="Close">✕</button>
    `;
    dialog.appendChild(header);

    // Body — scrollable filter controls
    const body = document.createElement('div');
    body.className = 'flex-1 overflow-y-auto p-4';
    dialog.appendChild(body);

    /** Rebuild the body controls using the current draft */
    function _rebuildBody() {
      body.innerHTML = '';
      _renderFilterControls(body, filterDefs, draft, (id, stateObj) => {
        if (!stateObj) {
          delete draft[id];
        } else {
          draft[id] = stateObj;
        }
      });
    }
    _rebuildBody();

    // Footer — Reset + Apply
    const footer = document.createElement('div');
    footer.className = 'flex items-center justify-between gap-2 px-4 py-3 border-t border-border-subtle shrink-0';
    footer.innerHTML = `
      <button type="button" class="js-reset btn-ghost btn-sm text-sm">Reset</button>
      <button type="button" class="js-apply btn-primary btn-sm">Apply</button>
    `;
    dialog.appendChild(footer);

    overlay.appendChild(dialog);
    modalRoot.appendChild(overlay);

    const close = () => {
      overlay.remove();
      overlayEl = null;
    };

    header.querySelector('.js-close')?.addEventListener('click', close);
    overlay.addEventListener('click', e => { if (e.target === overlay) close(); });

    footer.querySelector('.js-reset')?.addEventListener('click', () => {
      // Reset draft to defaults and rebuild
      draft = _buildDefaultFilters(filterDefs);
      _rebuildBody();
    });

    footer.querySelector('.js-apply')?.addEventListener('click', () => {
      committed = { ...draft };
      _updateBadge();
      onChange({ ...committed });
      close();
    });

    // Focus Apply on open
    setTimeout(() => /** @type {HTMLElement|null} */ (footer.querySelector('.js-apply'))?.focus(), 50);
  }

  triggerBtn.addEventListener('click', () => {
    if (overlayEl) return; // already open
    _openModal();
  });

  _updateBadge();

  return () => {
    overlayEl?.remove();
    overlayEl = null;
  };
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/**
 * Normalize a filter state value from either adjacently-tagged ({kind, data}) or
 * externally-tagged ({Selection: data}) serde format to the adjacently-tagged form:
 * { kind: 'Selection'|'Checkbox'|'TextInput', data: ... }.
 * @param {any} raw
 * @returns {FilterState|null}
 */
function _normalizeFilterState(raw) {
  if (!raw || typeof raw !== 'object') return /** @type {any} */ (raw);
  if (typeof raw.kind === 'string') return /** @type {FilterState} */ (raw);
  // Externally-tagged: { Selection: {...} } → { kind: 'Selection', data: {...} }
  const entries = Object.entries(raw);
  if (entries.length === 1) return /** @type {FilterState} */ ({ kind: entries[0][0], data: entries[0][1] });
  return null;
}

/**
 * Build an initial filter state from filter defaults.
 * @param {FilterDef[]} filterDefs
 * @returns {Record<string, FilterState>}
 */
function _buildDefaultFilters(filterDefs) {
  /** @type {Record<string, FilterState>} */
  const defaults = {};
  for (const f of filterDefs) {
    if (f.default_value) {
      const normalized = _normalizeFilterState(f.default_value);
      if (normalized) defaults[f.id] = normalized;
    }
  }
  return defaults;
}

/**
 * Normalise filter type tag to a lowercase internal identifier.
 * @param {FilterDef} filter
 * @returns {string}
 */
function _filterTag(filter) {
  const rawType = filter.tag ?? filter.filter_type ?? '';
  return rawType === 'TextInput'    ? 'text-input'
    : rawType === 'Select'          ? 'select'
    : rawType === 'Checkbox'        ? 'checkbox'
    : rawType === 'Sort'            ? 'sort'
    : rawType === 'Multiselect'     ? 'multiselect'
    : rawType.toLowerCase();
}

/**
 * Render filter controls into a container element.
 * @param {HTMLElement} container
 * @param {FilterDef[]} filterDefs
 * @param {Record<string, FilterState>} current  — read for initial state; mutations via onFilterChange
 * @param {(id: string, stateObj: FilterState | null) => void} onFilterChange
 */
function _renderFilterControls(container, filterDefs, current, onFilterChange) {
  const grid = document.createElement('div');
  grid.className = 'grid grid-cols-1 sm:grid-cols-2 gap-3';
  container.appendChild(grid);

  for (const filter of filterDefs) {
    const tag = _filterTag(filter);
    const isCheckbox = tag === 'checkbox';
    const isMultiselect = tag === 'multiselect';

    const wrap = document.createElement('div');
    // Checkbox uses a horizontal row layout (label left, toggle right)
    // Multiselect spans full width
    wrap.className = isCheckbox
      ? 'flex items-center justify-between gap-3 py-1'
      : isMultiselect
        ? 'flex flex-col gap-1 col-span-2'
        : 'flex flex-col gap-1';

    const label = document.createElement(isCheckbox ? 'span' : 'label');
    label.className = 'text-xs font-medium text-text-muted uppercase tracking-wider';
    label.textContent = filter.name; // UI still uses the pretty name
    wrap.appendChild(label);

    // Read state using the safe backend ID
    const curState = current[filter.id] ?? filter.default_value;

    if (tag === 'select') {
      const curVal = curState?.kind === 'Selection' ? curState.data.value : '';
      const hasDefault = !!filter.default_value;
      const select = document.createElement('select');
      select.className = 'input text-sm';
      // Only show "Any" if no default is provided (default already implies a selection)
      if (!hasDefault) {
        select.innerHTML = `<option value="">— Any —</option>`;
      }
      select.innerHTML += filter.options.map(o =>
          `<option value="${escapeHtml(o.value)}"${curVal === o.value ? ' selected' : ''}>${escapeHtml(o.name)}</option>`
        ).join('');
        
      select.addEventListener('change', () => {
        if (!select.value) {
          onFilterChange(filter.id, null);
        } else {
          const opt = filter.options.find(o => o.value === select.value);
          // @ts-ignore
          onFilterChange(filter.id, { kind: 'Selection', data: { name: opt.name, value: opt.value } });
        }
      });
      wrap.appendChild(select);

    } else if (tag === 'checkbox') {
      const checked = curState?.kind === 'Checkbox' ? curState.data : false;
      const toggleLabel = document.createElement('label');
      toggleLabel.className = 'kani-toggle shrink-0 cursor-pointer';
      toggleLabel.innerHTML = `
        <input type="checkbox" class="kani-toggle__input"${checked ? ' checked' : ''} />
        <span class="kani-toggle__track"></span>
      `;
      const input = /** @type {HTMLInputElement} */ (toggleLabel.querySelector('input'));
      input.addEventListener('change', () => {
        onFilterChange(filter.id, input.checked ? { kind: 'Checkbox', data: true } : null);
      });
      wrap.appendChild(toggleLabel);

    } else if (tag === 'text-input') {
      const textVal = curState?.kind === 'TextInput' ? curState.data : '';
      const input = document.createElement('input');
      input.type = 'text';
      input.className = 'input text-sm';
      input.value = textVal;
      input.placeholder = filter.name;
      input.addEventListener('input', () => {
        const trimmed = input.value.trim();
        onFilterChange(filter.id, trimmed ? { kind: 'TextInput', data: trimmed } : null);
      });
      wrap.appendChild(input);

    } else if (tag === 'sort') {
      const curVal = curState?.kind === 'Selection' ? curState.data.value : '';
      const hasDefault = !!filter.default_value;
      const select = document.createElement('select');
      select.className = 'input text-sm';

      if (!hasDefault) {
        select.innerHTML += `<option value="">— Default —</option>`;
      }

      for (const opt of filter.options) {
        const ascVal = `${opt.value}:asc`;
        const descVal = `${opt.value}:desc`;
        select.innerHTML += `
          <option value="${escapeHtml(ascVal)}"${curVal === ascVal ? ' selected' : ''}>${escapeHtml(opt.name)} ↑</option>
          <option value="${escapeHtml(descVal)}"${curVal === descVal ? ' selected' : ''}>${escapeHtml(opt.name)} ↓</option>
        `;
      }

      if (hasDefault && !curVal) {
        const defaultVal = curState?.kind === 'Selection' ? curState.data.value : '';
        const defaultOpt = select.querySelector(`option[value="${CSS.escape(defaultVal)}"]`);
        if (defaultOpt) /** @type {HTMLOptionElement} */ (defaultOpt).selected = true;
      }

      select.addEventListener('change', () => {
        if (!select.value) {
          onFilterChange(filter.id, null);
        } else {
          const [baseVal, dir] = select.value.split(':');
          const opt = filter.options.find(o => o.value === baseVal);
          const dirLabel = dir === 'asc' ? '↑' : '↓';
          // @ts-ignore
          onFilterChange(filter.id, {
            kind: 'Selection',
            data: { name: `${opt.name} ${dirLabel}`, value: select.value }
          });
        }
      });
      wrap.appendChild(select);

    } else if (tag === 'multiselect') {
      // Map filter options to Combobox-compatible {id, name} using index as id
      const comboOptions = filter.options.map((o, i) => ({ id: i, name: o.name }));
      const optionValues = filter.options.map(o => o.value || o.name);

      // Convert current selected values → numeric ids
      const selectedVals = curState?.kind === 'Multiselect' ? curState.data : [];
      const selectedIds = selectedVals
        .map(v => optionValues.indexOf(v))
        .filter(i => i !== -1);

      const mountPoint = document.createElement('div');
      wrap.appendChild(mountPoint);

      const _renderCombo = (/** @type {number[]} */ ids) => {
        render(html`<${Combobox}
          multiple=${true}
          options=${comboOptions}
          value=${ids}
          placeholder=${'Select ' + filter.name.toLowerCase() + '…'}
          onChange=${(/** @type {number[]} */ newIds) => {
            const newVals = newIds.map(i => optionValues[i]).filter(v => v != null);
            onFilterChange(filter.id, newVals.length > 0 ? { kind: 'Multiselect', data: newVals } : null);
            _renderCombo(newIds);
          }}
        />`, mountPoint);
      };
      _renderCombo(selectedIds);
    }

    grid.appendChild(wrap);
  }
}