// @ts-check
// Global app-shell header — mounted once by app.js, updated per-page via setPageHeader().

import { navigate } from '../router.js';
import { iconEllipsisVertical } from '../icons.js';
import { t } from '../i18n.js';

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
/** @type {HTMLElement | null} */
let _overflowPanel = null;
/** @type {(() => void) | null} */
let _removeOverflowListeners = null;

/** Below this width the header can't hold more than one action beside the crumb. */
const ACTION_COLLAPSE_WIDTH = 768;

/**
 * Mount the global app header into `container`.
 * Call once from app.js after rendering the sidebar.
 * Returns a reference to the #header-notifications div so app.js can mount the panel.
 *
 * @param {HTMLElement} container
 * @returns {{ notificationsMount: HTMLElement, destroy: () => void }}
 */
let _resizeBound = false;

export function mountAppHeader(container) {
  if (!_resizeBound) {
    let raf = 0;
    window.addEventListener('resize', () => {
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(() => _applyState());
    });
    _resizeBound = true;
  }
  _headerEl = document.createElement('header');
  _headerEl.className = 'app-header';
  _headerEl.setAttribute('aria-label', t('app_header.aria'));

  _breadcrumbSlot = document.createElement('nav');
  _breadcrumbSlot.className = 'breadcrumb';
  _breadcrumbSlot.setAttribute('aria-label', t('app_header.breadcrumb_aria'));

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

  _closeOverflow();
  _actionsSlot.innerHTML = '';
  const actions = _state.actions;
  const actionEls = actions
    ? (Array.isArray(actions) ? actions : [actions])
    : [];
  for (const el of actionEls) _actionsSlot.appendChild(el);

  _applyActionOverflow(actionEls);
}

/**
 * On narrow screens a row of action pills squeezes the breadcrumb down to a
 * letter or two. Keep the last action (pages put their primary one last) in the
 * bar and move the rest into a kebab menu — the real nodes are moved, not cloned,
 * so their listeners survive.
 * @param {HTMLElement[]} actionEls
 */
function _applyActionOverflow(actionEls) {
  if (!_actionsSlot || !_headerEl) return;

  const collapse = window.innerWidth < ACTION_COLLAPSE_WIDTH && actionEls.length > 1;
  if (!collapse) return;

  const overflow = actionEls.slice(0, -1);
  if (!overflow.length) return;

  const panel = document.createElement('div');
  panel.className = 'header-overflow-panel';
  panel.hidden = true;
  for (const el of overflow) panel.appendChild(el);

  const kebab = document.createElement('button');
  kebab.type = 'button';
  kebab.className = 'btn-icon shrink-0';
  kebab.setAttribute('aria-label', t('app_header.more_actions'));
  kebab.setAttribute('aria-haspopup', 'true');
  kebab.setAttribute('aria-expanded', 'false');
  kebab.innerHTML = `<span class="icon-sm">${iconEllipsisVertical}</span>`;
  kebab.addEventListener('click', (e) => {
    e.stopPropagation();
    const open = panel.hidden;
    panel.hidden = !open;
    kebab.setAttribute('aria-expanded', String(open));
  });

  _actionsSlot.insertBefore(kebab, _actionsSlot.firstChild);
  _actionsSlot.appendChild(panel);
  _overflowPanel = panel;

  const onDocClick = (/** @type {MouseEvent} */ ev) => {
    if (!panel.hidden && !panel.contains(/** @type {Node} */ (ev.target)) && ev.target !== kebab) {
      panel.hidden = true;
      kebab.setAttribute('aria-expanded', 'false');
    }
  };
  document.addEventListener('click', onDocClick);
  _removeOverflowListeners = () => document.removeEventListener('click', onDocClick);
}

function _closeOverflow() {
  _removeOverflowListeners?.();
  _removeOverflowListeners = null;
  _overflowPanel = null;
}
