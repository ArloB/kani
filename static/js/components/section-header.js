// @ts-check

import { h, render } from 'preact';
import htm from 'htm';
import { t } from '../i18n.js';
const html = htm.bind(h);

/**
 * @param {{
 *   title: string,
 *   description?: string,
 *   actions?: any,
 *   dirty?: boolean,
 * }} props
 */
export function SectionHeader({ title, description, actions, dirty }) {
  return html`
    <div class="section-header">
      <div>
        <h2 class="flex items-center gap-2">
          ${title}
          ${dirty ? html`<span class="dirty-dot" aria-label=${t('settings.unsaved.title')}></span>` : null}
        </h2>
        ${description ? html`<p>${description}</p>` : null}
      </div>
      ${actions ? html`<div class="flex items-center gap-2 shrink-0">${actions}</div>` : null}
    </div>
  `;
}

/**
 * Mount SectionHeader into a DOM node.
 * @param {HTMLElement} el
 * @param {{ title: string, description?: string, dirty?: boolean, actions?: any }} props
 */
export function mountSectionHeader(el, props) {
  render(html`<${SectionHeader} ...${props} />`, el);
}
