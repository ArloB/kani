// @ts-check
// Settings — Downloads section.

import * as api from '../../api.js';
import { getLocal, setLocal } from '../../utils.js';
import { mkSettingsGroup, mkSettingsGroupCard, mkSettingsRow, mkToggleRow, showResult } from './_shared.js';

/**
 * @param {HTMLElement} el
 * @param {any} settings
 */
export function mount(el, settings) {
  const fields = [
    { key: 'concurrent_page_downloads',   label: 'Concurrent page downloads',   desc: 'Number of pages downloaded in parallel per chapter.',   min: 1 },
    { key: 'concurrent_manga_downloads',  label: 'Concurrent manga downloads',  desc: 'Number of chapters downloaded simultaneously.',          min: 1 },
    { key: 'chapter_queue_size',          label: 'Chapter queue size',          desc: 'Maximum chapters waiting in the download queue.',        min: 1 },
    { key: 'max_retries',                 label: 'Max retries',                 desc: 'How many times to retry a failed page download.',        min: 0 },
    { key: 'initial_retry_delay_ms',      label: 'Initial retry delay (ms)',    desc: 'Starting delay before the first retry.',                 min: 0 },
  ];

  const serverGroup = mkSettingsGroup('Server download settings');
  const serverCard  = mkSettingsGroupCard(serverGroup);

  for (const f of fields) {
    const input = document.createElement('input');
    input.type = 'number';
    input.id = f.key;
    input.className = 'input w-24 text-sm js-dl-field';
    input.dataset.key = f.key;
    input.min = String(f.min);
    input.value = String(settings?.[f.key] ?? '');
    serverCard.appendChild(mkSettingsRow({ label: f.label, description: f.desc, control: input }));
  }

  const saveRow = document.createElement('div');
  saveRow.className = 'flex items-center gap-3 px-4 py-3';
  saveRow.innerHTML = `<button type="button" class="btn-primary btn-sm js-dl-save">Save</button><span class="js-dl-result text-sm hidden"></span>`;
  serverCard.appendChild(saveRow);
  el.appendChild(serverGroup);

  const saveBtn  = /** @type {HTMLButtonElement} */ (el.querySelector('.js-dl-save'));
  const resultEl = /** @type {HTMLElement} */ (el.querySelector('.js-dl-result'));

  saveBtn.addEventListener('click', async () => {
    saveBtn.disabled = true;
    /** @type {Record<string, number>} */
    const payload = {};
    for (const input of /** @type {NodeListOf<HTMLInputElement>} */ (el.querySelectorAll('.js-dl-field'))) {
      const key = input.dataset.key;
      if (key) payload[key] = Number(input.value);
    }
    try {
      await api.updateSettings({ Download: payload });
      showResult(resultEl, true, 'Saved.');
    } catch (e) {
      showResult(resultEl, false, e?.message ?? 'Failed to save.');
    } finally {
      saveBtn.disabled = false;
    }
  });

  // Download ahead — client-side localStorage setting
  const aheadGroup = mkSettingsGroup('Download ahead');
  const aheadCard  = mkSettingsGroupCard(aheadGroup);

  const aheadToggleLabel = document.createElement('label');
  aheadToggleLabel.className = 'kani-toggle';
  const aheadEnabledInput = document.createElement('input');
  aheadEnabledInput.type = 'checkbox';
  aheadEnabledInput.className = 'kani-toggle__input';
  aheadEnabledInput.checked = getLocal('kani_download_ahead_enabled') === 'true';
  const aheadToggleTrack = document.createElement('span');
  aheadToggleTrack.className = 'kani-toggle__track';
  aheadToggleLabel.appendChild(aheadEnabledInput);
  aheadToggleLabel.appendChild(aheadToggleTrack);
  aheadCard.appendChild(mkSettingsRow({
    label: 'Enable download ahead',
    description: 'While reading, automatically download the next N chapters in advance.',
    control: aheadToggleLabel,
  }));

  const aheadCountInput = document.createElement('input');
  aheadCountInput.type = 'number';
  aheadCountInput.className = 'input w-20 text-sm';
  aheadCountInput.min = '1';
  aheadCountInput.max = '10';
  aheadCountInput.value = getLocal('kani_download_ahead_count') || '3';
  const aheadCountRow = mkSettingsRow({
    label: 'Chapters ahead to download',
    description: 'How many chapters to pre-download while reading (1–10).',
    control: aheadCountInput,
  });
  aheadCountRow.style.display = aheadEnabledInput.checked ? '' : 'none';
  aheadCard.appendChild(aheadCountRow);
  el.appendChild(aheadGroup);

  aheadEnabledInput.addEventListener('change', () => {
    setLocal('kani_download_ahead_enabled', String(aheadEnabledInput.checked));
    aheadCountRow.style.display = aheadEnabledInput.checked ? '' : 'none';
  });
  aheadCountInput.addEventListener('change', () => {
    const v = Math.max(1, Math.min(10, Number(aheadCountInput.value) || 3));
    aheadCountInput.value = String(v);
    setLocal('kani_download_ahead_count', String(v));
  });

  return { destroy() { el.innerHTML = ''; } };
}
