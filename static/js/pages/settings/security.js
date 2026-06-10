// @ts-check
// Settings — Security tab: 2FA/TOTP, session inventory, security status.

import { h, Fragment } from 'preact';
import { useState, useEffect } from 'preact/hooks';
import htm from 'htm';
import * as api from '../../api.js';
import { mkSettingsGroup, mkSettingsGroupCard, mkSettingsRow } from './_shared.js';
import { SessionList } from '../../components/session-list.js';
import { TotpWizard } from '../../components/totp-wizard.js';

const html = htm.bind(h);

/** @param {HTMLElement} el */
export function mount(el) {
  const destroys = [];

  // ── 2FA section ────────────────────────────────────────────────────────────
  const twoFaGroup = mkSettingsGroup('Two-Factor Authentication');
  const twoFaCard  = mkSettingsGroupCard(twoFaGroup);
  el.appendChild(twoFaGroup);

  // Mount TOTP status row (Preact for reactive state)
  const totpEl = document.createElement('div');
  twoFaCard.appendChild(totpEl);
  // Mount Preact TOTP status row directly into the card element
  import('preact').then(({ render }) => {
    render(html`<${TotpStatusRow} />`, totpEl);
    destroys.push(() => render(null, totpEl));
  });
  if (unmountTotp) destroys.push(unmountTotp);

  // ── Session inventory section ───────────────────────────────────────────────
  const sessGroup = mkSettingsGroup('Active Sessions');
  const sessCard  = mkSettingsGroupCard(sessGroup);
  el.appendChild(sessGroup);

  const sessEl = document.createElement('div');
  sessCard.appendChild(sessEl);
  import('preact').then(({ render }) => {
    render(html`<${SessionList} />`, sessEl);
    destroys.push(() => render(null, sessEl));
  });
  if (unmountSessions) destroys.push(unmountSessions);

  // ── Security status section ─────────────────────────────────────────────────
  const statusGroup = mkSettingsGroup('Security Status');
  const statusCard  = mkSettingsGroupCard(statusGroup);
  el.appendChild(statusGroup);

  api.getFeatures().then(features => {
    // Public instance badge
    if (features?.public_instance) {
      const badge = document.createElement('div');
      badge.className = 'px-3 py-2 rounded bg-accent/10 text-accent text-sm font-medium';
      badge.textContent = 'Public instance mode is active — hardened security profile enabled.';
      statusCard.appendChild(badge);
    }
    statusCard.appendChild(mkSettingsRow({
      label: 'HTTPS mode',
      description: features?.public_instance
        ? 'Secure cookies are enforced in public instance mode.'
        : 'Set KANI_SECURE_COOKIES=true and use a TLS-terminating reverse proxy.',
    }));
    statusCard.appendChild(mkSettingsRow({
      label: 'Security headers',
      description: 'X-Content-Type-Options, X-Frame-Options, Referrer-Policy, Permissions-Policy, and CSP are active.',
    }));
  }).catch(() => {});

  return {
    destroy() {
      destroys.forEach(d => d?.());
    },
  };
}

// ── TOTP status row ─────────────────────────────────────────────────────────

function TotpStatusRow() {
  const [enabled, setEnabled] = useState(/** @type {boolean|null} */ (null));
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    api.getFeatures()
      .then(f => { setEnabled(!!f?.totp_enabled); setLoading(false); })
      .catch(() => { setEnabled(false); setLoading(false); });
  }, []);

  if (loading) {
    return html`<div class="px-3 py-3 text-sm text-text-muted">Loading…</div>`;
  }

  if (enabled) {
    return html`
      <div class="flex items-center justify-between gap-3 px-3 py-3">
        <div>
          <div class="text-sm font-medium text-text">Two-factor authentication</div>
          <div class="text-xs text-success mt-0.5">Enabled — your account is protected</div>
        </div>
        <button type="button" class="btn-danger btn-sm"
          onClick=${() => _showDisableTotpModal(setEnabled)}>
          Disable
        </button>
      </div>
    `;
  }

  return html`
    <div class="flex items-center justify-between gap-3 px-3 py-3">
      <div>
        <div class="text-sm font-medium text-text">Two-factor authentication</div>
        <div class="text-xs text-text-muted mt-0.5">Add an extra layer of security to your account</div>
      </div>
      <button type="button" class="btn-primary btn-sm"
        onClick=${() => _showSetupTotpModal(setEnabled)}>
        Set up
      </button>
    </div>
  `;
}

function _showSetupTotpModal(onDone) {
  import('preact').then(({ render }) => {
    const root = document.getElementById('modal-root');
    if (!root) return;
    const cleanup = () => render(null, root);
    render(html`<${TotpWizard}
      onComplete=${() => { cleanup(); onDone(true); }}
      onCancel=${() => cleanup()}
    />`, root);
  });
}

async function _showDisableTotpModal(onDone) {
  const code = prompt('Enter your TOTP code to disable two-factor authentication:');
  if (!code) return;
  try {
    await api.disableTotp(code);
    onDone(false);
  } catch (e) {
    alert('Failed to disable TOTP: ' + (/** @type {any} */ (e)?.message ?? 'unknown error'));
  }
}
