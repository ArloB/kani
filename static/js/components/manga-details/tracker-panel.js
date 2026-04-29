// @ts-check
// Manage tab — Tracking + External Trackers sections.

import * as api from '../../api.js';
import { mkCard, mkTitledCard, mkRow, mkItem } from './_shared.js';

/**
 * @param {HTMLElement} containerEl
 * @param {{ dbId: number }} ctx
 */
export function mountTrackerPanel(containerEl, ctx) {
  const { dbId } = ctx;

  // ── Internal tracking ──────────────────────────────────────────────────────

  const trackCard = mkTitledCard('Status & Score', 'Track your progress');

  const statusOptions = [
    { value: '', label: 'Not tracked' },
    { value: 'reading', label: 'Reading' },
    { value: 'on_hold', label: 'On Hold' },
    { value: 'dropped', label: 'Dropped' },
    { value: 'plan_to_read', label: 'Plan to Read' },
    { value: 'completed', label: 'Completed' },
    { value: 'rereading', label: 'Rereading' },
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

  trackCard.appendChild(mkItem(mkRow('Sync enabled', 'Sync this manga with external trackers', toggleLabel)));
  trackCard.appendChild(mkItem(mkRow('Status', 'Your reading status', statusSelect)));
  trackCard.appendChild(mkItem(mkRow('Score', 'Rate 0–10', scoreInput)));
  trackCard.appendChild(mkItem(mkRow('Progress', 'Chapters read / total', progressText)));
  containerEl.appendChild(trackCard);

  api.getMangaTracking(dbId).then(tracking => {
    trackingToggle.checked = tracking.tracking_enabled ?? true;
    if (tracking.status) statusSelect.value = tracking.status;
    if (tracking.score != null) scoreInput.value = String(tracking.score);
    progressText.textContent = `${tracking.chapters_read} / ${tracking.total_chapters}`;
  }).catch(() => {});

  trackingToggle.addEventListener('change', () => {
    api.setMangaTracking(dbId, { tracking_enabled: trackingToggle.checked }).catch(() => {});
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
  extBody.textContent = 'Loading trackers...';
  extCard.appendChild(extBody);
  containerEl.appendChild(extCard);

  Promise.all([api.getTrackers(), api.getTrackerMappings(dbId)])
    .then(([trackers, mappings]) => {
      extBody.textContent = '';
      const configuredTrackers = trackers.filter(t => t.configured);
      if (!configuredTrackers.length) {
        extBody.textContent = 'No trackers configured. Add OAuth app credentials in Settings → Trackers.';
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
          statusEl.textContent = 'Not linked — link in Settings';
        } else if (mapping?.tracker_manga_id) {
          statusEl.textContent = `Mapped to ID: ${mapping.tracker_manga_id}`;
        } else {
          statusEl.textContent = 'Linked but not mapped to this manga';
        }
        info.appendChild(statusEl);
        row.appendChild(info);

        const btnGroup = document.createElement('div');
        btnGroup.className = 'flex items-center gap-2 shrink-0';

        if (t.linked && !mapping?.tracker_manga_id) {
          const searchBtn = document.createElement('button');
          searchBtn.type = 'button';
          searchBtn.className = 'btn-ghost btn-sm';
          searchBtn.textContent = 'Search & Link';
          searchBtn.addEventListener('click', async () => {
            const query = prompt(`Search ${t.name} for manga title:`);
            if (!query) return;
            try {
              const results = await api.searchTrackerManga(t.id, query);
              if (!results.length) { alert('No results found.'); return; }
              const choice = prompt(
                results.map((r, i) => `${i + 1}. ${r.title} (${r.tracker_manga_id})`).join('\n') +
                '\n\nEnter number to link:'
              );
              const idx = parseInt(choice ?? '', 10) - 1;
              if (idx >= 0 && idx < results.length) {
                await api.setTrackerMapping(dbId, t.id, results[idx].tracker_manga_id);
                statusEl.textContent = `Mapped to ID: ${results[idx].tracker_manga_id}`;
              }
            } catch (err) {
              alert('Search failed: ' + (/** @type {any} */(err)?.message ?? err));
            }
          });
          btnGroup.appendChild(searchBtn);
        }

        if (t.linked && mapping?.tracker_manga_id) {
          const syncBtn = document.createElement('button');
          syncBtn.type = 'button';
          syncBtn.className = 'btn-ghost btn-sm';
          syncBtn.textContent = 'Sync';
          syncBtn.addEventListener('click', async () => {
            syncBtn.disabled = true;
            syncBtn.textContent = 'Syncing...';
            try {
              await api.syncMangaTrackers(dbId);
              syncBtn.textContent = 'Done';
              setTimeout(() => { syncBtn.textContent = 'Sync'; }, 2000);
            } catch {
              syncBtn.textContent = 'Failed';
              setTimeout(() => { syncBtn.textContent = 'Sync'; }, 2000);
            } finally { syncBtn.disabled = false; }
          });
          btnGroup.appendChild(syncBtn);

          const unlinkBtn = document.createElement('button');
          unlinkBtn.type = 'button';
          unlinkBtn.className = 'btn-ghost btn-sm text-danger';
          unlinkBtn.textContent = 'Unmap';
          unlinkBtn.addEventListener('click', async () => {
            await api.deleteTrackerMapping(dbId, t.id);
            statusEl.textContent = 'Linked but not mapped to this manga';
          });
          btnGroup.appendChild(unlinkBtn);
        }

        row.appendChild(btnGroup);
        extBody.appendChild(row);
      }
    })
    .catch(() => {
      extBody.textContent = 'Failed to load tracker info.';
    });
}

