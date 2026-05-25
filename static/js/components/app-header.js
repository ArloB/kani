// @ts-check
// Global app-shell header — mounted once by app.js, updated per-page via setPageHeader().

import { navigate } from '../router.js';

/** @typedef {{ label: string, href?: string }} Crumb */
/** @typedef {{ crumbs?: Crumb[], actions?: HTMLElement | HTMLElement[] | null }} HeaderState */

/** @type {HTMLElement | null} */
let _headerEl = null;
/** @type {HTMLElement | null} */
let _breadcrumbSlot = null;
/** @type {HTMLElement | null} */
let _actionsSlot = null;
/** @type {HeaderState} */
let _state = { crumbs: [], actions: null };

/**
 * Mount the global app header into `container`.
 * Call once from app.js after rendering the sidebar.
 * Returns a reference to the #header-notifications div so app.js can mount the panel.
 *
 * @param {HTMLElement} container
 * @returns {{ notificationsMount: HTMLElement, destroy: () => void }}
 */
export function mountAppHeader(container) {
  _headerEl = document.createElement('header');
  _headerEl.className = 'app-header';
  _headerEl.setAttribute('aria-label', 'Page header');

  _breadcrumbSlot = document.createElement('nav');
  _breadcrumbSlot.className = 'breadcrumb';
  _breadcrumbSlot.setAttribute('aria-label', 'Breadcrumb');

  _actionsSlot = document.createElement('div');
  _actionsSlot.className = 'header-actions';

  const notificationsMount = document.createElement('div');
  notificationsMount.id = 'header-notifications';

  _headerEl.appendChild(_breadcrumbSlot);
  _headerEl.appendChild(_actionsSlot);
  _headerEl.appendChild(notificationsMount);

  container.appendChild(_headerEl);
  _applyState();

  return {
    notificationsMount,
    destroy() {
      _headerEl?.remove();
      _headerEl = null;
      _breadcrumbSlot = null;
      _actionsSlot = null;
      _state = { crumbs: [], actions: null };
    },
  };
}

/**
 * Called by each page's init() to set the header content.
 *
 * @param {HeaderState} state
 */
export function setPageHeader(state) {
  _state = state;
  _applyState();
}

/**
 * Called by each page's destroy() to clear page-specific content.
 */
export function clearPageHeader() {
  _state = { crumbs: [], actions: null };
  _applyState();
}

function _applyState() {
  if (!_breadcrumbSlot || !_actionsSlot) return;

  _breadcrumbSlot.innerHTML = '';
  const crumbs = _state.crumbs ?? [];
  if (crumbs.length > 0) {
    const leadSep = document.createElement('span');
    leadSep.className = 'sep';
    leadSep.setAttribute('aria-hidden', 'true');
    leadSep.textContent = '/';
    _breadcrumbSlot.appendChild(leadSep);
  }
  crumbs.forEach((crumb, i) => {
    const isLast = i === crumbs.length - 1;

    if (i > 0) {
      const sep = document.createElement('span');
      sep.className = 'sep';
      sep.setAttribute('aria-hidden', 'true');
      sep.textContent = '/';
      _breadcrumbSlot?.appendChild(sep);
    }

    if (isLast || !crumb.href) {
      const span = document.createElement('span');
      span.className = 'cur';
      span.setAttribute('aria-current', 'page');
      span.textContent = crumb.label;
      _breadcrumbSlot?.appendChild(span);
    } else {
      const a = document.createElement('a');
      a.href = crumb.href;
      a.textContent = crumb.label;
      a.addEventListener('click', (e) => {
        e.preventDefault();
        navigate(/** @type {string} */ (crumb.href));
      });
      _breadcrumbSlot?.appendChild(a);
    }
  });

  _actionsSlot.innerHTML = '';
  const actions = _state.actions;
  if (actions) {
    if (Array.isArray(actions)) {
      for (const el of actions) _actionsSlot.appendChild(el);
    } else {
      _actionsSlot.appendChild(actions);
    }
  }
}
