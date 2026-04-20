// @ts-check
// Combobox — searchable select with virtual scrolling.
// Supports single-select (default) and multi-select (multiple={true}) modes.

import { h } from 'preact';
import { useState, useEffect, useRef, useMemo } from 'preact/hooks';
import htm from 'htm';
import { iconX, iconChevronDown } from '../icons.js';
import { Icon } from './icon.js';
import { Pill } from './pill.js';
const html = htm.bind(h);

const ITEM_H = 36;
const VISIBLE = 8;

/**
 * Single-select mode (default):
 *   value: number | null, onChange: (id: number | null) => void
 *
 * Multi-select mode (multiple={true}):
 *   value: number[], onChange: (ids: number[]) => void
 *
 * @param {{
 *   options: Array<{ id: number, name: string }>,
 *   value: number | null | number[],
 *   onChange: ((id: number | null) => void) | ((ids: number[]) => void),
 *   placeholder?: string,
 *   disabled?: boolean,
 *   multiple?: boolean,
 * }} props
 */
export function Combobox({ options, value, onChange, placeholder = 'Select…', disabled = false, multiple = false }) {
  if (multiple) {
    return html`<${MultiCombobox}
      options=${options}
      value=${/** @type {number[]} */ (Array.isArray(value) ? value : [])}
      onChange=${/** @type {(ids: number[]) => void} */ (onChange)}
      placeholder=${placeholder}
      disabled=${disabled}
    />`;
  }
  return html`<${SingleCombobox}
    options=${options}
    value=${/** @type {number | null} */ (value)}
    onChange=${/** @type {(id: number | null) => void} */ (onChange)}
    placeholder=${placeholder}
    disabled=${disabled}
  />`;
}

// ── Single-select ─────────────────────────────────────────────────────────────

