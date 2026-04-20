// @ts-check
// Login page — username/password form, submits as JSON to /rest/auth/login.

import { iconX } from '../icons.js';

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
          <span aria-hidden="true" class="shrink-0 [&_svg]:w-4 [&_svg]:h-4">${iconX}</span>
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
      </div>
    </div>
  `;

  const form   = /** @type {HTMLFormElement}   */ (container.querySelector('#login-form'));
  const btn    = /** @type {HTMLButtonElement} */ (container.querySelector('#login-submit'));
  const errBox = /** @type {HTMLElement}       */ (container.querySelector('#login-error'));
  const errMsg = /** @type {HTMLElement}       */ (container.querySelector('#login-error-msg'));

  form.addEventListener('submit', async (e) => {
    e.preventDefault();
    errBox.classList.add('hidden');
    errBox.classList.remove('flex');
    btn.disabled = true;
    btn.textContent = 'Signing in…';

    const username = /** @type {HTMLInputElement} */ (container.querySelector('#login-username')).value;
    const password = /** @type {HTMLInputElement} */ (container.querySelector('#login-password')).value;

    try {
      const res = await fetch('/rest/auth/login', {
        method: 'POST',
        credentials: 'include',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ username, password }),
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
    } catch {
      errMsg.textContent = 'Could not reach the server. Please try again.';
      errBox.classList.remove('hidden');
      errBox.classList.add('flex');
    } finally {
      btn.disabled = false;
      btn.textContent = 'Sign in';
    }
  });
}

/** @param {HTMLElement} container */
export function destroy(container) {
  container.innerHTML = '';
}
