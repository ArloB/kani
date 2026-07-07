// @ts-check

import { h, render } from 'preact';
import { useState, useEffect, useRef } from 'preact/hooks';
import htm from 'htm';
import * as api from '../../api.js';
import { t } from '../../i18n.js';
import { getState, setState } from '../../state.js';
import { showAlert } from '../modal.js';
const html = htm.bind(h);

/**
 * @param {HTMLElement} containerEl
 * @param {{ dbId: number }} ctx
 */
export function mountTrackerPanel(containerEl, ctx) {
  const mount = document.createElement('div');
  containerEl.appendChild(mount);
  render(html`<${TrackerPanel} dbId=${ctx.dbId} />`, mount);
}

// ── Internal tracking ─────────────────────────────────────────────────────────

function InternalTrackingCard({ dbId }) {
  const [trackingEnabled, setTrackingEnabled] = useState(true);
  const [notify, setNotify] = useState(true);
  const [status, setStatus] = useState('');
  const [score, setScore] = useState('');
  const [progress, setProgress] = useState('—');
  const scoreTimerRef = useRef(/** @type {ReturnType<typeof setTimeout>|null} */ (null));

  useEffect(() => {
    api.getMangaTracking(dbId).then(tracking => {
      setTrackingEnabled(tracking.tracking_enabled ?? true);
      const n = tracking.notify_new_chapters ?? true;
      setNotify(n);
      const prefs = new Map(getState('mangaNotifyPrefs'));
      prefs.set(dbId, n);
      setState('mangaNotifyPrefs', prefs);
      if (tracking.status) setStatus(tracking.status);
      if (tracking.score != null) setScore(String(tracking.score));
      setProgress(`${tracking.chapters_read} / ${tracking.total_chapters}`);
    }).catch(() => {});
  }, [dbId]);

  function handleScoreInput(e) {
    const raw = /** @type {HTMLInputElement} */ (e.target).value;
    setScore(raw);
    if (scoreTimerRef.current) clearTimeout(scoreTimerRef.current);
    scoreTimerRef.current = setTimeout(() => {
      const val = parseFloat(raw);
      if (!isNaN(val) && val >= 0 && val <= 10) {
        api.setMangaTracking(dbId, { score: val }).catch(() => {});
      }
    }, 800);
  }

  async function handleNotifyChange(e) {
    const val = /** @type {HTMLInputElement} */ (e.target).checked;
    if (val && 'Notification' in window) {
      if (Notification.permission === 'denied') {
        setNotify(false);
        await showAlert(t('manga.tracker.notify.blocked'), { title: t('manga.tracker.notify.blocked.title') });
        return;
      }
      if (Notification.permission !== 'granted') {
        const perm = await Notification.requestPermission();
        if (perm !== 'granted') { setNotify(false); return; }
      }
      if (localStorage.getItem('kani_browser_notifications') !== 'true') {
        localStorage.setItem('kani_browser_notifications', 'true');
      }
    }
    setNotify(val);
    api.setMangaTracking(dbId, { notify_new_chapters: val }).catch(() => {});
    const prefs = new Map(getState('mangaNotifyPrefs'));
    prefs.set(dbId, val);
    setState('mangaNotifyPrefs', prefs);
  }

  const statusOptions = [
    { value: '', label: t('manga.tracker.status.untracked') },
    { value: 'reading', label: t('manga.tracker.status.reading') },
    { value: 'on_hold', label: t('manga.tracker.status.on_hold') },
    { value: 'dropped', label: t('manga.tracker.status.dropped') },
    { value: 'plan_to_read', label: t('manga.tracker.status.plan_to_read') },
    { value: 'completed', label: t('manga.tracker.status.completed') },
    { value: 'rereading', label: t('manga.tracker.status.rereading') },
  ];

  return html`
    <div class="bg-surface border border-border rounded-xl p-4 md:p-6">
      <h3 class="text-sm font-semibold text-text">${t('manga.tracker.card.title')}</h3>
      <p class="text-xs text-text-muted mt-0.5">${t('manga.tracker.card.desc')}</p>
      <div class="border-t border-border-subtle mt-3 mb-4"></div>

      <${TrackingRow} label=${t('manga.tracker.sync_enabled')} desc=${t('manga.tracker.sync_enabled.desc')}>
        <label class="kani-toggle shrink-0">
          <input type="checkbox" class="kani-toggle__input" checked=${trackingEnabled} onChange=${e => {
            const val = /** @type {HTMLInputElement} */ (e.target).checked;
            setTrackingEnabled(val);
            api.setMangaTracking(dbId, { tracking_enabled: val }).catch(() => {});
          }} />
          <span class="kani-toggle__track"></span>
        </label>
      <//>

      <${TrackingRow} label=${t('manga.tracker.status')} desc=${t('manga.tracker.status.desc')}>
        <select class="bg-surface border border-border rounded-lg px-3 py-1.5 text-sm text-text shrink-0" value=${status} onChange=${e => {
          const val = /** @type {HTMLSelectElement} */ (e.target).value;
          setStatus(val);
          api.setMangaTracking(dbId, val ? { status: val } : { status: null }).catch(() => {});
        }}>
          ${statusOptions.map(o => html`<option key=${o.value} value=${o.value}>${o.label}</option>`)}
        </select>
      <//>

      <${TrackingRow} label=${t('manga.tracker.score')} desc=${t('manga.tracker.score.desc')}>
        <input
          type="number" min="0" max="10" step="0.5" placeholder="—"
          class="bg-surface border border-border rounded-lg px-3 py-1.5 text-sm text-text w-20 shrink-0"
          value=${score}
          onInput=${handleScoreInput}
        />
      <//>

      <${TrackingRow} label=${t('manga.tracker.progress')} desc=${t('manga.tracker.progress.desc')}>
        <span class="text-sm text-text-muted shrink-0">${progress}</span>
      <//>

      <${TrackingRow} label=${t('manga.tracker.notify')} desc=${t('manga.tracker.notify.desc')}>
        <label class="kani-toggle shrink-0">
          <input type="checkbox" class="kani-toggle__input" checked=${notify} onChange=${handleNotifyChange} />
          <span class="kani-toggle__track"></span>
        </label>
      <//>
    </div>
  `;
}

