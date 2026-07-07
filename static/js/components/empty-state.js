// @ts-check
import { h, render } from 'preact';
import htm from 'htm';

const html = htm.bind(h);

export function EmptyState({ icon, title, subtitle, action }) {
  return html`
    <div class="flex flex-col items-center justify-center gap-4 py-16 text-center">
      ${icon && html`<span class="text-text-muted icon-3xl" aria-hidden="true" dangerouslySetInnerHTML=${{ __html: icon }} />`}
      <p class="text-base font-medium text-text">${title}</p>
      ${subtitle && html`<p class="text-sm text-text-muted">${subtitle}</p>`}
      ${action && ('href' in action
        ? html`<a href=${action.href} class="btn-primary">${action.label}</a>`
        : html`<button type="button" class="btn-primary" onClick=${action.onClick}>${action.label}</button>`
      )}
    </div>
  `;
}

export function createEmptyState(opts) {
  const el = document.createElement('div');
  render(html`<${EmptyState} ...${opts} />`, el);
  return el;
}
