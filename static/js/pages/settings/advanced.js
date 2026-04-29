// @ts-check
// Settings — Advanced section (FlareSolverr, library path, WASM instances).

import * as api from '../../api.js';
import { getLocal, setLocal } from '../../utils.js';
import { addPendingFields } from '../../components/restart-tray.js';
import { mkSettingsGroup, mkSettingsGroupCard, mkSettingsRow, mkToggleRow, showResult } from './_shared.js';

/**
 * @param {HTMLElement} el
 * @param {any} settings
 * @param {string} bootId
 */
export function mount(el, settings, bootId) {
  const advGroup = mkSettingsGroup('Server');
  const advCard  = mkSettingsGroupCard(advGroup);

  const flareInput = document.createElement('input');
  flareInput.type = 'url';
  flareInput.id = 'flaresolverr-url';
  flareInput.className = 'input w-56 text-sm js-adv-field';
  flareInput.dataset.key = 'flaresolverr_url';
  flareInput.placeholder = 'http://localhost:8191';
  flareInput.value = settings?.flaresolverr_url ?? '';
  advCard.appendChild(mkSettingsRow({ label: 'FlareSolverr URL', description: 'Optional. Used by sources that require Cloudflare bypass.', control: flareInput }));

  const libPathInput = document.createElement('input');
  libPathInput.type = 'text';
  libPathInput.id = 'library-path';
  libPathInput.className = 'input w-56 text-sm js-adv-field';
  libPathInput.dataset.key = 'library_path';
  libPathInput.placeholder = '/data/library';
  libPathInput.value = settings?.library_path ?? '';
  advCard.appendChild(mkSettingsRow({ label: 'Library path', description: 'Filesystem path where downloaded chapters are stored.', badge: 'Restart required', control: libPathInput }));

  const wasmInput = document.createElement('input');
  wasmInput.type = 'number';
  wasmInput.id = 'max-wasm-instances';
  wasmInput.className = 'input w-24 text-sm js-adv-num';
  wasmInput.dataset.key = 'max_wasm_instances';
  wasmInput.min = '1';
  wasmInput.value = String(settings?.max_wasm_instances ?? '');
  advCard.appendChild(mkSettingsRow({ label: 'Max WASM instances', description: 'Sandbox limit for source extensions.', badge: 'Restart required', control: wasmInput }));

  let httpLogging = settings?.http_request_logging ?? false;
  advCard.appendChild(mkToggleRow({
    label: 'HTTP request logging',
    description: 'Log incoming HTTP requests to the server console.',
    checked: httpLogging,
    onChange: (v) => { httpLogging = v; },
  }));

  let registrationEnabled = settings?.registration_enabled ?? false;
  advCard.appendChild(mkToggleRow({
    label: 'Public registration',
    description: 'Allow anyone to create an account via the /register page.',
    checked: registrationEnabled,
    onChange: (v) => { registrationEnabled = v; },
  }));

  const saveRow = document.createElement('div');
  saveRow.className = 'flex items-center gap-3 px-4 py-3';
  saveRow.innerHTML = `<button type="button" class="btn-primary btn-sm js-adv-save">Save</button><span class="js-adv-result text-sm hidden"></span>`;
  advCard.appendChild(saveRow);
  el.appendChild(advGroup);

  const saveBtn  = /** @type {HTMLButtonElement} */ (el.querySelector('.js-adv-save'));
  const resultEl = /** @type {HTMLElement} */ (el.querySelector('.js-adv-result'));

  saveBtn.addEventListener('click', async () => {
    saveBtn.disabled = true;
    /** @type {Record<string, any>} */
    const payload = {};
    for (const input of /** @type {NodeListOf<HTMLInputElement>} */ (el.querySelectorAll('.js-adv-field'))) {
      const key = input.dataset.key;
      if (key) payload[key] = input.value;
    }
    for (const input of /** @type {NodeListOf<HTMLInputElement>} */ (el.querySelectorAll('.js-adv-num'))) {
      const key = input.dataset.key;
      if (key && input.value !== '') payload[key] = Number(input.value);
    }
    payload['http_request_logging'] = httpLogging;
    payload['registration_enabled'] = registrationEnabled;

    try {
      await api.updateSettings({ Advanced: payload });
      showResult(resultEl, true, 'Saved. Some changes require a server restart.');
      const restartFields = [];
      if ((payload.library_path ?? '') !== (settings?.library_path ?? '')) restartFields.push('library_path');
      if ((payload.max_wasm_instances ?? null) !== (settings?.max_wasm_instances ?? null)) restartFields.push('max_wasm_instances');
      if (restartFields.length) {
        setLocal('kani_restart_boot_id', bootId);
        addPendingFields(restartFields);
        // Notify RestartTray via storage event
        window.dispatchEvent(new StorageEvent('storage', { key: 'kani_restart_needed' }));
      }
    } catch (e) {
      showResult(resultEl, false, e?.message ?? 'Failed to save.');
    } finally {
      saveBtn.disabled = false;
    }
  });

  return { destroy() { el.innerHTML = ''; } };
}
