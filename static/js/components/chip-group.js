// @ts-check
// Chip group — multi-select filter chips.

/**
 * @typedef {{ id: string | number, label: string }} ChipItem
 */

/**
 * @param {HTMLElement} container
 * @param {{
 *   items: ChipItem[],
 *   selected: Set<string | number>,
 *   onToggle: (id: string | number, selected: boolean) => void,
 *   multi?: boolean,
 * }} props
 * @returns {{ update: (selected: Set<string | number>) => void, destroy: () => void }}
 */
export function renderChipGroup(container, { items, selected, onToggle, multi = true }) {
  let _selected = new Set(selected);

  const wrap = document.createElement('div');
  wrap.className = 'flex flex-wrap gap-2';
  container.appendChild(wrap);

  function _render() {
    wrap.innerHTML = '';
    for (const item of items) {
      const isActive = _selected.has(item.id);
      const chip = document.createElement('button');
      chip.type = 'button';
      chip.className = isActive ? 'chip chip-active' : 'chip';
      chip.textContent = item.label;
      chip.addEventListener('click', () => {
        if (multi) {
          const nowActive = !_selected.has(item.id);
          if (nowActive) {
            _selected.add(item.id);
          } else {
            _selected.delete(item.id);
          }
          onToggle(item.id, nowActive);
        } else {
          // Single-select: only one active at a time
          const alreadyActive = _selected.has(item.id);
          _selected = new Set(alreadyActive ? [] : [item.id]);
          onToggle(item.id, !alreadyActive);
        }
        _render();
      });
      wrap.appendChild(chip);
    }
  }

  _render();

  return {
    update(newSelected) {
      _selected = new Set(newSelected);
      _render();
    },
    destroy() {
      wrap.remove();
    },
  };
}
