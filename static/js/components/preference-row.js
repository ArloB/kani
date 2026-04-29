// @ts-check
// Preference row — renders a single source preference of any widget type.

import { h } from 'preact';
import { useState } from 'preact/hooks';
import htm from 'htm';
import * as api from '../api.js';
import { iconX, iconChevronRight, iconChevronLeft } from '../icons.js';
import { Icon } from './icon.js';
import { getJsonSafe } from '../utils.js';
import { Combobox } from './combobox.js';
const html = htm.bind(h);

/**
 * @typedef {{
 *   key: string,
 *   kind: 'Toggle'|'Select'|'Text'|'Label'|'TextInput'|'Checkbox'|'Number'|'MultiSelect'|'MultiValueList',
 *   label?: string,
 *   title?: string,
 *   description?: string | null,
 *   options?: Array<[string, string]|{label: string, value: string}> | null,
 *   secret?: boolean,
 *   requires_key?: string | null,
 * }} PreferenceDescriptor
 */

/**
 * @param {{
 *   sourceId: number,
 *   descriptor: PreferenceDescriptor,
 *   currentValue: any,
 *   liveValues: Record<string, any>,
 *   onValueChange: (key: string, value: any) => void,
 *   onDirtyChange?: (dirty: boolean) => void,
 *   dirty?: boolean,
 * }} props
 */
