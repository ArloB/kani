// @ts-check
// Settings — Server section (restart, stop, admin controls).

import * as api from '../../api.js';
import { showConfirm } from '../../components/modal.js';
import { showToast } from '../../components/toast.js';
import { mkSettingsGroup, mkSettingsGroupCard, mkSettingsRow } from './_shared.js';
import { t } from '../../i18n.js';

/** @param {HTMLElement} el */
export function mount(el) {
  // ── Server info ───────────────────────────────────────────────────────────
  // Show the OPDS catalog URL so users can point e-reader apps at it.

  const infoGroup = mkSettingsGroup(t('settings.server.integrations.group'));
  const infoCard  = mkSettingsGroupCard(infoGroup);

  const opdsBase = `${window.location.origin}/opds`;
  const opdsInput = document.createElement('input');
  opdsInput.type = 'text';
  opdsInput.readOnly = true;
  opdsInput.value = opdsBase;
  opdsInput.className = 'input input-sm font-mono text-xs w-56 select-all cursor-pointer';
  opdsInput.title = t('settings.server.opds.click_select');
  opdsInput.addEventListener('click', () => opdsInput.select());

  const copyBtn = document.createElement('button');
  copyBtn.type = 'button';
  copyBtn.className = 'btn-ghost btn-sm';
  copyBtn.textContent = t('common.copy');
  copyBtn.addEventListener('click', async () => {
    try {
      await navigator.clipboard.writeText(opdsBase);
      copyBtn.textContent = t('common.copied');
      setTimeout(() => { copyBtn.textContent = t('common.copy'); }, 2000);
    } catch { /* clipboard not available */ }
  });

  const opdsControl = document.createElement('div');
  opdsControl.className = 'flex items-center gap-2';
  opdsControl.appendChild(opdsInput);
  opdsControl.appendChild(copyBtn);

  infoCard.appendChild(mkSettingsRow({
    label: t('settings.server.opds.label'),
    description: t('settings.server.opds.desc'),
    control: opdsControl,
  }));

  el.appendChild(infoGroup);

  // ── Admin actions ─────────────────────────────────────────────────────────

  const adminGroup = mkSettingsGroup(t('settings.server.admin.group'));
  const adminCard  = mkSettingsGroupCard(adminGroup);

  /** @param {string} label @param {string} btnLabel @param {string} btnClass @param {() => Promise<void>} onClick */
  function _mkActionRow(label, description, btnLabel, btnClass, onClick) {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = `${btnClass} btn-sm`;
    btn.textContent = btnLabel;
    btn.addEventListener('click', async () => {
      btn.disabled = true;
      try { await onClick(); } finally { btn.disabled = false; }
    });
    adminCard.appendChild(mkSettingsRow({ label, description, control: btn }));
    return btn;
  }

  _mkActionRow(
    t('settings.server.admin.cancel_downloads.label'),
    t('settings.server.admin.cancel_downloads.desc'),
    t('settings.server.admin.cancel_downloads.btn'),
    'btn-ghost',
    async () => {
      if (!(await showConfirm(t('settings.server.admin.cancel_downloads.confirm'), { title: t('settings.server.admin.cancel_downloads.label'), confirmLabel: t('settings.server.admin.cancel_downloads.btn'), danger: true }))) return;
      await api.cancelAllGlobalDownloads();
      showToast(t('settings.server.admin.cancel_downloads.done'), { type: 'info' });
    },
  );

  _mkActionRow(
    t('settings.server.admin.stop_scan.label'),
    t('settings.server.admin.stop_scan.desc'),
    t('settings.server.admin.stop_scan.btn'),
    'btn-ghost',
    async () => {
      if (!(await showConfirm(t('settings.server.admin.stop_scan.confirm'), { title: t('settings.server.admin.stop_scan.btn'), confirmLabel: t('settings.server.admin.stop_scan.btn'), danger: true }))) return;
      await api.stopScan();
      showToast(t('settings.server.admin.stop_scan.done'), { type: 'info' });
    },
  );

  _mkActionRow(
    t('settings.server.admin.clear_cache.label'),
    t('settings.server.admin.clear_cache.desc'),
    t('settings.server.admin.clear_cache.btn'),
    'btn-ghost',
    async () => {
      await api.clearCache();
      showToast(t('settings.server.admin.clear_cache.done'), { type: 'success' });
    },
  );

  el.appendChild(adminGroup);

  // ── Danger zone ───────────────────────────────────────────────────────────

  const dangerGroup = mkSettingsGroup(t('settings.server.danger.group'));
  const dangerCard  = mkSettingsGroupCard(dangerGroup);
  dangerCard.classList.add('border', 'border-danger/20');

  const restartBtn = document.createElement('button');
  restartBtn.type = 'button';
  restartBtn.className = 'btn-primary btn-sm';
  restartBtn.textContent = t('settings.server.danger.restart.btn');
  dangerCard.appendChild(mkSettingsRow({ label: t('settings.server.danger.restart.label'), description: t('settings.server.danger.restart.desc'), control: restartBtn }));

  const stopBtn = document.createElement('button');
  stopBtn.type = 'button';
  stopBtn.className = 'btn-danger btn-sm';
  stopBtn.textContent = t('settings.server.danger.stop.btn');
  dangerCard.appendChild(mkSettingsRow({ label: t('settings.server.danger.stop.label'), description: t('settings.server.danger.stop.desc'), control: stopBtn }));

  el.appendChild(dangerGroup);

  restartBtn.addEventListener('click', async () => {
    if (!(await showConfirm(t('settings.server.danger.restart.confirm'), { title: t('settings.server.danger.restart.label'), confirmLabel: t('settings.server.danger.restart.btn'), danger: true }))) return;
    restartBtn.disabled = true;
    stopBtn.disabled    = true;
    try {
      await api.serverRestart();
      _showRestartOverlay();
    } catch (e) {
      showToast(e?.hint ?? e?.message ?? t('settings.server.danger.restart.failed'), { type: 'error' });
      restartBtn.disabled = false;
      stopBtn.disabled    = false;
    }
  });

  stopBtn.addEventListener('click', async () => {
    if (!(await showConfirm(t('settings.server.danger.stop.confirm'), { title: t('settings.server.danger.stop.label'), confirmLabel: t('settings.server.danger.stop.btn'), danger: true }))) return;
    restartBtn.disabled = true;
    stopBtn.disabled    = true;
    try {
      await api.serverStop();
      showToast(t('settings.server.danger.stop.stopping'), { type: 'info', duration: 8000 });
    } catch (e) {
      showToast(e?.hint ?? e?.message ?? t('settings.server.danger.stop.failed'), { type: 'error' });
      restartBtn.disabled = false;
      stopBtn.disabled    = false;
    }
  });

  return { destroy() { el.innerHTML = ''; } };
}

function _showRestartOverlay() {
  const overlay = document.createElement('div');
  overlay.id = 'restart-overlay';
  overlay.className = [
    'fixed inset-0 z-top flex flex-col items-center justify-center gap-4',
    'bg-bg/90 backdrop-blur-sm',
  ].join(' ');
  overlay.innerHTML = `
    <div class="w-10 h-10 border-4 border-accent border-t-transparent rounded-full animate-spin"></div>
    <p class="text-lg font-semibold text-text">${t('settings.server.restart_overlay.title')}</p>
    <p class="text-sm text-text-muted">${t('settings.server.restart_overlay.desc')}</p>
  `;
  document.body.appendChild(overlay);

  const poll = setInterval(async () => {
    try {
      const res = await fetch('/health');
      if (res.ok) { clearInterval(poll); window.location.reload(); }
    } catch { /* still down */ }
  }, 2000);

  window.addEventListener('kani:server-restart', () => { clearInterval(poll); window.location.reload(); }, { once: true });
}
