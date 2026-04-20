// @ts-check
// Error state — error message with optional retry action.

/**
 * Creates an error state element.
 * @param {{
 *   message?: string,
 *   onRetry?: () => void,
 * }} props
 * @returns {HTMLElement}
 */
export function createErrorState({ message = 'Something went wrong.', onRetry } = {}) {
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
    btn.textContent = 'Try again';
    btn.addEventListener('click', onRetry);
    el.appendChild(btn);
  }

  return el;
}
