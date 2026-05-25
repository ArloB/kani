// @ts-check
// MasterDetail — CSS-grid two-pane layout (list left, detail right).
// Both panes scroll independently.

/**
 * Builds a master-detail shell and mounts it into `el`.
 * Returns references to the list and detail pane containers.
 *
 * @param {HTMLElement} el
 * @param {{ listWidth?: number }} [opts]
 * @returns {{ listEl: HTMLElement, detailEl: HTMLElement, destroy: () => void }}
 */
export function mountMasterDetail(el, { listWidth = 300 } = {}) {
  const wrap = document.createElement('div');
  wrap.style.cssText = `display:grid;grid-template-columns:${listWidth}px 1fr;grid-template-rows:1fr;flex:1;min-height:0;overflow:hidden;`;

  const listEl = document.createElement('div');
  listEl.className = 'list-pane';

  const detailEl = document.createElement('div');
  detailEl.style.cssText = 'overflow-y:auto;min-height:0;';

  wrap.appendChild(listEl);
  wrap.appendChild(detailEl);
  el.appendChild(wrap);

  return {
    listEl,
    detailEl,
    destroy() { wrap.remove(); },
  };
}
