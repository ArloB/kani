// @ts-check
// Manage tab — Tracking + External Trackers sections.

import * as api from '../../api.js';
import { t } from '../../i18n.js';
import { getState, setState } from '../../state.js';
import { showAlert } from '../modal.js';
import { mkCard, mkTitledCard, mkRow, mkItem } from './_shared.js';

/**
 * @param {HTMLElement} containerEl
 * @param {{ dbId: number }} ctx
 */
export function mountTrackerPanel(containerEl, ctx) {
  const { dbId } = ctx;

  // ── Internal tracking ──────────────────────────────────────────────────────

  const trackCard = mkTitledCard(t('manga.tracker.card.title'), t('manga.tracker.card.desc'));

  const statusOptions = [
    { value: '', label: t('manga.tracker.status.untracked') },
    { value: 'reading', label: t('manga.tracker.status.reading') },
    { value: 'on_hold', label: t('manga.tracker.status.on_hold') },
    { value: 'dropped', label: t('manga.tracker.status.dropped') },
    { value: 'plan_to_read', label: t('manga.tracker.status.plan_to_read') },
    { value: 'completed', label: t('manga.tracker.status.completed') },
    { value: 'rereading', label: t('manga.tracker.status.rereading') },
  ];

  const statusSelect = document.createElement('select');
  statusSelect.className = 'bg-surface border border-border rounded-lg px-3 py-1.5 text-sm text-text';
  for (const opt of statusOptions) {
    const o = document.createElement('option');
    o.value = opt.value;
    o.textContent = opt.label;
    statusSelect.appendChild(o);
  }

  const scoreInput = document.createElement('input');
  scoreInput.type = 'number';
  scoreInput.min = '0';
  scoreInput.max = '10';
  scoreInput.step = '0.5';
  scoreInput.placeholder = '—';
  scoreInput.className = 'bg-surface border border-border rounded-lg px-3 py-1.5 text-sm text-text w-20';

  const progressText = document.createElement('span');
  progressText.className = 'text-sm text-text-muted';
  progressText.textContent = '—';

  const toggleId = `tracking-enabled-${dbId}`;
  const toggleLabel = document.createElement('label');
  toggleLabel.className = 'kani-toggle';
  toggleLabel.setAttribute('for', toggleId);
  toggleLabel.innerHTML = `
    <input type="checkbox" id="${toggleId}" class="kani-toggle__input js-tracking-enabled" checked>
    <span class="kani-toggle__track"></span>
  `;
  const trackingToggle = /** @type {HTMLInputElement} */ (toggleLabel.querySelector('.js-tracking-enabled'));

  trackCard.appendChild(mkItem(mkRow(t('manga.tracker.sync_enabled'), t('manga.tracker.sync_enabled.desc'), toggleLabel)));
  trackCard.appendChild(mkItem(mkRow(t('manga.tracker.status'), t('manga.tracker.status.desc'), statusSelect)));
  trackCard.appendChild(mkItem(mkRow(t('manga.tracker.score'), t('manga.tracker.score.desc'), scoreInput)));
  trackCard.appendChild(mkItem(mkRow(t('manga.tracker.progress'), t('manga.tracker.progress.desc'), progressText)));
  containerEl.appendChild(trackCard);

  const notifyId = `notify-new-chapters-${dbId}`;
  const notifyLabel = document.createElement('label');
  notifyLabel.className = 'kani-toggle';
  notifyLabel.setAttribute('for', notifyId);
  notifyLabel.innerHTML = `
    <input type="checkbox" id="${notifyId}" class="kani-toggle__input js-notify-chapters" checked>
    <span class="kani-toggle__track"></span>
  `;
  const notifyToggle = /** @type {HTMLInputElement} */ (notifyLabel.querySelector('.js-notify-chapters'));
  trackCard.appendChild(mkItem(mkRow(t('manga.tracker.notify'), t('manga.tracker.notify.desc'), notifyLabel)));

  api.getMangaTracking(dbId).then(tracking => {
    trackingToggle.checked = tracking.tracking_enabled ?? true;
    const notify = tracking.notify_new_chapters ?? true;
    notifyToggle.checked = notify;
    const prefs = new Map(getState('mangaNotifyPrefs'));
    prefs.set(dbId, notify);
    setState('mangaNotifyPrefs', prefs);
    if (tracking.status) statusSelect.value = tracking.status;
    if (tracking.score != null) scoreInput.value = String(tracking.score);
    progressText.textContent = `${tracking.chapters_read} / ${tracking.total_chapters}`;
  }).catch(() => {});

  trackingToggle.addEventListener('change', () => {
    api.setMangaTracking(dbId, { tracking_enabled: trackingToggle.checked }).catch(() => {});
  });

  notifyToggle.addEventListener('change', async () => {
    const val = notifyToggle.checked;
    if (val && 'Notification' in window) {
      if (Notification.permission === 'denied') {
        notifyToggle.checked = false;
        await showAlert(t('manga.tracker.notify.blocked'), { title: t('manga.tracker.notify.blocked.title') });
        return;
      }
      if (Notification.permission !== 'granted') {
        const perm = await Notification.requestPermission();
        if (perm !== 'granted') { notifyToggle.checked = false; return; }
      }
      // Ensure the global kill-switch is on
      if (localStorage.getItem('kani_browser_notifications') !== 'true') {
        localStorage.setItem('kani_browser_notifications', 'true');
      }
    }
    api.setMangaTracking(dbId, { notify_new_chapters: val }).catch(() => {});
    const prefs = new Map(getState('mangaNotifyPrefs'));
    prefs.set(dbId, val);
    setState('mangaNotifyPrefs', prefs);
  });

  statusSelect.addEventListener('change', () => {
    const body = statusSelect.value ? { status: statusSelect.value } : { status: null };
    api.setMangaTracking(dbId, body).catch(() => {});
  });

  let scoreTimer = /** @type {ReturnType<typeof setTimeout>|null} */ (null);
  scoreInput.addEventListener('input', () => {
    if (scoreTimer) clearTimeout(scoreTimer);
    scoreTimer = setTimeout(() => {
      const val = parseFloat(scoreInput.value);
      if (!isNaN(val) && val >= 0 && val <= 10) {
        api.setMangaTracking(dbId, { score: val }).catch(() => {});
      }
    }, 800);
  });

  // ── External trackers ──────────────────────────────────────────────────────

  const extCard = mkCard();
  const extBody = document.createElement('div');
  extBody.className = 'py-3 text-sm text-text-muted';
  extBody.textContent = t('manga.tracker.loading');
  extCard.appendChild(extBody);
  containerEl.appendChild(extCard);

  Promise.all([api.getTrackers(), api.getTrackerMappings(dbId)])
    .then(([trackers, mappings]) => {
      extBody.textContent = '';
      const configuredTrackers = trackers.filter(t => t.configured);
      if (!configuredTrackers.length) {
        extBody.textContent = t('manga.tracker.none_configured');
        return;
      }

      for (const t of configuredTrackers) {
        const mapping = mappings.find(m => m.tracker_id === t.id);
        const row = document.createElement('div');
        row.className = 'flex items-center justify-between gap-4 py-3 border-b border-border-subtle last:border-b-0';

        const info = document.createElement('div');
        const nameEl = document.createElement('p');
        nameEl.className = 'text-sm font-medium text-text';
        nameEl.textContent = t.name;
        info.appendChild(nameEl);

        const statusEl = document.createElement('p');
        statusEl.className = 'text-xs text-text-muted mt-0.5';
        if (!t.linked) {
          statusEl.textContent = t('manga.tracker.not_linked');
        } else if (mapping?.tracker_manga_id) {
          statusEl.textContent = t('manga.tracker.mapped', { id: mapping.tracker_manga_id });
        } else {
          statusEl.textContent = t('manga.tracker.linked_not_mapped');
        }
        info.appendChild(statusEl);
        row.appendChild(info);

        const btnGroup = document.createElement('div');
        btnGroup.className = 'flex items-center gap-2 shrink-0';

        if (t.linked && !mapping?.tracker_manga_id) {
          const searchBtn = document.createElement('button');
          searchBtn.type = 'button';
          searchBtn.className = 'btn-ghost btn-sm';
          searchBtn.textContent = t('manga.tracker.search_link');
          searchBtn.addEventListener('click', async () => {
            const query = prompt(t('manga.tracker.search_prompt', { name: t.name }));
            if (!query) return;
            try {
              const results = await api.searchTrackerManga(t.id, query);
              if (!results.length) { await showAlert(t('manga.tracker.no_results'), { title: t('manga.tracker.search.title') }); return; }
              const choice = prompt(
                results.map((r, i) => `${i + 1}. ${r.title} (${r.tracker_manga_id})`).join('\n') +
                '\n\n' + t('manga.tracker.enter_number')
              );
              const idx = parseInt(choice ?? '', 10) - 1;
              if (idx >= 0 && idx < results.length) {
                await api.setTrackerMapping(dbId, t.id, results[idx].tracker_manga_id);
                statusEl.textContent = t('manga.tracker.mapped', { id: results[idx].tracker_manga_id });
              }
            } catch (err) {
              await showAlert(t('manga.tracker.search_failed', { message: /** @type {any} */(err)?.message ?? String(err) }), { title: t('manga.tracker.error.title') });
            }
          });
          btnGroup.appendChild(searchBtn);
        }

        if (t.linked && mapping?.tracker_manga_id) {
          const syncBtn = document.createElement('button');
          syncBtn.type = 'button';
          syncBtn.className = 'btn-ghost btn-sm';
          syncBtn.textContent = t('manga.tracker.sync');
          syncBtn.addEventListener('click', async () => {
            syncBtn.disabled = true;
            syncBtn.textContent = t('manga.tracker.syncing');
            try {
              await api.syncMangaTrackers(dbId);
              syncBtn.textContent = t('manga.tracker.sync_done');
              setTimeout(() => { syncBtn.textContent = t('manga.tracker.sync'); }, 2000);
            } catch {
              syncBtn.textContent = t('manga.tracker.sync_failed_btn');
              setTimeout(() => { syncBtn.textContent = t('manga.tracker.sync'); }, 2000);
            } finally { syncBtn.disabled = false; }
          });
          btnGroup.appendChild(syncBtn);

          const unlinkBtn = document.createElement('button');
          unlinkBtn.type = 'button';
          unlinkBtn.className = 'btn-ghost btn-sm text-danger';
          unlinkBtn.textContent = t('manga.tracker.unmap');
          unlinkBtn.addEventListener('click', async () => {
            await api.deleteTrackerMapping(dbId, t.id);
            statusEl.textContent = t('manga.tracker.linked_not_mapped');
          });
          btnGroup.appendChild(unlinkBtn);
        }

        row.appendChild(btnGroup);
        extBody.appendChild(row);
      }
    })
    .catch(() => {
      extBody.textContent = t('manga.tracker.load_failed');
    });
}

