// @ts-check
// Pagination component — prev/next controls.

import { iconChevronLeft, iconChevronRight } from '../icons.js';

/**
 * @param {HTMLElement} container
 * @param {{ page: number, hasNext: boolean, onPageChange: (page: number) => void }} props
 * @returns {{ update: (props: { page: number, hasNext: boolean }) => void, destroy: () => void }}
 */
export function renderPagination(container, { page, hasNext, onPageChange }) {
  /** @type {{ page: number, hasNext: boolean }} */
  let _props = { page, hasNext };

  function _render() {
    const { page, hasNext } = _props;
    container.innerHTML = `
      <div class="flex items-center justify-center gap-3 py-4">
        <button class="btn-ghost" ${page <= 1 ? 'disabled' : ''} aria-label="Previous page">
          ${iconChevronLeft} Prev
        </button>
        <span class="text-sm text-text-muted">Page ${page}</span>
        <button class="btn-ghost" ${!hasNext ? 'disabled' : ''} aria-label="Next page">
          Next ${iconChevronRight}
        </button>
      </div>
    `;
    container.querySelectorAll('button')[0]?.addEventListener('click', () => {
      if (_props.page > 1) onPageChange(_props.page - 1);
    });
    container.querySelectorAll('button')[1]?.addEventListener('click', () => {
      if (_props.hasNext) onPageChange(_props.page + 1);
    });
  }

  _render();

  return {
    update(newProps) {
      _props = { ..._props, ...newProps };
      _render();
    },
    destroy() {
      container.innerHTML = '';
    },
  };
}
