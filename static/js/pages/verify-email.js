// @ts-check
// Email verification page — automatically verifies the token from the URL.

import { verifyEmail, resendVerification } from '../api.js';
import { t } from '../i18n.js';

/** @param {HTMLElement} container */
export async function init(container) {
  document.title = t('auth.verify.page_title');

  const token = new URLSearchParams(location.search).get('token') ?? '';

  container.innerHTML = `
    <div class="min-h-screen flex items-center justify-center p-4 bg-bg">
      <div class="w-full max-w-sm bg-surface rounded-2xl shadow-lg border border-border p-8 flex flex-col gap-6 text-center">
        <h1 class="text-2xl font-bold text-text">${t('auth.verify.title')}</h1>
        <p class="text-sm text-text-muted" id="ve-status">${t('auth.verify.verifying')}</p>
        <div id="ve-resend" class="hidden flex-col gap-3">
          <button type="button" class="btn-primary w-full h-11" id="ve-resend-btn">${t('auth.verify.resend')}</button>
          <p class="text-xs text-text-muted" id="ve-resend-msg"></p>
        </div>
        <a href="/" class="text-sm text-accent hover:underline">${t('auth.verify.go_library')}</a>
      </div>
    </div>
  `;

  const statusEl  = /** @type {HTMLElement}       */ (container.querySelector('#ve-status'));
  const resendDiv = /** @type {HTMLElement}       */ (container.querySelector('#ve-resend'));
  const resendBtn = /** @type {HTMLButtonElement} */ (container.querySelector('#ve-resend-btn'));
  const resendMsg = /** @type {HTMLElement}       */ (container.querySelector('#ve-resend-msg'));

  if (!token) {
    statusEl.textContent = t('auth.verify.error.invalid');
    resendDiv.classList.remove('hidden');
    resendDiv.classList.add('flex');
    return;
  }

  try {
    await verifyEmail(token);
    statusEl.textContent = t('auth.verify.success');
  } catch (/** @type {any} */ err) {
    statusEl.textContent = err?.message ?? t('auth.verify.error.failed');
    resendDiv.classList.remove('hidden');
    resendDiv.classList.add('flex');
  }

  resendBtn.addEventListener('click', async () => {
    resendBtn.disabled = true;
    resendBtn.textContent = t('auth.verify.resend.sending');
    try {
      await resendVerification();
      resendMsg.textContent = t('auth.verify.resend.success');
    } catch {
      resendMsg.textContent = t('auth.verify.resend.failed');
    } finally {
      resendBtn.disabled = false;
      resendBtn.textContent = t('auth.verify.resend');
    }
  });
}

/** @param {HTMLElement} container */
export function destroy(container) {
  container.innerHTML = '';
}