export function PreferenceRow({ sourceId, descriptor, currentValue, liveValues, onValueChange, onOpenDetail, onDirtyChange, dirty = false }) {
  // Support both new shape (label, kind as plain string, options as [[label,val],...])
  // and old shape (title, kind as tagged-enum object, options nested inside kind data).
  const key = descriptor.key;
  const title = descriptor.label ?? descriptor.title ?? '';
  const description = descriptor.description;
  const requires_key = descriptor.requires_key;

  const rawKind = descriptor.kind;
  const kindName = typeof rawKind === 'string' ? rawKind : Object.keys(rawKind ?? {})[0] ?? '';
  const kindData = typeof rawKind === 'string' ? {} : (rawKind[kindName] ?? {});

  // Options: new format [[label, value], ...] tuples, or old format {label, value} objects
  /** @type {Array<{label: string, value: string}>} */
  const selectOptions = (() => {
    const src = Array.isArray(descriptor.options) ? descriptor.options
              : Array.isArray(kindData.options)   ? kindData.options
              : [];
    return src.map(opt => Array.isArray(opt) ? { label: opt[0], value: opt[1] } : opt);
  })();

  // Visibility check
  if (requires_key) {
    const dep = liveValues[requires_key];
    const isTruthy = dep === true || dep === 'true' || (typeof dep === 'string' && dep !== '' && dep !== 'false');
    if (!isTruthy) return null;
  }

  const [textVal, setTextVal] = useState(String(currentValue ?? ''));
  const [numVal, setNumVal] = useState(String(currentValue ?? ''));
  const [saving, setSaving] = useState(false);
  const [mvlInput, setMvlInput] = useState('');

  async function save(value) {
    setSaving(true);
    try {
      await api.setPreference(sourceId, key, String(value));
      onValueChange(key, value);
    } finally {
      setSaving(false);
    }
  }

  const meta = html`
    <div class="flex flex-col gap-0.5 flex-1 min-w-0">
      <span class="text-sm font-medium text-text">${title}</span>
      ${description && html`<span class="text-xs text-text-muted">${description}</span>`}
    </div>
  `;

  let control;

  if (kindName === 'Label') {
    return html`
      <div class="flex items-start gap-4 py-3">
        ${meta}
        <span class="text-sm text-text-muted shrink-0">${String(currentValue ?? '')}</span>
      </div>
    `;
  }

  if (kindName === 'TextInput' || kindName === 'Text') {
    const isSecret = descriptor.secret || kindName === 'TextInput' && kindData.secret;
    control = html`
      <div class="flex items-center gap-2">
        <input
          type=${isSecret ? 'password' : 'text'}
          class="input"
          value=${textVal}
          onInput=${(e) => setTextVal(/** @type {HTMLInputElement} */ (e.target).value)}
        />
        <button class="btn-primary btn-sm" disabled=${saving} onClick=${() => save(textVal)}>
          ${saving ? '…' : 'Save'}
        </button>
      </div>
    `;
  } else if (kindName === 'Checkbox' || kindName === 'Toggle') {
    const checked = currentValue === true || currentValue === 'true';
    control = html`
      <label class="kani-toggle">
        <input
          type="checkbox"
          class="kani-toggle__input"
          checked=${checked}
          aria-label=${title}
          onChange=${async () => {
            await save(JSON.stringify(!checked));
            onValueChange(key, !checked);
          }}
        />
        <span class="kani-toggle__track"></span>
      </label>
    `;
  } else if (kindName === 'Number') {
    control = html`
      <div class="flex items-center gap-2">
        <input
          type="number"
          class="input w-24"
          value=${numVal}
          onInput=${(e) => setNumVal(/** @type {HTMLInputElement} */ (e.target).value)}
        />
        <button class="btn-primary btn-sm" disabled=${saving} onClick=${() => save(numVal)}>
          ${saving ? '…' : 'Save'}
        </button>
      </div>
    `;
  } else if (kindName === 'Select') {
    control = html`
      <select
        class="input"
        value=${currentValue}
        onChange=${async (e) => save(/** @type {HTMLSelectElement} */ (e.target).value)}
      >
        ${selectOptions.map(opt => html`<option key=${opt.value} value=${opt.value}>${opt.label}</option>`)}
      </select>
    `;
  } else if (kindName === 'MultiSelect') {
    const selected = /** @type {string[]} */ (Array.isArray(currentValue) ? currentValue : []);
    // Map selectOptions to Combobox {id, name} using index as id
    const comboOptions = selectOptions.map((o, i) => ({ id: i, name: o.label }));
    const selectedIds = selected.map(v => selectOptions.findIndex(o => o.value === v)).filter(i => i !== -1);

    control = html`
      <${Combobox}
        multiple=${true}
        options=${comboOptions}
        value=${selectedIds}
        onChange=${async (/** @type {number[]} */ newIds) => {
          const newVals = newIds.map(i => selectOptions[i]?.value).filter(Boolean);
          // Compute added and removed values to call the API
          const added = newVals.filter(v => !selected.includes(v));
          const removed = selected.filter(v => !newVals.includes(v));
          for (const v of added) await api.togglePreferenceSelect(sourceId, key, v, true);
          for (const v of removed) await api.togglePreferenceSelect(sourceId, key, v, false);
          onValueChange(key, newVals);
        }}
      />`;
  } else if (kindName === 'MultiValueList') {
    const list /** @type {string[]} */ = getJsonSafe(currentValue) || [];
    const placeholder = selectOptions.find(o => o.label === 'placeholder')?.value ?? 'Add item…';

    if (onOpenDetail) {
      return html`
        <button
          class="flex items-center gap-4 py-3 w-full text-left hover:bg-surface-2 -mx-4 px-4 rounded-lg transition-colors"
          onClick=${() => onOpenDetail(descriptor)}
        >
          ${meta}
          <div class="shrink-0 flex items-center gap-1.5 text-text-muted icon-sm">
            <span class="text-sm">${list.length} item${list.length !== 1 ? 's' : ''}</span>
            <${Icon} svg=${iconChevronRight} />
          </div>
        </button>
      `;
    }

    control = html`
      <div class="flex flex-col gap-2">
        <ul class="flex flex-col gap-1">
          ${list.map((item, i) => html`
            <li key=${i} class="flex items-center gap-2 px-2 py-1 rounded bg-surface-2">
              <span class="flex-1 text-sm text-text">${item}</span>
              <button
                class="btn-icon w-8 h-8 text-text-muted"
                aria-label=${'Remove ' + item}
                onClick=${async () => {
                  await api.removePreferenceItem(sourceId, key, item);
                  onValueChange(key, list.filter((_, j) => j !== i));
                }}
              ><${Icon} svg=${iconX} /></button>
            </li>
          `)}
        </ul>
        <div class="flex items-center gap-2">
          <input
            type="text"
            class="input flex-1"
            placeholder=${placeholder}
            value=${mvlInput}
            onInput=${(e) => setMvlInput(/** @type {HTMLInputElement} */ (e.target).value)}
            onKeyDown=${async (e) => {
              if (e.key === 'Enter' && mvlInput.trim()) {
                await api.appendPreferenceItem(sourceId, key, mvlInput.trim());
                onValueChange(key, [...list, mvlInput.trim()]);
                setMvlInput('');
              }
            }}
          />
          <button
            class="btn-primary btn-sm"
            disabled=${!mvlInput.trim()}
            onClick=${async () => {
              if (!mvlInput.trim()) return;
              await api.appendPreferenceItem(sourceId, key, mvlInput.trim());
              onValueChange(key, [...list, mvlInput.trim()]);
              setMvlInput('');
            }}
          >Add</button>
        </div>
      </div>
    `;
  } else {
    control = html`<span class="text-sm text-danger">Unknown preference kind: ${kindName}</span>`;
  }

  return html`
    <div class=${'flex items-start gap-4 py-3' + (dirty ? ' pref-dirty' : '')}>
      ${meta}
      <div class=${'shrink-0 pref-ctrl' + (dirty ? ' [&_.input]:border-warn [&_select]:border-warn' : '')}>${control}</div>
    </div>
  `;
}

