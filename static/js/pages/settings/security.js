// @ts-check
// Settings — Security tab: 2FA/TOTP, session inventory, security status.

import { h, Fragment } from 'preact';
import { useState, useEffect } from 'preact/hooks';
import htm from 'htm';
import * as api from '../../api.js';
import { mkSettingsGroup, mkSettingsGroupCard, mkSettingsRow } from './_shared.js';
import { SessionList } from '../../components/session-list.js';
import { createErrorState } from '../../components/error-state.js';
import { TotpWizard } from '../../components/totp-wizard.js';
import { t } from '../../i18n.js';

const html = htm.bind(h);

/** @param {HTMLElement} el */
export function mount(el) {
  const destroys = [];

  // ── 2FA section ────────────────────────────────────────────────────────────
  const twoFaGroup = mkSettingsGroup(t('settings.security.totp.group'));
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

  // ── Session inventory section ───────────────────────────────────────────────
  const sessGroup = mkSettingsGroup(t('settings.security.sessions.group'));
  const sessCard  = mkSettingsGroupCard(sessGroup);
  el.appendChild(sessGroup);

  const sessEl = document.createElement('div');
  sessCard.appendChild(sessEl);
  import('preact').then(({ render }) => {
    render(html`<${SessionList} />`, sessEl);
    destroys.push(() => render(null, sessEl));
  });

  // ── Security status section ─────────────────────────────────────────────────
  const statusGroup = mkSettingsGroup(t('settings.security.status.group'));
  const statusCard  = mkSettingsGroupCard(statusGroup);
  el.appendChild(statusGroup);

  function _loadStatus() {
    statusCard.innerHTML = '';
    api.getFeatures().then(features => {
      if (features?.public_instance) {
        const badge = document.createElement('div');
        badge.className = 'px-3 py-2 rounded bg-accent/10 text-accent text-sm font-medium';
        badge.textContent = t('settings.security.status.public_instance');
        statusCard.appendChild(badge);
      }
      statusCard.appendChild(mkSettingsRow({
        label: t('settings.security.status.https.label'),
        description: features?.public_instance
          ? t('settings.security.status.https.desc_public')
          : t('settings.security.status.https.desc'),
      }));
      statusCard.appendChild(mkSettingsRow({
        label: t('settings.security.status.headers.label'),
        description: t('settings.security.status.headers.desc'),
      }));
    }).catch(() => {
      statusCard.appendChild(createErrorState({
        message: t('settings.security.status.load_failed'),
        onRetry: _loadStatus,
      }));
    });
  }
  _loadStatus();

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
    return html`<div class="px-3 py-3 text-sm text-text-muted">${t('common.loading')}</div>`;
  }

  if (enabled) {
    return html`
      <div class="flex items-center justify-between gap-3 px-3 py-3">
        <div>
          <div class="text-sm font-medium text-text">${t('settings.security.totp.title')}</div>
          <div class="text-xs text-success mt-0.5">${t('settings.security.totp.enabled')}</div>
        </div>
        <button type="button" class="btn-danger btn-sm"
          onClick=${() => _showDisableTotpModal(setEnabled)}>
          ${t('settings.security.totp.disable')}
        </button>
      </div>
    `;
  }

  return html`
    <div class="flex items-center justify-between gap-3 px-3 py-3">
      <div>
        <div class="text-sm font-medium text-text">${t('settings.security.totp.title')}</div>
        <div class="text-xs text-text-muted mt-0.5">${t('settings.security.totp.desc')}</div>
      </div>
      <button type="button" class="btn-primary btn-sm"
        onClick=${() => _showSetupTotpModal(setEnabled)}>
        ${t('settings.security.totp.setup')}
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
  const code = prompt(t('settings.security.totp.disable_prompt'));
  if (!code) return;
  try {
    await api.disableTotp(code);
    onDone(false);
  } catch (e) {
    alert(t('settings.security.totp.disable_error', { msg: /** @type {any} */ (e)?.message ?? '' }));
  }
}
