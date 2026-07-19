// @ts-check
// Checkbox — the labelled checkbox for forms and option lists. The design
// system has two boolean controls: kani-toggle for on/off settings rows, and
// this for multi-pick lists and modal options. Bare `accent-accent` native
// checkboxes should migrate here when touched.

import { h } from 'preact';
import htm from 'htm';

const html = htm.bind(h);

/**
 * @param {{
 *   label: any,
 *   checked: boolean,
 *   onChange: (checked: boolean) => void,
 *   disabled?: boolean,
 *   class?: string,
 * }} props
 */
export function Checkbox({ label, checked, onChange, disabled = false, class: klass = '' }) {
  return html`
    <label class=${'kani-checkbox ' + klass}>
      <input
        type="checkbox"
        checked=${checked}
        disabled=${disabled}
        onChange=${(/** @type {any} */ e) => onChange(e.target.checked)}
      />
      <span class="kani-checkbox__box" aria-hidden="true">
        <svg viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="2"
          stroke-linecap="round" stroke-linejoin="round"><path d="m2.5 6.5 2.5 2.5 4.5-5.5"/></svg>
      </span>
      <span class="min-w-0">${label}</span>
    </label>
  `;
}
