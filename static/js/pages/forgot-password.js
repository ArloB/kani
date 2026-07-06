// @ts-check
// Forgot password page — submits an email address to request a reset link.

import { iconX } from '../icons.js';
import { requestPasswordReset } from '../api.js';
import { t } from '../i18n.js';

/** @param {HTMLElement} container */
export function init(container) {
  document.title = t('auth.forgot.page_title');

  container.innerHTML = `
    <div class="min-h-screen flex items-center justify-center p-4 bg-bg">
      <div class="w-full max-w-sm bg-surface rounded-2xl shadow-lg border border-border p-8 flex flex-col gap-6">
        <div class="text-center flex flex-col gap-1">
          <h1 class="text-2xl font-bold text-text">${t('auth.forgot.title')}</h1>
          <p class="text-sm text-text-muted">${t('auth.forgot.subtitle')}</p>
        </div>

        <div
          class="hidden items-center gap-2 px-3 py-2.5 rounded-lg bg-danger/10 border border-danger/30 text-sm text-danger"
          role="alert"
          id="fp-error"
        >
          <span aria-hidden="true" class="shrink-0 icon-sm">${iconX}</span>
          <span id="fp-error-msg"></span>
        </div>

        <div id="fp-success" class="hidden px-3 py-2.5 rounded-lg bg-success/10 border border-success/30 text-sm text-success">
          ${t('auth.forgot.success')}
        </div>

        <form class="flex flex-col gap-4" id="fp-form" novalidate>
          <div class="flex flex-col gap-1.5">
            <label class="text-sm font-medium text-text" for="fp-email">${t('auth.forgot.email')}</label>
            <input
              id="fp-email"
              class="input"
              type="email"
              name="email"
              autocomplete="email"
              required
              autofocus
            />
          </div>
          <button type="submit" class="btn-primary w-full h-11 mt-2" id="fp-submit">${t('auth.forgot.submit')}</button>
        </form>

        <p class="text-center text-sm text-text-muted">
          <a href="/login" class="text-accent hover:underline">${t('auth.forgot.back')}</a>
        </p>
      </div>
    </div>
  `;

  const form    = /** @type {HTMLFormElement}   */ (container.querySelector('#fp-form'));
  const btn     = /** @type {HTMLButtonElement} */ (container.querySelector('#fp-submit'));
  const errBox  = /** @type {HTMLElement}       */ (container.querySelector('#fp-error'));
  const errMsg  = /** @type {HTMLElement}       */ (container.querySelector('#fp-error-msg'));
  const success = /** @type {HTMLElement}       */ (container.querySelector('#fp-success'));

  function _showError(msg) {
    errMsg.textContent = msg;
    errBox.classList.remove('hidden');
    errBox.classList.add('flex');
    success.classList.add('hidden');
  }

  form.addEventListener('submit', async (e) => {
    e.preventDefault();
    errBox.classList.add('hidden');
    errBox.classList.remove('flex');
    btn.disabled = true;
    btn.textContent = t('auth.forgot.submitting');

    const email = /** @type {HTMLInputElement} */ (container.querySelector('#fp-email')).value.trim();
    if (!email) {
      _showError(t('auth.forgot.error.empty_email'));
      btn.disabled = false;
      btn.textContent = t('auth.forgot.submit');
      return;
    }

    try {
      await requestPasswordReset(email);
      form.classList.add('hidden');
      success.classList.remove('hidden');
    } catch {
      _showError(t('auth.error.network'));
      btn.disabled = false;
      btn.textContent = t('auth.forgot.submit');
    }
  });
}

/** @param {HTMLElement} container */
export function destroy(container) {
  container.innerHTML = '';
}
