// @ts-check
// Login page — username/password form, submits as JSON to /rest/auth/login.

import { iconX } from '../icons.js';
import { getPasswordResetEnabled, getRegistrationEnabled } from '../api.js';

/** @param {HTMLElement} container */
export function init(container) {
  document.title = 'Login - Kani';

  container.innerHTML = `
    <div class="min-h-screen flex items-center justify-center p-4 bg-bg">
      <div class="w-full max-w-sm bg-surface rounded-2xl shadow-lg border border-border p-8 flex flex-col gap-6">
        <div class="text-center flex flex-col gap-1">
          <h1 class="text-2xl font-bold text-text">Kani</h1>
          <p class="text-sm text-text-muted">Sign in to continue</p>
        </div>

        <div
          class="hidden items-center gap-2 px-3 py-2.5 rounded-lg bg-danger/10 border border-danger/30 text-sm text-danger"
          role="alert"
          aria-live="assertive"
          id="login-error"
        >
          <span aria-hidden="true" class="shrink-0 icon-sm">${iconX}</span>
          <span id="login-error-msg"></span>
        </div>

        <form class="flex flex-col gap-4" id="login-form" novalidate>
          <div class="flex flex-col gap-1.5">
            <label class="text-sm font-medium text-text" for="login-username">Username</label>
            <input
              id="login-username"
              class="input"
              type="text"
              name="username"
              autocomplete="username"
              required
              autofocus
              aria-describedby="login-error"
            />
          </div>
          <div class="flex flex-col gap-1.5">
            <label class="text-sm font-medium text-text" for="login-password">Password</label>
            <input
              id="login-password"
              class="input"
              type="password"
              name="password"
              autocomplete="current-password"
              required
              aria-describedby="login-error"
            />
          </div>
          <button type="submit" class="btn-primary w-full h-11 mt-2" id="login-submit">Sign in</button>
        </form>
        <p class="text-center text-sm text-text-muted hidden" id="login-forgot-link">
          <a href="/forgot-password" class="text-accent hover:underline">Forgot password?</a>
        </p>
        <p class="text-center text-sm text-text-muted hidden" id="login-register-link">
          Don't have an account? <a href="/register" class="text-accent hover:underline">Create one</a>
        </p>
      </div>
    </div>
  `;

  const form   = /** @type {HTMLFormElement}   */ (container.querySelector('#login-form'));
  const btn    = /** @type {HTMLButtonElement} */ (container.querySelector('#login-submit'));
  const errBox = /** @type {HTMLElement}       */ (container.querySelector('#login-error'));
  const errMsg = /** @type {HTMLElement}       */ (container.querySelector('#login-error-msg'));

  // Show register link if registration is enabled
  getRegistrationEnabled()
    .then(d => {
      if (d?.enabled) {
        container.querySelector('#login-register-link')?.classList.remove('hidden');
      }
    })
    .catch(() => {});

  // Show forgot password link if password reset is enabled
  getPasswordResetEnabled()
    .then(d => {
      if (d?.enabled) {
        container.querySelector('#login-forgot-link')?.classList.remove('hidden');
      }
    })
    .catch(() => {});

  form.addEventListener('submit', async (e) => {
    e.preventDefault();
    errBox.classList.add('hidden');
    errBox.classList.remove('flex');
    btn.disabled = true;
    btn.textContent = 'Signing in…';

    const username = /** @type {HTMLInputElement} */ (container.querySelector('#login-username')).value;
    const password = /** @type {HTMLInputElement} */ (container.querySelector('#login-password')).value;

    const ctrl = new AbortController();
    const timer = setTimeout(() => ctrl.abort(new DOMException('Request timed out', 'TimeoutError')), 15_000);
    try {
      const res = await fetch('/rest/auth/login', {
        method: 'POST',
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ username, password }),
        signal: ctrl.signal,
      });

      if (res.ok) {
        window.location.href = '/';
        return;
      }

      const data = await res.json().catch(() => ({}));
      const msg = res.status === 401
        ? 'Invalid username or password.'
        : (data.error ?? 'Something went wrong. Please try again.');

      errMsg.textContent = msg;
      errBox.classList.remove('hidden');
      errBox.classList.add('flex');
    } catch (/** @type {any} */ err) {
      errMsg.textContent = err?.name === 'TimeoutError'
        ? 'Server is taking too long to respond. Please try again.'
        : 'Could not reach the server. Please try again.';
      errBox.classList.remove('hidden');
      errBox.classList.add('flex');
    } finally {
      clearTimeout(timer);
      btn.disabled = false;
      btn.textContent = 'Sign in';
    }
  });
}

/** @param {HTMLElement} container */
export function destroy(container) {
  container.innerHTML = '';
}
