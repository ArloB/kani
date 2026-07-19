// @ts-check
// Settings — Trackers section.

import * as api from '../../api.js';
import { escapeHtml } from '../../utils.js';
import { showConfirm } from '../../components/modal.js';
import { showToast, showApiError } from '../../components/toast.js';
import { hasPermission } from '../../session.js';
import { mkSettingsGroup, mkSettingsGroupCard, mkSettingsRow, mkToggleRow, mkNumberRow } from './_shared.js';
import { skeletonSettingsCards } from '../../components/skeletons.js';
import { createErrorState } from '../../components/error-state.js';
import { t } from '../../i18n.js';

/**
 * @param {HTMLElement} el
 * @param {any} settings
 */
export function mount(el, settings) {
  const isAdmin = hasPermission('settings:edit_advanced');

  async function _render() {
    el.innerHTML = skeletonSettingsCards(3);

    let trackers = [];
    try {
      trackers = await api.getTrackers();
    } catch (e) {
      el.innerHTML = '';
      el.appendChild(createErrorState({ message: t('trackers.error.load_failed'), onRetry: _render }));
      return;
    }

    el.innerHTML = '';

    const saveTracking = async () => {
      await api.updateSettings({
        Tracking: {
          default_tracking_enabled: settings.default_tracking_enabled ?? true,
          tracker_auto_sync_enabled: settings.tracker_auto_sync_enabled ?? false,
          tracker_sync_interval_hours: settings.tracker_sync_interval_hours ?? 24,
        },
      });
    };

    const defaultGroup = mkSettingsGroup(t('settings.trackers.behaviour'));
    const defaultCard  = mkSettingsGroupCard(defaultGroup);
    const trackingEnabled = settings?.default_tracking_enabled ?? true;
    defaultCard.appendChild(mkToggleRow({
      label: t('settings.trackers.default_enabled'),
      description: t('settings.trackers.default_enabled_desc'),
      checked: trackingEnabled,
      onChange: async (checked) => {
        const prev = settings.default_tracking_enabled;
        settings.default_tracking_enabled = checked;
        try {
          await saveTracking();
        } catch (e) {
          settings.default_tracking_enabled = prev;
          showApiError(e);
        }
      },
    }));
    defaultCard.appendChild(mkToggleRow({
      label: t('settings.trackers.auto_sync'),
      description: t('settings.trackers.auto_sync_desc'),
      checked: settings?.tracker_auto_sync_enabled ?? false,
      onChange: async (checked) => {
        const prev = settings.tracker_auto_sync_enabled;
        settings.tracker_auto_sync_enabled = checked;
        try {
          await saveTracking();
        } catch (e) {
          settings.tracker_auto_sync_enabled = prev;
          showApiError(e);
        }
      },
    }));
    defaultCard.appendChild(mkNumberRow({
      label: t('settings.trackers.sync_interval'),
      description: t('settings.trackers.sync_interval_desc'),
      id: 'tracker-sync-interval',
      value: settings?.tracker_sync_interval_hours ?? 24,
      min: 1,
      max: 168,
      onChange: async (val) => {
        const prev = settings.tracker_sync_interval_hours;
        settings.tracker_sync_interval_hours = val;
        try {
          await saveTracking();
        } catch (e) {
          settings.tracker_sync_interval_hours = prev;
          showApiError(e);
        }
      },
    }));
    el.appendChild(defaultGroup);

    for (const tracker of trackers) {
      const trackerGroup = mkSettingsGroup(tracker.name);
      const trackerCard  = mkSettingsGroupCard(trackerGroup);

      if (tracker.configured) {
        const linkBtn = document.createElement('button');
        linkBtn.type = 'button';
        linkBtn.className = tracker.linked ? 'btn-danger btn-sm' : 'btn-secondary btn-sm';
        linkBtn.textContent = tracker.linked ? t('trackers.unlink') : t('trackers.link');
        trackerCard.appendChild(mkSettingsRow({
          label: tracker.linked ? t('trackers.linked_label') : t('trackers.not_linked_label'),
          description: tracker.linked
            ? t('trackers.linked_desc', { name: tracker.name })
            : t('trackers.not_linked_desc', { name: tracker.name }),
          control: linkBtn,
        }));

        linkBtn.addEventListener('click', async () => {
          if (tracker.linked) {
            if (!(await showConfirm(t('trackers.confirm.unlink.msg', { name: tracker.name }), { title: t('trackers.confirm.unlink.title'), danger: true }))) return;
            linkBtn.disabled = true;
            try {
              await api.unlinkTracker(tracker.id);
              tracker.linked = false;
              await _render();
            } catch (e) {
              showToast(e?.message ?? t('trackers.error.unlink_failed'), { type: 'error' });
              linkBtn.disabled = false;
            }
          } else {
            _openTrackerPopup(tracker.id, tracker.name, () => {
              tracker.linked = true;
              _render();
            });
          }
        });
      } else {
        const notConfiguredEl = document.createElement('span');
        notConfiguredEl.className = 'text-xs text-text-muted';
        trackerCard.appendChild(mkSettingsRow({
          label: t('trackers.not_configured'),
          description: isAdmin
            ? t('trackers.not_configured.admin_desc')
            : t('trackers.not_configured.user_desc'),
          control: notConfiguredEl,
        }));
      }

      if (isAdmin) {
        let config = null;
        try { config = await api.getTrackerConfig(tracker.id); } catch { /* may not exist yet */ }

        const isAniList = tracker.name === 'AniList';
        const isMAL = tracker.name === 'MyAnimeList';

        const setupGroup = mkSettingsGroup(t('trackers.setup.group'));
        const setupCard  = mkSettingsGroupCard(setupGroup);

        const instructions = isAniList ? `
          <p class="text-xs text-text-muted leading-relaxed mb-2">
            Register a free OAuth application at <strong>anilist.co → Settings → Developer → Create New Client</strong>.
            Set the redirect URL to <code class="font-mono bg-surface-alt px-1 rounded">${location.origin}/rest/trackers/${tracker.id}/callback</code>.
          </p>
        ` : isMAL ? `
          <p class="text-xs text-text-muted leading-relaxed mb-2">
            Register a free API client at <strong>myanimelist.net → Account Settings → API → Create ID</strong>.
            Set App Type to <strong>web</strong> and redirect URL to <code class="font-mono bg-surface-alt px-1 rounded">${location.origin}/rest/trackers/${tracker.id}/callback</code>.
          </p>
        ` : '';

        setupCard.innerHTML = `
          <div class="px-4 py-4 flex flex-col gap-3">
            ${instructions}
            <div class="flex flex-col gap-1">
              <label class="text-xs font-medium text-text" for="tracker-${tracker.id}-client-id">${escapeHtml(t('trackers.setup.client_id'))}</label>
              <input type="text" id="tracker-${tracker.id}-client-id" class="input text-sm js-client-id font-mono"
                value="${escapeHtml(config?.client_id ?? '')}" placeholder="${escapeHtml(t('trackers.setup.client_id.placeholder'))}"
                autocomplete="off" spellcheck="false">
            </div>
            ${isAniList ? `
            <div class="flex flex-col gap-1">
              <label class="text-xs font-medium text-text" for="tracker-${tracker.id}-secret">${escapeHtml(t('trackers.setup.client_secret'))}</label>
              <input type="password" id="tracker-${tracker.id}-secret" class="input text-sm js-client-secret font-mono"
                placeholder="${escapeHtml(config?.secret_configured ? t('trackers.setup.client_secret.already_set') : t('trackers.setup.client_secret.placeholder'))}"
                autocomplete="off">
              <p class="text-xs text-text-muted">${escapeHtml(t('trackers.setup.client_secret.note'))}</p>
            </div>` : ''}
            <div class="flex items-center gap-2 flex-wrap">
              <button type="button" class="btn-secondary btn-sm js-config-save">${escapeHtml(t('trackers.setup.save'))}</button>
              ${config?.client_id ? `<button type="button" class="btn-danger btn-sm js-config-delete">${escapeHtml(t('trackers.setup.remove'))}</button>` : ''}
            </div>
          </div>
        `;

        const clientIdEl = /** @type {HTMLInputElement} */ (setupCard.querySelector('.js-client-id'));
        const secretEl   = /** @type {HTMLInputElement|null} */ (setupCard.querySelector('.js-client-secret'));
        const saveBtn    = /** @type {HTMLButtonElement} */ (setupCard.querySelector('.js-config-save'));
        const deleteBtn  = /** @type {HTMLButtonElement|null} */ (setupCard.querySelector('.js-config-delete'));

        saveBtn.addEventListener('click', async () => {
          const clientId = clientIdEl.value.trim();
          if (!clientId) { showToast(t('trackers.setup.client_id.required'), { type: 'error' }); return; }
          saveBtn.disabled = true;
          try {
            const body = { client_id: clientId };
            if (secretEl?.value) body.client_secret = secretEl.value;
            await api.setTrackerConfig(tracker.id, body);
            showToast(t('common.saved'), { type: 'success' });
            await _render();
          } catch (e) {
            showApiError(e);
          } finally {
            saveBtn.disabled = false;
          }
        });

        deleteBtn?.addEventListener('click', async () => {
          if (!(await showConfirm(t('trackers.confirm.remove_creds.msg', { name: tracker.name }), { title: t('trackers.confirm.remove_creds.title'), danger: true }))) return;
          deleteBtn.disabled = true;
          try {
            await api.deleteTrackerConfig(tracker.id);
            _render();
          } catch (e) {
            showToast(e?.message ?? t('trackers.error.remove_failed'), { type: 'error' });
            deleteBtn.disabled = false;
          }
        });

        trackerGroup.appendChild(setupGroup);
      }

      el.appendChild(trackerGroup);
    }
  }

  _render();
  return { destroy() { el.innerHTML = ''; } };
}

