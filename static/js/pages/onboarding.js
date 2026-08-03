// @ts-check
// First-run onboarding wizard — shown only to admins on a fresh installation.

import * as api from '../api.js';
import { hasPermission, getState } from '../session.js';
import { navigate, consumeIntendedDestination } from '../router.js';
import { setPageHeader, clearPageHeader } from '../components/app-header.js';
import { showApiError } from '../components/toast.js';
import { t } from '../i18n.js';

/** @param {HTMLElement} container */
export async function init(container) {
  if (!hasPermission('admin:manage')) {
    navigate('/');
    return;
  }

  document.title = t('onboarding.title') + ' - Kani';
  setPageHeader({ breadcrumb: [{ label: t('onboarding.title') }] });

  let step = 0;
  let libraryPath = '/library';

  const wrap = document.createElement('div');
  wrap.className = 'max-w-lg mx-auto px-4 py-10 flex flex-col gap-8';
  container.appendChild(wrap);

  const stepEls = [_buildStep0(), _buildStep1(), _buildStep2(), _buildStep3()];

  function _render() {
    wrap.innerHTML = '';
    const progress = document.createElement('div');
    progress.className = 'flex items-center gap-2 mb-2';
    for (let i = 0; i < stepEls.length; i++) {
      const dot = document.createElement('div');
      dot.className = `h-2 rounded-full flex-1 ${i <= step ? 'bg-accent' : 'bg-border'}`;
      progress.appendChild(dot);
    }
    wrap.appendChild(progress);
    wrap.appendChild(stepEls[step]);
  }

  function _buildStep0() {
    const el = document.createElement('div');
    el.className = 'flex flex-col gap-4';
    const heading = document.createElement('h1');
    heading.className = 'text-2xl font-bold text-text';
    heading.textContent = t('onboarding.welcome');
    const sub = document.createElement('p');
    sub.className = 'text-text-muted';
    sub.textContent = t('onboarding.step0.desc');
    const btn = document.createElement('button');
    btn.className = 'btn-primary self-start mt-4';
    btn.textContent = t('onboarding.get_started');
    btn.addEventListener('click', () => { step = 1; _render(); });
    el.append(heading, sub, btn);
    return el;
  }

  function _buildStep1() {
    const el = document.createElement('div');
    el.className = 'flex flex-col gap-4';
    const heading = document.createElement('h2');
    heading.className = 'text-xl font-semibold text-text';
    heading.textContent = t('onboarding.step1.title');
    const desc = document.createElement('p');
    desc.className = 'text-text-muted text-sm';
    desc.textContent = t('onboarding.step1.desc');
    const input = document.createElement('input');
    input.type = 'text';
    input.className = 'input w-full font-mono text-sm';
    input.value = libraryPath;
    input.addEventListener('input', () => { libraryPath = input.value.trim() || '/library'; });
    const row = document.createElement('div');
    row.className = 'flex gap-3 mt-2';
    const btn = document.createElement('button');
    btn.className = 'btn-primary';
    btn.textContent = t('onboarding.next');
    btn.addEventListener('click', async () => {
      btn.disabled = true;
      try {
        const current = await api.getSettings();
        await api.updateSettings({ Advanced: { ...current, library_path: libraryPath } });
      } catch (e) {
        showApiError(e);
      } finally {
        btn.disabled = false;
      }
      step = 2;
      _render();
    });
    row.appendChild(btn);
    el.append(heading, desc, input, row);
    return el;
  }

  function _buildStep2() {
    const el = document.createElement('div');
    el.className = 'flex flex-col gap-4';
    const heading = document.createElement('h2');
    heading.className = 'text-xl font-semibold text-text';
    heading.textContent = t('onboarding.step2.title');
    const desc = document.createElement('p');
    desc.className = 'text-text-muted text-sm';
    desc.textContent = t('onboarding.step2.desc');

    const mkField = (labelKey, type) => {
      const label = document.createElement('label');
      label.className = 'flex flex-col gap-1 text-sm font-medium text-text';
      label.textContent = t(labelKey);
      const input = document.createElement('input');
      input.type = type;
      input.className = 'input w-full';
      label.appendChild(input);
      return { label, input };
    };

    const { label: lCurrent, input: inCurrent } = mkField('onboarding.step2.current', 'password');
    const { label: lNew, input: inNew } = mkField('onboarding.step2.new', 'password');
    const { label: lConfirm, input: inConfirm } = mkField('onboarding.step2.confirm', 'password');

    const errMsg = document.createElement('p');
    errMsg.className = 'text-sm text-error hidden';

    const row = document.createElement('div');
    row.className = 'flex gap-3 mt-2';
    const btn = document.createElement('button');
    btn.className = 'btn-primary';
    btn.textContent = t('onboarding.next');
    btn.addEventListener('click', async () => {
      errMsg.classList.add('hidden');
      if (inNew.value !== inConfirm.value) {
        errMsg.textContent = t('onboarding.step2.mismatch');
        errMsg.classList.remove('hidden');
        return;
      }
      if (inNew.value.length < 8) {
        errMsg.textContent = t('onboarding.step2.too_short');
        errMsg.classList.remove('hidden');
        return;
      }
      btn.disabled = true;
      try {
        await api.changePassword(inCurrent.value, inNew.value);
        // Changing a password invalidates the session, which used to strand the
        // wizard: the final step's "first run complete" call 401'd silently, the
        // flag was never set, and signing back in returned the user to
        // onboarding. Re-authenticate with the password just chosen so the rest
        // of the wizard runs with a live session.
        const username = getState('user')?.username;
        if (username) {
          await api.login(username, inNew.value).catch(() => {});
        }
        step = 3;
        _render();
      } catch (e) {
        showApiError(e);
        btn.disabled = false;
      }
    });
    const skipBtn = document.createElement('button');
    skipBtn.className = 'btn-ghost self-center text-sm';
    skipBtn.textContent = t('onboarding.skip');
    skipBtn.addEventListener('click', () => { step = 3; _render(); });
    row.append(btn, skipBtn);
    el.append(heading, desc, lCurrent, lNew, lConfirm, errMsg, row);
    return el;
  }

  function _buildStep3() {
    const el = document.createElement('div');
    el.className = 'flex flex-col gap-4';
    const heading = document.createElement('h2');
    heading.className = 'text-xl font-semibold text-text';
    heading.textContent = t('onboarding.step3.title');
    const desc = document.createElement('p');
    desc.className = 'text-text-muted text-sm';
    desc.textContent = t('onboarding.step3.desc');
    const row = document.createElement('div');
    row.className = 'flex gap-3 mt-2';
    const btn = document.createElement('button');
    btn.className = 'btn-primary';
    btn.textContent = t('onboarding.done');
    btn.addEventListener('click', async () => {
      btn.disabled = true;
      try {
        await api.markFirstRunComplete();
        navigate(consumeIntendedDestination() ?? '/');
      } catch (e) {
        // A dead session here means the wizard cannot record that setup is
        // done, and the user would be walked through it again on next sign-in.
        // Say which it is rather than failing mutely.
        showApiError(e);
        btn.disabled = false;
      }
    });
    const skip = document.createElement('a');
    skip.href = '/sources';
    skip.className = 'btn-ghost self-center text-sm';
    skip.textContent = t('onboarding.browse_sources');
    row.append(btn, skip);
    el.append(heading, desc, row);
    return el;
  }

  _render();
}

export function destroy(container) {
  container.innerHTML = '';
  clearPageHeader();
}
