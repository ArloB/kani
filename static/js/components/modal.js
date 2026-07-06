// @ts-check
// Generic modal component — focus trap, escape-to-close, backdrop click.

import { h, render } from 'preact';
import { useEffect, useRef } from 'preact/hooks';
import htm from 'htm';
import { iconX } from '../icons.js';
import { Icon } from './icon.js';
import { confirmDialog } from '../utils.js';
import { t } from '../i18n.js';
const html = htm.bind(h);

/**
 * Modal component. Renders a centred overlay with a card.
 * @param {{
 *   open: boolean,
 *   onClose: () => void,
 *   title?: string,
 *   wide?: boolean,
 *   footer?: any,
 *   children?: any,
 * }} props
 */
export function Modal({ open, onClose, title, wide = false, footer, children }) {
  const dialogRef = useRef(/** @type {HTMLDivElement | null} */ (null));
  const triggerRef = useRef(/** @type {Element | null} */ (null));
  const titleId = 'modal-title';

  // Store trigger element and restore focus on close
  useEffect(() => {
    if (open) {
      triggerRef.current = document.activeElement;
    } else {
      if (triggerRef.current instanceof HTMLElement) triggerRef.current.focus();
      triggerRef.current = null;
    }
  }, [open]);

  // Escape key to close
  useEffect(() => {
    if (!open) return;
    /** @param {KeyboardEvent} e */
    const onKey = (e) => { if (e.key === 'Escape') onClose(); };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [open, onClose]);

  // Focus trap: keep focus inside the modal
  useEffect(() => {
    if (!open || !dialogRef.current) return;
    const el = dialogRef.current;
    const focusable = /** @type {NodeListOf<HTMLElement>} */ (
      el.querySelectorAll('a,button:not(:disabled),input,select,textarea,[tabindex]:not([tabindex="-1"])')
    );
    if (focusable.length) focusable[0].focus();
    else el.focus();

    /** @param {KeyboardEvent} e */
    const trapTab = (e) => {
      if (e.key !== 'Tab' || !focusable.length) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (e.shiftKey ? document.activeElement === first : document.activeElement === last) {
        e.preventDefault();
        (e.shiftKey ? last : first).focus();
      }
    };
    document.addEventListener('keydown', trapTab);
    return () => document.removeEventListener('keydown', trapTab);
  }, [open]);

  if (!open) return null;

  return html`
    <div
      class="fixed inset-0 z-modal flex items-end sm:items-center justify-center p-0 sm:p-4 bg-scrim"
      onClick=${(e) => { if (e.target === e.currentTarget) onClose(); }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby=${title ? titleId : undefined}
        tabindex="-1"
        class=${'relative bg-surface shadow-lg w-full flex flex-col overflow-hidden outline-none rounded-t-2xl sm:rounded-xl max-h-[85vh] sm:max-h-modal ' + (wide ? 'modal-wide' : 'modal-narrow')}
        ref=${dialogRef}
      >
        ${title && html`
          <div class="flex items-center justify-between gap-3 px-5 py-4 border-b border-border-subtle shrink-0">
            <h2 id=${titleId} class="text-lg font-semibold text-text">${title}</h2>
            <button class="btn-icon" aria-label=${t('common.close')} onClick=${onClose}><${Icon} svg=${iconX} /></button>
          </div>
        `}
        <div class="flex-1 overflow-y-auto p-5">${children}</div>
        ${footer && html`<div class="flex items-center justify-end gap-3 px-5 py-4 border-t border-border-subtle shrink-0">${footer}</div>`}
      </div>
    </div>
  `;
}

/**
 * Imperatively render a Preact tree into #modal-root.
 * Returns a cleanup function that unmounts.
 * @param {any} vnode
 * @returns {() => void}
 */
export function mountIntoModalRoot(vnode) {
  const root = document.getElementById('modal-root');
  if (!root) return () => {};
  render(vnode, root);
  return () => render(null, root);
}

// ── Imperative helpers ────────────────────────────────────────────────────────

/**
 * Shows a confirm dialog. Returns a Promise resolving to true (confirmed) or
 * false (cancelled / Escape). Delegates to `confirmDialog` in utils.js so both
 * call paths share one implementation with full feature parity (danger,
 * rememberKey, cancelLabel, Tab-trap, Escape).
 * @param {string} message
 * @param {{ title?: string, confirmLabel?: string, cancelLabel?: string, danger?: boolean, rememberKey?: string }} [opts]
 * @returns {Promise<boolean>}
 */
export function showConfirm(message, opts = {}) {
  return confirmDialog({ message, ...opts });
}

/**
 * @param {{ message: string, title?: string, closeLabel?: string, onClose: () => void }} props
 */
function AlertModal({ message, title = 'Notice', closeLabel = 'OK', onClose }) {
  return html`
    <${Modal}
      open=${true}
      title=${title}
      onClose=${onClose}
      footer=${html`
        <button type="button" class="btn-primary btn-sm" onClick=${onClose}>${closeLabel}</button>
      `}
    >
      <p class="text-sm text-text">${message}</p>
    </${Modal}>
  `;
}

/**
 * Shows an alert dialog using the app modal. Returns a Promise that resolves
 * when the user dismisses it.
 * @param {string} message
 * @param {{ title?: string }} [opts]
 * @returns {Promise<void>}
 */
export function showAlert(message, opts = {}) {
  return new Promise((resolve) => {
    let cleanup = () => {};
    cleanup = mountIntoModalRoot(html`
      <${AlertModal}
        message=${message}
        title=${opts.title ?? 'Notice'}
        onClose=${() => { cleanup(); resolve(); }}
      />
    `);
  });
}
