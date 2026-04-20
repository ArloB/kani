// @ts-check
// Generic modal component — focus trap, escape-to-close, backdrop click.

import { h, render } from 'preact';
import { useEffect, useRef } from 'preact/hooks';
import htm from 'htm';
import { iconX } from '../icons.js';
import { Icon } from './icon.js';
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
      class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/60"
      onClick=${(e) => { if (e.target === e.currentTarget) onClose(); }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby=${title ? titleId : undefined}
        tabindex="-1"
        class=${'relative bg-surface rounded-xl shadow-lg w-full flex flex-col overflow-hidden max-h-[90vh] outline-none ' + (wide ? 'max-w-[800px]' : 'max-w-[600px]')}
        ref=${dialogRef}
      >
        ${title && html`
          <div class="flex items-center justify-between gap-3 px-6 py-4 border-b border-border-subtle shrink-0">
            <h2 id=${titleId} class="text-lg font-semibold text-text">${title}</h2>
            <button class="btn-icon" aria-label="Close" onClick=${onClose}><${Icon} svg=${iconX} /></button>
          </div>
        `}
        <div class="flex-1 overflow-y-auto p-6">${children}</div>
        ${footer && html`<div class="flex items-center justify-end gap-3 px-6 py-4 border-t border-border-subtle shrink-0">${footer}</div>`}
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
