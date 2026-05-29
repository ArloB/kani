// @ts-check
// Settings — Trackers section.

import * as api from '../../api.js';
import { escapeHtml, openConfirm } from '../../utils.js';
import { showToast, showApiError } from '../../components/toast.js';
import { hasPermission } from '../../state.js';
import { mkSettingsGroup, mkSettingsGroupCard, mkSettingsRow, mkToggleRow } from './_shared.js';
import { skeletonSettingsCards } from '../../components/skeletons.js';
import { createErrorState } from '../../components/error-state.js';

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
      el.appendChild(createErrorState({ message: 'Failed to load trackers.', onRetry: _render }));
      return;
    }

    el.innerHTML = '';

    const defaultGroup = mkSettingsGroup('Behaviour');
    const defaultCard  = mkSettingsGroupCard(defaultGroup);
    const trackingEnabled = settings?.default_tracking_enabled ?? true;
    defaultCard.appendChild(mkToggleRow({
      label: 'Enable tracking by default',
      description: 'New manga added to the library will have sync enabled.',
      checked: trackingEnabled,
      onChange: async (checked) => {
        try {
          await api.updateSettings({ Tracking: { default_tracking_enabled: checked } });
          settings.default_tracking_enabled = checked;
        } catch (e) {
          showToast(e?.message ?? 'Failed to save.', { type: 'error' });
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
        linkBtn.className = tracker.linked ? 'btn-danger btn-sm' : 'btn-primary btn-sm';
        linkBtn.textContent = tracker.linked ? 'Unlink' : 'Link Account';
        trackerCard.appendChild(mkSettingsRow({
          label: tracker.linked ? 'Account linked' : 'Not linked',
          description: tracker.linked
            ? `Your ${tracker.name} account is connected.`
            : `Connect your ${tracker.name} account to sync progress.`,
          control: linkBtn,
        }));

        linkBtn.addEventListener('click', async () => {
          if (tracker.linked) {
            if (!(await openConfirm({ title: 'Unlink tracker', message: `Unlink your ${tracker.name} account?`, danger: true }))) return;
            linkBtn.disabled = true;
            try {
              await api.unlinkTracker(tracker.id);
              tracker.linked = false;
              await _render();
            } catch (e) {
              showToast(e?.message ?? 'Failed to unlink.', { type: 'error' });
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
          label: 'Not configured',
          description: isAdmin
            ? 'Add credentials below to enable this tracker.'
            : 'Contact your server admin to configure this tracker.',
          control: notConfiguredEl,
        }));
      }

      if (isAdmin) {
        let config = null;
        try { config = await api.getTrackerConfig(tracker.id); } catch { /* may not exist yet */ }

        const isAniList = tracker.name === 'AniList';
        const isMAL = tracker.name === 'MyAnimeList';

        const setupGroup = mkSettingsGroup('Setup');
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
              <label class="text-xs font-medium text-text" for="tracker-${tracker.id}-client-id">Client ID</label>
              <input type="text" id="tracker-${tracker.id}-client-id" class="input text-sm js-client-id font-mono"
                value="${escapeHtml(config?.client_id ?? '')}" placeholder="Paste your client ID here"
                autocomplete="off" spellcheck="false">
            </div>
            ${isAniList ? `
            <div class="flex flex-col gap-1">
              <label class="text-xs font-medium text-text" for="tracker-${tracker.id}-secret">Client Secret</label>
              <input type="password" id="tracker-${tracker.id}-secret" class="input text-sm js-client-secret font-mono"
                placeholder="${config?.secret_configured ? 'Already set — leave blank to keep current value' : 'Paste your client secret here'}"
                autocomplete="off">
              <p class="text-xs text-text-muted">Stored on the server only, never exposed to users.</p>
            </div>` : ''}
            <div class="flex items-center gap-2 flex-wrap">
              <button type="button" class="btn-primary btn-sm js-config-save">Save credentials</button>
              ${config?.client_id ? `<button type="button" class="btn-danger btn-sm js-config-delete">Remove credentials</button>` : ''}
            </div>
          </div>
        `;

        const clientIdEl = /** @type {HTMLInputElement} */ (setupCard.querySelector('.js-client-id'));
        const secretEl   = /** @type {HTMLInputElement|null} */ (setupCard.querySelector('.js-client-secret'));
        const saveBtn    = /** @type {HTMLButtonElement} */ (setupCard.querySelector('.js-config-save'));
        const deleteBtn  = /** @type {HTMLButtonElement|null} */ (setupCard.querySelector('.js-config-delete'));

        saveBtn.addEventListener('click', async () => {
          const clientId = clientIdEl.value.trim();
          if (!clientId) { showToast('Client ID is required.', { type: 'error' }); return; }
          saveBtn.disabled = true;
          try {
            const body = { client_id: clientId };
            if (secretEl?.value) body.client_secret = secretEl.value;
            await api.setTrackerConfig(tracker.id, body);
            showToast('Saved.', { type: 'success' });
            await _render();
          } catch (e) {
            showApiError(e);
          } finally {
            saveBtn.disabled = false;
          }
        });

        deleteBtn?.addEventListener('click', async () => {
          if (!(await openConfirm({ title: 'Remove credentials', message: `Remove all ${tracker.name} credentials? This will unlink all users.`, danger: true }))) return;
          deleteBtn.disabled = true;
          try {
            await api.deleteTrackerConfig(tracker.id);
            _render();
          } catch (e) {
            showToast(e?.message ?? 'Failed to remove.', { type: 'error' });
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
        showToast('Popup was blocked. Please allow popups for this site.', { type: 'error' });
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
      showToast(e?.message ?? `Failed to get ${trackerName} auth URL.`, { type: 'error' });
    });
  });
}
