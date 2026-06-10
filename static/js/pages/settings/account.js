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

  // Email verification status row
  const emailGroup = mkSettingsGroup('Email');
  const emailCard  = mkSettingsGroupCard(emailGroup);
  const verifyCtrl = document.createElement('div');
  verifyCtrl.className = 'flex items-center gap-2';
  const verifyStatus = document.createElement('span');
  verifyStatus.className = 'text-xs text-text-muted';
  verifyStatus.textContent = 'Loading…';
  verifyCtrl.appendChild(verifyStatus);
  const resendBtn = document.createElement('button');
  resendBtn.type = 'button';
  resendBtn.className = 'btn-ghost btn-sm hidden';
  resendBtn.textContent = 'Resend';
  verifyCtrl.appendChild(resendBtn);
  emailCard.appendChild(mkSettingsRow({ label: 'Email verification', description: 'Verify your email address to enable email features.', control: verifyCtrl }));
  el.appendChild(emailGroup);

  api.getCurrentUser().then(user => {
    if (user?.email_verified_at) {
      verifyStatus.textContent = `Verified on ${new Date(user.email_verified_at).toLocaleDateString()}`;
      verifyStatus.classList.add('text-success');
      verifyStatus.classList.remove('text-text-muted');
    } else {
      verifyStatus.textContent = 'Not verified';
      resendBtn.classList.remove('hidden');
    }
  }).catch(() => {
    verifyStatus.textContent = 'Unable to load';
  });

  resendBtn.addEventListener('click', async () => {
    resendBtn.disabled = true;
    resendBtn.textContent = 'Sending…';
    try {
      await api.resendVerification();
      resendBtn.textContent = 'Sent!';
      setTimeout(() => { resendBtn.textContent = 'Resend'; resendBtn.disabled = false; }, 3000);
    } catch (e) {
      import('../../components/toast.js').then(({ showToast }) => {
        showToast(/** @type {any} */(e)?.message ?? 'Failed to send.', { type: 'error' });
      });
      resendBtn.disabled = false;
      resendBtn.textContent = 'Resend';
    }
  });

  const sessGroup = mkSettingsGroup('Sessions');
  const sessCard  = mkSettingsGroupCard(sessGroup);
  const securityLink = document.createElement('a');
  securityLink.href = '/settings?section=security';
  securityLink.className = 'btn-ghost btn-sm';
  securityLink.textContent = 'Manage sessions';
  securityLink.addEventListener('click', e => { e.preventDefault(); import('../../router.js').then(({ navigate }) => navigate('/settings?section=security')); });
  sessCard.appendChild(mkSettingsRow({ label: 'Active sessions', description: 'View and revoke individual sessions.', control: securityLink }));
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
