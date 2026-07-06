// @ts-check
// Reset password page — validates token, then allows the user to set a new password.

import { iconX } from '../icons.js';
import { validateResetToken, confirmPasswordReset } from '../api.js';
import { t } from '../i18n.js';

/** @param {HTMLElement} container */
export async function init(container) {
  document.title = t('auth.reset.page_title');

  const token = new URLSearchParams(location.search).get('token') ?? '';

  container.innerHTML = `
    <div class="min-h-screen flex items-center justify-center p-4 bg-bg">
      <div class="w-full max-w-sm bg-surface rounded-2xl shadow-lg border border-border p-8 flex flex-col gap-6">
        <div class="text-center flex flex-col gap-1">
          <h1 class="text-2xl font-bold text-text">${t('auth.reset.title')}</h1>
          <p class="text-sm text-text-muted" id="rp-subtitle">${t('auth.reset.verifying')}</p>
        </div>

        <div
          class="hidden items-center gap-2 px-3 py-2.5 rounded-lg bg-danger/10 border border-danger/30 text-sm text-danger"
          role="alert"
          id="rp-error"
        >
          <span aria-hidden="true" class="shrink-0 icon-sm">${iconX}</span>
          <span id="rp-error-msg"></span>
        </div>

        <div id="rp-success" class="hidden px-3 py-2.5 rounded-lg bg-success/10 border border-success/30 text-sm text-success">
          ${t('auth.reset.success')} <a href="/login" class="underline">${t('auth.reset.success.signin')}</a>
        </div>

        <form class="hidden flex-col gap-4" id="rp-form" novalidate>
          <div class="flex flex-col gap-1.5">
            <label class="text-sm font-medium text-text" for="rp-new-pw">${t('auth.reset.new_password')}</label>
            <input id="rp-new-pw" class="input" type="password" autocomplete="new-password" required autofocus />
          </div>
          <div class="flex flex-col gap-1.5">
            <label class="text-sm font-medium text-text" for="rp-conf-pw">${t('auth.reset.confirm_password')}</label>
            <input id="rp-conf-pw" class="input" type="password" autocomplete="new-password" required />
          </div>
          <button type="submit" class="btn-primary w-full h-11 mt-2" id="rp-submit">${t('auth.reset.submit')}</button>
        </form>

        <p id="rp-invalid-link" class="hidden text-center text-sm">
          <a href="/forgot-password" class="text-accent hover:underline">${t('auth.reset.request_link')}</a>
        </p>

        <p class="text-center text-sm text-text-muted">
          <a href="/login" class="text-accent hover:underline">${t('auth.reset.back')}</a>
        </p>
      </div>
    </div>
  `;

  const subtitle   = /** @type {HTMLElement}       */ (container.querySelector('#rp-subtitle'));
  const errBox     = /** @type {HTMLElement}       */ (container.querySelector('#rp-error'));
  const errMsg     = /** @type {HTMLElement}       */ (container.querySelector('#rp-error-msg'));
  const successEl  = /** @type {HTMLElement}       */ (container.querySelector('#rp-success'));
  const form       = /** @type {HTMLFormElement}   */ (container.querySelector('#rp-form'));
  const btn        = /** @type {HTMLButtonElement} */ (container.querySelector('#rp-submit'));
  const invalidLink = /** @type {HTMLElement}      */ (container.querySelector('#rp-invalid-link'));

  function _showError(msg) {
    errMsg.textContent = msg;
    errBox.classList.remove('hidden');
    errBox.classList.add('flex');
  }

  if (!token) {
    subtitle.textContent = t('auth.reset.error.invalid_link');
    invalidLink.classList.remove('hidden');
    return;
  }

  try {
    const data = await validateResetToken(token);
    subtitle.textContent = t('auth.reset.for_email', { email: data.email_hint });
    form.classList.remove('hidden');
    form.classList.add('flex');
  } catch {
    subtitle.textContent = t('auth.reset.error.expired');
    invalidLink.classList.remove('hidden');
    return;
  }

  form.addEventListener('submit', async (e) => {
    e.preventDefault();
    errBox.classList.add('hidden');
    errBox.classList.remove('flex');

    const newPw  = /** @type {HTMLInputElement} */ (container.querySelector('#rp-new-pw')).value;
    const confPw = /** @type {HTMLInputElement} */ (container.querySelector('#rp-conf-pw')).value;

    if (newPw.length < 8) {
      _showError(t('auth.reset.error.too_short'));
      return;
    }
    if (newPw !== confPw) {
      _showError(t('auth.reset.error.mismatch'));
      return;
    }

    btn.disabled = true;
    btn.textContent = t('common.saving');

    try {
      await confirmPasswordReset(token, newPw);
      form.classList.add('hidden');
      successEl.classList.remove('hidden');
    } catch (/** @type {any} */ err) {
      _showError(err?.message ?? t('auth.reset.error.failed'));
      btn.disabled = false;
      btn.textContent = t('auth.reset.submit');
    }
  });
}

/** @param {HTMLElement} container */
export function destroy(container) {
  container.innerHTML = '';
}
