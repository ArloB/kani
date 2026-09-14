// @ts-check

import { t } from '../i18n.js';

/**
 * @param {{ onTab: (tab: 'extensions' | 'repos') => void, canInstall: boolean }} opts
 */
export function createSourcesHeaderActions({ onTab, canInstall }) {
  const tabs = document.createElement('div');
  tabs.className = 'flex gap-1';

  /** @param {string} label @param {'extensions' | 'repos'} tab */
  const tabButton = (label, tab) => {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'btn-ghost btn-sm';
    btn.textContent = label;
    btn.addEventListener('click', () => onTab(tab));
    tabs.appendChild(btn);
    return btn;
  };
  const extensionsTab = tabButton(t('sources.tab.extensions'), 'extensions');
  const reposTab = tabButton(t('repo.tab'), 'repos');

  /** @type {HTMLButtonElement | null} */
  let addSourceBtn = null;
  if (canInstall) {
    addSourceBtn = document.createElement('button');
    addSourceBtn.type = 'button';
    addSourceBtn.className = 'btn-primary btn-sm';
    addSourceBtn.textContent = t('source.add.title');
  }

  /** @param {'extensions' | 'repos'} tab */
  const setActive = (tab) => {
    extensionsTab.classList.toggle('bg-surface-2', tab === 'extensions');
    reposTab.classList.toggle('bg-surface-2', tab === 'repos');
    extensionsTab.setAttribute('aria-pressed', String(tab === 'extensions'));
    reposTab.setAttribute('aria-pressed', String(tab === 'repos'));
    addSourceBtn?.classList.toggle('hidden', tab === 'repos');
  };

  const actions = /** @type {HTMLElement[]} */ ([tabs]);
  if (addSourceBtn) actions.push(addSourceBtn);

  return { actions, addSourceBtn, setActive };
}
