// @ts-check
// Settings — Scan section.

import * as api from '../../api.js';
import { showToast, showApiError } from '../../components/toast.js';
import { mkSettingsGroup, mkSettingsGroupCard, mkSettingsRow } from './_shared.js';
import { t } from '../../i18n.js';

/**
 * @param {HTMLElement} el
 * @param {any} settings
 */
export function mount(el, settings) {
  let autoScan = !!settings?.auto_scan;
  let interval = settings?.scan_interval_minutes ?? 60;

  const scanGroup = mkSettingsGroup(t('settings.scan.group'));
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
  scanCard.appendChild(mkSettingsRow({ label: t('settings.scan.auto.label'), description: t('settings.scan.auto.desc'), control: autoToggleLabel }));

  const intervalInput = document.createElement('input');
  intervalInput.type = 'number';
  intervalInput.id = 'scan-interval';
  intervalInput.className = 'input w-24 text-sm';
  intervalInput.min = '1';
  intervalInput.value = String(interval);
  const intervalRow = mkSettingsRow({ label: t('settings.scan.interval.label'), description: t('settings.scan.interval.desc'), control: intervalInput });
  intervalRow.style.display = autoScan ? '' : 'none';
  scanCard.appendChild(intervalRow);

  const excludeToggleLabel = document.createElement('label');
  excludeToggleLabel.className = 'kani-toggle';
  const excludeEl = document.createElement('input');
  excludeEl.type = 'checkbox';
  excludeEl.id = 'scan-exclude-completed';
  excludeEl.className = 'kani-toggle__input';
  excludeEl.checked = !!settings?.scan_exclude_completed;
  const excludeTrack = document.createElement('span');
  excludeTrack.className = 'kani-toggle__track';
  excludeToggleLabel.appendChild(excludeEl);
  excludeToggleLabel.appendChild(excludeTrack);
  scanCard.appendChild(mkSettingsRow({ label: t('settings.scan.exclude.label'), description: t('settings.scan.exclude.desc'), control: excludeToggleLabel }));

  const saveRow = document.createElement('div');
  saveRow.className = 'flex items-center gap-3 px-4 py-3';
  saveRow.innerHTML = `<button type="button" class="btn-primary btn-sm js-scan-save">${t('common.save')}</button>`;
  scanCard.appendChild(saveRow);
  el.appendChild(scanGroup);

  const saveBtn = /** @type {HTMLButtonElement} */ (el.querySelector('.js-scan-save'));

  let lastSaved = {
    auto_scan: !!settings?.auto_scan,
    scan_interval_minutes: settings?.scan_interval_minutes ?? 60,
    scan_exclude_completed: !!settings?.scan_exclude_completed,
  };

  autoEl.addEventListener('change', () => {
    autoScan = autoEl.checked;
    intervalRow.style.display = autoScan ? '' : 'none';
  });

  function buildPayload() {
    return {
      auto_scan: autoEl.checked,
      scan_interval_minutes: Number(intervalInput.value) || 60,
      scan_exclude_completed: excludeEl.checked,
    };
  }

  saveBtn.addEventListener('click', async () => {
    saveBtn.disabled = true;
    const payload = buildPayload();
    if (JSON.stringify(payload) === JSON.stringify(lastSaved)) {
      saveBtn.disabled = false;
      return;
    }
    try {
      await api.updateSettings({ Scan: payload });
      lastSaved = { ...payload };
      showToast(t('common.saved'), { type: 'success' });
    } catch (e) {
      showApiError(e);
    } finally {
      saveBtn.disabled = false;
    }
  });

  return {
    destroy() { el.innerHTML = ''; },
    isDirty() { return JSON.stringify(buildPayload()) !== JSON.stringify(lastSaved); },
  };
}
