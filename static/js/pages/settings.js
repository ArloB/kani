// @ts-check
// Settings page — scan, download, advanced, categories, account.

import * as api from '../api.js';
import { hasPermission } from '../state.js';
import { navigate } from '../router.js';
import { getLocal, setLocal, escapeHtml } from '../utils.js';
import { showToast } from '../components/toast.js';
import { iconLock, iconWarning, iconArrowUp, iconArrowDown, iconPencil, iconX } from '../icons.js';

// ── Module state ──────────────────────────────────────────────────────────────

/** @type {(() => void)[]} */ let _panelDestroys = [];
/** @type {string | null} */ let _activeSection = null;

// ── Init ──────────────────────────────────────────────────────────────────────

/** @param {HTMLElement} container */
export async function init(container) {
  document.title = 'Settings - Kani';
  _panelDestroys = [];
  _activeSection = null;

  if (!hasPermission('settings:view')) {
    container.innerHTML = '';
    container.appendChild(_createAccessDenied());
    return;
  }

  // Fetch everything in parallel
  const [settings, categories, bootData] = await Promise.allSettled([
    api.getSettings(),
    api.getCategories(),
    api.getBootId(),
  ]).then(r => r.map(s => s.status === 'fulfilled' ? s.value : null));

  const bootId = bootData?.boot_id ?? bootData ?? '';
  const catList = Array.isArray(categories) ? categories : [];

  // ── Section definitions (ordered) ──
  const allSections = [
    {
      id: 'general',
      label: 'General',
      description: 'Display preferences, reading behaviour, and notifications.',
      perm: /** @type {string|null} */ (null),
      render: (el, banner) => _renderDisplaySection(el),
    },
    {
      id: 'library',
      label: 'Library',
      description: 'Manage categories to organise your manga collection.',
      perm: 'library:manage',
      render: (el, banner) => _renderCategoriesSection(el, catList),
    },
    {
      id: 'downloads',
      label: 'Downloads',
      description: 'Control download concurrency, queue size, and reading-ahead behaviour.',
      perm: 'settings:edit_download',
      render: (el, banner) => _renderDownloadSection(el, settings),
    },
    {
      id: 'scan',
      label: 'Scan',
      description: 'Configure automatic scanning for new chapters.',
      perm: 'settings:edit_scan',
      render: (el, banner) => _renderScanSection(el, settings, bootId, banner),
    },
    {
      id: 'trackers',
      label: 'Trackers',
      description: 'Link external tracking services like AniList and MyAnimeList.',
      perm: /** @type {string|null} */ (null),
      render: (el, banner) => _renderTrackersSection(el, settings),
    },
    {
      id: 'advanced',
      label: 'Advanced',
      description: 'FlareSolverr, library path, and other low-level options. Requires restart.',
      perm: 'settings:edit_advanced',
      render: (el, banner) => _renderAdvancedSection(el, settings, bootId, banner),
    },
    {
      id: 'account',
      label: 'My Account',
      description: 'Change your password and manage active sessions.',
      perm: /** @type {string|null} */ (null),
      render: (el, banner) => _renderAccountSection(el),
    },
    {
      id: 'server',
      label: 'Server',
      description: 'Stop or restart the server process.',
      perm: 'server:manage',
      render: (el, banner) => _renderServerSection(el),
    },
  ];

  const sections = allSections.filter(s => !s.perm || hasPermission(s.perm));

  container.innerHTML = `
    <div class="flex min-h-full">

      <!-- Sidebar (lg+) -->
      <aside
        class="hidden lg:flex flex-col w-52 shrink-0 border-r border-border bg-surface sticky top-14 overflow-y-auto"
        style="height: calc(100vh - 3.5rem)"
        aria-label="Settings categories"
      >
        <div class="p-3 flex flex-col gap-0.5">
          <p class="px-3 py-1 text-xs font-semibold uppercase tracking-wider text-text-muted mb-1">Settings</p>
          ${sections.map(s => `
            <button
              type="button"
              data-section="${s.id}"
              class="js-nav-btn w-full text-left px-3 py-2 text-sm rounded-lg transition-colors hover:bg-surface-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent text-text-muted"
            >${s.label}</button>
          `).join('')}
        </div>
      </aside>

      <!-- Main panel -->
      <div class="flex-1 min-w-0 flex flex-col">

        <!-- Restart banner -->
        <div class="js-restart-banner hidden mx-4 mt-4 md:mx-6 items-center gap-3 px-4 py-3 rounded-xl bg-warn/10 border border-warn/30 text-sm text-warn" role="alert"></div>

        <!-- Mobile: category list -->
        <div class="js-mobile-list lg:hidden flex flex-col gap-0 px-0 py-2">
          <h1 class="text-lg font-semibold text-text px-4 py-3">Settings</h1>
          <div class="flex flex-col divide-y divide-border-subtle border-t border-border-subtle">
            ${sections.map(s => `
              <button
                type="button"
                data-section="${s.id}"
                class="js-mobile-nav-btn w-full text-left px-4 py-3.5 text-sm text-text hover:bg-surface-2 transition-colors flex items-center justify-between"
              >
                <span>${s.label}</span>
                <span class="text-text-muted text-xs">›</span>
              </button>
            `).join('')}
          </div>
        </div>

        <!-- Mobile: back button (shown when in a section) -->
        <button
          type="button"
          class="js-mobile-back lg:hidden hidden items-center gap-2 px-4 py-3 text-sm text-accent hover:text-accent/80 transition-colors"
        >
          <span aria-hidden="true">‹</span> Back
        </button>

        <!-- Content area -->
        <div class="js-content max-w-[860px] w-full px-4 md:px-8 py-4 md:py-6 flex flex-col gap-6">
          <!-- Filled by _showSection() -->
        </div>

      </div>
    </div>
  `;

  const restartBanner  = /** @type {HTMLElement} */ (container.querySelector('.js-restart-banner'));
  const contentEl      = /** @type {HTMLElement} */ (container.querySelector('.js-content'));
  const mobileListEl   = /** @type {HTMLElement} */ (container.querySelector('.js-mobile-list'));
  const mobileBackBtn  = /** @type {HTMLButtonElement} */ (container.querySelector('.js-mobile-back'));

  _checkRestartBanner(restartBanner, bootId);

  /** @param {string} sectionId */
  function _showSection(sectionId) {
    _activeSection = sectionId;
    const section = sections.find(s => s.id === sectionId);
    if (!section) return;

    // Update sidebar active state (desktop)
    container.querySelectorAll('.js-nav-btn').forEach(btn => {
      const active = /** @type {HTMLElement} */(btn).dataset.section === sectionId;
      btn.classList.toggle('bg-surface-2', active);
      btn.classList.toggle('text-text', active);
      btn.classList.toggle('font-medium', active);
      btn.classList.toggle('text-text-muted', !active);
    });

    // Mobile: hide list, show back button
    mobileListEl.classList.add('hidden');
    mobileBackBtn.classList.remove('hidden');
    mobileBackBtn.classList.add('flex');

    // Render section content
    for (const d of _panelDestroys) d();
    _panelDestroys = [];
    contentEl.innerHTML = '';
    const headerEl = document.createElement('div');
    headerEl.className = 'flex flex-col gap-1 pb-2 border-b border-border-subtle';
    headerEl.innerHTML = `
      <h2 class="text-xl font-semibold text-text">${section.label}</h2>
      ${section.description ? `<p class="text-sm text-text-muted">${escapeHtml(section.description)}</p>` : ''}
    `;
    contentEl.appendChild(headerEl);
    const bodyEl = document.createElement('div');
    bodyEl.className = 'flex flex-col gap-5';
    contentEl.appendChild(bodyEl);
    section.render(bodyEl, restartBanner);
  }

  function _showMobileList() {
    _activeSection = null;
    mobileListEl.classList.remove('hidden');
    mobileBackBtn.classList.add('hidden');
    mobileBackBtn.classList.remove('flex');
    for (const d of _panelDestroys) d();
    _panelDestroys = [];
    contentEl.innerHTML = '';
    // Clear desktop active state
    container.querySelectorAll('.js-nav-btn').forEach(btn => {
      btn.classList.remove('bg-surface-2', 'text-text', 'font-medium');
      btn.classList.add('text-text-muted');
    });
  }

  // Desktop nav buttons
  container.querySelectorAll('.js-nav-btn').forEach(btn => {
    btn.addEventListener('click', () => {
      _showSection(/** @type {HTMLElement} */(btn).dataset.section ?? '');
    });
  });

  // Mobile nav buttons
  container.querySelectorAll('.js-mobile-nav-btn').forEach(btn => {
    btn.addEventListener('click', () => {
      _showSection(/** @type {HTMLElement} */(btn).dataset.section ?? '');
    });
  });

  // Mobile back button
  mobileBackBtn.addEventListener('click', _showMobileList);

  // On desktop (lg+) auto-select the first section; on mobile show the list
  if (sections.length > 0 && window.innerWidth >= 1024) {
    _showSection(sections[0].id);
  }
}

