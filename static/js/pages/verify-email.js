// @ts-check
// Email verification page — automatically verifies the token from the URL.

import { verifyEmail, resendVerification } from '../api.js';

/** @param {HTMLElement} container */
export async function init(container) {
  document.title = 'Verify Email - Kani';

  const token = new URLSearchParams(location.search).get('token') ?? '';

  container.innerHTML = `
    <div class="min-h-screen flex items-center justify-center p-4 bg-bg">
      <div class="w-full max-w-sm bg-surface rounded-2xl shadow-lg border border-border p-8 flex flex-col gap-6 text-center">
        <h1 class="text-2xl font-bold text-text">Email verification</h1>
        <p class="text-sm text-text-muted" id="ve-status">Verifying…</p>
        <div id="ve-resend" class="hidden flex-col gap-3">
          <button type="button" class="btn-primary w-full h-11" id="ve-resend-btn">Resend verification email</button>
          <p class="text-xs text-text-muted" id="ve-resend-msg"></p>
        </div>
        <a href="/" class="text-sm text-accent hover:underline">Go to library</a>
      </div>
    </div>
  `;

  const statusEl  = /** @type {HTMLElement}       */ (container.querySelector('#ve-status'));
  const resendDiv = /** @type {HTMLElement}       */ (container.querySelector('#ve-resend'));
  const resendBtn = /** @type {HTMLButtonElement} */ (container.querySelector('#ve-resend-btn'));
  const resendMsg = /** @type {HTMLElement}       */ (container.querySelector('#ve-resend-msg'));

  if (!token) {
    statusEl.textContent = 'Invalid or missing verification link.';
    resendDiv.classList.remove('hidden');
    resendDiv.classList.add('flex');
    return;
  }

  try {
    await verifyEmail(token);
    statusEl.textContent = 'Your email has been verified.';
  } catch (/** @type {any} */ err) {
    statusEl.textContent = err?.message ?? 'Verification failed. The link may have expired.';
    resendDiv.classList.remove('hidden');
    resendDiv.classList.add('flex');
  }

  resendBtn.addEventListener('click', async () => {
    resendBtn.disabled = true;
    resendBtn.textContent = 'Sending…';
    try {
      await resendVerification();
      resendMsg.textContent = 'Verification email sent. Check your inbox.';
    } catch {
      resendMsg.textContent = 'Failed to resend. Please try again later.';
    } finally {
      resendBtn.disabled = false;
      resendBtn.textContent = 'Resend verification email';
    }
  });
}

/** @param {HTMLElement} container */
export function destroy(container) {
  container.innerHTML = '';
}
