// @ts-check
// Settings — Advanced section (FlareSolverr, library path, WASM instances).

import { h, render } from 'preact';
import htm from 'htm';
import * as api from '../../api.js';
import { setLocal } from '../../utils.js';
import { addPendingFields } from '../../components/restart-tray.js';
import { showToast, showApiError } from '../../components/toast.js';
import { mkSettingsGroup, mkSettingsGroupCard, mkSettingsRow, mkToggleRow } from './_shared.js';
import { FolderPicker } from '../../components/folder-picker.js';
import { PathMigrationDialog } from '../../components/path-migration-dialog.js';
import { t } from '../../i18n.js';

const html = htm.bind(h);

/**
 * @param {HTMLElement} el
 * @param {any} settings
 * @param {string} bootId
 */
export function mount(el, settings, bootId) {
  // Preact roots for dialogs (appended first so they stack above everything)
  const pickerRoot = document.createElement('div');
  const migRoot    = document.createElement('div');
  el.appendChild(pickerRoot);
  el.appendChild(migRoot);

  // ── Picker state ──────────────────────────────────────────────────────────
  let pickerOpen = false;
  let pickerInitial = '/';
  let pickerOnSelect = /** @type {((p: string) => void)|null} */ (null);

  function openPicker(initialPath, onSelect) {
    pickerInitial = initialPath || '/';
    pickerOnSelect = onSelect;
    pickerOpen = true;
    renderDialogs();
  }

  // ── Migration dialog state ─────────────────────────────────────────────────
  let migOpen = false;
  let migField = '';
  let migCurrentPath = '';
  let migNewPath = '';
  let migOnDone = /** @type {((moved: boolean) => void)|null} */ (null);
  let migOnCancel = /** @type {(() => void)|null} */ (null);

  function renderDialogs() {
    render(html`
      <${FolderPicker}
        open=${pickerOpen}
        initialPath=${pickerInitial}
        onClose=${() => { pickerOpen = false; renderDialogs(); }}
        onSelect=${(/** @type {string} */ path) => {
          pickerOpen = false;
          renderDialogs();
          pickerOnSelect?.(path);
        }}
      />
      <${PathMigrationDialog}
        open=${migOpen}
        field=${migField}
        currentPath=${migCurrentPath}
        newPath=${migNewPath}
        onDone=${(/** @type {boolean} */ moved) => {
          migOpen = false;
          renderDialogs();
          migOnDone?.(moved);
        }}
        onCancel=${() => {
          migOpen = false;
          renderDialogs();
          migOnCancel?.();
        }}
      />
    `, migRoot);
  }

  // Initial render (both closed)
  renderDialogs();

  const advGroup = mkSettingsGroup(t('settings.advanced.server.group'));
  const advCard  = mkSettingsGroupCard(advGroup);

  const flareInput = document.createElement('input');
  flareInput.type = 'url';
  flareInput.id = 'flaresolverr-url';
  flareInput.className = 'input w-56 text-sm js-adv-field';
  flareInput.dataset.key = 'flaresolverr_url';
  flareInput.placeholder = 'http://localhost:8191';
  flareInput.value = settings?.flaresolverr_url ?? '';
  advCard.appendChild(mkSettingsRow({ label: t('settings.advanced.flaresolverr.label'), description: t('settings.advanced.flaresolverr.desc'), control: flareInput }));

  // ── Library path control ──────────────────────────────────────────────────
  const { el: libEl, hidden: libHidden, setPath: setLibPath } = makePathControl('library_path', settings?.library_path ?? '');
  advCard.appendChild(mkSettingsRow({
    label: t('settings.advanced.library_path.label'),
    description: t('settings.advanced.library_path.desc'),
    control: libEl,
  }));

  libEl.querySelector('button')?.addEventListener('click', () => {
    openPicker(libHidden.value || '/', (path) => setLibPath(path));
  });

  // ── WASM storage path control ─────────────────────────────────────────────
  const { el: wasmPathEl, hidden: wasmPathHidden, setPath: setWasmPath } = makePathControl('wasm_storage_path', settings?.wasm_storage_path ?? '');
  advCard.appendChild(mkSettingsRow({
    label: t('settings.advanced.wasm_path.label'),
    description: t('settings.advanced.wasm_path.desc'),
    control: wasmPathEl,
  }));

  wasmPathEl.querySelector('button')?.addEventListener('click', () => {
    openPicker(wasmPathHidden.value || '/', (path) => setWasmPath(path));
  });

  // ── Other numeric / toggle fields ─────────────────────────────────────────
  const wasmInput = document.createElement('input');
  wasmInput.type = 'number';
  wasmInput.id = 'max-wasm-instances';
  wasmInput.className = 'input w-24 text-sm js-adv-num';
  wasmInput.dataset.key = 'max_wasm_instances';
  wasmInput.min = '1';
  wasmInput.value = String(settings?.max_wasm_instances ?? '');
  advCard.appendChild(mkSettingsRow({ label: t('settings.advanced.wasm_instances.label'), description: t('settings.advanced.wasm_instances.desc'), badge: t('settings.badge.restart_required'), control: wasmInput }));

  const coverDimInput = document.createElement('input');
  coverDimInput.type = 'number';
  coverDimInput.id = 'cover-max-dimension';
  coverDimInput.className = 'input w-24 text-sm js-adv-num';
  coverDimInput.dataset.key = 'cover_max_dimension';
  coverDimInput.min = '100';
  coverDimInput.max = '2000';
  coverDimInput.placeholder = '800';
  coverDimInput.value = settings?.cover_max_dimension != null ? String(settings.cover_max_dimension) : '';
  advCard.appendChild(mkSettingsRow({ label: t('settings.advanced.cover_dim.label'), description: t('settings.advanced.cover_dim.desc'), control: coverDimInput }));

  let httpLogging = settings?.http_request_logging ?? false;
  advCard.appendChild(mkToggleRow({
    label: t('settings.advanced.http_logging.label'),
    description: t('settings.advanced.http_logging.desc'),
    checked: httpLogging,
    onChange: (v) => { httpLogging = v; },
  }));

  let browserDebugLogging = settings?.browser_debug_logging ?? false;
  advCard.appendChild(mkToggleRow({
    label: t('settings.advanced.browser_logging.label'),
    description: t('settings.advanced.browser_logging.desc'),
    checked: browserDebugLogging,
    onChange: (v) => { browserDebugLogging = v; },
  }));

  let registrationEnabled = settings?.registration_enabled ?? false;
  advCard.appendChild(mkToggleRow({
    label: t('settings.advanced.registration.label'),
    description: t('settings.advanced.registration.desc'),
    checked: registrationEnabled,
    onChange: (v) => { registrationEnabled = v; },
  }));

  const saveRow = document.createElement('div');
  saveRow.className = 'flex items-center gap-3 px-4 py-3';
  saveRow.innerHTML = `<button type="button" class="btn-primary btn-sm js-adv-save">${t('common.save')}</button>`;
  advCard.appendChild(saveRow);
  el.appendChild(advGroup);

  // ── Credential Encryption ──────────────────────────────────────────────────
  const encGroup = mkSettingsGroup(t('settings.advanced.encryption.group'));
  const encCard  = mkSettingsGroupCard(encGroup);

  const encStatusEl = document.createElement('p');
  encStatusEl.className = 'text-sm text-text-muted px-1';
  encStatusEl.textContent = t('common.loading');

  const encMigrateBtn = document.createElement('button');
  encMigrateBtn.type = 'button';
  encMigrateBtn.className = 'btn-primary btn-sm hidden';
  encMigrateBtn.textContent = t('settings.advanced.encryption.btn');

  const encResultEl = document.createElement('span');
  encResultEl.className = 'text-xs hidden';

  const encCtrl = document.createElement('div');
  encCtrl.className = 'flex flex-col gap-2';
  encCtrl.appendChild(encStatusEl);

  const encBtnRow = document.createElement('div');
  encBtnRow.className = 'flex items-center gap-3';
  encBtnRow.appendChild(encMigrateBtn);
  encBtnRow.appendChild(encResultEl);
  encCtrl.appendChild(encBtnRow);

  encCard.appendChild(mkSettingsRow({
    label: t('settings.advanced.encryption.label'),
    description: t('settings.advanced.encryption.desc'),
    tooltip: t('settings.advanced.encryption.tooltip'),
    control: encCtrl,
  }));
  el.appendChild(encGroup);

  async function refreshEncStatus() {
    try {
      const s = await api.getCredentialEncryptionStatus();
      if (!s.encryption_enabled) {
        encStatusEl.textContent = t('settings.advanced.encryption.disabled');
        encMigrateBtn.classList.add('hidden');
      } else if (s.plaintext_count > 0) {
        encStatusEl.textContent = t('settings.advanced.encryption.partial', { count: s.plaintext_count, s: s.plaintext_count === 1 ? '' : 's' });
        encMigrateBtn.classList.remove('hidden');
      } else {
        encStatusEl.textContent = t('settings.advanced.encryption.all_done');
        encMigrateBtn.classList.add('hidden');
      }
    } catch {
      encStatusEl.textContent = t('settings.advanced.encryption.load_failed');
    }
  }

  refreshEncStatus();

  encMigrateBtn.addEventListener('click', async () => {
    encMigrateBtn.disabled = true;
    encMigrateBtn.textContent = t('settings.advanced.encryption.encrypting');
    encResultEl.className = 'text-xs hidden';
    try {
      await api.migrateCredentialsToEncrypted();
      encResultEl.textContent = t('settings.advanced.encryption.result_done');
      encResultEl.className = 'text-xs text-success';
      await refreshEncStatus();
    } catch (/** @type {any} */ e) {
      encResultEl.textContent = e?.message ?? 'Failed';
      encResultEl.className = 'text-xs text-error';
    } finally {
      encMigrateBtn.disabled = false;
      encMigrateBtn.textContent = t('settings.advanced.encryption.btn');
    }
  });

  const saveBtn = /** @type {HTMLButtonElement} */ (el.querySelector('.js-adv-save'));

  let lastSaved = {
    flaresolverr_url: settings?.flaresolverr_url ?? '',
    library_path: settings?.library_path ?? '',
    wasm_storage_path: settings?.wasm_storage_path ?? '',
    max_wasm_instances: settings?.max_wasm_instances ?? null,
    cover_max_dimension: settings?.cover_max_dimension ?? null,
    http_request_logging: settings?.http_request_logging ?? false,
    browser_debug_logging: settings?.browser_debug_logging ?? false,
    registration_enabled: settings?.registration_enabled ?? false,
  };

  /** @returns {Record<string, any>} */
  function buildAdvPayload() {
    /** @type {Record<string, any>} */
    const payload = {};
    for (const input of /** @type {NodeListOf<HTMLInputElement>} */ (el.querySelectorAll('.js-adv-field'))) {
      const key = input.dataset.key;
      if (key) payload[key] = input.value;
    }
    for (const input of /** @type {NodeListOf<HTMLInputElement>} */ (el.querySelectorAll('.js-adv-num'))) {
      const key = input.dataset.key;
      if (key && input.value !== '') payload[key] = Number(input.value);
      else if (key) payload[key] = null;
    }
    payload['http_request_logging'] = httpLogging;
    payload['browser_debug_logging'] = browserDebugLogging;
    payload['registration_enabled'] = registrationEnabled;
    return payload;
  }

  /**
   * Prompts the migration dialog for a field if its value changed.
   * Resolves immediately if unchanged.
   * @param {string} field
   * @param {string} oldPath
   * @param {string} newPath
   * @returns {Promise<boolean>} true if files were moved
   */
  function promptMigration(field, oldPath, newPath) {
    if (!newPath || newPath === oldPath) return Promise.resolve(false);
    return new Promise((resolve) => {
      migField = field;
      migCurrentPath = oldPath;
      migNewPath = newPath;
      migOpen = true;
      migOnDone = (moved) => {
        if (moved) {
          // Update lastSaved for this field; the backend already persisted it
          lastSaved[field] = newPath;
          if (field === 'library_path') setLibPath(newPath);
          else if (field === 'wasm_storage_path') setWasmPath(newPath);
        }
        resolve(moved);
      };
      migOnCancel = () => resolve(false);
      renderDialogs();
    });
  }

  saveBtn.addEventListener('click', async () => {
    saveBtn.disabled = true;
    const payload = buildAdvPayload();

    if (JSON.stringify(payload) === JSON.stringify(lastSaved)) {
      saveBtn.disabled = false;
      return;
    }

    const libChanged = payload.library_path !== lastSaved.library_path;
    const wasmChanged = payload.wasm_storage_path !== lastSaved.wasm_storage_path;

    // Handle path migrations sequentially before the normal settings save
    if (libChanged) {
      await promptMigration('library_path', lastSaved.library_path, payload.library_path);
    }
    if (wasmChanged) {
      await promptMigration('wasm_storage_path', lastSaved.wasm_storage_path, payload.wasm_storage_path);
    }

    // Save all settings (including any updated paths) via the normal endpoint
    try {
      const freshPayload = buildAdvPayload();
      await api.updateSettings({ Advanced: freshPayload });
      lastSaved = { ...freshPayload };
      showToast(t('common.saved'), { type: 'success' });
      if ((freshPayload.max_wasm_instances ?? null) !== (settings?.max_wasm_instances ?? null)) {
        setLocal('kani_restart_boot_id', bootId);
        addPendingFields(['max_wasm_instances']);
        window.dispatchEvent(new StorageEvent('storage', { key: 'kani_restart_needed' }));
      }
    } catch (e) {
      showApiError(e);
    } finally {
      saveBtn.disabled = false;
    }
  });

  // ── Maintenance ────────────────────────────────────────────────────────────

  const maintGroup = mkSettingsGroup(t('settings.advanced.maintenance.group'));
  const maintCard  = mkSettingsGroupCard(maintGroup);

  const maintRow = document.createElement('div');
  maintRow.className = 'flex items-center gap-3 px-4 py-3';
  const maintBtn = document.createElement('button');
  maintBtn.type = 'button';
  maintBtn.className = 'btn-ghost btn-sm';
  maintBtn.textContent = t('settings.advanced.maintenance.btn');
  maintRow.appendChild(maintBtn);
  maintCard.appendChild(mkSettingsRow({
    label: t('settings.advanced.maintenance.label'),
    description: t('settings.advanced.maintenance.desc'),
    control: maintRow,
  }));
  el.appendChild(maintGroup);

  maintBtn.addEventListener('click', async () => {
    maintBtn.disabled = true;
    maintBtn.textContent = t('settings.advanced.maintenance.running');
    try {
      const res = await api.runMaintenance();
      const freed = (res?.before_bytes ?? 0) - (res?.after_bytes ?? 0);
      const mb = (freed / 1024 / 1024).toFixed(1);
      const msg = freed > 0 ? t('settings.advanced.maintenance.freed', { mb }) : t('settings.advanced.maintenance.no_freed');
      showToast(msg, { type: 'success' });
    } catch (e) {
      showApiError(e);
    } finally {
      maintBtn.disabled = false;
      maintBtn.textContent = t('settings.advanced.maintenance.btn');
    }
  });

  // ── Duplicate rescan ───────────────────────────────────────────────────────

  const dupRescanRow = document.createElement('div');
  dupRescanRow.className = 'flex items-center gap-3 px-4 py-3';
  const dupRescanBtn = document.createElement('button');
  dupRescanBtn.type = 'button';
  dupRescanBtn.className = 'btn-ghost btn-sm';
  dupRescanBtn.textContent = t('settings.advanced.dedup.btn');
  dupRescanRow.appendChild(dupRescanBtn);
  maintCard.appendChild(mkSettingsRow({
    label: t('settings.advanced.dedup.label'),
    description: t('settings.advanced.dedup.desc'),
    control: dupRescanRow,
  }));

  dupRescanBtn.addEventListener('click', async () => {
    dupRescanBtn.disabled = true;
    dupRescanBtn.textContent = t('settings.advanced.dedup.scanning');
    try {
      const res = await api.rescanDuplicates();
      const n = res?.new_pairs ?? 0;
      showToast(n === 0 ? t('settings.advanced.dedup.no_pairs') : t('settings.advanced.dedup.pairs', { count: n, s: n === 1 ? '' : 's' }), { type: 'success' });
    } catch (e) {
      showApiError(e);
    } finally {
      dupRescanBtn.disabled = false;
      dupRescanBtn.textContent = t('settings.advanced.dedup.btn');
    }
  });

  return {
    destroy() {
      render(null, migRoot);
      el.innerHTML = '';
    },
    isDirty() { return JSON.stringify(buildAdvPayload()) !== JSON.stringify(lastSaved); },
  };
}

// ── Path control builder ───────────────────────────────────────────────────────

/**
 * @param {string} field
 * @param {string} initial
 * @returns {{ el: HTMLElement, hidden: HTMLInputElement, setPath: (p: string) => void }}
 */
function makePathControl(field, initial) {
  const container = document.createElement('div');
  container.className = 'flex items-center gap-2';

  const display = document.createElement('span');
  display.className = 'text-sm font-mono text-text truncate max-w-xs';
  display.title = initial;
  display.textContent = initial || t('settings.advanced.path.not_set');

  const hidden = document.createElement('input');
  hidden.type = 'hidden';
  hidden.className = 'js-adv-field';
  hidden.dataset.key = field;
  hidden.value = initial;

  const browseBtn = document.createElement('button');
  browseBtn.type = 'button';
  browseBtn.className = 'btn-ghost btn-sm';
  browseBtn.textContent = t('settings.advanced.path.browse');

  container.appendChild(display);
  container.appendChild(hidden);
  container.appendChild(browseBtn);

  function setPath(path) {
    display.textContent = path;
    display.title = path;
    hidden.value = path;
  }

  return { el: container, hidden, setPath };
}
