// @ts-check
// Settings — Server section (restart, stop).

import * as api from '../../api.js';
import { openConfirm } from '../../utils.js';
import { showToast } from '../../components/toast.js';
import { mkSettingsGroup, mkSettingsGroupCard, mkSettingsRow } from './_shared.js';

/** @param {HTMLElement} el */
export function mount(el) {
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
