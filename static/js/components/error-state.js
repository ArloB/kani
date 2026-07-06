// @ts-check
// Error state — error message with optional retry action.

import { t } from '../i18n.js';

/**
 * Creates an error state element.
 * @param {{
 *   message?: string,
 *   onRetry?: () => void,
 * }} props
 * @returns {HTMLElement}
 */
export function createErrorState({ message, onRetry } = {}) {
  message = message ?? t('common.something_wrong');
  const el = document.createElement('div');
  el.className = 'flex flex-col items-center gap-4 py-8 text-center';

  const msg = document.createElement('p');
  msg.className = 'text-sm text-danger';
  msg.textContent = message;
  el.appendChild(msg);

  if (onRetry) {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'btn-ghost';
    btn.textContent = t('common.retry');
    btn.addEventListener('click', onRetry);
    el.appendChild(btn);
  }

  return el;
}
