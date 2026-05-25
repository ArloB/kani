// @ts-check
// Empty state — no-results placeholder with optional action.

/**
 * Creates an empty state element.
 * @param {{
 *   icon?: string,
 *   title: string,
 *   subtitle?: string,
 *   action?: { label: string, onClick: () => void } | { label: string, href: string },
 * }} props
 * @returns {HTMLElement}
 */
export function createEmptyState({ icon, title, subtitle, action }) {
  const el = document.createElement('div');
  el.className = 'flex flex-col items-center justify-center gap-4 py-16 text-center';

  if (icon) {
    const iconEl = document.createElement('span');
    iconEl.className = 'text-text-muted [&_svg]:w-12 [&_svg]:h-12';
    iconEl.setAttribute('aria-hidden', 'true');
    iconEl.innerHTML = icon;
    el.appendChild(iconEl);
  }

  const titleEl = document.createElement('p');
  titleEl.className = 'text-base font-medium text-text';
  titleEl.textContent = title;
  el.appendChild(titleEl);

  if (subtitle) {
    const sub = document.createElement('p');
    sub.className = 'text-sm text-text-muted';
    sub.textContent = subtitle;
    el.appendChild(sub);
  }

  if (action) {
    if ('href' in action) {
      const link = document.createElement('a');
      link.href = action.href;
      link.className = 'btn-primary';
      link.textContent = action.label;
      el.appendChild(link);
    } else {
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.className = 'btn-primary';
      btn.textContent = action.label;
      btn.addEventListener('click', action.onClick);
      el.appendChild(btn);
    }
  }

  return el;
}
