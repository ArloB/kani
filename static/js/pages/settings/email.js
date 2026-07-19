// @ts-check
// Settings — Email / SMTP section.

import * as api from '../../api.js';
import { showToast, showApiError } from '../../components/toast.js';
import { mkSettingsGroup, mkSettingsGroupCard, mkSettingsRow, mkToggleRow } from './_shared.js';
import { t } from '../../i18n.js';

/**
 * @param {HTMLElement} el
 * @param {any} settings
 */
export function mount(el, settings) {
  // Parse stored provider config
  let _config = {};
  try { _config = JSON.parse(settings?.email_provider_config ?? '{}'); } catch { _config = {}; }

  let _dirty = false;
  const _mark = () => { _dirty = true; };

  // ── General group ─────────────────────────────────────────────────────────
  const generalGroup = mkSettingsGroup(t('settings.email.general.group'));
  const generalCard  = mkSettingsGroupCard(generalGroup);

  let _emailEnabled = settings?.email_enabled ?? false;
  generalCard.appendChild(mkToggleRow({
    label: t('settings.email.general.enable.label'),
    description: t('settings.email.general.enable.desc'),
    checked: _emailEnabled,
    onChange: v => { _emailEnabled = v; _mark(); },
  }));

  const fromInput = document.createElement('input');
  fromInput.type = 'email';
  fromInput.className = 'input w-56 text-sm';
  fromInput.placeholder = 'noreply@example.com';
  fromInput.value = settings?.email_from_address ?? '';
  fromInput.addEventListener('input', _mark);
  generalCard.appendChild(mkSettingsRow({ label: t('settings.email.general.from.label'), description: t('settings.email.general.from.desc'), control: fromInput }));

  const appUrlInput = document.createElement('input');
  appUrlInput.type = 'url';
  appUrlInput.className = 'input w-56 text-sm';
  appUrlInput.placeholder = 'https://kani.example.com';
  appUrlInput.value = settings?.app_url ?? '';
  appUrlInput.addEventListener('input', _mark);
  generalCard.appendChild(mkSettingsRow({ label: t('settings.email.general.app_url.label'), description: t('settings.email.general.app_url.desc'), control: appUrlInput }));

  el.appendChild(generalGroup);

  // ── Feature toggles group ─────────────────────────────────────────────────
  const featGroup = mkSettingsGroup(t('settings.email.features.group'));
  const featCard  = mkSettingsGroupCard(featGroup);

  let _resetEnabled = settings?.password_reset_enabled ?? true;
  featCard.appendChild(mkToggleRow({
    label: t('settings.email.features.reset.label'),
    description: t('settings.email.features.reset.desc'),
    checked: _resetEnabled,
    onChange: v => { _resetEnabled = v; _mark(); },
  }));

  let _verifyRequired = settings?.email_verification_required ?? false;
  featCard.appendChild(mkToggleRow({
    label: t('settings.email.features.verify.label'),
    description: t('settings.email.features.verify.desc'),
    checked: _verifyRequired,
    onChange: v => { _verifyRequired = v; _mark(); },
  }));

  el.appendChild(featGroup);

  // ── SMTP provider group ───────────────────────────────────────────────────
  const smtpGroup = mkSettingsGroup(t('settings.email.smtp.group'));
  const smtpCard  = mkSettingsGroupCard(smtpGroup);

  const disclaimer = document.createElement('p');
  disclaimer.className = 'text-xs text-text-muted px-1';
  disclaimer.textContent = t('settings.email.smtp.disclaimer');
  smtpGroup.insertBefore(disclaimer, smtpCard);

  /** @param {string} key @param {string} def */
  const _cfg = (key, def = '') => _config[key] ?? def;

  const hostInput = document.createElement('input');
  hostInput.type = 'text';
  hostInput.className = 'input w-56 text-sm';
  hostInput.placeholder = 'smtp.example.com';
  hostInput.value = _cfg('host');
  hostInput.addEventListener('input', _mark);
  smtpCard.appendChild(mkSettingsRow({ label: t('settings.email.smtp.host.label'), description: t('settings.email.smtp.host.desc'), control: hostInput }));

  const portInput = document.createElement('input');
  portInput.type = 'number';
  portInput.className = 'input w-24 text-sm';
  portInput.min = '1';
  portInput.max = '65535';
  portInput.value = String(_cfg('port', '587'));
  portInput.addEventListener('input', _mark);
  smtpCard.appendChild(mkSettingsRow({ label: t('settings.email.smtp.port.label'), control: portInput }));

  const tlsSelect = document.createElement('select');
  tlsSelect.className = 'input w-32 text-sm';
  for (const [val, lbl] of [['starttls', 'STARTTLS'], ['tls', 'TLS'], ['none', 'None']]) {
    const opt = document.createElement('option');
    opt.value = val;
    opt.textContent = lbl;
    opt.selected = _cfg('tls_mode', 'starttls') === val;
    tlsSelect.appendChild(opt);
  }
  tlsSelect.addEventListener('change', _mark);
  smtpCard.appendChild(mkSettingsRow({ label: t('settings.email.smtp.tls.label'), control: tlsSelect }));

  const userInput = document.createElement('input');
  userInput.type = 'text';
  userInput.className = 'input w-56 text-sm';
  userInput.autocomplete = 'off';
  userInput.placeholder = 'username (optional)';
  userInput.value = _cfg('username');
  userInput.addEventListener('input', _mark);
  smtpCard.appendChild(mkSettingsRow({ label: t('settings.email.smtp.username.label'), control: userInput }));

  const pwInput = document.createElement('input');
  pwInput.type = 'password';
  pwInput.className = 'input w-56 text-sm';
  pwInput.autocomplete = 'new-password';
  pwInput.placeholder = 'password (optional)';
  pwInput.value = _cfg('password');
  pwInput.addEventListener('input', _mark);
  smtpCard.appendChild(mkSettingsRow({ label: t('settings.email.smtp.password.label'), control: pwInput }));

  el.appendChild(smtpGroup);

  // ── Test row ──────────────────────────────────────────────────────────────
  const actionsGroup = mkSettingsGroup();
  const actionsCard  = mkSettingsGroupCard(actionsGroup);

  const testInput = document.createElement('input');
  testInput.type = 'email';
  testInput.className = 'input w-40 text-sm';
  testInput.placeholder = 'recipient@example.com';

  const testBtn = document.createElement('button');
  testBtn.type = 'button';
  testBtn.className = 'btn-ghost btn-sm';
  testBtn.textContent = t('settings.email.test.btn');

  const testWrap = document.createElement('div');
  testWrap.className = 'flex items-center gap-2';
  testWrap.appendChild(testInput);
  testWrap.appendChild(testBtn);

  actionsCard.appendChild(mkSettingsRow({ label: t('settings.email.test.label'), description: t('settings.email.test.desc'), control: testWrap }));

  el.appendChild(actionsGroup);

  // ── Event handlers ────────────────────────────────────────────────────────

  async function _save() {
    const providerConfig = JSON.stringify({
      host: hostInput.value.trim(),
      port: Number(portInput.value) || 587,
      tls_mode: tlsSelect.value,
      username: userInput.value,
      password: pwInput.value,
    });
    await api.updateSettings({
      Email: {
        email_enabled: _emailEnabled,
        email_provider: 'smtp',
        email_provider_config: providerConfig,
        email_from_address: fromInput.value.trim(),
        app_url: appUrlInput.value.trim(),
        password_reset_enabled: _resetEnabled,
        email_verification_required: _verifyRequired,
      },
    });
    _dirty = false;
    showToast(t('common.saved'), { type: 'success' });
  }

  testBtn.addEventListener('click', async () => {
    const to = testInput.value.trim();
    if (!to) { showToast(t('settings.email.test.no_recipient'), { type: 'error' }); return; }
    testBtn.disabled = true;
    testBtn.textContent = t('common.sending');
    try {
      await api.sendTestEmail(to);
      showToast(t('settings.email.test.success'), { type: 'success' });
    } catch (e) {
      showApiError(e);
    } finally {
      testBtn.disabled = false;
      testBtn.textContent = t('settings.email.test.btn');
    }
  });

  return {
    destroy() { el.innerHTML = ''; },
    isDirty() { return _dirty; },
    save: _save,
  };
}
