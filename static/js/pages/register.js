// @ts-check
// Registration page — public account creation with math captcha.

import { iconX } from '../icons.js';
import { navigate } from '../router.js';

/** @param {HTMLElement} container */
export async function init(container) {
  document.title = 'Create Account - Kani';

  // Pre-flight: redirect if registration is disabled
  try {
    const res = await fetch('/rest/auth/registration-enabled', { credentials: 'include' });
    const data = await res.json().catch(() => ({}));
    if (!data?.enabled) { navigate('/login'); return; }
  } catch {
    navigate('/login'); return;
  }

  let captchaId = '';
  let captchaPrompt = '';

  async function _loadCaptcha() {
    try {
      const res = await fetch('/rest/auth/captcha', { credentials: 'include' });
      const data = await res.json();
      captchaId = data.id ?? '';
      captchaPrompt = data.prompt ?? '';
      const promptEl = container.querySelector('#reg-captcha-prompt');
      if (promptEl) promptEl.textContent = captchaPrompt;
    } catch { /* ignore */ }
  }

  container.innerHTML = `
    <div class="min-h-screen flex items-center justify-center p-4 bg-bg">
      <div class="w-full max-w-sm bg-surface rounded-2xl shadow-lg border border-border p-8 flex flex-col gap-6">
        <div class="text-center flex flex-col gap-1">
          <h1 class="text-2xl font-bold text-text">Kani</h1>
          <p class="text-sm text-text-muted">Create an account</p>
        </div>

        <div
          class="hidden items-center gap-2 px-3 py-2.5 rounded-lg bg-danger/10 border border-danger/30 text-sm text-danger"
          role="alert"
          aria-live="assertive"
          id="reg-error"
        >
          <span aria-hidden="true" class="shrink-0 icon-sm">${iconX}</span>
          <span id="reg-error-msg"></span>
        </div>

        <form class="flex flex-col gap-4" id="reg-form" novalidate>
          <div class="flex flex-col gap-1.5">
            <label class="text-sm font-medium text-text" for="reg-username">Username</label>
            <input id="reg-username" class="input" type="text" name="username" autocomplete="username" required autofocus />
          </div>
          <div class="flex flex-col gap-1.5">
            <label class="text-sm font-medium text-text" for="reg-email">Email</label>
            <input id="reg-email" class="input" type="email" name="email" autocomplete="email" />
          </div>
          <div class="flex flex-col gap-1.5">
            <label class="text-sm font-medium text-text" for="reg-password">Password</label>
            <input id="reg-password" class="input" type="password" name="password" autocomplete="new-password" required />
          </div>
          <div class="flex flex-col gap-1.5">
            <label class="text-sm font-medium text-text" for="reg-captcha">
              <span id="reg-captcha-prompt">Loading…</span>
            </label>
            <input id="reg-captcha" class="input" type="number" name="captcha" required />
          </div>
          <button type="submit" class="btn-primary w-full h-11 mt-2" id="reg-submit">Create account</button>
        </form>

        <p class="text-center text-sm text-text-muted">
          Already have an account? <a href="/login" class="text-accent hover:underline">Sign in</a>
        </p>
      </div>
    </div>
  `;

  await _loadCaptcha();

  const form   = /** @type {HTMLFormElement}   */ (container.querySelector('#reg-form'));
  const btn    = /** @type {HTMLButtonElement} */ (container.querySelector('#reg-submit'));
  const errBox = /** @type {HTMLElement}       */ (container.querySelector('#reg-error'));
  const errMsg = /** @type {HTMLElement}       */ (container.querySelector('#reg-error-msg'));

  function _showError(msg) {
    errMsg.textContent = msg;
    errBox.classList.remove('hidden');
    errBox.classList.add('flex');
  }

  form.addEventListener('submit', async (e) => {
    e.preventDefault();
    errBox.classList.add('hidden');
    errBox.classList.remove('flex');
    btn.disabled = true;
    btn.textContent = 'Creating…';

    const username = /** @type {HTMLInputElement} */ (container.querySelector('#reg-username')).value.trim();
    const email    = /** @type {HTMLInputElement} */ (container.querySelector('#reg-email')).value.trim();
    const password = /** @type {HTMLInputElement} */ (container.querySelector('#reg-password')).value;
    const captcha  = Number(/** @type {HTMLInputElement} */ (container.querySelector('#reg-captcha')).value);

    try {
      const res = await fetch('/rest/auth/register', {
        method: 'POST',
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ username, email, password, captcha_id: captchaId, captcha_answer: captcha }),
      });

      if (res.ok) {
        navigate('/login');
        return;
      }

      const data = await res.json().catch(() => ({}));
      _showError(data.error ?? 'Registration failed. Please try again.');
      await _loadCaptcha();
    } catch {
      _showError('Could not reach the server. Please try again.');
    } finally {
      btn.disabled = false;
      btn.textContent = 'Create account';
    }
  });
}

/** @param {HTMLElement} container */
export function destroy(container) {
  container.innerHTML = '';
}
