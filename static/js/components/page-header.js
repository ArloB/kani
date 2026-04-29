// @ts-check
// Factory for the sticky per-page header (.page-header) with breadcrumb + actions slots.
// Used opt-in by pages that want the consistent chrome (Settings, Accounts, etc.).

/**
 * Builds and returns a `.page-header` element.
 *
 * @param {{
 *   breadcrumb?: Array<{ label: string, href?: string }>,
 *   actions?: HTMLElement | string,
 * }} opts
 * @returns {HTMLElement}
 */
export function createPageHeader({ breadcrumb = [], actions } = {}) {
  const header = document.createElement('header');
  header.className = 'page-header';

  if (breadcrumb.length) {
    const nav = document.createElement('nav');
    nav.className = 'breadcrumb';
    nav.setAttribute('aria-label', 'Breadcrumb');

    breadcrumb.forEach((crumb, i) => {
      const isLast = i === breadcrumb.length - 1;
      if (i > 0) {
        const sep = document.createElement('span');
        sep.className = 'sep';
        sep.setAttribute('aria-hidden', 'true');
        sep.textContent = '/';
        nav.appendChild(sep);
      }
      if (crumb.href && !isLast) {
        const a = document.createElement('a');
        a.href = crumb.href;
        a.textContent = crumb.label;
        nav.appendChild(a);
      } else {
        const span = document.createElement('span');
        span.className = isLast ? 'cur' : '';
        span.textContent = crumb.label;
        nav.appendChild(span);
      }
    });

    header.appendChild(nav);
  }

  if (actions) {
    const wrap = document.createElement('div');
    wrap.className = 'header-actions';
    if (typeof actions === 'string') {
      wrap.innerHTML = actions;
    } else {
      wrap.appendChild(actions);
    }
    header.appendChild(wrap);
  }

  return header;
}
