// @ts-check
// API tokens ("Clients") settings section — Preact/htm. Lets a user mint and
// revoke long-lived tokens for OPDS-capable reading apps.

import { h } from 'preact';
import { useState, useEffect, useCallback } from 'preact/hooks';
import htm from 'htm';
import * as api from '../../api.js';
import { t } from '../../i18n.js';
import { formatDate, formatRelativeTime } from '../../utils.js';
import { SettingsGroup, SettingsRow } from './_shared.js';
import { EmptyState } from '../../components/empty-state.js';
import { Modal, showConfirm } from '../../components/modal.js';
import { showApiError, showToast } from '../../components/toast.js';
import { useBusy } from '../../hooks/use-busy.js';

const html = htm.bind(h);

/** @param {any} tok */
function tokenMeta(tok) {
  const parts = [`${t('clients.created')} ${formatDate(tok.created_at)}`];
  parts.push(
    tok.last_used_at
      ? `${t('clients.last_used')} ${formatRelativeTime(new Date(tok.last_used_at * 1000))}`
      : t('clients.never_used'),
  );
  if (tok.expires_at) parts.push(`${t('clients.expires')} ${formatDate(tok.expires_at)}`);
  return parts.join(' · ');
}

export function ClientsSection() {
  const [tokens, setTokens] = useState(/** @type {any[] | null} */ (null));
  const [reveal, setReveal] = useState(/** @type {any} */ (null));
  const [name, setName] = useState('');
  const [expiry, setExpiry] = useState('');
  const { busy, run } = useBusy();

  const load = useCallback(async () => {
    try {
      setTokens(await api.listApiTokens());
    } catch (e) {
      showApiError(e);
      setTokens([]);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const onCreate = async (/** @type {Event} */ e) => {
    e.preventDefault();
    const trimmed = name.trim();
    if (!trimmed) return;
    await run(async () => {
      try {
        const created = await api.createApiToken(trimmed, expiry ? Number(expiry) : null);
        setReveal(created);
        setName('');
        setExpiry('');
        await load();
      } catch (err) {
        showApiError(err);
      }
    });
  };

  const onRevoke = async (/** @type {any} */ tok) => {
    const ok = await showConfirm(t('clients.revoke.confirm'), {
      title: t('clients.revoke'),
      confirmLabel: t('clients.revoke'),
      danger: true,
    });
    if (!ok) return;
    try {
      await api.revokeApiToken(tok.id);
      await load();
    } catch (e) {
      showApiError(e);
    }
  };

  const copyToken = async () => {
    if (!reveal) return;
    try {
      await navigator.clipboard.writeText(reveal.raw_token);
      showToast(t('clients.copied'), { type: 'success' });
    } catch {
      /* clipboard blocked — the field is selectable as a fallback */
    }
  };

  const opdsUrl = `${location.origin}/opds`;
  const copyUrl = async () => {
    try {
      await navigator.clipboard.writeText(opdsUrl);
      showToast(t('common.copied'), { type: 'success' });
    } catch {
      /* clipboard blocked — the field is selectable as a fallback */
    }
  };

  return html`
    <div class="flex flex-col gap-6">
      <${SettingsGroup} label=${t('clients.opds.group')}>
        <${SettingsRow} label=${t('clients.opds.url.label')} description=${t('clients.opds.url.desc')}>
          <div class="flex items-center gap-2">
            <input
              readonly
              class="input text-sm font-mono w-64"
              value=${opdsUrl}
              onClick=${(/** @type {Event} */ e) =>
                /** @type {HTMLInputElement} */ (e.target).select()}
            />
            <button type="button" class="btn-ghost btn-sm" onClick=${copyUrl}>
              ${t('common.copy')}
            </button>
          </div>
        <//>
      <//>

      ${tokens === null
        ? null
        : tokens.length === 0
        ? html`<${EmptyState}
            title=${t('clients.empty')}
            subtitle=${t('clients.empty.subtitle')}
          />`
        : html`<${SettingsGroup} label=${t('clients.list.title')}>
            ${tokens.map(
              (tok) => html`
                <${SettingsRow} key=${tok.id} label=${tok.name} description=${tokenMeta(tok)}>
                  <button
                    type="button"
                    class="btn-ghost btn-sm text-danger"
                    onClick=${() => onRevoke(tok)}
                  >
                    ${t('clients.revoke')}
                  </button>
                <//>
              `,
            )}
          <//>`}

      <${SettingsGroup} label=${t('clients.create.title')}>
        <form class="flex flex-col gap-3 p-4" onSubmit=${onCreate}>
          <label class="flex flex-col gap-1">
            <span class="text-sm font-medium text-text">${t('clients.name.label')}</span>
            <input
              type="text"
              class="input text-sm"
              placeholder=${t('clients.name.placeholder')}
              value=${name}
              maxLength="100"
              onInput=${(/** @type {Event} */ e) =>
                setName(/** @type {HTMLInputElement} */ (e.target).value)}
            />
          </label>
          <label class="flex flex-col gap-1">
            <span class="text-sm font-medium text-text">${t('clients.expiry.label')}</span>
            <select
              class="input text-sm"
              value=${expiry}
              onChange=${(/** @type {Event} */ e) =>
                setExpiry(/** @type {HTMLSelectElement} */ (e.target).value)}
            >
              <option value="">${t('clients.expiry.never')}</option>
              <option value="30">${t('clients.expiry.30')}</option>
              <option value="90">${t('clients.expiry.90')}</option>
              <option value="365">${t('clients.expiry.365')}</option>
            </select>
          </label>
          <div>
            <button
              type="submit"
              class="btn-primary btn-sm"
              disabled=${busy || !name.trim()}
            >
              ${t('clients.create')}
            </button>
          </div>
        </form>
      <//>

      <p class="text-xs text-text-muted px-1">${t('clients.apps.note')}</p>

      <${Modal}
        open=${!!reveal}
        onClose=${() => setReveal(null)}
        title=${t('clients.token.reveal.title')}
      >
        ${reveal &&
        html`
          <div class="flex flex-col gap-3">
            <p class="text-sm text-warn">${t('clients.token.reveal.warning')}</p>
            <div class="flex gap-2">
              <input
                readonly
                class="input text-sm font-mono flex-1"
                value=${reveal.raw_token}
                onClick=${(/** @type {Event} */ e) =>
                  /** @type {HTMLInputElement} */ (e.target).select()}
              />
              <button type="button" class="btn-secondary btn-sm" onClick=${copyToken}>
                ${t('clients.copy')}
              </button>
            </div>
            <label class="flex flex-col gap-1">
              <span class="text-xs text-text-muted">${t('clients.opds.url.label')}</span>
              <input
                readonly
                class="input text-sm font-mono"
                value=${opdsUrl}
                onClick=${(/** @type {Event} */ e) =>
                  /** @type {HTMLInputElement} */ (e.target).select()}
              />
            </label>
          </div>
        `}
      <//>
    </div>
  `;
}
