// @ts-check
// BulkBar — the floating bulk-action bar shown in select mode. One component
// for every selectable surface (library grid, chapter list); it replaces two
// independent implementations that had drifted apart in both placement and
// button semantics.
//
// Pure view: the host owns selection state and every handler. Actions are
// btn-secondary by default and btn-danger for destructive ones — a bulk bar
// never carries an accent fill.
//
// Layout: the bar is one line of three parts — what is selected, how to change
// the selection, what to do with it. On a phone it sits above the bottom nav
// (bottom-16, the nav's h-16) rather than on top of it — at bottom-0 the action
// row was covered by the tab bar and its buttons could not be reached.
//
// Centred with mx-auto rather than a translate: a transformed ancestor becomes
// the containing block for `position: fixed` descendants, which sent the
// Select dropdown's viewport coordinates to the far left of the screen.
// `md:left-sidebar` centres it over the content rather than the viewport, so
// the count does not sit behind the sidebar on a narrow desktop. Selection helpers past a couple collapse
// into a single "Select" menu rather than each claiming bar width; nine
// same-weight buttons in a wrapping row read as a wall and pushed the actions
// onto a second and third line as surfaces added selectors.

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
