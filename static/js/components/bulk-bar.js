
import { h } from 'preact';
import { useRef, useState } from 'preact/hooks';
import htm from 'htm';
import { t } from '../i18n.js';
import { ContextMenu } from './menu.js';
import { Icon } from './icon.js';
import { iconChevronDown } from '../icons.js';

const html = htm.bind(h);

/** Above this many, helpers move into the menu instead of sitting in the bar. */
const INLINE_HELPER_LIMIT = 2;

/**
 * @typedef {{ label: string, onClick: () => void, kind?: 'secondary'|'danger', disabled?: boolean, title?: string }} BulkAction
 * @typedef {{ label: string, onClick: () => void, title?: string, disabled?: boolean }} BulkHelper
 */

/**
 * @param {{
 *   countLabel: string,
 *   statLine?: string | null,
 *   helpers?: BulkHelper[],
 *   actions: BulkAction[],
 *   onCancel: () => void,
 *   busy?: boolean,
 * }} props
 */
export function BulkBar({ countLabel, statLine = null, helpers = [], actions, onCancel, busy = false }) {
  const menuBtn = useRef(/** @type {HTMLButtonElement|null} */ (null));
  const [menuOpen, setMenuOpen] = useState(false);
  const collapsed = helpers.length > INLINE_HELPER_LIMIT;

  const helperGroup = helpers.length === 0 ? null : collapsed
    ? html`
        <button
          ref=${menuBtn}
          type="button"
          class="btn-ghost btn-sm inline-flex items-center gap-1 whitespace-nowrap"
          aria-haspopup="menu"
          aria-expanded=${menuOpen}
          onClick=${() => setMenuOpen((o) => !o)}
        >
          ${t('bulk.select')}
          <${Icon} svg=${iconChevronDown} class="icon-xs" />
        </button>
        ${menuOpen && html`<${ContextMenu}
          items=${helpers.map((hp) => ({ label: hp.label, action: hp.onClick, disabled: hp.disabled }))}
          trigger=${menuBtn}
          onClose=${() => setMenuOpen(false)}
        />`}
      `
    : helpers.map((hp) => html`
        <button key=${hp.label} type="button" class="btn-ghost btn-sm whitespace-nowrap"
          title=${hp.title} disabled=${hp.disabled} onClick=${hp.onClick}>
          ${hp.label}
        </button>
      `);

  return html`
    <div class="bulk-bar-dock z-40 bg-surface border border-border-subtle rounded-none md:rounded-2xl shadow-xl flex items-center gap-x-3 gap-y-2 px-4 py-2.5 flex-wrap pb-safe md:pb-2.5">
      <div class="flex flex-col min-w-0 mr-auto">
        <span class="text-sm font-medium text-text-muted whitespace-nowrap">${countLabel}</span>
        ${statLine && html`<span class="text-xs text-text-faint whitespace-nowrap">${statLine}</span>`}
      </div>

      ${helperGroup && html`
        <div class="flex items-center gap-1 relative">
          ${helperGroup}
        </div>
        <span class="hidden md:block w-px self-stretch bg-border-subtle" aria-hidden="true"></span>
      `}

      <div class="flex items-center gap-1.5 flex-wrap">
        ${actions.map(a => html`
          <button
            key=${a.label}
            type="button"
            class=${(a.kind === 'danger' ? 'btn-danger' : 'btn-secondary') + ' btn-sm js-bulk-action whitespace-nowrap'}
            disabled=${busy || a.disabled}
            title=${a.title}
            onClick=${a.onClick}
          >${a.label}</button>
        `)}
        <button type="button" class="btn-ghost btn-sm whitespace-nowrap" onClick=${onCancel}>${t('common.cancel')}</button>
      </div>
    </div>
  `;
}
