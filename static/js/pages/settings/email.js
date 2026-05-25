// @ts-check
// Settings — Email / SMTP section.

import * as api from '../../api.js';
import { mkSettingsGroup, mkSettingsGroupCard, mkSettingsRow, mkToggleRow, showResult } from './_shared.js';

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
  const generalGroup = mkSettingsGroup('General');
  const generalCard  = mkSettingsGroupCard(generalGroup);

  let _emailEnabled = settings?.email_enabled ?? false;
  generalCard.appendChild(mkToggleRow({
    label: 'Enable email',
    description: 'Required for password reset and email verification.',
    checked: _emailEnabled,
    onChange: v => { _emailEnabled = v; _mark(); },
  }));

  const fromInput = document.createElement('input');
  fromInput.type = 'email';
  fromInput.className = 'input w-56 text-sm';
  fromInput.placeholder = 'noreply@example.com';
  fromInput.value = settings?.email_from_address ?? '';
  fromInput.addEventListener('input', _mark);
  generalCard.appendChild(mkSettingsRow({ label: 'From address', description: 'Address emails are sent from.', control: fromInput }));

  const appUrlInput = document.createElement('input');
  appUrlInput.type = 'url';
  appUrlInput.className = 'input w-56 text-sm';
  appUrlInput.placeholder = 'https://kani.example.com';
  appUrlInput.value = settings?.app_url ?? '';
  appUrlInput.addEventListener('input', _mark);
  generalCard.appendChild(mkSettingsRow({ label: 'App URL', description: 'Used to build links in emails (e.g. reset link).', control: appUrlInput }));

  el.appendChild(generalGroup);

  // ── Feature toggles group ─────────────────────────────────────────────────
  const featGroup = mkSettingsGroup('Features');
  const featCard  = mkSettingsGroupCard(featGroup);

  let _resetEnabled = settings?.password_reset_enabled ?? true;
  featCard.appendChild(mkToggleRow({
    label: 'Password reset',
    description: 'Allow users to reset their password via email.',
    checked: _resetEnabled,
    onChange: v => { _resetEnabled = v; _mark(); },
  }));

  let _verifyRequired = settings?.email_verification_required ?? false;
  featCard.appendChild(mkToggleRow({
    label: 'Require email verification',
    description: 'New accounts must verify their email before signing in.',
    checked: _verifyRequired,
    onChange: v => { _verifyRequired = v; _mark(); },
  }));

  el.appendChild(featGroup);

  // ── SMTP provider group ───────────────────────────────────────────────────
  const smtpGroup = mkSettingsGroup('SMTP');
  const smtpCard  = mkSettingsGroupCard(smtpGroup);

  const disclaimer = document.createElement('p');
  disclaimer.className = 'text-xs text-text-muted px-1';
  disclaimer.textContent = 'SMTP credentials are stored in the database without encryption. Ensure the server and database file are adequately protected at the OS level.';
  smtpGroup.insertBefore(disclaimer, smtpCard);

  /** @param {string} key @param {string} def */
  const _cfg = (key, def = '') => _config[key] ?? def;

  const hostInput = document.createElement('input');
  hostInput.type = 'text';
  hostInput.className = 'input w-56 text-sm';
  hostInput.placeholder = 'smtp.example.com';
  hostInput.value = _cfg('host');
  hostInput.addEventListener('input', _mark);
  smtpCard.appendChild(mkSettingsRow({ label: 'Host', description: 'SMTP server hostname.', control: hostInput }));

  const portInput = document.createElement('input');
  portInput.type = 'number';
  portInput.className = 'input w-24 text-sm';
  portInput.min = '1';
  portInput.max = '65535';
  portInput.value = String(_cfg('port', '587'));
  portInput.addEventListener('input', _mark);
  smtpCard.appendChild(mkSettingsRow({ label: 'Port', control: portInput }));

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
  smtpCard.appendChild(mkSettingsRow({ label: 'TLS mode', control: tlsSelect }));

  const userInput = document.createElement('input');
  userInput.type = 'text';
  userInput.className = 'input w-56 text-sm';
  userInput.autocomplete = 'off';
  userInput.placeholder = 'username (optional)';
  userInput.value = _cfg('username');
  userInput.addEventListener('input', _mark);
  smtpCard.appendChild(mkSettingsRow({ label: 'Username', control: userInput }));

  const pwInput = document.createElement('input');
  pwInput.type = 'password';
  pwInput.className = 'input w-56 text-sm';
  pwInput.autocomplete = 'new-password';
  pwInput.placeholder = 'password (optional)';
  pwInput.value = _cfg('password');
  pwInput.addEventListener('input', _mark);
  smtpCard.appendChild(mkSettingsRow({ label: 'Password', control: pwInput }));

  el.appendChild(smtpGroup);

  // ── Save / test row ───────────────────────────────────────────────────────
  const actionsGroup = mkSettingsGroup();
  const actionsCard  = mkSettingsGroupCard(actionsGroup);

  const saveBtn = document.createElement('button');
  saveBtn.type = 'button';
  saveBtn.className = 'btn-primary btn-sm';
  saveBtn.textContent = 'Save';

  const saveStatus = document.createElement('span');
  saveStatus.className = 'text-xs text-text-muted hidden';

  const saveWrap = document.createElement('div');
  saveWrap.className = 'flex items-center gap-3';
  saveWrap.appendChild(saveBtn);
  saveWrap.appendChild(saveStatus);

  actionsCard.appendChild(mkSettingsRow({ label: 'Email settings', description: 'Save all email configuration above.', control: saveWrap }));

  // Test email row
  const testInput = document.createElement('input');
  testInput.type = 'email';
  testInput.className = 'input w-40 text-sm';
  testInput.placeholder = 'recipient@example.com';

  const testBtn = document.createElement('button');
  testBtn.type = 'button';
  testBtn.className = 'btn-ghost btn-sm';
  testBtn.textContent = 'Send test';

  const testStatus = document.createElement('span');
  testStatus.className = 'text-xs hidden';

  const testWrap = document.createElement('div');
  testWrap.className = 'flex items-center gap-2';
  testWrap.appendChild(testInput);
  testWrap.appendChild(testBtn);
  testWrap.appendChild(testStatus);

  actionsCard.appendChild(mkSettingsRow({ label: 'Test email', description: 'Send a test message to verify your configuration.', control: testWrap }));

  el.appendChild(actionsGroup);

  // ── Event handlers ────────────────────────────────────────────────────────

  saveBtn.addEventListener('click', async () => {
    saveBtn.disabled = true;
    const providerConfig = JSON.stringify({
      host: hostInput.value.trim(),
      port: Number(portInput.value) || 587,
      tls_mode: tlsSelect.value,
      username: userInput.value,
      password: pwInput.value,
    });
    try {
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
      showResult(saveStatus, true, 'Saved');
    } catch (/** @type {any} */ e) {
      showResult(saveStatus, false, e?.message ?? 'Save failed');
    } finally {
      saveBtn.disabled = false;
    }
  });

  testBtn.addEventListener('click', async () => {
    const to = testInput.value.trim();
    if (!to) { showResult(testStatus, false, 'Enter a recipient'); return; }
    testBtn.disabled = true;
    testBtn.textContent = 'Sending…';
    try {
      await api.sendTestEmail(to);
      showResult(testStatus, true, 'Sent!');
    } catch (/** @type {any} */ e) {
      showResult(testStatus, false, e?.message ?? 'Failed');
    } finally {
      testBtn.disabled = false;
      testBtn.textContent = 'Send test';
    }
  });

  return {
    destroy() { el.innerHTML = ''; },
    isDirty() { return _dirty; },
  };
}