// ── Settings layout helpers ────────────────────────────────────────────────────

/**
 * Creates a settings card group with an optional group label.
 * @param {string} [groupLabel]
 * @returns {HTMLElement}
 */
function _mkSettingsGroup(groupLabel) {
  const wrap = document.createElement('div');
  wrap.className = 'flex flex-col gap-1.5';
  if (groupLabel) {
    const lbl = document.createElement('p');
    lbl.className = 'text-xs font-semibold uppercase tracking-wide text-text-muted px-1';
    lbl.textContent = groupLabel;
    wrap.appendChild(lbl);
  }
  const card = document.createElement('div');
  card.className = 'bg-surface-2 rounded-xl divide-y divide-border-subtle overflow-hidden';
  wrap.appendChild(card);
  return wrap;
}

/** Returns the inner card element from a group created with `_mkSettingsGroup`. */
function _mkSettingsGroupCard(groupEl) {
  return /** @type {HTMLElement} */ (groupEl.lastElementChild);
}

/**
 * Creates a settings row: label (+ optional description) on the left, control on the right.
 * @param {{ label: string, description?: string, badge?: string, control: HTMLElement }} opts
 * @returns {HTMLElement}
 */
function _mkSettingsRow({ label, description, badge, control }) {
  const row = document.createElement('div');
  row.className = 'flex items-center justify-between gap-4 px-4 py-3.5';
  const left = document.createElement('div');
  left.className = 'flex flex-col gap-0.5 min-w-0';
  const labelEl = document.createElement('div');
  labelEl.className = 'flex items-center gap-2';
  const labelText = document.createElement('span');
  labelText.className = 'text-sm font-medium text-text';
  labelText.textContent = label;
  labelEl.appendChild(labelText);
  if (badge) {
    const badgeEl = document.createElement('span');
    badgeEl.className = 'text-xs px-1.5 py-0.5 rounded bg-warn/20 text-warn font-medium';
    badgeEl.textContent = badge;
    labelEl.appendChild(badgeEl);
  }
  left.appendChild(labelEl);
  if (description) {
    const desc = document.createElement('span');
    desc.className = 'text-xs text-text-muted';
    desc.textContent = description;
    left.appendChild(desc);
  }
  row.appendChild(left);
  control.classList.add('shrink-0');
  row.appendChild(control);
  return row;
}

/**
 * Creates a toggle row.
 * @param {{ label: string, description?: string, checked: boolean, onChange: (v: boolean) => void }} opts
 * @returns {HTMLElement}
 */
function _mkToggleRow({ label, description, checked, onChange }) {
  const toggleLabel = document.createElement('label');
  toggleLabel.className = 'kani-toggle';
  const input = document.createElement('input');
  input.type = 'checkbox';
  input.className = 'kani-toggle__input';
  input.checked = checked;
  input.addEventListener('change', () => onChange(input.checked));
  const track = document.createElement('span');
  track.className = 'kani-toggle__track';
  toggleLabel.appendChild(input);
  toggleLabel.appendChild(track);
  return _mkSettingsRow({ label, description, control: toggleLabel });
}

