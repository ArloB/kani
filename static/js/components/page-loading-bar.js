// @ts-check
// Page loading bar — thin top progress bar for route transitions.

/** @type {HTMLElement | null} */
let _bar = null;
/** @type {HTMLElement | null} */
let _fill = null;
/** @type {ReturnType<typeof setTimeout> | null} */
let _timer = null;

function _init() {
  if (_bar) return;
  _bar = document.createElement('div');
  _bar.className = 'page-loading-bar';
  _fill = document.createElement('div');
  _fill.className = 'page-loading-bar__fill';
  _bar.appendChild(_fill);
  document.body.prepend(_bar);
}

/** Start the loading bar animation. */
export function startLoading() {
  _init();
  if (_timer) clearTimeout(_timer);
  _bar?.classList.add('page-loading-bar--active');
}

/**
 * Finish the loading bar — briefly shows full width then hides.
 * @param {{ delay?: number }} [opts]
 */
export function finishLoading({ delay = 300 } = {}) {
  _init();
  _bar?.classList.remove('page-loading-bar--active');
  if (_timer) clearTimeout(_timer);
  _timer = setTimeout(() => {
    if (_fill) _fill.style.width = '';
  }, delay);
}