function TrackingRow({ label, desc, children }) {
  return html`
    <div class="py-4 first:pt-3 last:pb-3 border-b border-border-subtle last:border-b-0">
      <div class="flex items-center justify-between gap-4">
        <div>
          <p class="text-sm font-medium text-text">${label}</p>
          <p class="text-xs text-text-muted mt-0.5">${desc}</p>
        </div>
        ${children}
      </div>
    </div>
  `;
}

// ── External trackers ─────────────────────────────────────────────────────────

function ExternalTrackersCard({ dbId }) {
  const [state, setState_] = useState(/** @type {'loading'|'ready'|'error'} */ ('loading'));
  const [trackers, setTrackers] = useState(/** @type {any[]} */ ([]));
  const [mappings, setMappings] = useState(/** @type {any[]} */ ([]));

  useEffect(() => {
    Promise.all([api.getTrackers(), api.getTrackerMappings(dbId)])
      .then(([ts, ms]) => {
        setTrackers(ts.filter(t => t.configured));
        setMappings(Array.isArray(ms) ? ms : []);
        setState_('ready');
      })
      .catch(() => setState_('error'));
  }, [dbId]);

  return html`
    <div class="bg-surface border border-border rounded-xl px-4 md:px-6 py-1">
      ${state === 'loading' && html`<p class="py-3 text-sm text-text-muted">${t('manga.tracker.loading')}</p>`}
      ${state === 'error' && html`<p class="py-3 text-sm text-text-muted">${t('manga.tracker.load_failed')}</p>`}
      ${state === 'ready' && (trackers.length === 0
        ? html`<p class="py-3 text-sm text-text-muted">${t('manga.tracker.none_configured')}</p>`
        : trackers.map(tr => html`<${TrackerRow}
            key=${tr.id}
            tracker=${tr}
            mapping=${mappings.find(m => m.tracker_id === tr.id)}
            dbId=${dbId}
            onMappingSet=${(id) => setMappings(prev => {
              const next = prev.filter(m => m.tracker_id !== tr.id);
              next.push({ tracker_id: tr.id, tracker_manga_id: id });
              return next;
            })}
            onMappingClear=${() => setMappings(prev => prev.filter(m => m.tracker_id !== tr.id))}
          />`)
      )}
    </div>
  `;
}