/**
 * Creates a number input row.
 * @param {{ label: string, description?: string, badge?: string, id: string, value: any, min?: number, max?: number, onChange: (v: number) => void }} opts
 * @returns {HTMLElement}
 */
function _mkNumberRow({ label, description, badge, id, value, min, max, onChange }) {
  const input = document.createElement('input');
  input.type = 'number';
  input.id = id;
  input.className = 'input w-24 text-sm';
  if (value != null) input.value = String(value);
  if (min != null) input.min = String(min);
  if (max != null) input.max = String(max);
  input.addEventListener('change', () => onChange(Number(input.value)));
  return _mkSettingsRow({ label, description, badge, control: input });
}

// ── Display preferences ────────────────────────────────────────────────────────

/** @param {HTMLElement} container */
function _renderDisplaySection(container) {
  const paginationPrefs = [
    { label: 'Chapter list',  key: 'kani_chapter_pagination',  desc: 'How chapters are loaded in the chapter list.' },
    { label: 'Library',       key: 'kani_library_pagination',  desc: 'How manga are loaded in the library grid.' },
    { label: 'Source browse', key: 'kani_source_pagination',   desc: 'How manga are loaded when browsing a source.' },
  ];

  function _render() {
    container.innerHTML = '';

    // Pagination group
    const paginGroup = _mkSettingsGroup('Pagination');
    const paginCard  = _mkSettingsGroupCard(paginGroup);
    for (const { label, key, desc } of paginationPrefs) {
      const current = getLocal(key) || 'paginated';
      const chips = document.createElement('div');
      chips.className = 'flex gap-2 shrink-0';
      for (const [val, chipLabel] of [['paginated', 'Paginated'], ['infinite', 'Infinite scroll']]) {
        const btn = document.createElement('button');
        btn.type = 'button';
        btn.className = current === val ? 'chip chip-active' : 'chip';
        btn.textContent = chipLabel;
        btn.setAttribute('aria-pressed', String(current === val));
        btn.addEventListener('click', () => { setLocal(key, val); _render(); });
        chips.appendChild(btn);
      }
      paginCard.appendChild(_mkSettingsRow({ label, description: desc, control: chips }));
    }
    container.appendChild(paginGroup);

    // Reading group
    const readGroup = _mkSettingsGroup('Reading');
    const readCard  = _mkSettingsGroupCard(readGroup);
    readCard.appendChild(_mkToggleRow({
      label: 'Sync read status across scanlators',
      description: 'Marking a chapter read also marks all other versions of it as read.',
      checked: getLocal('kani_coalesce_read') === 'true',
      onChange: v => setLocal('kani_coalesce_read', v ? 'true' : 'false'),
    }));
    readCard.appendChild(_mkToggleRow({
      label: 'Warn before opening external links',
      description: 'Show a confirmation dialog when clicking links in manga descriptions.',
      checked: getLocal('kani_skip_external_warning') !== 'true',
      onChange: v => setLocal('kani_skip_external_warning', v ? 'false' : 'true'),
    }));
    container.appendChild(readGroup);

    // Notifications group
    const notifGroup = _mkSettingsGroup('Notifications');
    const notifCard  = _mkSettingsGroupCard(notifGroup);
    notifCard.appendChild(_mkToggleRow({
      label: 'Show new chapter notifications',
      description: 'Show a notification badge when new chapters are found during a scan.',
      checked: getLocal('kani_disable_notifications') !== 'true',
      onChange: v => setLocal('kani_disable_notifications', v ? 'false' : 'true'),
    }));
    container.appendChild(notifGroup);

    const note = document.createElement('p');
    note.className = 'text-xs text-text-muted';
    note.textContent = 'These preferences are saved to this device only.';
    container.appendChild(note);
  }

  _render();
}

// ── Access denied ──────────────────────────────────────────────────────────────

function _createAccessDenied() {
  const el = document.createElement('div');
  el.className = 'flex flex-col items-center justify-center gap-3 py-20 text-text-muted';
  el.innerHTML = `
    <span class="[&_svg]:w-8 [&_svg]:h-8 opacity-40" aria-hidden="true">${iconLock}</span>
    <p class="text-base font-medium text-text">Access denied</p>
    <p class="text-sm">You do not have permission to view settings.</p>
  `;
  return el;
}

// ── Restart banner ─────────────────────────────────────────────────────────────

/**
 * @param {HTMLElement} banner
 * @param {string} currentBootId
 */
function _checkRestartBanner(banner, currentBootId) {
  const needed = getLocal('kani_restart_needed');
  if (!needed) return;

  const storedBootId = getLocal('kani_restart_boot_id');
  if (storedBootId && storedBootId !== currentBootId) {
    localStorage.removeItem('kani_restart_needed');
    localStorage.removeItem('kani_restart_boot_id');
    localStorage.removeItem('kani_pending_fields');
    return;
  }

  const fields = getLocal('kani_pending_fields');
  banner.innerHTML = `
    <span aria-hidden="true" class="shrink-0 [&_svg]:w-4 [&_svg]:h-4">${iconWarning}</span>
    <span class="flex-1">
      A server restart is required for changes to take effect.
      ${fields ? `Pending: <strong>${escapeHtml(fields)}</strong>.` : ''}
    </span>
    <button type="button" class="btn-ghost btn-sm shrink-0 js-banner-dismiss">Dismiss</button>
  `;
  banner.classList.remove('hidden');
  banner.classList.add('flex');

  banner.querySelector('.js-banner-dismiss')?.addEventListener('click', () => {
    localStorage.removeItem('kani_restart_needed');
    localStorage.removeItem('kani_restart_boot_id');
    localStorage.removeItem('kani_pending_fields');
    banner.classList.add('hidden');
    banner.classList.remove('flex');
  });
}

