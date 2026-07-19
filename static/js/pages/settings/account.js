// @ts-check
// Settings — My Account section (password modal, sessions).

import { h } from 'preact';
import { useState } from 'preact/hooks';
import htm from 'htm';
import * as api from '../../api.js';
import { navigate } from '../../router.js';
import { mkSettingsGroup, mkSettingsGroupCard, mkSettingsRow } from './_shared.js';
import { Modal, mountIntoModalRoot, showConfirm } from '../../components/modal.js';
import { t } from '../../i18n.js';
const html = htm.bind(h);

/** @param {HTMLElement} el */
export function mount(el) {
  const pwGroup = mkSettingsGroup(t('settings.account.password.group'));
  const pwCard  = mkSettingsGroupCard(pwGroup);
  const changePwBtn = document.createElement('button');
  changePwBtn.type = 'button';
  changePwBtn.className = 'btn-ghost btn-sm';
  changePwBtn.textContent = t('settings.account.password.btn');
  pwCard.appendChild(mkSettingsRow({ label: t('settings.account.password.label'), description: t('settings.account.password.desc'), control: changePwBtn }));
  el.appendChild(pwGroup);

  // Email verification status row
  const emailGroup = mkSettingsGroup(t('settings.account.email.group'));
  const emailCard  = mkSettingsGroupCard(emailGroup);
  const verifyCtrl = document.createElement('div');
  verifyCtrl.className = 'flex items-center gap-2';
  const verifyStatus = document.createElement('span');
  verifyStatus.className = 'text-xs text-text-muted';
  verifyStatus.textContent = t('common.loading');
  verifyCtrl.appendChild(verifyStatus);
  const resendBtn = document.createElement('button');
  resendBtn.type = 'button';
  resendBtn.className = 'btn-ghost btn-sm hidden';
  resendBtn.textContent = t('settings.account.email.resend');
  verifyCtrl.appendChild(resendBtn);
  emailCard.appendChild(mkSettingsRow({ label: t('settings.account.email.verify.label'), description: t('settings.account.email.verify.desc'), control: verifyCtrl }));
  el.appendChild(emailGroup);

  api.getCurrentUser().then(user => {
    if (user?.email_verified_at) {
      verifyStatus.textContent = t('settings.account.email.verified_on', { date: new Date(user.email_verified_at).toLocaleDateString() });
      verifyStatus.classList.add('text-success');
      verifyStatus.classList.remove('text-text-muted');
    } else {
      verifyStatus.textContent = t('settings.account.email.not_verified');
      resendBtn.classList.remove('hidden');
    }
  }).catch(() => {
    verifyStatus.textContent = t('settings.account.email.load_failed');
  });

  resendBtn.addEventListener('click', async () => {
    resendBtn.disabled = true;
    resendBtn.textContent = t('common.sending');
    try {
      await api.resendVerification();
      resendBtn.textContent = t('settings.account.email.sent');
      setTimeout(() => { resendBtn.textContent = t('settings.account.email.resend'); resendBtn.disabled = false; }, 3000);
    } catch (e) {
      import('../../components/toast.js').then(({ showToast }) => {
        showToast(/** @type {any} */(e)?.message ?? t('settings.account.email.send_failed'), { type: 'error' });
      });
      resendBtn.disabled = false;
      resendBtn.textContent = t('settings.account.email.resend');
    }
  });

  const sessGroup = mkSettingsGroup(t('settings.account.sessions.group'));
  const sessCard  = mkSettingsGroupCard(sessGroup);
  const securityLink = document.createElement('a');
  securityLink.href = '/settings?section=security';
  securityLink.className = 'btn-ghost btn-sm';
  securityLink.textContent = t('settings.account.sessions.manage');
  securityLink.addEventListener('click', e => { e.preventDefault(); import('../../router.js').then(({ navigate }) => navigate('/settings?section=security')); });
  sessCard.appendChild(mkSettingsRow({ label: t('settings.account.sessions.active.label'), description: t('settings.account.sessions.active.desc'), control: securityLink }));
  const logoutBtn = document.createElement('button');
  logoutBtn.type = 'button';
  logoutBtn.className = 'btn-danger btn-sm';
  logoutBtn.textContent = t('settings.account.sessions.signout_all');
  sessCard.appendChild(mkSettingsRow({ label: t('settings.account.sessions.signout_all.label'), description: t('settings.account.sessions.signout_all.desc'), control: logoutBtn }));
  el.appendChild(sessGroup);

  changePwBtn.addEventListener('click', () => _showChangePasswordModal());

  logoutBtn.addEventListener('click', async () => {
    if (!(await showConfirm(t('settings.account.sessions.confirm.message'), { title: t('settings.account.sessions.signout_all'), danger: true }))) return;
    logoutBtn.disabled = true;
    try {
      await api.logoutEverywhere();
      navigate('/login');
    } catch (e) {
      import('../../components/toast.js').then(({ showToast }) => {
        showToast(/** @type {any} */(e)?.message ?? t('settings.account.sessions.signout_failed'), { type: 'error' });
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
      if (!cur || !next) { setError(t('settings.account.password.modal.error.empty')); return; }
      if (next !== conf)  { setError(t('auth.reset.error.mismatch')); return; }
      setSaving(true);
      setError('');
      try {
        await api.changePassword(cur, next);
        onClose();
        import('../../components/toast.js').then(({ showToast }) => showToast(t('settings.account.password.modal.success')));
      } catch (e) {
        setError(/** @type {any} */(e)?.message ?? t('settings.account.password.modal.error.failed'));
        setSaving(false);
      }
    }

    return html`
      <${Modal}
        open=${true}
        title=${t('settings.account.password.modal.title')}
        onClose=${onClose}
        footer=${html`
          <button type="button" class="btn-ghost btn-sm" onClick=${onClose}>${t('common.cancel')}</button>
          <button type="button" class="btn-primary btn-sm" onClick=${_save} disabled=${saving}>${t('settings.account.password.modal.title')}</button>
        `}
      >
        <div class="flex flex-col gap-4 px-1">
          <div class="flex flex-col gap-1.5">
            <label class="text-sm font-medium text-text" for="modal-cur-pw">${t('settings.account.password.modal.current')}</label>
            <input type="password" id="modal-cur-pw" class="input" autocomplete="current-password"
              autoFocus value=${cur} onInput=${(/** @type {any} */ e) => setCur(e.target.value)}
              onKeyDown=${(/** @type {KeyboardEvent} */ e) => e.key === 'Enter' && _save()} />
          </div>
          <div class="flex flex-col gap-1.5">
            <label class="text-sm font-medium text-text" for="modal-new-pw">${t('settings.account.password.modal.new')}</label>
            <input type="password" id="modal-new-pw" class="input" autocomplete="new-password"
              value=${next} onInput=${(/** @type {any} */ e) => setNext(e.target.value)}
              onKeyDown=${(/** @type {KeyboardEvent} */ e) => e.key === 'Enter' && _save()} />
          </div>
          <div class="flex flex-col gap-1.5">
            <label class="text-sm font-medium text-text" for="modal-conf-pw">${t('settings.account.password.modal.confirm')}</label>
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
