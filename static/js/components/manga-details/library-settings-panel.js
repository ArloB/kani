// @ts-check
// Manage tab — Library section (refresh, auto-download, preferred-only).

import * as api from '../../api.js';
import { hasPermission } from '../../state.js';
import { mkCard, mkRow, mkItem } from './_shared.js';

/**
 * @param {HTMLElement} containerEl
 * @param {{ dbId: number, autoScan: boolean }} ctx
 */
export function mountLibrarySettingsPanel(containerEl, ctx) {
  const { dbId, autoScan } = ctx;

  const card = mkCard();

  if (hasPermission('library:refresh')) {
    const refreshBtn = document.createElement('button');
    refreshBtn.type = 'button';
    refreshBtn.className = 'btn-ghost btn-sm';
    refreshBtn.textContent = 'Refresh';
    refreshBtn.addEventListener('click', async () => {
      refreshBtn.disabled = true;
      try {
        await api.refreshManga(dbId);
        refreshBtn.textContent = 'Done';
        setTimeout(() => { refreshBtn.textContent = 'Refresh'; }, 3000);
      } finally { refreshBtn.disabled = false; }
    });
    card.appendChild(mkItem(mkRow('Refresh metadata', 'Re-fetch title, cover, and description from source', refreshBtn)));
  }

  if (autoScan && hasPermission('library:manage')) {
    const toggle = document.createElement('label');
    toggle.className = 'kani-toggle cursor-pointer';
    toggle.innerHTML = `<input type="checkbox" class="kani-toggle__input" aria-label="Auto-download new chapters"><span class="kani-toggle__track"></span>`;
    const input = /** @type {HTMLInputElement} */ (toggle.querySelector('.kani-toggle__input'));
    api.getMangaDetails(dbId).then(res => { input.checked = res?.auto_download ?? false; }).catch(() => {});
    input.addEventListener('change', async () => {
      try { await api.toggleAutoDownload(dbId, input.checked); } catch { input.checked = !input.checked; }
    });
    card.appendChild(mkItem(mkRow('Auto-download', 'Automatically download new chapters when found', toggle)));
  }

  if (hasPermission('chapter:download')) {
    const toggle = document.createElement('label');
    toggle.className = 'kani-toggle cursor-pointer';
    toggle.innerHTML = `<input type="checkbox" class="kani-toggle__input" aria-label="Download All: preferred only"><span class="kani-toggle__track"></span>`;
    const input = /** @type {HTMLInputElement} */ (toggle.querySelector('.kani-toggle__input'));
    api.getMangaDetails(dbId).then(res => { input.checked = res?.download_all_preferred_only ?? true; }).catch(() => {});
    input.addEventListener('change', async () => {
      try { await api.toggleDownloadAllPreferred(dbId, input.checked); } catch { input.checked = !input.checked; }
    });
    card.appendChild(mkItem(mkRow('Download All: preferred only', 'When enabled, "Download All" downloads one version per chapter using scanlator preferences', toggle)));
  }

  containerEl.appendChild(card);
}
