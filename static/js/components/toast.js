// @ts-check
// Toast notification — self-dismissing message overlay.

/** @type {HTMLElement | null} */
let _container = null;

function _getContainer() {
  if (!_container) {
    _container = document.createElement('div');
    _container.id = 'toast-container';
    _container.className = 'fixed bottom-6 left-1/2 -translate-x-1/2 flex flex-col items-center gap-2 z-[2000] pointer-events-none';
    _container.setAttribute('aria-live', 'polite');
    _container.setAttribute('aria-atomic', 'false');
    document.body.appendChild(_container);
  }
  return _container;
}

/**
 * Show a toast notification.
 * @param {string} message
 * @param {{ type?: 'info' | 'success' | 'warn' | 'error', duration?: number }} [opts]
 */
export function showToast(message, { type = 'info', duration = 3000 } = {}) {
  const container = _getContainer();

  const typeClasses = {
    info:    'bg-surface-3 text-text border border-border',
    success: 'bg-success text-white',
    warn:    'bg-warn text-black',
    error:   'bg-danger text-white',
  };

  const toast = document.createElement('div');
  toast.className = [
    'pointer-events-auto px-5 py-3 rounded-md text-sm shadow-md',
    'max-w-[360px] text-center transition-opacity duration-300',
    typeClasses[type] ?? typeClasses.info,
  ].join(' ');
  toast.setAttribute('role', 'status');
  toast.textContent = message;

  container.appendChild(toast);

  // Fade out then remove
  const timer = setTimeout(() => {
    toast.style.opacity = '0';
    setTimeout(() => toast.remove(), 300);
  }, duration);

  // Click to dismiss early
  toast.addEventListener('click', () => {
    clearTimeout(timer);
    toast.style.opacity = '0';
    setTimeout(() => toast.remove(), 300);
  });
}

/**
 * Show an error toast from an API error, preferring the hint over the raw message.
 * @param {any} err
 */
export function showApiError(err) {
  showToast(err?.hint ?? err?.message ?? 'An error occurred', { type: 'error' });
}