/**
 * @param {string} fieldName
 * @param {string} bootId
 */
function _markRestartNeeded(fieldName, bootId) {
  setLocal('kani_restart_needed', '1');
  setLocal('kani_restart_boot_id', bootId);
  const existing = getLocal('kani_pending_fields');
  const fields = existing ? existing.split(',').map(s => s.trim()) : [];
  if (!fields.includes(fieldName)) {
    fields.push(fieldName);
    setLocal('kani_pending_fields', fields.join(', '));
  }
}

// ── Scan settings section ──────────────────────────────────────────────────────

/**
 * @param {HTMLElement} el
 * @param {any} settings
 * @param {string} bootId
 * @param {HTMLElement} restartBanner
 */
function _renderScanSection(el, settings, bootId, restartBanner) {
  let autoScan = !!settings?.auto_scan;
  let interval = settings?.scan_interval_minutes ?? 60;

  const scanGroup = _mkSettingsGroup('Automatic scanning');
  const scanCard  = _mkSettingsGroupCard(scanGroup);

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
  scanCard.appendChild(_mkSettingsRow({ label: 'Auto scan', description: 'Automatically scan for new chapters on an interval.', control: autoToggleLabel }));

  const intervalInput = document.createElement('input');
  intervalInput.type = 'number';
  intervalInput.id = 'scan-interval';
  intervalInput.className = 'input w-24 text-sm';
  intervalInput.min = '1';
  intervalInput.value = String(interval);
  const intervalRow = _mkSettingsRow({ label: 'Interval (minutes)', description: 'How often to scan for new chapters.', control: intervalInput });
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
      _showResult(resultEl, true, 'Saved.');
    } catch (e) {
      _showResult(resultEl, false, e?.message ?? 'Failed to save.');
    } finally {
      saveBtn.disabled = false;
    }
  });
}

// ── Download settings section ──────────────────────────────────────────────────

/**
 * @param {HTMLElement} el
 * @param {any} settings
 */
