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

  // ── Library path control ──────────────────────────────────────────────────
  const { el: libEl, hidden: libHidden, setPath: setLibPath } = makePathControl('library_path', settings?.library_path ?? '');
  advCard.appendChild(mkSettingsRow({
    label: 'Library path',
    description: 'Filesystem path where downloaded chapters and covers are stored.',
    control: libEl,
  }));

  libEl.querySelector('button')?.addEventListener('click', () => {
    openPicker(libHidden.value || '/', (path) => setLibPath(path));
  });

  // ── WASM storage path control ─────────────────────────────────────────────
  const { el: wasmPathEl, hidden: wasmPathHidden, setPath: setWasmPath } = makePathControl('wasm_storage_path', settings?.wasm_storage_path ?? '');
  advCard.appendChild(mkSettingsRow({
    label: 'WASM storage path',
    description: 'Directory where extension .wasm files are loaded from.',
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
  advCard.appendChild(mkSettingsRow({ label: 'Max WASM instances', description: 'Sandbox limit for source extensions.', badge: 'Restart required', control: wasmInput }));

  const coverDimInput = document.createElement('input');
  coverDimInput.type = 'number';
  coverDimInput.id = 'cover-max-dimension';
  coverDimInput.className = 'input w-24 text-sm js-adv-num';
  coverDimInput.dataset.key = 'cover_max_dimension';
  coverDimInput.min = '100';
  coverDimInput.max = '2000';
  coverDimInput.placeholder = '800';
  coverDimInput.value = settings?.cover_max_dimension != null ? String(settings.cover_max_dimension) : '';
  advCard.appendChild(mkSettingsRow({ label: 'Cover max dimension', description: 'Maximum width/height in pixels when downloading covers (default 800). Lower values save disk space.', control: coverDimInput }));

  let httpLogging = settings?.http_request_logging ?? false;
  advCard.appendChild(mkToggleRow({
    label: 'HTTP request logging',
    description: 'Log incoming HTTP requests to the server console.',
    checked: httpLogging,
    onChange: (v) => { httpLogging = v; },
  }));

  let browserDebugLogging = settings?.browser_debug_logging ?? false;
  advCard.appendChild(mkToggleRow({
    label: 'Browser token capture logging',
    description: 'Log all URLs seen by the headless browser when capturing auth tokens. Useful for debugging sources that time out during page loading.',
    checked: browserDebugLogging,
    onChange: (v) => { browserDebugLogging = v; },
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
  saveRow.innerHTML = `<button type="button" class="btn-primary btn-sm js-adv-save">Save</button>`;
  advCard.appendChild(saveRow);
  el.appendChild(advGroup);

  // ── Credential Encryption ──────────────────────────────────────────────────
  const encGroup = mkSettingsGroup('Credential Encryption');
  const encCard  = mkSettingsGroupCard(encGroup);

  const encStatusEl = document.createElement('p');
  encStatusEl.className = 'text-sm text-text-muted px-1';
  encStatusEl.textContent = 'Loading…';

  const encMigrateBtn = document.createElement('button');
  encMigrateBtn.type = 'button';
  encMigrateBtn.className = 'btn-primary btn-sm hidden';
  encMigrateBtn.textContent = 'Encrypt stored credentials';

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
    label: 'Encryption at rest',
    description: 'Encrypts SMTP password and OAuth tokens using ChaCha20-Poly1305 (KANI_SECRET_KEY).',
    tooltip: 'Set KANI_SECRET_KEY to a random 32-byte hex string in your environment and restart Kani to enable encryption at rest.',
    control: encCtrl,
  }));
  el.appendChild(encGroup);

  async function refreshEncStatus() {
    try {
      const s = await api.getCredentialEncryptionStatus();
      if (!s.encryption_enabled) {
        encStatusEl.textContent = 'Encryption is disabled. Set KANI_SECRET_KEY in your environment and restart to enable.';
        encMigrateBtn.classList.add('hidden');
      } else if (s.plaintext_count > 0) {
        encStatusEl.textContent = `Encryption is enabled. ${s.plaintext_count} credential${s.plaintext_count === 1 ? '' : 's'} stored in plaintext.`;
        encMigrateBtn.classList.remove('hidden');
      } else {
        encStatusEl.textContent = 'All credentials are encrypted.';
        encMigrateBtn.classList.add('hidden');
      }
    } catch {
      encStatusEl.textContent = 'Unable to load encryption status.';
    }
  }

  refreshEncStatus();

  encMigrateBtn.addEventListener('click', async () => {
    encMigrateBtn.disabled = true;
    encMigrateBtn.textContent = 'Encrypting…';
    encResultEl.className = 'text-xs hidden';
    try {
      await api.migrateCredentialsToEncrypted();
      encResultEl.textContent = 'Done.';
      encResultEl.className = 'text-xs text-success';
      await refreshEncStatus();
    } catch (/** @type {any} */ e) {
      encResultEl.textContent = e?.message ?? 'Failed';
      encResultEl.className = 'text-xs text-error';
    } finally {
      encMigrateBtn.disabled = false;
      encMigrateBtn.textContent = 'Encrypt stored credentials';
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
      showToast('Saved.', { type: 'success' });
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

  const maintGroup = mkSettingsGroup('Maintenance');
  const maintCard  = mkSettingsGroupCard(maintGroup);

  const maintRow = document.createElement('div');
  maintRow.className = 'flex items-center gap-3 px-4 py-3';
  const maintBtn = document.createElement('button');
  maintBtn.type = 'button';
  maintBtn.className = 'btn-ghost btn-sm';
  maintBtn.textContent = 'Run maintenance';
  maintRow.appendChild(maintBtn);
  maintCard.appendChild(mkSettingsRow({
    label: 'Database maintenance',
    description: 'Run WAL checkpoint and VACUUM to compact the database and reclaim disk space.',
    control: maintRow,
  }));
  el.appendChild(maintGroup);

  maintBtn.addEventListener('click', async () => {
    maintBtn.disabled = true;
    maintBtn.textContent = 'Running…';
    try {
      const res = await api.runMaintenance();
      const freed = (res?.before_bytes ?? 0) - (res?.after_bytes ?? 0);
      const mb = (freed / 1024 / 1024).toFixed(1);
      const msg = freed > 0 ? `Freed ${mb} MB` : 'No space freed';
      showToast(msg, { type: 'success' });
    } catch (e) {
      showApiError(e);
    } finally {
      maintBtn.disabled = false;
      maintBtn.textContent = 'Run maintenance';
    }
  });

  // ── Duplicate rescan ───────────────────────────────────────────────────────

  const dupRescanRow = document.createElement('div');
  dupRescanRow.className = 'flex items-center gap-3 px-4 py-3';
  const dupRescanBtn = document.createElement('button');
  dupRescanBtn.type = 'button';
  dupRescanBtn.className = 'btn-ghost btn-sm';
  dupRescanBtn.textContent = 'Rescan for duplicates';
  dupRescanRow.appendChild(dupRescanBtn);
  maintCard.appendChild(mkSettingsRow({
    label: 'Duplicate detection',
    description: 'Re-run the full-library duplicate scan. New pairs are saved to the Duplicates tab. Dismissed pairs are not re-flagged. May be slow for large libraries.',
    control: dupRescanRow,
  }));

  dupRescanBtn.addEventListener('click', async () => {
    dupRescanBtn.disabled = true;
    dupRescanBtn.textContent = 'Scanning…';
    try {
      const res = await api.rescanDuplicates();
      const n = res?.new_pairs ?? 0;
      showToast(n === 0 ? 'No new duplicates found.' : `Found ${n} new duplicate pair${n === 1 ? '' : 's'}.`, { type: 'success' });
    } catch (e) {
      showApiError(e);
    } finally {
      dupRescanBtn.disabled = false;
      dupRescanBtn.textContent = 'Rescan for duplicates';
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
  display.textContent = initial || '(not set)';

  const hidden = document.createElement('input');
  hidden.type = 'hidden';
  hidden.className = 'js-adv-field';
  hidden.dataset.key = field;
  hidden.value = initial;

  const browseBtn = document.createElement('button');
  browseBtn.type = 'button';
  browseBtn.className = 'btn-ghost btn-sm';
  browseBtn.textContent = 'Browse…';

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
