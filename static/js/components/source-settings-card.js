// @ts-check
// Source settings card — enable/star/configure/install/delete a source extension.

import { h } from 'preact';
import { useState, useEffect } from 'preact/hooks';
import htm from 'htm';
import * as api from '../api.js';
import { Modal } from './modal.js';
import { PreferenceRow } from './preference-row.js';
import { iconWarning, iconStarFilled, iconStarOutline } from '../icons.js';
import { Icon } from './icon.js';
import { showToast, showApiError } from './toast.js';
import { t } from '../i18n.js';
const html = htm.bind(h);

/**
 * @param {{
 *   source: any,
 *   activeIds: Set<number>,
 *   onDeleted: (id: number) => void,
 * }} props
 */
export function SourceSettingsCard({ source, activeIds, onDeleted }) {
  const sid = source.id;

  const [enabled, setEnabled] = useState(source.enabled ?? false);
  const [starred, setStarred] = useState(source.favourited ?? false);
  const [confirming, setConfirming] = useState(false);       // unsafe enable confirm
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  const [modalOpen, setModalOpen] = useState(false);

  const [installOpen, setInstallOpen] = useState(false);
  const [wasmUrl, setWasmUrl] = useState('');
  const [wasmFetching, setWasmFetching] = useState(false);

  const [schema, setSchema] = useState(/** @type {any[]} */ ([]));
  const [liveValues, setLiveValues] = useState(/** @type {Record<string,any>} */ ({}));
  const [prefsLoading, setPrefsLoading] = useState(false);

  // Load prefs when modal opens
  useEffect(() => {
    if (!modalOpen) return;
    setPrefsLoading(true);
    Promise.all([api.getPreferenceSchema(sid), api.getPreferences(sid)])
      .then(([schemaRes, prefsRes]) => {
        setSchema(Array.isArray(schemaRes) ? schemaRes : []);
        setLiveValues(prefsRes && typeof prefsRes === 'object' ? prefsRes : {});
      })
      .catch(e => showApiError(e))
      .finally(() => setPrefsLoading(false));
  }, [modalOpen, sid]);

  async function toggleEnabled(val) {
    if (val && source.unrestricted_http && !confirming) {
      setConfirming(true);
      return;
    }
    setConfirming(false);
    try {
      await api.toggleSourceEnabled(sid, val);
      setEnabled(val);
    } catch (e) {
      showApiError(e);
    }
  }

  async function toggleStarred(val) {
    try {
      await api.toggleSourceFavourite(sid, val);
      setStarred(val);
    } catch { /* ignore */ }
  }

  async function handleDelete() {
    try {
      await api.deleteSource(sid);
      onDeleted(sid);
    } catch (e) {
      showApiError(e);
    }
  }

  async function handleFetchWasm() {
    setWasmFetching(true);
    try {
      await api.fetchWasm(sid, wasmUrl);
      setWasmUrl('');
      setInstallOpen(false);
      showToast(t('source.card.install.success'), { type: 'success' });
    } catch (e) {
      showApiError(e);
    } finally {
      setWasmFetching(false);
    }
  }

  const isActive = activeIds.has(sid);

  // Group schema by group name
  /** @type {Map<string, any[]>} */
  const groups = new Map();
  for (const d of schema) {
    const g = d.group ?? '';
    if (!groups.has(g)) groups.set(g, []);
    groups.get(g).push(d);
  }

  const prefsModal = html`
    <${Modal}
      open=${modalOpen}
      onClose=${() => setModalOpen(false)}
      title=${t('source.card.prefs.title', { name: source.name })}
    >
      ${prefsLoading
        ? html`<p class="text-sm text-text-muted py-2">${t('source.card.prefs.loading')}</p>`
        : schema.length === 0
          ? html`<p class="text-sm text-text-muted">${t('source.card.prefs.empty')}</p>`
          : html`
            <div class="flex flex-col">
              ${[...groups.entries()].map(([group, descriptors]) => html`
                <div key=${group} class="flex flex-col">
                  ${group && html`<div class="py-2 text-xs font-semibold uppercase tracking-wider text-text-muted">${group}</div>`}
                  ${descriptors.map(d => html`
                    <${PreferenceRow}
                      key=${d.key}
                      sourceId=${sid}
                      descriptor=${d}
                      currentValue=${liveValues[d.key]}
                      liveValues=${liveValues}
                      onValueChange=${(key, val) => setLiveValues(prev => ({ ...prev, [key]: val }))}
                    />
                  `)}
                </div>
              `)}
            </div>
          `
      }
    <//>
  `;

  return html`
    <div class=${'bg-surface border rounded-xl p-4 flex flex-col gap-4 ' + (source.unrestricted_http ? 'border-warn/50' : 'border-border') + (!enabled ? ' opacity-60' : '')}>

      <div class="flex items-start justify-between gap-3">
        <div class="flex flex-col gap-0.5 min-w-0">
          <div class="flex items-center gap-2">
            <span class="text-sm font-semibold text-text">${source.name}</span>
            ${source.unrestricted_http && html`
              <span class="inline-flex items-center gap-1 px-1.5 py-0.5 text-xs rounded bg-warn/20 text-warn" title=${t('source.card.badge.unsafe.title')}>
                <${Icon} svg=${iconWarning} /> ${t('source.card.badge.unsafe')}
              </span>
            `}
          </div>
          <span class="text-xs text-text-muted">v${source.version ?? '?'}</span>
        </div>

        <div class="flex items-center gap-2 shrink-0">
          <span class=${'inline-flex items-center px-2 py-0.5 text-xs font-medium rounded-full ' + (isActive ? 'bg-success/20 text-success' : 'bg-surface-2 text-text-muted')}>
            ${isActive ? t('source.card.badge.loaded') : t('source.card.badge.unloaded')}
          </span>

          <label class="star-checkbox" title=${t('source.card.star.label')}>
            <input
              type="checkbox"
              class="star-checkbox__input"
              checked=${starred}
              onChange=${(e) => toggleStarred(/** @type {HTMLInputElement} */ (e.target).checked)}
              aria-label=${t('source.card.star.label')}
            />
            <span class="star-checkbox__icon" aria-hidden="true"><${Icon} svg=${starred ? iconStarFilled : iconStarOutline} /></span>
          </label>

          <label class="kani-toggle">
            <input
              type="checkbox"
              class="kani-toggle__input"
              checked=${enabled}
              aria-label=${enabled ? t('source.card.toggle.disable') : t('source.card.toggle.enable')}
              onChange=${(e) => toggleEnabled(/** @type {HTMLInputElement} */ (e.target).checked)}
            />
            <span class="kani-toggle__track"></span>
          </label>
        </div>
      </div>

      ${confirming && html`
        <div class="rounded-lg bg-warn/10 border border-warn/30 p-3 flex flex-col gap-2">
          <p class="text-sm text-warn">
            ${t('source.card.unsafe.warning')}
          </p>
          <div class="flex items-center gap-2 justify-end">
            <button class="btn-ghost btn-sm" onClick=${() => setConfirming(false)}>${t('common.cancel')}</button>
            <button class="btn-danger btn-sm" onClick=${() => toggleEnabled(true)}>${t('source.card.unsafe.enable')}</button>
          </div>
        </div>
      `}

      <div class="flex items-center gap-2 flex-wrap">
        <button class="btn-ghost btn-sm" onClick=${() => setModalOpen(true)}>${t('source.card.action.configure')}</button>
        <button class="btn-ghost btn-sm" onClick=${() => setInstallOpen(v => !v)}>${t('source.card.action.install_wasm')}</button>

        ${!confirmingDelete
          ? html`<button class="btn-danger btn-sm" onClick=${() => setConfirmingDelete(true)}>${t('common.delete')}</button>`
          : html`
            <div class="flex flex-col gap-2">
              <p class="text-sm text-danger">${t('source.card.delete.confirm')}</p>
              <div class="flex items-center gap-2 justify-end">
                <button class="btn-ghost btn-sm" onClick=${() => setConfirmingDelete(false)}>${t('common.cancel')}</button>
                <button class="btn-danger btn-sm" onClick=${handleDelete}>${t('source.card.delete.btn')}</button>
              </div>
            </div>
          `
        }
      </div>

      ${installOpen && html`
        <div class="flex flex-col gap-2 pt-2 border-t border-border-subtle">
          <div class="flex items-center gap-2">
            <input
              type="url"
              class="input flex-1"
              placeholder="https://example.com/extension.wasm"
              value=${wasmUrl}
              disabled=${wasmFetching}
              onInput=${(e) => setWasmUrl(/** @type {HTMLInputElement} */ (e.target).value)}
              onKeyDown=${(e) => { if (e.key === 'Enter') handleFetchWasm(); }}
            />
            <button
              class="btn-primary btn-sm"
              disabled=${wasmFetching || !wasmUrl.trim()}
              onClick=${handleFetchWasm}
            >${wasmFetching ? t('source.card.install.fetching') : t('source.card.install.fetch')}</button>
          </div>
        </div>
      `}

      ${prefsModal}
    </div>
  `;
}