function _renderDownloadSection(el, settings) {
  const fields = [
    { key: 'concurrent_page_downloads',   label: 'Concurrent page downloads',   desc: 'Number of pages downloaded in parallel per chapter.',   min: 1 },
    { key: 'concurrent_manga_downloads',  label: 'Concurrent manga downloads',  desc: 'Number of chapters downloaded simultaneously.',          min: 1 },
    { key: 'chapter_queue_size',          label: 'Chapter queue size',          desc: 'Maximum chapters waiting in the download queue.',        min: 1 },
    { key: 'max_retries',                 label: 'Max retries',                 desc: 'How many times to retry a failed page download.',        min: 0 },
    { key: 'initial_retry_delay_ms',      label: 'Initial retry delay (ms)',    desc: 'Starting delay before the first retry.',                 min: 0 },
  ];

  const serverGroup = _mkSettingsGroup('Server download settings');
  const serverCard  = _mkSettingsGroupCard(serverGroup);

  for (const f of fields) {
    const input = document.createElement('input');
    input.type = 'number';
    input.id = f.key;
    input.className = 'input w-24 text-sm js-dl-field';
    input.dataset.key = f.key;
    input.min = String(f.min);
    input.value = String(settings?.[f.key] ?? '');
    serverCard.appendChild(_mkSettingsRow({ label: f.label, description: f.desc, control: input }));
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
      _showResult(resultEl, true, 'Saved.');
    } catch (e) {
      _showResult(resultEl, false, e?.message ?? 'Failed to save.');
    } finally {
      saveBtn.disabled = false;
    }
  });

  // Download ahead — client-side setting stored in localStorage
  const aheadGroup = _mkSettingsGroup('Download ahead');
  const aheadCard  = _mkSettingsGroupCard(aheadGroup);

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
  aheadCard.appendChild(_mkSettingsRow({
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
  const aheadCountRow = _mkSettingsRow({
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
}

// ── Advanced settings section ──────────────────────────────────────────────────

/**
 * @param {HTMLElement} el
 * @param {any} settings
 * @param {string} bootId
 * @param {HTMLElement} restartBanner
 */
function _renderAdvancedSection(el, settings, bootId, restartBanner) {
  const advGroup = _mkSettingsGroup('Server');
  const advCard  = _mkSettingsGroupCard(advGroup);

  const flareInput = document.createElement('input');
  flareInput.type = 'url';
  flareInput.id = 'flaresolverr-url';
  flareInput.className = 'input w-56 text-sm js-adv-field';
  flareInput.dataset.key = 'flaresolverr_url';
  flareInput.placeholder = 'http://localhost:8191';
  flareInput.value = settings?.flaresolverr_url ?? '';
  advCard.appendChild(_mkSettingsRow({ label: 'FlareSolverr URL', description: 'Optional. Used by sources that require Cloudflare bypass.', control: flareInput }));

  const libPathInput = document.createElement('input');
  libPathInput.type = 'text';
  libPathInput.id = 'library-path';
  libPathInput.className = 'input w-56 text-sm js-adv-field';
  libPathInput.dataset.key = 'library_path';
  libPathInput.placeholder = '/data/library';
  libPathInput.value = settings?.library_path ?? '';
  advCard.appendChild(_mkSettingsRow({ label: 'Library path', description: 'Filesystem path where downloaded chapters are stored.', badge: 'Restart required', control: libPathInput }));

  const wasmInput = document.createElement('input');
  wasmInput.type = 'number';
  wasmInput.id = 'max-wasm-instances';
  wasmInput.className = 'input w-24 text-sm js-adv-num';
  wasmInput.dataset.key = 'max_wasm_instances';
  wasmInput.min = '1';
  wasmInput.value = String(settings?.max_wasm_instances ?? '');
  advCard.appendChild(_mkSettingsRow({ label: 'Max WASM instances', description: 'Sandbox limit for source extensions.', badge: 'Restart required', control: wasmInput }));

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

    try {
      await api.updateSettings({ Advanced: payload });
      _showResult(resultEl, true, 'Saved. Some changes require a server restart.');
      if (payload.library_path) _markRestartNeeded('library_path', bootId);
      if (payload.max_wasm_instances) _markRestartNeeded('max_wasm_instances', bootId);
      const needed = getLocal('kani_restart_needed');
      if (needed && restartBanner) {
        _checkRestartBanner(restartBanner, bootId);
      }
    } catch (e) {
      _showResult(resultEl, false, e?.message ?? 'Failed to save.');
    } finally {
      saveBtn.disabled = false;
    }
  });
}

// ── Trackers section ───────────────────────────────────────────────────────────

/**
 * @param {HTMLElement} el
 * @param {any} settings
 */
function _renderTrackersSection(el, settings) {
  const isAdmin = hasPermission('settings:edit_advanced');

  async function _render() {
    el.innerHTML = '<p class="text-sm text-text-muted p-1">Loading…</p>';

    let trackers = [];
    try {
      trackers = await api.getTrackers();
    } catch {
      el.innerHTML = '<p class="text-sm text-danger p-1">Failed to load trackers.</p>';
      return;
    }

    el.innerHTML = '';

    // ── Default tracking toggle ──
    const defaultGroup = _mkSettingsGroup('Behaviour');
    const defaultCard  = _mkSettingsGroupCard(defaultGroup);
    const trackingEnabled = settings?.default_tracking_enabled ?? true;
    defaultCard.appendChild(_mkToggleRow({
      label: 'Enable tracking by default',
      description: 'New manga added to the library will have sync enabled.',
      checked: trackingEnabled,
      onChange: async (checked) => {
        try {
          await api.updateSettings({ Tracking: { default_tracking_enabled: checked } });
          settings.default_tracking_enabled = checked;
        } catch (e) {
          showToast(e?.message ?? 'Failed to save.', { type: 'error' });
        }
      },
    }));
    el.appendChild(defaultGroup);

    // ── Per-tracker rows ──
    for (const tracker of trackers) {
      const trackerGroup = _mkSettingsGroup(tracker.name);
      const trackerCard  = _mkSettingsGroupCard(trackerGroup);

      // Status / link row
      if (tracker.configured) {
        const linkBtn = document.createElement('button');
        linkBtn.type = 'button';
        linkBtn.className = tracker.linked ? 'btn-danger btn-sm' : 'btn-primary btn-sm';
        linkBtn.textContent = tracker.linked ? 'Unlink' : 'Link Account';
        trackerCard.appendChild(_mkSettingsRow({
          label: tracker.linked ? 'Account linked' : 'Not linked',
          description: tracker.linked ? `Your ${tracker.name} account is connected.` : `Connect your ${tracker.name} account to sync progress.`,
          control: linkBtn,
        }));

        linkBtn.addEventListener('click', async () => {
          if (tracker.linked) {
            if (!confirm(`Unlink your ${tracker.name} account?`)) return;
            linkBtn.disabled = true;
            try {
              await api.unlinkTracker(tracker.id);
              tracker.linked = false;
              await _render();
            } catch (e) {
              showToast(e?.message ?? 'Failed to unlink.', { type: 'error' });
              linkBtn.disabled = false;
            }
          } else {
            _openTrackerPopup(tracker.id, tracker.name, () => {
              tracker.linked = true;
              _render();
            });
          }
        });
      } else {
        const notConfiguredEl = document.createElement('span');
        notConfiguredEl.className = 'text-xs text-text-muted';
        notConfiguredEl.textContent = isAdmin ? 'Not configured' : 'Not configured';
        trackerCard.appendChild(_mkSettingsRow({
          label: 'Not configured',
          description: isAdmin ? 'Add credentials below to enable this tracker.' : 'Contact your server admin to configure this tracker.',
          control: notConfiguredEl,
        }));
      }

      // Admin credentials sub-section
      if (isAdmin) {
        let config = null;
        try {
          config = await api.getTrackerConfig(tracker.id);
        } catch { /* tracker may not have config yet */ }

        const isAniList = tracker.name === 'AniList';
        const isMAL = tracker.name === 'MyAnimeList';

        const setupGroup = _mkSettingsGroup('Setup');
        const setupCard  = _mkSettingsGroupCard(setupGroup);

        const instructions = isAniList ? `
          <p class="text-xs text-text-muted leading-relaxed mb-2">
            Register a free OAuth application at <strong>anilist.co → Settings → Developer → Create New Client</strong>.
            Set the redirect URL to <code class="font-mono bg-surface-alt px-1 rounded">${location.origin}/rest/trackers/${tracker.id}/callback</code>.
          </p>
        ` : isMAL ? `
          <p class="text-xs text-text-muted leading-relaxed mb-2">
            Register a free API client at <strong>myanimelist.net → Account Settings → API → Create ID</strong>.
            Set App Type to <strong>web</strong> and redirect URL to <code class="font-mono bg-surface-alt px-1 rounded">${location.origin}/rest/trackers/${tracker.id}/callback</code>.
          </p>
        ` : '';

        setupCard.innerHTML = `
          <div class="px-4 py-4 flex flex-col gap-3">
            ${instructions}
            <div class="flex flex-col gap-1">
              <label class="text-xs font-medium text-text" for="tracker-${tracker.id}-client-id">Client ID</label>
              <input type="text" id="tracker-${tracker.id}-client-id" class="input text-sm js-client-id font-mono"
                value="${escapeHtml(config?.client_id ?? '')}" placeholder="Paste your client ID here"
                autocomplete="off" spellcheck="false">
            </div>
            ${isAniList ? `
            <div class="flex flex-col gap-1">
              <label class="text-xs font-medium text-text" for="tracker-${tracker.id}-secret">Client Secret</label>
              <input type="password" id="tracker-${tracker.id}-secret" class="input text-sm js-client-secret font-mono"
                placeholder="${config?.secret_configured ? 'Already set — leave blank to keep current value' : 'Paste your client secret here'}"
                autocomplete="off">
              <p class="text-xs text-text-muted">Stored on the server only, never exposed to users.</p>
            </div>` : ''}
            <div class="flex items-center gap-2 flex-wrap">
              <button type="button" class="btn-primary btn-sm js-config-save">Save credentials</button>
              ${config?.client_id ? `<button type="button" class="btn-danger btn-sm js-config-delete">Remove credentials</button>` : ''}
              <span class="js-config-result text-xs hidden"></span>
            </div>
          </div>
        `;

        const clientIdEl = /** @type {HTMLInputElement} */ (setupCard.querySelector('.js-client-id'));
        const secretEl = /** @type {HTMLInputElement|null} */ (setupCard.querySelector('.js-client-secret'));
        const saveBtn = /** @type {HTMLButtonElement} */ (setupCard.querySelector('.js-config-save'));
        const deleteBtn = /** @type {HTMLButtonElement|null} */ (setupCard.querySelector('.js-config-delete'));
        const resultEl = /** @type {HTMLElement} */ (setupCard.querySelector('.js-config-result'));

        saveBtn.addEventListener('click', async () => {
          const clientId = clientIdEl.value.trim();
          if (!clientId) { _showResult(resultEl, false, 'Client ID is required.'); return; }
          saveBtn.disabled = true;
          try {
            const body = { client_id: clientId };
            if (secretEl?.value) body.client_secret = secretEl.value;
            await api.setTrackerConfig(tracker.id, body);
            _showResult(resultEl, true, 'Saved.');
            await _render();
          } catch (e) {
            _showResult(resultEl, false, e?.message ?? 'Failed to save.');
          } finally {
            saveBtn.disabled = false;
          }
        });

        deleteBtn?.addEventListener('click', async () => {
          if (!confirm(`Remove all ${tracker.name} credentials? This will unlink all users.`)) return;
          deleteBtn.disabled = true;
          try {
            await api.deleteTrackerConfig(tracker.id);
            _render();
          } catch (e) {
            showToast(e?.message ?? 'Failed to remove.', { type: 'error' });
            deleteBtn.disabled = false;
          }
        });

        trackerGroup.appendChild(setupGroup);
      }

      el.appendChild(trackerGroup);
    }
  }

  _render();
}

/**
 * Open the OAuth popup, notify parent on success via postMessage.
 * @param {number} trackerId
 * @param {string} trackerName
 * @param {() => void} onLinked
 */
function _openTrackerPopup(trackerId, trackerName, onLinked) {
  const redirectUri = `${location.origin}/rest/trackers/${trackerId}/callback`;

  api.getTrackerAuthUrl(trackerId, redirectUri).then(({ url }) => {
    const popup = window.open(url, `link_${trackerName}`, 'popup,width=640,height=720');
    if (!popup) {
      showToast('Popup was blocked. Please allow popups for this site.', { type: 'error' });
      return;
    }

    /** @param {MessageEvent} e */
    function onMessage(e) {
      if (e.origin !== location.origin) return;
      if (e.data?.type === 'tracker_linked') {
        window.removeEventListener('message', onMessage);
        clearInterval(closedTimer);
        popup.close();
        onLinked();
      }
    }

    window.addEventListener('message', onMessage);

    // Detect popup closed without completing auth
    const closedTimer = setInterval(() => {
      if (popup.closed) {
        clearInterval(closedTimer);
        window.removeEventListener('message', onMessage);
      }
    }, 500);
  }).catch(e => {
    showToast(e?.message ?? `Failed to get ${trackerName} auth URL.`, { type: 'error' });
  });
}

// ── Categories section ─────────────────────────────────────────────────────────

/** @param {HTMLElement} el @param {any[]} initialCategories */
function _renderCategoriesSection(el, initialCategories) {
  let cats = [...initialCategories];

  function _render() {
    el.innerHTML = '';

    // Categories list group
    const catGroup = _mkSettingsGroup('Categories');
    const catCard  = _mkSettingsGroupCard(catGroup);
    catCard.classList.add('js-cat-list');
    el.appendChild(catGroup);

    // Add category group
    const addGroup = _mkSettingsGroup('');
    const addCard  = _mkSettingsGroupCard(addGroup);
    addCard.innerHTML = `
      <div class="px-4 py-4 flex flex-col gap-2">
        <label class="text-sm font-medium text-text" for="new-cat-name">Add category</label>
        <div class="flex items-center gap-2">
          <input type="text" id="new-cat-name" class="input flex-1 max-w-xs js-new-cat" placeholder="Category name">
          <button type="button" class="btn-primary js-add-cat">Add</button>
        </div>
        <span class="js-cat-error text-sm text-danger hidden"></span>
      </div>
    `;
    el.appendChild(addGroup);

    const listEl = /** @type {HTMLElement} */ (el.querySelector('.js-cat-list'));
    const nameEl = /** @type {HTMLInputElement} */ (el.querySelector('.js-new-cat'));
    const addBtn = /** @type {HTMLButtonElement} */ (el.querySelector('.js-add-cat'));
    const errEl  = /** @type {HTMLElement} */ (el.querySelector('.js-cat-error'));

    if (cats.length === 0) {
      listEl.innerHTML = '<p class="text-sm text-text-muted px-4 py-3">No categories yet.</p>';
    }

    for (let i = 0; i < cats.length; i++) {
      const cat = cats[i];
      const row = document.createElement('div');
      row.className = 'flex items-center gap-2 px-4 py-3';
      row.innerHTML = `
        <span class="flex-1 text-sm text-text js-cat-name" data-id="${cat.id}">${escapeHtml(cat.name)}</span>
        <input type="text" class="input flex-1 text-sm js-cat-edit hidden" value="${escapeHtml(cat.name)}" aria-label="Rename ${escapeHtml(cat.name)}">
        <div class="flex items-center gap-1 shrink-0">
          <button type="button" class="btn-icon js-cat-up" ${i === 0 ? 'disabled' : ''} aria-label="Move ${escapeHtml(cat.name)} up">${iconArrowUp}</button>
          <button type="button" class="btn-icon js-cat-down" ${i === cats.length - 1 ? 'disabled' : ''} aria-label="Move ${escapeHtml(cat.name)} down">${iconArrowDown}</button>
          <button type="button" class="btn-icon js-cat-edit-btn" aria-label="Rename ${escapeHtml(cat.name)}">${iconPencil}</button>
          <button type="button" class="btn-icon text-danger js-cat-delete" aria-label="Delete ${escapeHtml(cat.name)}">${iconX}</button>
        </div>
      `;

      const nameSpan  = /** @type {HTMLElement} */ (row.querySelector('.js-cat-name'));
      const editInput = /** @type {HTMLInputElement} */ (row.querySelector('.js-cat-edit'));
      const editBtn   = /** @type {HTMLButtonElement} */ (row.querySelector('.js-cat-edit-btn'));
      const delBtn    = /** @type {HTMLButtonElement} */ (row.querySelector('.js-cat-delete'));
      const upBtn     = /** @type {HTMLButtonElement} */ (row.querySelector('.js-cat-up'));
      const downBtn   = /** @type {HTMLButtonElement} */ (row.querySelector('.js-cat-down'));

      editBtn.addEventListener('click', () => {
        nameSpan.classList.add('hidden');
        editInput.classList.remove('hidden');
        editInput.focus();
        editInput.select();
      });

      const _saveEdit = async () => {
        const newName = editInput.value.trim();
        if (!newName || newName === cat.name) {
          editInput.classList.add('hidden');
          nameSpan.classList.remove('hidden');
          return;
        }
        try {
          await api.renameCategory(cat.id, newName);
          cat.name = newName;
          nameSpan.textContent = newName;
        } catch (e) {
          showToast(e?.message ?? 'Failed to rename.', { type: 'error' });
        }
        editInput.classList.add('hidden');
        nameSpan.classList.remove('hidden');
      };

      editInput.addEventListener('blur', _saveEdit);
      editInput.addEventListener('keydown', (e) => {
        if (e.key === 'Enter') { e.preventDefault(); _saveEdit(); }
        if (e.key === 'Escape') {
          editInput.value = cat.name;
          editInput.classList.add('hidden');
          nameSpan.classList.remove('hidden');
        }
      });

      delBtn.addEventListener('click', async () => {
        if (!confirm(`Delete category "${cat.name}"?`)) return;
        delBtn.disabled = true;
        try {
          await api.deleteCategory(cat.id);
          cats = cats.filter(c => c.id !== cat.id);
          _render();
        } catch (e) {
          showToast(e?.message ?? 'Failed to delete.', { type: 'error' });
          delBtn.disabled = false;
        }
      });

      upBtn.addEventListener('click', async () => {
        if (i === 0) return;
        [cats[i - 1], cats[i]] = [cats[i], cats[i - 1]];
        try {
          await api.reorderCategories(cats.map(c => c.id));
        } catch (e) {
          showToast(e?.message ?? 'Failed to reorder.', { type: 'error' });
        }
        _render();
      });

      downBtn.addEventListener('click', async () => {
        if (i === cats.length - 1) return;
        [cats[i], cats[i + 1]] = [cats[i + 1], cats[i]];
        try {
          await api.reorderCategories(cats.map(c => c.id));
        } catch (e) {
          showToast(e?.message ?? 'Failed to reorder.', { type: 'error' });
        }
        _render();
      });

      listEl.appendChild(row);
    }

    addBtn.addEventListener('click', async () => {
      const name = nameEl.value.trim();
      if (!name) return;
      addBtn.disabled = true;
      errEl.classList.add('hidden');
      try {
        await api.createCategory(name, cats.length);
        nameEl.value = '';
        const updated = await api.getCategories();
        cats = Array.isArray(updated) ? updated : cats;
        _render();
      } catch (e) {
        errEl.textContent = e?.message ?? 'Failed to add category.';
        errEl.classList.remove('hidden');
        addBtn.disabled = false;
      }
    });

    nameEl.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') addBtn.click();
    });
  }

  _render();
}