function SingleCombobox({ options, value, onChange, placeholder, disabled }) {
  const [inputText, setInputText] = useState(() => {
    const opt = options.find(o => o.id === value);
    return opt ? opt.name : '';
  });
  const [open, setOpen] = useState(false);
  const [highlighted, setHighlighted] = useState(0);
  const [scrollTop, setScrollTop] = useState(0);
  const isTyping = useRef(false);
  // Tracks the last confirmed selection independently of the value prop, so _close()
  // doesn't depend on a stale closure if the parent re-renders asynchronously.
  const selectedIdRef = useRef(value);
  const dropdownRef = useRef(/** @type {HTMLDivElement | null} */(null));
  const wrapRef = useRef(/** @type {HTMLDivElement | null} */(null));

  // Sync input text and selectedIdRef from value prop when not typing
  useEffect(() => {
    selectedIdRef.current = value;
    if (isTyping.current) return;
    const opt = options.find(o => o.id === value);
    setInputText(opt ? opt.name : '');
  }, [value, options]);

  const filtered = useMemo(() => {
    const q = inputText.trim().toLowerCase();
    if (!q || !isTyping.current) return options;
    return options.filter(o => o.name.toLowerCase().includes(q));
  }, [inputText, options]);

  // Reset highlight when filtered list changes
  useEffect(() => { setHighlighted(0); }, [filtered]);

  // Close on outside click
  useEffect(() => {
    if (!open) return;
    /** @param {MouseEvent} e */
    const handler = (e) => {
      const target = /** @type {Node} */ (e.target);
      // If the clicked element was detached by a synchronous re-render triggered from
      // onChange (e.g. _mountComboboxes), treat it as an inside click.
      if (!document.contains(target)) return;
      if (!wrapRef.current?.contains(target)) _close();
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, [open]);

  function _close() {
    isTyping.current = false;
    setOpen(false);
    // Use selectedIdRef rather than the value prop to avoid stale-closure issues
    // when the parent hasn't re-rendered yet with the new value.
    const opt = options.find(o => o.id === selectedIdRef.current);
    setInputText(opt ? opt.name : '');
  }

  function _select(opt) {
    selectedIdRef.current = opt.id;
    isTyping.current = false;
    onChange(opt.id);
    setInputText(opt.name);
    setOpen(false);
  }

  /** @param {MouseEvent} e */
  function _clear(e) {
    e.stopPropagation();
    selectedIdRef.current = null;
    isTyping.current = false;
    onChange(null);
    setInputText('');
    setOpen(false);
  }

  function _scrollTo(idx) {
    const dd = dropdownRef.current;
    if (!dd) return;
    const top = idx * ITEM_H;
    if (top < dd.scrollTop) dd.scrollTop = top;
    else if (top + ITEM_H > dd.scrollTop + dd.clientHeight) dd.scrollTop = top + ITEM_H - dd.clientHeight;
  }

  /** @param {KeyboardEvent} e */
  function _onKeyDown(e) {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      const next = Math.min(highlighted + 1, filtered.length - 1);
      setHighlighted(next);
      _scrollTo(next);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      const prev = Math.max(highlighted - 1, 0);
      setHighlighted(prev);
      _scrollTo(prev);
    } else if (e.key === 'Enter') {
      e.preventDefault();
      if (open && filtered[highlighted]) _select(filtered[highlighted]);
      else setOpen(true);
    } else if (e.key === 'Escape' || e.key === 'Tab') {
      _close();
    }
  }

  // Virtual scroll window
  const winStart = Math.max(0, Math.floor(scrollTop / ITEM_H) - 2);
  const winItems = filtered.slice(winStart, winStart + VISIBLE + 4);
  const totalH = filtered.length * ITEM_H;
  const dropH = Math.min(filtered.length, VISIBLE) * ITEM_H;

  return html`
    <div class="relative" ref=${wrapRef}>
      <div class="relative flex items-center">
        <input
          type="text"
          role="combobox"
          class="input pr-8"
          value=${inputText}
          placeholder=${placeholder}
          disabled=${disabled}
          aria-expanded=${open}
          aria-autocomplete="list"
          aria-activedescendant=${open && filtered[highlighted] ? 'combobox-opt-' + filtered[highlighted].id : undefined}
          onInput=${(e) => {
            isTyping.current = true;
            setInputText(/** @type {HTMLInputElement} */(e.target).value);
            if (!open) setOpen(true);
          }}
          onFocus=${() => { if (!disabled) setOpen(true); }}
          onKeyDown=${_onKeyDown}
        />
        ${(value != null || inputText.trim() !== '')
          ? html`<button type="button" class="absolute right-1 top-1/2 -translate-y-1/2 btn-icon w-8 h-8 border-0" aria-label="Clear" onClick=${_clear}><${Icon} svg=${iconX} /></button>`
          : html`<span class="absolute right-2 top-1/2 -translate-y-1/2 text-text-muted pointer-events-none [&_svg]:w-4 [&_svg]:h-4" aria-hidden="true"><${Icon} svg=${iconChevronDown} /></span>`
        }
      </div>
      ${open && filtered.length > 0 && html`
        <div
          role="listbox"
          class="absolute top-full left-0 right-0 mt-1 bg-surface border border-border rounded-lg shadow-lg z-[300] overflow-y-auto"
          ref=${dropdownRef}
          style=${{ height: dropH + 'px' }}
          onScroll=${(e) => setScrollTop(/** @type {HTMLElement} */(e.target).scrollTop)}
        >
          <div style=${{ height: totalH + 'px', position: 'relative' }}>
            ${winItems.map((opt, i) => {
              const idx = winStart + i;
              return html`
                <div
                  id=${'combobox-opt-' + opt.id}
                  key=${opt.id}
                  role="option"
                  aria-selected=${idx === highlighted}
                  class=${'flex items-center px-3 text-sm cursor-pointer ' + (idx === highlighted ? 'bg-surface-2 text-text' : 'text-text hover:bg-surface-2')}
                  style=${{ position: 'absolute', top: idx * ITEM_H + 'px', width: '100%', height: ITEM_H + 'px' }}
                  onMouseDown=${(e) => { e.preventDefault(); _select(opt); }}
                  onMouseEnter=${() => setHighlighted(idx)}
                >
                  ${opt.name}
                </div>
              `;
            })}
          </div>
        </div>
      `}
    </div>
  `;
}

// ── Multi-select ──────────────────────────────────────────────────────────────

function MultiCombobox({ options, value, onChange, placeholder, disabled }) {
  const selectedIds = /** @type {number[]} */ (Array.isArray(value) ? value : []);

  const [inputText, setInputText] = useState('');
  const [open, setOpen] = useState(false);
  const [highlighted, setHighlighted] = useState(0);
  const [scrollTop, setScrollTop] = useState(0);
  const dropdownRef = useRef(/** @type {HTMLDivElement | null} */(null));
  const wrapRef = useRef(/** @type {HTMLDivElement | null} */(null));
  const inputRef = useRef(/** @type {HTMLInputElement | null} */(null));

  const filtered = useMemo(() => {
    const q = inputText.trim().toLowerCase();
    if (!q) return options;
    return options.filter(o => o.name.toLowerCase().includes(q));
  }, [inputText, options]);

  useEffect(() => { setHighlighted(0); }, [filtered]);

  // Close on outside click
  useEffect(() => {
    if (!open) return;
    /** @param {MouseEvent} e */
    const handler = (e) => {
      const target = /** @type {Node} */ (e.target);
      if (!document.contains(target)) return;
      if (!wrapRef.current?.contains(target)) { setOpen(false); setInputText(''); }
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, [open]);

  function _toggle(opt) {
    const idx = selectedIds.indexOf(opt.id);
    onChange(idx === -1 ? [...selectedIds, opt.id] : selectedIds.filter(id => id !== opt.id));
  }

  function _scrollTo(idx) {
    const dd = dropdownRef.current;
    if (!dd) return;
    const top = idx * ITEM_H;
    if (top < dd.scrollTop) dd.scrollTop = top;
    else if (top + ITEM_H > dd.scrollTop + dd.clientHeight) dd.scrollTop = top + ITEM_H - dd.clientHeight;
  }

  /** @param {KeyboardEvent} e */
  function _onKeyDown(e) {
    if (e.key === 'Backspace' && inputText === '' && selectedIds.length > 0) {
      // Remove last pill on backspace when search is empty
      onChange(selectedIds.slice(0, -1));
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      const next = Math.min(highlighted + 1, filtered.length - 1);
      setHighlighted(next);
      _scrollTo(next);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      const prev = Math.max(highlighted - 1, 0);
      setHighlighted(prev);
      _scrollTo(prev);
    } else if (e.key === 'Enter') {
      e.preventDefault();
      if (open && filtered[highlighted]) _toggle(filtered[highlighted]);
      else setOpen(true);
    } else if (e.key === 'Escape') {
      setOpen(false);
      setInputText('');
    }
  }

  const winStart = Math.max(0, Math.floor(scrollTop / ITEM_H) - 2);
  const winItems = filtered.slice(winStart, winStart + VISIBLE + 4);
  const totalH = filtered.length * ITEM_H;
  const dropH = Math.min(filtered.length, VISIBLE) * ITEM_H;

  return html`
    <div class="relative flex flex-col gap-1.5" ref=${wrapRef}>
      ${selectedIds.length > 0 && html`
        <div
          class="flex flex-wrap gap-1.5"
          onClick=${(/** @type {MouseEvent} */ e) => { e.stopPropagation(); if (open) { setOpen(false); setInputText(''); } }}
        >
          ${selectedIds.map(id => {
            const opt = options.find(o => o.id === id);
            if (!opt) return null;
            return html`<${Pill} key=${id} label=${opt.name} onDismiss=${() => _toggle(opt)} />`;
          })}
        </div>
      `}
      <div class="relative flex items-center">
        <input
          ref=${inputRef}
          type="text"
          role="combobox"
          class="input pr-8"
          value=${inputText}
          placeholder=${placeholder}
          disabled=${disabled}
          aria-expanded=${open}
          aria-autocomplete="list"
          aria-multiselectable="true"
          onInput=${(/** @type {any} */ e) => {
            setInputText(/** @type {HTMLInputElement} */(e.target).value);
            if (!open) setOpen(true);
          }}
          onFocus=${() => { if (!disabled) setOpen(true); }}
          onKeyDown=${_onKeyDown}
        />
        <span class="absolute right-2 top-1/2 -translate-y-1/2 text-text-muted pointer-events-none [&_svg]:w-4 [&_svg]:h-4" aria-hidden="true">
          <${Icon} svg=${iconChevronDown} />
        </span>
      </div>
      ${open && filtered.length > 0 && html`
        <div
          role="listbox"
          aria-multiselectable="true"
          class="absolute top-full left-0 right-0 mt-1 bg-surface border border-border rounded-lg shadow-lg z-[300] overflow-y-auto"
          ref=${dropdownRef}
          style=${{ height: dropH + 'px' }}
          onScroll=${(/** @type {any} */ e) => setScrollTop(/** @type {HTMLElement} */(e.target).scrollTop)}
        >
          <div style=${{ height: totalH + 'px', position: 'relative' }}>
            ${winItems.map((opt, i) => {
              const idx = winStart + i;
              const isSelected = selectedIds.includes(opt.id);
              const isHighlighted = idx === highlighted;
              return html`
                <div
                  id=${'combobox-multi-opt-' + opt.id}
                  key=${opt.id}
                  role="option"
                  aria-selected=${isSelected}
                  class=${'flex items-center gap-2 px-3 text-sm cursor-pointer select-none '
                    + (isHighlighted ? 'bg-surface-2 text-text' : 'text-text hover:bg-surface-2')}
                  style=${{ position: 'absolute', top: idx * ITEM_H + 'px', width: '100%', height: ITEM_H + 'px' }}
                  onMouseDown=${(/** @type {MouseEvent} */ e) => { e.preventDefault(); _toggle(opt); }}
                  onMouseEnter=${() => setHighlighted(idx)}
                >
                  <span class=${'shrink-0 w-4 h-4 rounded border flex items-center justify-center text-[10px] leading-none '
                    + (isSelected ? 'bg-accent border-accent text-white' : 'border-border bg-surface')}>
                    ${isSelected ? '✓' : ''}
                  </span>
                  ${opt.name}
                </div>
              `;
            })}
          </div>
        </div>
      `}
    </div>
  `;
}
