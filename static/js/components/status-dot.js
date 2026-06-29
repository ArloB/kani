// @ts-check
import { h } from 'preact';
import htm from 'htm';
const html = htm.bind(h);

/**
 * A colour-coded status dot with an accessible label so colour is not the sole signal.
 * @param {{ state: 'open' | 'half_open', label: string }} props
 */
export function StatusDot({ state, label }) {
  return html`<span
    role="img"
    aria-label=${'Status: ' + label}
    class=${'w-2 h-2 rounded-full shrink-0 ' + (state === 'open' ? 'bg-danger' : 'bg-warn')}
  ></span>`;
}

/**
 * Vanilla-DOM equivalent — returns a configured <span> element.
 * @param {'open' | 'half_open'} state
 * @param {string} label
 * @returns {HTMLSpanElement}
 */
export function createStatusDot(state, label) {
  const span = document.createElement('span');
  span.className = `w-2 h-2 rounded-full shrink-0 ${state === 'open' ? 'bg-danger' : 'bg-warn'}`;
  span.setAttribute('role', 'img');
  span.setAttribute('aria-label', 'Status: ' + label);
  return span;
}