// ── Account section ────────────────────────────────────────────────────────────

/** @param {HTMLElement} el */
function _renderAccountSection(el) {
  // Change password group
  const pwGroup = _mkSettingsGroup('Password');
  const pwCard  = _mkSettingsGroupCard(pwGroup);
  pwCard.innerHTML = `
    <div class="flex flex-col gap-3 px-4 py-4">
      <div class="flex flex-col gap-1.5">
        <label class="text-sm font-medium text-text" for="cur-pw">Current password</label>
        <input type="password" id="cur-pw" class="input max-w-sm js-cur-pw" autocomplete="current-password">
      </div>
      <div class="flex flex-col gap-1.5">
        <label class="text-sm font-medium text-text" for="new-pw">New password</label>
        <input type="password" id="new-pw" class="input max-w-sm js-new-pw" autocomplete="new-password">
      </div>
      <div class="flex flex-col gap-1.5">
        <label class="text-sm font-medium text-text" for="conf-pw">Confirm new password</label>
        <input type="password" id="conf-pw" class="input max-w-sm js-conf-pw" autocomplete="new-password">
      </div>
      <div class="flex items-center gap-3">
        <button type="button" class="btn-primary btn-sm js-change-pw">Change password</button>
        <span class="js-pw-result text-sm hidden"></span>
      </div>
    </div>
  `;
  el.appendChild(pwGroup);

  // Sessions group
  const sessGroup = _mkSettingsGroup('Sessions');
  const sessCard  = _mkSettingsGroupCard(sessGroup);
  const logoutBtn = document.createElement('button');
  logoutBtn.type = 'button';
  logoutBtn.className = 'btn-danger btn-sm js-logout-all';
  logoutBtn.textContent = 'Sign out everywhere';
  sessCard.appendChild(_mkSettingsRow({ label: 'Sign out of all devices', description: 'Invalidates all active sessions, including this one.', control: logoutBtn }));
  el.appendChild(sessGroup);

  const curPwEl      = /** @type {HTMLInputElement} */ (el.querySelector('.js-cur-pw'));
  const newPwEl      = /** @type {HTMLInputElement} */ (el.querySelector('.js-new-pw'));
  const confPwEl     = /** @type {HTMLInputElement} */ (el.querySelector('.js-conf-pw'));
  const changePwBtn  = /** @type {HTMLButtonElement} */ (el.querySelector('.js-change-pw'));
  const pwResult     = /** @type {HTMLElement} */ (el.querySelector('.js-pw-result'));
  const logoutAllBtn = /** @type {HTMLButtonElement} */ (el.querySelector('.js-logout-all'));

  changePwBtn.addEventListener('click', async () => {
    const cur  = curPwEl.value;
    const next = newPwEl.value;
    const conf = confPwEl.value;

    if (!cur || !next) {
      _showResult(pwResult, false, 'Please fill in all fields.');
      return;
    }
    if (next !== conf) {
      _showResult(pwResult, false, 'Passwords do not match.');
      return;
    }

    changePwBtn.disabled = true;
    try {
      await api.changePassword(cur, next);
      curPwEl.value = '';
      newPwEl.value = '';
      confPwEl.value = '';
      _showResult(pwResult, true, 'Password changed.');
    } catch (e) {
      _showResult(pwResult, false, e?.message ?? 'Failed to change password.');
    } finally {
      changePwBtn.disabled = false;
    }
  });

  logoutAllBtn.addEventListener('click', async () => {
    if (!confirm('Sign out of all sessions?')) return;
    logoutAllBtn.disabled = true;
    try {
      await api.logoutEverywhere();
      navigate('/login');
    } catch (e) {
      showToast(e?.message ?? 'Failed to sign out.', { type: 'error' });
      logoutAllBtn.disabled = false;
    }
  });
}

