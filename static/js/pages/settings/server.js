// @ts-check
// Settings — Server section (restart, stop, admin controls).

import * as api from '../../api.js';
import { openConfirm } from '../../utils.js';
import { showToast } from '../../components/toast.js';
import { mkSettingsGroup, mkSettingsGroupCard, mkSettingsRow } from './_shared.js';

/** @param {HTMLElement} el */
export function mount(el) {
  // ── Admin actions ─────────────────────────────────────────────────────────

  const adminGroup = mkSettingsGroup('Admin actions');
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
    'Cancel all downloads',
    'Stop all in-progress and queued chapter downloads.',
    'Cancel downloads',
    'btn-ghost',
    async () => {
      if (!(await openConfirm({ title: 'Cancel all downloads', message: 'Cancel all in-progress and queued downloads?', confirmLabel: 'Cancel downloads', danger: true }))) return;
      await api.cancelAllGlobalDownloads();
      showToast('All downloads cancelled.', { type: 'info' });
    },
  );

  _mkActionRow(
    'Stop current scan',
    'Abort the in-progress library refresh scan.',
    'Stop scan',
    'btn-ghost',
    async () => {
      if (!(await openConfirm({ title: 'Stop scan', message: 'Abort the currently running library scan?', confirmLabel: 'Stop scan', danger: true }))) return;
      await api.stopScan();
      showToast('Scan aborted.', { type: 'info' });
    },
  );

  _mkActionRow(
    'Clear request cache',
    'Flush the in-memory cache for source manga details, chapter lists, and search results.',
    'Clear cache',
    'btn-ghost',
    async () => {
      await api.clearCache();
      showToast('Cache cleared.', { type: 'success' });
    },
  );

  el.appendChild(adminGroup);

  // ── Danger zone ───────────────────────────────────────────────────────────

  const dangerGroup = mkSettingsGroup('Danger zone');
  const dangerCard  = mkSettingsGroupCard(dangerGroup);
  dangerCard.classList.add('border', 'border-danger/20');

  const restartBtn = document.createElement('button');
  restartBtn.type = 'button';
  restartBtn.className = 'btn-primary btn-sm';
  restartBtn.textContent = 'Restart';
  dangerCard.appendChild(mkSettingsRow({ label: 'Restart server', description: 'Restart the server process. The page will reload automatically.', control: restartBtn }));

  const stopBtn = document.createElement('button');
  stopBtn.type = 'button';
  stopBtn.className = 'btn-danger btn-sm';
  stopBtn.textContent = 'Stop';
  dangerCard.appendChild(mkSettingsRow({ label: 'Stop server', description: 'Shut down the server. Only auto-restarts if managed by Docker or systemd.', control: stopBtn }));

  el.appendChild(dangerGroup);

  restartBtn.addEventListener('click', async () => {
    if (!(await openConfirm({ title: 'Restart server', message: 'Restart the server? The page will reload automatically when the server comes back online.', confirmLabel: 'Restart', danger: true }))) return;
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
    if (!(await openConfirm({ title: 'Stop server', message: 'Stop the server? It will only restart automatically if managed by Docker or systemd.', confirmLabel: 'Stop', danger: true }))) return;
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
    <p class="text-lg font-semibold text-text">Server is restarting…</p>
    <p class="text-sm text-text-muted">The page will reload automatically.</p>
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
