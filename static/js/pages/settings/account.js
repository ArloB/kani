// @ts-check
// Settings — My Account section (password modal, sessions).

import { h } from 'preact';
import { useState } from 'preact/hooks';
import htm from 'htm';
import * as api from '../../api.js';
import { openConfirm } from '../../utils.js';
import { navigate } from '../../router.js';
import { mkSettingsGroup, mkSettingsGroupCard, mkSettingsRow } from './_shared.js';
import { Modal, mountIntoModalRoot } from '../../components/modal.js';
const html = htm.bind(h);

/** @param {HTMLElement} el */
export function mount(el) {
  const pwGroup = mkSettingsGroup('Password');
  const pwCard  = mkSettingsGroupCard(pwGroup);
  const changePwBtn = document.createElement('button');
  changePwBtn.type = 'button';
  changePwBtn.className = 'btn-ghost btn-sm';
  changePwBtn.textContent = 'Change password';
  pwCard.appendChild(mkSettingsRow({ label: 'Password', description: 'Update your account password.', control: changePwBtn }));
  el.appendChild(pwGroup);

  const sessGroup = mkSettingsGroup('Sessions');
  const sessCard  = mkSettingsGroupCard(sessGroup);
  const logoutBtn = document.createElement('button');
  logoutBtn.type = 'button';
  logoutBtn.className = 'btn-danger btn-sm';
  logoutBtn.textContent = 'Sign out everywhere';
  sessCard.appendChild(mkSettingsRow({ label: 'Sign out of all devices', description: 'Invalidates all active sessions, including this one.', control: logoutBtn }));
  el.appendChild(sessGroup);

  changePwBtn.addEventListener('click', () => _showChangePasswordModal());

  logoutBtn.addEventListener('click', async () => {
    if (!(await openConfirm({ title: 'Sign out everywhere', message: 'Sign out of all sessions? You will need to log in again.', danger: true }))) return;
    logoutBtn.disabled = true;
    try {
      await api.logoutEverywhere();
      navigate('/login');
    } catch (e) {
      import('../../components/toast.js').then(({ showToast }) => {
        showToast(/** @type {any} */(e)?.message ?? 'Failed to sign out.', { type: 'error' });
      });
      logoutBtn.disabled = false;
    }
  });

  return { destroy() { el.innerHTML = ''; } };
}

function _showChangePasswordModal() {
  function ChangePasswordModal({ onClose }) {
    const [cur, setCur]   = useState('');
    const [next, setNext] = useState('');
    const [conf, setConf] = useState('');
    const [error, setError] = useState('');
    const [saving, setSaving] = useState(false);

    async function _save() {
      if (!cur || !next) { setError('Please fill in all fields.'); return; }
      if (next !== conf)  { setError('Passwords do not match.'); return; }
      setSaving(true);
      setError('');
      try {
        await api.changePassword(cur, next);
        onClose();
        import('../../components/toast.js').then(({ showToast }) => showToast('Password changed.'));
      } catch (e) {
        setError(/** @type {any} */(e)?.message ?? 'Failed to change password.');
        setSaving(false);
      }
    }

    return html`
      <${Modal}
        open=${true}
        title="Change password"
        onClose=${onClose}
        footer=${html`
          <button type="button" class="btn-ghost btn-sm" onClick=${onClose}>Cancel</button>
          <button type="button" class="btn-primary btn-sm" onClick=${_save} disabled=${saving}>Change password</button>
        `}
      >
        <div class="flex flex-col gap-4 px-1">
          <div class="flex flex-col gap-1.5">
            <label class="text-sm font-medium text-text" for="modal-cur-pw">Current password</label>
            <input type="password" id="modal-cur-pw" class="input" autocomplete="current-password"
              autoFocus value=${cur} onInput=${(/** @type {any} */ e) => setCur(e.target.value)}
              onKeyDown=${(/** @type {KeyboardEvent} */ e) => e.key === 'Enter' && _save()} />
          </div>
          <div class="flex flex-col gap-1.5">
            <label class="text-sm font-medium text-text" for="modal-new-pw">New password</label>
            <input type="password" id="modal-new-pw" class="input" autocomplete="new-password"
              value=${next} onInput=${(/** @type {any} */ e) => setNext(e.target.value)}
              onKeyDown=${(/** @type {KeyboardEvent} */ e) => e.key === 'Enter' && _save()} />
          </div>
          <div class="flex flex-col gap-1.5">
            <label class="text-sm font-medium text-text" for="modal-conf-pw">Confirm new password</label>
            <input type="password" id="modal-conf-pw" class="input" autocomplete="new-password"
              value=${conf} onInput=${(/** @type {any} */ e) => setConf(e.target.value)}
              onKeyDown=${(/** @type {KeyboardEvent} */ e) => e.key === 'Enter' && _save()} />
          </div>
          ${error && html`<p class="text-sm text-danger">${error}</p>`}
        </div>
      </${Modal}>
    `;
  }

  const unmount = mountIntoModalRoot(html`
    <${ChangePasswordModal} onClose=${() => unmount()} />
  `);
}
