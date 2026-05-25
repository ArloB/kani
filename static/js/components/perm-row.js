// @ts-check
// PermRow — a single permission with optional provenance label.

import { h } from 'preact';
import htm from 'htm';
const html = htm.bind(h);

/**
 * @param {{
 *   permission: string,
 *   via?: string,
 * }} props
 */
export function PermRow({ permission, via }) {
  return html`
    <div class="flex items-center justify-between gap-3 px-3 py-2 text-sm border-b border-border-subtle last:border-0">
      <span class="font-mono text-xs text-text">${permission}</span>
      ${via ? html`<span class="meta shrink-0">via ${via}</span>` : null}
    </div>
  `;
}
