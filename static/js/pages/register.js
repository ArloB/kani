// @ts-check
// Registration page — public account creation with math captcha.

import { h, render } from 'preact';
import { useState, useEffect } from 'preact/hooks';
import htm from 'htm';
import { navigate } from '../router.js';
import { AuthCard, AuthError, AuthField } from '../components/auth-card.js';
import { PasswordStrength } from '../components/password-strength.js';
import { useBusy } from '../hooks/use-busy.js';
import { t } from '../i18n.js';
const html = htm.bind(h);

function RegisterPage() {
  const [username, setUsername] = useState('');
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [captchaAnswer, setCaptchaAnswer] = useState('');
  const [captcha, setCaptcha] = useState({ id: '', prompt: '' });
  const [error, setError] = useState('');
  const [ready, setReady] = useState(false);
  const { busy, run } = useBusy();

  async function loadCaptcha() {
    try {
      const res = await fetch('/rest/auth/captcha', { credentials: 'include' });
      const data = await res.json();
      setCaptcha({ id: data.id ?? '', prompt: data.prompt ?? '' });
    } catch { /* ignore */ }
  }

  useEffect(() => {
    fetch('/rest/auth/registration-enabled', { credentials: 'include' })
      .then(res => res.json().catch(() => ({})))
      .then(data => { data?.enabled ? setReady(true) : navigate('/login'); })
      .catch(() => navigate('/login'));
    loadCaptcha();
  }, []);

  const submit = (/** @type {Event} */ e) => {
    e.preventDefault();
    run(async () => {
      setError('');
      const ctrl = new AbortController();
      const timer = setTimeout(() => ctrl.abort(new DOMException('Request timed out', 'TimeoutError')), 15_000);
      try {
        const res = await fetch('/rest/auth/register', {
          method: 'POST',
          credentials: 'include',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            username: username.trim(),
            email: email.trim(),
            password,
            captcha_id: captcha.id,
            captcha_answer: Number(captchaAnswer),
          }),
          signal: ctrl.signal,
        });
        if (res.ok) {
          navigate('/login');
          return;
        }
        const data = await res.json().catch(() => ({}));
        setError(data.error ?? t('auth.register.error.failed'));
        await loadCaptcha();
      } catch (/** @type {any} */ err) {
        setError(err?.name === 'TimeoutError'
          ? t('auth.error.server_slow')
          : t('auth.error.network'));
      } finally {
        clearTimeout(timer);
      }
    });
  };

  if (!ready) return null;

  return html`
    <${AuthCard} title="Kani" subtitle=${t('auth.register.subtitle')}>
      <${AuthError} message=${error} id="reg-error" />
      <form class="flex flex-col gap-4" novalidate onSubmit=${submit}>
        <${AuthField}
          id="reg-username"
          label=${t('auth.login.username')}
          value=${username}
          onInput=${setUsername}
          autocomplete="username"
          required=${true}
          autofocus=${true}
        />
        <${AuthField}
          id="reg-email"
          label=${t('auth.register.email')}
          type="email"
          value=${email}
          onInput=${setEmail}
          autocomplete="email"
        />
        <div>
          <${AuthField}
            id="reg-password"
            label=${t('auth.login.password')}
            type="password"
            value=${password}
            onInput=${setPassword}
            autocomplete="new-password"
            required=${true}
          />
          <${PasswordStrength} password=${password} identity=${username} />
        </div>
        <${AuthField}
          id="reg-captcha"
          label=${captcha.prompt || t('common.loading')}
          type="number"
          inputMode="numeric"
          value=${captchaAnswer}
          onInput=${setCaptchaAnswer}
          required=${true}
        />
        <button type="submit" class="btn-primary w-full h-11 mt-2" disabled=${busy}>
          ${busy ? t('auth.register.submitting') : t('auth.register.submit')}
        </button>
      </form>
      <p class="text-center text-sm text-text-muted">
        ${t('auth.register.have_account')} <a href="/login" class="text-text-muted underline hover:text-text">${t('auth.register.sign_in')}</a>
      </p>
    </${AuthCard}>
  `;
}

/** @param {HTMLElement} container */
export function init(container) {
  document.title = t('auth.register.page_title');
  render(html`<${RegisterPage} />`, container);
}

/** @param {HTMLElement} container */
export function destroy(container) {
  render(null, container);
  container.innerHTML = '';
}
