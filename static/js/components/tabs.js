// @ts-check
// Generic tab bar component — renders a horizontal tab strip.

/**
 * @template T
 * @typedef {{ id: T, name: string }} Tab
 */

/**
 * Renders a generic tab bar into `container`.
 *
 * @template T
 * @param {HTMLElement} container
 * @param {{
 *   tabs: Tab<T>[],
 *   activeId: T,
 *   onSelect: (id: T) => void,
 * }} props
 * @returns {{ update: (activeId: T) => void, destroy: () => void }}
 */
export function renderTabs(container, { tabs, activeId, onSelect }) {
  let _activeId = activeId;

  const bar = document.createElement('div');
  bar.className = 'flex gap-1 overflow-x-auto [scrollbar-width:none] border-b border-border';
  bar.setAttribute('role', 'tablist');
  container.appendChild(bar);

  function _render() {
    bar.innerHTML = '';
    for (const tab of tabs) {
      const isActive = tab.id === _activeId;
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.setAttribute('role', 'tab');
      btn.setAttribute('aria-selected', String(isActive));
      btn.className = 'px-4 py-2 text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent rounded-t-md'
        + (isActive ? ' text-accent border-b-2 border-accent' : ' text-text-muted');
      btn.textContent = tab.name;
      btn.addEventListener('click', () => {
        _activeId = tab.id;
        _render();
        onSelect(tab.id);
      });
      bar.appendChild(btn);
    }
  }

  _render();

  return {
    update(newActiveId) {
      _activeId = newActiveId;
      _render();
    },
    destroy() {
      bar.remove();
    },
  };
}

/**
 * Convenience wrapper for the common category-filter case where IDs are
 * `number | null` (null = "All").
 *
 * @param {HTMLElement} container
 * @param {{
 *   tabs: { id: number | null, name: string }[],
 *   activeId: number | null,
 *   onSelect: (id: number | null) => void,
 * }} props
 * @returns {{ update: (activeId: number | null) => void, destroy: () => void }}
 */
export function renderCategoryTabs(container, { tabs, activeId, onSelect }) {
  return renderTabs(container, { tabs, activeId, onSelect });
}
