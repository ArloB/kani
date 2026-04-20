// @ts-check
import { iconChevronRight } from '../icons.js';
import { navigate } from '../router.js';

/**
 * @param {Array<{ label: string, href?: string }>} crumbs
 * The last crumb has no href — it represents the current page (non-interactive).
 * @param {{ truncateLast?: boolean }} [options]
 * @returns {HTMLElement}
 */
export function createBreadcrumb(crumbs, { truncateLast = true } = {}) {
  const nav = document.createElement('nav');
  nav.setAttribute('aria-label', 'Breadcrumb');
  nav.className = 'flex items-center gap-1 text-sm';

  crumbs.forEach((crumb, i) => {
    const isLast = i === crumbs.length - 1;

    if (i > 0) {
      const sep = document.createElement('span');
      sep.className = 'text-text-muted [&_svg]:w-3 [&_svg]:h-3 shrink-0';
      sep.setAttribute('aria-hidden', 'true');
      sep.innerHTML = iconChevronRight;
      nav.appendChild(sep);
    }

    if (isLast || !crumb.href) {
      const span = document.createElement('span');
      span.className = 'text-base font-semibold text-text' + (truncateLast ? ' truncate max-w-[200px]' : '');
      span.setAttribute('aria-current', 'page');
      span.textContent = crumb.label;
      nav.appendChild(span);
    } else {
      const a = document.createElement('a');
      a.href = crumb.href;
      a.className = 'text-text-muted hover:text-text transition-colors focus-visible:outline-none focus-visible:underline truncate max-w-[160px]';
      a.textContent = crumb.label;
      a.addEventListener('click', e => {
        e.preventDefault();
        navigate(/** @type {string} */ (crumb.href));
      });
      nav.appendChild(a);
    }
  });

  return nav;
}