function TrackerRow({ tracker: tr, mapping, dbId, onMappingSet, onMappingClear }) {
  const [syncLabel, setSyncLabel] = useState(/** @type {string|null} */ (null));
  const [syncing, setSyncing] = useState(false);

  let statusText;
  if (!tr.linked) {
    statusText = t('manga.tracker.not_linked');
  } else if (mapping?.tracker_manga_id) {
    statusText = t('manga.tracker.mapped', { id: mapping.tracker_manga_id });
  } else {
    statusText = t('manga.tracker.linked_not_mapped');
  }

  async function handleSearch() {
    const query = prompt(t('manga.tracker.search_prompt', { name: tr.name }));
    if (!query) return;
    try {
      const results = await api.searchTrackerManga(tr.id, query);
      if (!results.length) { await showAlert(t('manga.tracker.no_results'), { title: t('manga.tracker.search.title') }); return; }
      const choice = prompt(
        results.map((r, i) => `${i + 1}. ${r.title} (${r.tracker_manga_id})`).join('\n') +
        '\n\n' + t('manga.tracker.enter_number')
      );
      const idx = parseInt(choice ?? '', 10) - 1;
      if (idx >= 0 && idx < results.length) {
        await api.setTrackerMapping(dbId, tr.id, results[idx].tracker_manga_id);
        onMappingSet(results[idx].tracker_manga_id);
      }
    } catch (err) {
      await showAlert(t('manga.tracker.search_failed', { message: /** @type {any} */(err)?.message ?? String(err) }), { title: t('manga.tracker.error.title') });
    }
  }

  async function handleSync() {
    setSyncing(true);
    setSyncLabel(t('manga.tracker.syncing'));
    try {
      await api.syncMangaTrackers(dbId);
      setSyncLabel(t('manga.tracker.sync_done'));
      setTimeout(() => setSyncLabel(null), 2000);
    } catch {
      setSyncLabel(t('manga.tracker.sync_failed_btn'));
      setTimeout(() => setSyncLabel(null), 2000);
    } finally { setSyncing(false); }
  }

  async function handleUnlink() {
    await api.deleteTrackerMapping(dbId, tr.id);
    onMappingClear();
  }

  return html`
    <div class="flex items-center justify-between gap-4 py-3 border-b border-border-subtle last:border-b-0">
      <div>
        <p class="text-sm font-medium text-text">${tr.name}</p>
        <p class="text-xs text-text-muted mt-0.5">${statusText}</p>
      </div>
      <div class="flex items-center gap-2 shrink-0">
        ${tr.linked && !mapping?.tracker_manga_id && html`
          <button type="button" class="btn-ghost btn-sm" onClick=${handleSearch}>${t('manga.tracker.search_link')}</button>
        `}
        ${tr.linked && mapping?.tracker_manga_id && html`
          <button type="button" class="btn-ghost btn-sm" disabled=${syncing} onClick=${handleSync}>
            ${syncLabel ?? t('manga.tracker.sync')}
          </button>
          <button type="button" class="btn-ghost btn-sm text-danger" onClick=${handleUnlink}>
            ${t('manga.tracker.unmap')}
          </button>
        `}
      </div>
    </div>
  `;
}

// ── Root ──────────────────────────────────────────────────────────────────────

function TrackerPanel({ dbId }) {
  return html`
    <div class="flex flex-col gap-4">
      <${InternalTrackingCard} dbId=${dbId} />
      <${ExternalTrackersCard} dbId=${dbId} />
    </div>
  `;
}
