// @ts-check
// Shared drag-and-drop sortable list component.
// Extracted from manga-details/scanlator-prefs-panel.js for reuse.

const DRAG_HANDLE_SVG = `<svg viewBox="0 0 24 24" fill="currentColor" xmlns="http://www.w3.org/2000/svg" aria-hidden="true">
  <circle cx="9" cy="6" r="1.5"/><circle cx="15" cy="6" r="1.5"/>
  <circle cx="9" cy="12" r="1.5"/><circle cx="15" cy="12" r="1.5"/>
  <circle cx="9" cy="18" r="1.5"/><circle cx="15" cy="18" r="1.5"/>
</svg>`;

/**
 * Mounts a drag-sortable list into `container`.
 *
 * @template T
 * @param {HTMLElement} container
 * @param {{
 *   items: T[],
 *   getId: (item: T) => string | number,
 *   renderItem: (item: T, index: number) => HTMLElement,
 *   onReorder: (ids: Array<string | number>, items: T[]) => void | Promise<void>,
 *   className?: string,
 * }} opts
 * @returns {{ update: (items: T[]) => void, destroy: () => void }}
 */
export function mountSortableList(container, opts) {
  let items = [...opts.items];

  /** @type {HTMLElement | null} */
  let dragSrc = null;
  /** @type {HTMLUListElement | null} */
  let ul = null;

  function _render() {
    container.innerHTML = '';
    ul = document.createElement('ul');
    ul.className = opts.className ?? 'flex flex-col divide-y divide-border-subtle';

    for (let i = 0; i < items.length; i++) {
      const item = items[i];
      const id = String(opts.getId(item));

      const li = document.createElement('li');
      li.className = 'flex items-center gap-3 py-2 px-2 hover:bg-surface-2 cursor-grab active:cursor-grabbing transition-colors';
      li.draggable = true;
      li.dataset.sortId = id;

      const grip = document.createElement('span');
      grip.className = 'text-text-muted shrink-0 cursor-grab select-none icon-sm';
      grip.innerHTML = DRAG_HANDLE_SVG;

      const content = opts.renderItem(item, i);
      li.appendChild(grip);
      li.appendChild(content);

      li.addEventListener('dragstart', (e) => {
        dragSrc = li;
        e.dataTransfer?.setData('text/plain', id);
        li.classList.add('opacity-50');
      });
      li.addEventListener('dragend', () => {
        dragSrc = null;
        li.classList.remove('opacity-50');
        ul?.querySelectorAll('li[data-sort-id]').forEach(el => el.classList.remove('border-t-2', 'border-t-accent'));
      });
      li.addEventListener('dragover', (e) => {
        e.preventDefault();
        if (dragSrc && dragSrc !== li) {
          ul?.querySelectorAll('li[data-sort-id]').forEach(el => el.classList.remove('border-t-2', 'border-t-accent'));
          li.classList.add('border-t-2', 'border-t-accent');
        }
      });
      li.addEventListener('drop', (e) => {
        e.preventDefault();
        if (!dragSrc || dragSrc === li) return;
        const srcId = dragSrc.dataset.sortId;
        const tgtId = li.dataset.sortId;
        const srcIdx = items.findIndex(it => String(opts.getId(it)) === srcId);
        const tgtIdx = items.findIndex(it => String(opts.getId(it)) === tgtId);
        if (srcIdx < 0 || tgtIdx < 0) return;
        const [moved] = items.splice(srcIdx, 1);
        items.splice(tgtIdx, 0, moved);
        _render();
        const ids = items.map(it => opts.getId(it));
        opts.onReorder(ids, [...items]);
      });

      ul.appendChild(li);
    }

    container.appendChild(ul);
  }

  _render();

  return {
    update(newItems) {
      items = [...newItems];
      _render();
    },
    destroy() {
      container.innerHTML = '';
      ul = null;
    },
  };
}