// ── Server section ────────────────────────────────────────────────────────────

/** @param {HTMLElement} el */
function _renderServerSection(el) {
  // Danger zone group
  const dangerGroup = _mkSettingsGroup('Danger zone');
  const dangerCard  = _mkSettingsGroupCard(dangerGroup);
  dangerCard.classList.add('border', 'border-danger/20');

  const restartBtn = document.createElement('button');
  restartBtn.type = 'button';
  restartBtn.className = 'btn-primary btn-sm js-restart-btn';
  restartBtn.textContent = 'Restart';
  dangerCard.appendChild(_mkSettingsRow({ label: 'Restart server', description: 'Restart the server process. The page will reload automatically.', control: restartBtn }));

  const stopBtn = document.createElement('button');
  stopBtn.type = 'button';
  stopBtn.className = 'btn-danger btn-sm js-stop-btn';
  stopBtn.textContent = 'Stop';
  dangerCard.appendChild(_mkSettingsRow({ label: 'Stop server', description: 'Shut down the server. Only auto-restarts if managed by Docker or systemd.', control: stopBtn }));

  el.appendChild(dangerGroup);

  //const restartBtn = /** @type {HTMLButtonElement} */ (el.querySelector('.js-restart-btn'));
  //const stopBtn    = /** @type {HTMLButtonElement} */ (el.querySelector('.js-stop-btn'));

  restartBtn.addEventListener('click', async () => {
    if (!confirm('Restart the server?\n\nThe page will reload automatically when the server comes back online.')) return;
    restartBtn.disabled = true;
    stopBtn.disabled    = true;
    try {
      await api.serverRestart();
      _showRestartOverlay();
    } catch (e) {
      showToast(e?.hint ?? e?.message ?? 'Failed to restart server.', { type: 'error' });
      restartBtn.disabled = false;
      stopBtn.disabled    = false;
    }
  });

  stopBtn.addEventListener('click', async () => {
    if (!confirm('Stop the server?\n\nThe server will only restart automatically if managed by Docker or systemd.')) return;
    restartBtn.disabled = true;
    stopBtn.disabled    = true;
    try {
      await api.serverStop();
      showToast('Server is stopping…', { type: 'info', duration: 8000 });
    } catch (e) {
      showToast(e?.hint ?? e?.message ?? 'Failed to stop server.', { type: 'error' });
      restartBtn.disabled = false;
      stopBtn.disabled    = false;
    }
  });
}

