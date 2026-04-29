// @ts-check
// Settings — Scan section.

import * as api from '../../api.js';
import { mkSettingsGroup, mkSettingsGroupCard, mkSettingsRow, showResult } from './_shared.js';

/**
 * @param {HTMLElement} el
 * @param {any} settings
 */
export function mount(el, settings) {
  let autoScan = !!settings?.auto_scan;
  let interval = settings?.scan_interval_minutes ?? 60;

  const scanGroup = mkSettingsGroup('Automatic scanning');
  const scanCard  = mkSettingsGroupCard(scanGroup);

  const autoToggleLabel = document.createElement('label');
  autoToggleLabel.className = 'kani-toggle';
  const autoEl = document.createElement('input');
  autoEl.type = 'checkbox';
  autoEl.id = 'auto-scan-toggle';
  autoEl.className = 'kani-toggle__input';
  autoEl.checked = autoScan;
  const autoTrack = document.createElement('span');
  autoTrack.className = 'kani-toggle__track';
  autoToggleLabel.appendChild(autoEl);
  autoToggleLabel.appendChild(autoTrack);
  scanCard.appendChild(mkSettingsRow({ label: 'Auto scan', description: 'Automatically scan for new chapters on an interval.', control: autoToggleLabel }));

  const intervalInput = document.createElement('input');
  intervalInput.type = 'number';
  intervalInput.id = 'scan-interval';
  intervalInput.className = 'input w-24 text-sm';
  intervalInput.min = '1';
  intervalInput.value = String(interval);
  const intervalRow = mkSettingsRow({ label: 'Interval (minutes)', description: 'How often to scan for new chapters.', control: intervalInput });
  intervalRow.style.display = autoScan ? '' : 'none';
  scanCard.appendChild(intervalRow);

  const saveRow = document.createElement('div');
  saveRow.className = 'flex items-center gap-3 px-4 py-3';
  saveRow.innerHTML = `<button type="button" class="btn-primary btn-sm js-scan-save">Save</button><span class="js-scan-result text-sm hidden"></span>`;
  scanCard.appendChild(saveRow);
  el.appendChild(scanGroup);

  const saveBtn  = /** @type {HTMLButtonElement} */ (el.querySelector('.js-scan-save'));
  const resultEl = /** @type {HTMLElement} */ (el.querySelector('.js-scan-result'));

  autoEl.addEventListener('change', () => {
    autoScan = autoEl.checked;
    intervalRow.style.display = autoScan ? '' : 'none';
  });

  saveBtn.addEventListener('click', async () => {
    saveBtn.disabled = true;
    interval = Number(intervalInput.value) || 60;
    try {
      await api.updateSettings({ Scan: { auto_scan: autoScan, scan_interval_minutes: interval } });
      showResult(resultEl, true, 'Saved.');
    } catch (e) {
      showResult(resultEl, false, e?.message ?? 'Failed to save.');
    } finally {
      saveBtn.disabled = false;
    }
  });

  return { destroy() { el.innerHTML = ''; } };
}
