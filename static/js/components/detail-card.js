// @ts-check

import { h } from 'preact';
import htm from 'htm';
const html = htm.bind(h);

/**
 * @param {{
 *   title: string,
 *   action?: any,
 *   flush?: boolean,
 *   children?: any,
 * }} props
 */
export function DetailCard({ title, action, flush = false, children }) {
  return html`
    <div class="detail-card">
      <div class="detail-card-head">
        <span>${title}</span>
        ${action ? html`<span>${action}</span>` : null}
      </div>
      <div class=${flush ? '' : 'p-2'}>
        ${children}
      </div>
    </div>
  `;
}