/**
 * @param {number} trackerId
 * @param {string} trackerName
 * @param {() => void} onLinked
 */
function _openTrackerPopup(trackerId, trackerName, onLinked) {
  const redirectUri = `${location.origin}/rest/trackers/${trackerId}/callback`;

  api.getTrackerAuthUrl(trackerId, redirectUri).then(({ url }) => {
    const popup = window.open(url, `link_${trackerName}`, 'popup,width=640,height=720');
    if (!popup) {
      // Import showToast lazily to avoid circular imports
      import('../../components/toast.js').then(({ showToast }) => {
        showToast(t('trackers.error.popup_blocked'), { type: 'error' });
      });
      return;
    }

    /** @param {MessageEvent} e */
    function onMessage(e) {
      if (e.origin !== location.origin) return;
      if (e.data?.type === 'tracker_linked') {
        window.removeEventListener('message', onMessage);
        clearInterval(closedTimer);
        popup.close();
        onLinked();
      }
    }

    window.addEventListener('message', onMessage);

    const closedTimer = setInterval(() => {
      if (popup.closed) {
        clearInterval(closedTimer);
        window.removeEventListener('message', onMessage);
      }
    }, 500);
  }).catch(e => {
    import('../../components/toast.js').then(({ showToast }) => {
      showToast(e?.message ?? t('trackers.error.auth_url_failed', { name: trackerName }), { type: 'error' });
    });
  });
}
