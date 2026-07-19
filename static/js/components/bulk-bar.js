// @ts-check
// BulkBar — the floating bulk-action bar shown in select mode. One component
// for every selectable surface (library grid, chapter list); it replaces two
// independent implementations that had drifted apart in both placement and
// button semantics.
//
// Pure view: the host owns selection state and every handler. Actions are
// btn-secondary by default and btn-danger for destructive ones — a bulk bar
// never carries an accent fill.

import { h } from 'preact';
import htm from 'htm';
import { t } from '../i18n.js';

const html = htm.bind(h);

/**
 * @typedef {{ label: string, onClick: () => void, kind?: 'secondary'|'danger', disabled?: boolean, title?: string }} BulkAction
 * @typedef {{ label: string, onClick: () => void, title?: string }} BulkHelper
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
  return html`
    <div class="fixed bottom-0 md:bottom-6 inset-x-0 md:inset-x-auto md:left-1/2 md:-translate-x-1/2 z-40 md:w-auto md:max-w-[calc(100vw-2rem)] bg-surface border border-border-subtle rounded-none md:rounded-2xl shadow-xl flex items-center gap-x-3 gap-y-1 px-4 py-2.5 flex-wrap pb-safe md:pb-2.5">
      <div class="flex flex-col min-w-0 mr-auto">
        <span class="text-sm font-medium text-text-muted whitespace-nowrap">${countLabel}</span>
        ${statLine && html`<span class="text-xs text-text-faint whitespace-nowrap">${statLine}</span>`}
      </div>
      ${helpers.length > 0 && html`
        <div class="flex items-center gap-1 flex-wrap border-r border-border-subtle pr-3">
          ${helpers.map(hp => html`
            <button key=${hp.label} type="button" class="btn-ghost btn-sm" title=${hp.title} onClick=${hp.onClick}>
              ${hp.label}
            </button>
          `)}
        </div>
      `}
      <div class="flex items-center gap-1.5 flex-wrap">
        ${actions.map(a => html`
          <button
            key=${a.label}
            type="button"
            class=${(a.kind === 'danger' ? 'btn-danger' : 'btn-secondary') + ' btn-sm js-bulk-action'}
            disabled=${busy || a.disabled}
            title=${a.title}
            onClick=${a.onClick}
          >${a.label}</button>
        `)}
        <button type="button" class="btn-ghost btn-sm" onClick=${onCancel}>${t('common.cancel')}</button>
      </div>
    </div>
  `;
}
