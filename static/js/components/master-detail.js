// @ts-check
// MasterDetail — CSS-grid two-pane layout (list left, detail right).
// On desktop, both panes show side-by-side and scroll independently.
// On mobile (<768px), only one pane is visible at a time; callers control
// which via setView('list' | 'detail').

/**
 * Builds a master-detail shell and mounts it into `el`.
 * Returns references to the list and detail pane containers plus a
 * setView helper for switching active pane on mobile.
 *
 * @param {HTMLElement} el
 * @param {{ listWidth?: number }} [opts]
 * @returns {{ listEl: HTMLElement, detailEl: HTMLElement, setView: (v: 'list'|'detail') => void, destroy: () => void }}
 */
export function mountMasterDetail(el, { listWidth = 300 } = {}) {
  const wrap = document.createElement('div');
  wrap.className = 'master-detail';
  wrap.style.setProperty('--md-list-w', `${listWidth}px`);
  wrap.dataset.view = 'list';

  const listEl = document.createElement('div');
  listEl.className = 'list-pane master-detail__list';

  const detailEl = document.createElement('div');
  detailEl.className = 'master-detail__detail';

  wrap.appendChild(listEl);
  wrap.appendChild(detailEl);
  el.appendChild(wrap);

  return {
    listEl,
    detailEl,
    setView(v) { wrap.dataset.view = v; },
    destroy() { wrap.remove(); },
  };
}
