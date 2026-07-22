// @ts-check
// Side-by-side comparison of a held chapter against what a source now offers.

import { h } from 'preact';
import htm from 'htm';
import * as api from '../api.js';
import { t } from '../i18n.js';
import { Modal, showConfirm } from './modal.js';
import { showApiError, showToast } from './toast.js';
import { useBusy } from '../hooks/use-busy.js';

const html = htm.bind(h);

/** @param {any} c */
function isReassurance(c) {
  return c.kind === 'source_downgraded';
}

/**
 * @param {{ label: string, held: any, candidate: any, better?: 'held'|'candidate'|null }} props
 */
function CompareRow({ label, held, candidate, better = null }) {
  const cell = (/** @type {any} */ v, /** @type {boolean} */ win) => html`
    <span class="text-sm ${win ? 'text-text font-medium' : 'text-text-muted'}">
      ${v ?? '—'}
    </span>
  `;
  return html`
    <div class="grid grid-cols-3 gap-2 items-baseline px-1 py-1.5">
      <span class="text-xs text-text-muted">${label}</span>
      ${cell(held, better === 'held')}
      ${cell(candidate, better === 'candidate')}
    </div>
  `;
}

/**
 * @param {{ open: boolean, candidate: any, chapterTitle: string,
 *           onClose: () => void, onChanged: () => void }} props
 */
export function UpgradeCompare({ open, candidate, chapterTitle, onClose, onChanged }) {
  const { busy, run } = useBusy();
  if (!candidate) return null;

  const reassurance = isReassurance(candidate);

  const apply = () =>
    run(async () => {
      const ok = await showConfirm(t('upgrade.replace.confirm'), {
        title: t('upgrade.replace'),
        confirmLabel: t('upgrade.replace'),
      });
      if (!ok) return;
      try {
        await api.applyChapterUpgrade(candidate.held_chapter_id);
        showToast(t('upgrade.replace.started'), { type: 'success' });
        onChanged();
        onClose();
      } catch (e) {
        showApiError(e);
      }
    });

  const dismiss = () =>
    run(async () => {
      try {
        await api.dismissChapterUpgrade(candidate.held_chapter_id);
        onChanged();
        onClose();
      } catch (e) {
        showApiError(e);
      }
    });

  const heldPages = candidate.held_page_count;
  const candPages = candidate.candidate_page_count;
  const pageWinner =
    heldPages == null || candPages == null
      ? null
      : candPages > heldPages
        ? 'candidate'
        : candPages < heldPages
          ? 'held'
          : null;

  return html`
    <${Modal}
      open=${open}
      title=${t('upgrade.title')}
      onClose=${onClose}
      footer=${reassurance
        ? html`<button type="button" class="btn-secondary btn-sm" onClick=${onClose}>
            ${t('common.done')}
          </button>`
        : html`
            <button type="button" class="btn-ghost btn-sm" disabled=${busy} onClick=${dismiss}>
              ${t('upgrade.dismiss')}
            </button>
            <button type="button" class="btn-primary btn-sm" disabled=${busy} onClick=${apply}>
              ${t('upgrade.replace')}
            </button>
          `}
    >
      <div class="flex flex-col gap-3 px-1">
        <p class="text-sm ${reassurance ? 'text-success' : 'text-text-muted'}">
          ${t(candidate.reason_key)}
        </p>
        <p class="text-xs text-text-muted">${chapterTitle}</p>

        <div class="border-t border-border-subtle pt-2">
          <div class="grid grid-cols-3 gap-2 px-1 pb-1">
            <span></span>
            <span class="text-xs font-medium text-text">${t('upgrade.yours')}</span>
            <span class="text-xs font-medium text-text">${t('upgrade.theirs')}</span>
          </div>
          <${CompareRow}
            label=${t('upgrade.pages')}
            held=${heldPages}
            candidate=${candPages}
            better=${pageWinner}
          />
          <${CompareRow}
            label=${t('upgrade.scanlator')}
            held=${candidate.held_scanlator ?? '—'}
            candidate=${candidate.candidate_scanlator ?? '—'}
          />
        </div>

        ${reassurance
          ? html`<p class="text-xs text-text-muted">${t('upgrade.downgrade.note')}</p>`
          : html`<p class="text-xs text-text-muted">${t('upgrade.replace.note')}</p>`}
      </div>
    <//>
  `;
}