/**
 * Full-width detail view for complex preference types (MultiValueList, MultiSelect).
 * Rendered in place of the accordion list when the user drills into a preference.
 * @param {{
 *   sourceId: number,
 *   descriptor: PreferenceDescriptor,
 *   currentValue: any,
 *   liveValues: Record<string, any>,
 *   onValueChange: (key: string, value: any) => void,
 *   onBack: () => void,
 * }} props
 */
export function PreferenceDetailView({ sourceId, descriptor, currentValue, liveValues, onValueChange, onBack }) {
  const key = descriptor.key;
  const title = descriptor.label ?? descriptor.title ?? '';
  const description = descriptor.description;

  const rawKind = descriptor.kind;
  const kindName = typeof rawKind === 'string' ? rawKind : Object.keys(rawKind ?? {})[0] ?? '';
  const kindData = typeof rawKind === 'string' ? {} : (rawKind[kindName] ?? {});

  const selectOptions = (() => {
    const src = Array.isArray(descriptor.options) ? descriptor.options
              : Array.isArray(kindData.options)   ? kindData.options
              : [];
    return src.map(opt => Array.isArray(opt) ? { label: opt[0], value: opt[1] } : opt);
  })();

  const [mvlInput, setMvlInput] = useState('');
  const [saving, setSaving] = useState(false);

  const header = html`
    <div class="flex items-start gap-3 pb-4 mb-2 border-b border-border-subtle">
      <button class="btn-ghost btn-sm shrink-0 flex items-center gap-1 icon-sm" onClick=${onBack}>
        <${Icon} svg=${iconChevronLeft} /> Back
      </button>
      <div class="flex flex-col gap-0.5 min-w-0">
        <span class="text-sm font-medium text-text">${title}</span>
        ${description && html`<span class="text-xs text-text-muted">${description}</span>`}
      </div>
    </div>
  `;

  if (kindName === 'MultiValueList') {
    const list = /** @type {string[]} */ (getJsonSafe(currentValue) || []);
    const placeholder = selectOptions.find(o => o.label === 'placeholder')?.value ?? 'Add item…';
    return html`
      <div class="flex flex-col gap-3 py-3">
        ${header}
        <ul class="flex flex-col gap-1">
          ${list.map((item, i) => html`
            <li key=${i} class="flex items-center gap-2 px-2 py-1 rounded bg-surface-2">
              <span class="flex-1 text-sm text-text">${item}</span>
              <button
                class="btn-icon w-8 h-8 text-text-muted"
                aria-label=${'Remove ' + item}
                onClick=${async () => {
                  await api.removePreferenceItem(sourceId, key, item);
                  onValueChange(key, list.filter((_, j) => j !== i));
                }}
              ><${Icon} svg=${iconX} /></button>
            </li>
          `)}
        </ul>
        <div class="flex items-center gap-2">
          <input
            type="text"
            class="input flex-1"
            placeholder=${placeholder}
            value=${mvlInput}
            onInput=${(e) => setMvlInput(/** @type {HTMLInputElement} */ (e.target).value)}
            onKeyDown=${async (e) => {
              if (e.key === 'Enter' && mvlInput.trim()) {
                await api.appendPreferenceItem(sourceId, key, mvlInput.trim());
                onValueChange(key, [...list, mvlInput.trim()]);
                setMvlInput('');
              }
            }}
          />
          <button
            class="btn-primary btn-sm"
            disabled=${!mvlInput.trim()}
            onClick=${async () => {
              if (!mvlInput.trim()) return;
              await api.appendPreferenceItem(sourceId, key, mvlInput.trim());
              onValueChange(key, [...list, mvlInput.trim()]);
              setMvlInput('');
            }}
          >Add</button>
        </div>
      </div>
    `;
  }

  if (kindName === 'MultiSelect') {
    const selected = /** @type {string[]} */ (Array.isArray(currentValue) ? currentValue : []);
    const comboOptions = selectOptions.map((o, i) => ({ id: i, name: o.label }));
    const selectedIds = selected.map(v => selectOptions.findIndex(o => o.value === v)).filter(i => i !== -1);
    return html`
      <div class="flex flex-col gap-3">
        ${header}
        <${Combobox}
          multiple=${true}
          options=${comboOptions}
          value=${selectedIds}
          onChange=${async (/** @type {number[]} */ newIds) => {
            setSaving(true);
            try {
              const newVals = newIds.map(i => selectOptions[i]?.value).filter(Boolean);
              const added = newVals.filter(v => !selected.includes(v));
              const removed = selected.filter(v => !newVals.includes(v));
              for (const v of added) await api.togglePreferenceSelect(sourceId, key, v, true);
              for (const v of removed) await api.togglePreferenceSelect(sourceId, key, v, false);
              onValueChange(key, newVals);
            } finally {
              setSaving(false);
            }
          }}
        />
      </div>
    `;
  }

  return html`<p class="text-sm text-danger">No detail view for kind: ${kindName}</p>`;
}
