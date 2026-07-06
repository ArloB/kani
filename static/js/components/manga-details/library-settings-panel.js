// @ts-check
// Manage tab — Library section (refresh, auto-download, preferred-only).

import * as api from '../../api.js';
import { hasPermission } from '../../state.js';
import { mkCard, mkRow, mkItem } from './_shared.js';
import { t } from '../../i18n.js';

/**
 * @param {HTMLElement} containerEl
 * @param {{ dbId: number, autoScan: boolean }} ctx
 */
export function mountLibrarySettingsPanel(containerEl, ctx) {
  const { dbId, autoScan } = ctx;

  const card = mkCard();

  if (hasPermission('library:manage')) {
    const toggle = document.createElement('label');
    toggle.className = 'kani-toggle cursor-pointer';
    toggle.innerHTML = `<input type="checkbox" class="kani-toggle__input" aria-label="${t('manga.settings.auto_scan')}"><span class="kani-toggle__track"></span>`;
    const input = /** @type {HTMLInputElement} */ (toggle.querySelector('.kani-toggle__input'));
    input.checked = autoScan;
    input.addEventListener('change', async () => {
      try { await api.toggleAutoScan(dbId, input.checked); } catch { input.checked = !input.checked; }
    });
    card.appendChild(mkItem(mkRow(t('manga.settings.auto_scan'), t('manga.settings.auto_scan.desc'), toggle)));
  }

  if (hasPermission('library:manage')) {
    const toggle = document.createElement('label');
    toggle.className = 'kani-toggle cursor-pointer';
    toggle.innerHTML = `<input type="checkbox" class="kani-toggle__input" aria-label="${t('manga.settings.auto_download')}"><span class="kani-toggle__track"></span>`;
    const input = /** @type {HTMLInputElement} */ (toggle.querySelector('.kani-toggle__input'));
    api.getMangaDetails(dbId).then(res => { input.checked = res?.auto_download ?? false; }).catch(() => {});
    input.addEventListener('change', async () => {
      try { await api.toggleAutoDownload(dbId, input.checked); } catch { input.checked = !input.checked; }
    });
    card.appendChild(mkItem(mkRow(t('manga.settings.auto_download'), t('manga.settings.auto_download.desc'), toggle)));
  }

  if (hasPermission('library:manage')) {
    const toggle = document.createElement('label');
    toggle.className = 'kani-toggle cursor-pointer';
    toggle.innerHTML = `<input type="checkbox" class="kani-toggle__input" aria-label="${t('manga.settings.webhooks')}"><span class="kani-toggle__track"></span>`;
    const input = /** @type {HTMLInputElement} */ (toggle.querySelector('.kani-toggle__input'));
    input.checked = true;
    api.getMangaWebhookNotify(dbId).then(res => { input.checked = res?.enabled ?? true; }).catch(() => {});
    input.addEventListener('change', async () => {
      try { await api.setMangaWebhookNotify(dbId, input.checked); } catch { input.checked = !input.checked; }
    });
    card.appendChild(mkItem(mkRow(t('manga.settings.webhooks'), t('manga.settings.webhooks.desc'), toggle)));
  }

  if (hasPermission('chapter:download')) {
    const toggle = document.createElement('label');
    toggle.className = 'kani-toggle cursor-pointer';
    toggle.innerHTML = `<input type="checkbox" class="kani-toggle__input" aria-label="${t('manga.settings.preferred_only')}"><span class="kani-toggle__track"></span>`;
    const input = /** @type {HTMLInputElement} */ (toggle.querySelector('.kani-toggle__input'));
    api.getMangaDetails(dbId).then(res => { input.checked = res?.download_all_preferred_only ?? true; }).catch(() => {});
    input.addEventListener('change', async () => {
      try { await api.toggleDownloadAllPreferred(dbId, input.checked); } catch { input.checked = !input.checked; }
    });
    card.appendChild(mkItem(mkRow(t('manga.settings.preferred_only'), t('manga.settings.preferred_only.desc'), toggle)));
  }

  containerEl.appendChild(card);
}