function _showRestartOverlay() {
  const overlay = document.createElement('div');
  overlay.id = 'restart-overlay';
  overlay.className = [
    'fixed inset-0 z-[9999] flex flex-col items-center justify-center gap-4',
    'bg-bg/90 backdrop-blur-sm',
  ].join(' ');
  overlay.innerHTML = `
    <div class="w-10 h-10 border-4 border-accent border-t-transparent rounded-full animate-spin"></div>
    <p class="text-lg font-semibold text-text">Server is restarting…</p>
    <p class="text-sm text-text-muted">The page will reload automatically.</p>
  `;
  document.body.appendChild(overlay);

  // Fallback: poll /health every 2s until it responds, then reload
  const poll = setInterval(async () => {
    try {
      const res = await fetch('/health');
      if (res.ok) { clearInterval(poll); window.location.reload(); }
    } catch { /* still down */ }
  }, 2000);

  // Reload when the server comes back (boot_id change triggers kani:server-restart)
  window.addEventListener('kani:server-restart', () => { clearInterval(poll); window.location.reload(); }, { once: true });
}

// ── Destroy ───────────────────────────────────────────────────────────────────

/** @param {HTMLElement} container */
export function destroy(container) {
  for (const d of _panelDestroys) d();
  _panelDestroys = [];
  _activeSection = null;
  container.innerHTML = '';
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/**
 * @param {HTMLElement} el
 * @param {boolean} ok
 * @param {string} msg
 */
function _showResult(el, ok, msg) {
  el.textContent = msg;
  el.classList.remove('text-success', 'text-danger', 'hidden');
  el.classList.add(ok ? 'text-success' : 'text-danger');
  setTimeout(() => { el.classList.add('hidden'); }, 4000);
}
