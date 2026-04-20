// @ts-check
// SidebarNav — shared vertical navigation sidebar component.
// Used by SourcesSidebar (sources) and Settings (settings categories).

import { h } from 'preact';
import htm from 'htm';
const html = htm.bind(h);

/**
 * A single nav item in a sidebar.
 *
 * @param {{
 *   label: string,
 *   active: boolean,
 *   badge?: string | number | null,
 *   onClick: () => void,
 * }} props
 */
export function SidebarNavItem({ label, active, badge, onClick }) {
  return html`
    <button
      type="button"
      class=${[
        'w-full text-left flex items-center justify-between gap-2',
        'px-3 py-2 text-sm transition-colors',
        'hover:bg-surface-2 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent',
        active ? 'text-text font-medium' : 'text-text-muted',
      ].join(' ')}
      onClick=${onClick}
    >
      <span class="flex-1 truncate">${label}</span>
      ${badge != null && html`<span class="text-xs text-text-faint shrink-0">${badge}</span>`}
    </button>
  `;
}

/**
 * Sidebar nav container with optional header title.
 *
 * @param {{
 *   title?: string,
 *   children: any,
 * }} props
 */
export function SidebarNav({ title, children, class: className = '' }) {
  return html`
    <div class=${'flex-1 overflow-y-auto flex flex-col gap-0.5 py-1 ' + className}>
      ${title && html`<p class="px-3 py-1 text-xs font-semibold uppercase tracking-wider text-text-muted mb-1">${title}</p>`}
      ${children}
    </div>
  `;
}
