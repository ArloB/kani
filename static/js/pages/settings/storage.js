// @ts-check
// Settings — Storage: disk usage and the library integrity scrub (admin only).

import { h } from 'preact';
import { useState, useEffect } from 'preact/hooks';
import htm from 'htm';
import * as api from '../../api.js';
import { EmptyState } from '../../components/empty-state.js';
import { t } from '../../i18n.js';
import { showApiError, showToast } from '../../components/toast.js';
import { SettingsGroup, SettingsRow } from './_shared.js';
import { formatBytes, formatRelativeTime } from '../../utils.js';
import { useSSE } from '../../hooks/use-sse.js';
import { showConfirm } from '../../components/modal.js';

const html = htm.bind(h);
const fmt = formatBytes;

function UsageGroup() {
  const [state, setState] = useState(
    /** @type {{ status: string, stats: any, error: string }} */ ({
      status: 'loading',
      stats: null,
      error: '',
    }),
  );

  useEffect(() => {
    (async () => {
      try {
        const stats = await api.getAdminStorageStats();
        setState({ status: 'ready', stats, error: '' });
      } catch (e) {
        setState({ status: 'error', stats: null, error: e?.message ?? t('storage.error.load_failed') });
      }
    })();
  }, []);

  const s = state.stats;
  /** @type {[string, any, boolean][]} */
  const rows = s
    ? [
        [t('storage.stat.library'), s.library_used_bytes, true],
        [t('storage.stat.chapters'), s.chapter_used_bytes, true],
        [t('storage.stat.covers'), s.cover_used_bytes, true],
        [t('storage.stat.data'), s.data_used_bytes, true],
        [t('storage.stat.free'), s.library_free_bytes, true],
        [t('storage.stat.manga'), s.total_manga, false],
        [t('storage.stat.chapters_count'), s.total_chapters, false],
      ]
    : [];

  return html`
    <${SettingsGroup} label=${t('storage.group.usage')}>
      ${state.status === 'loading'
        ? html`<div class="px-4 py-3 text-sm text-text-muted">${t('common.loading')}</div>`
        : state.status === 'error'
        ? html`<div class="px-4 py-3 text-sm text-danger">${state.error}</div>`
        : rows.map(
            ([label, value, isBytes]) => html`
              <${SettingsRow} key=${label} label=${label}>
                <span class="text-sm font-medium text-text"
                  >${isBytes ? fmt(value) : (value ?? '—').toString()}</span
                >
              <//>
            `,
          )}
    <//>
  `;
}

function Stat({ label, value }) {
  return html`
    <div class="flex items-center justify-between gap-4 px-4 py-2.5 text-sm">
      <span class="text-text-muted">${label}</span>
      <span class="font-medium text-text">${String(value)}</span>
    </div>
  `;
}

function ScrubGroup() {
  const [running, setRunning] = useState(false);
  const [depth, setDepth] = useState('quick');
  const [fix, setFix] = useState(false);
  const [last, setLast] = useState(/** @type {any} */ (undefined));

  const load = async () => {
    try {
      setLast(await api.getLastScrub());
    } catch (e) {
      showApiError(e);
      setLast(null);
    }
  };

  useEffect(() => {
    load();
  }, []);

  // The scrub is a job, so completion arrives over SSE rather than in the
  // submit response.
  useSSE('job_completed', (/** @type {any} */ ev) => {
    if (ev?.job_type === 'integrity_scrub') {
      setRunning(false);
      load();
    }
  });

  const run = async () => {
    setRunning(true);
    try {
      await api.runScrub(depth, fix);
      showToast(t('storage.scrub.started'), { type: 'success' });
    } catch (e) {
      setRunning(false);
      showApiError(e);
    }
  };

  const orphans = last?.report?.orphaned_files ?? [];

  const removeOrphans = async () => {
    const preview = await api.deleteOrphans(orphans, true);
    const ok = await showConfirm(
      t('storage.scrub.orphans.confirm', { n: preview.removed_count }),
      { title: t('storage.scrub.orphans.delete'), confirmLabel: t('common.delete'), danger: true },
    );
    if (!ok) return;
    try {
      const res = await api.deleteOrphans(orphans, false);
      showToast(t('storage.scrub.orphans.removed', { n: res.removed_count }), { type: 'success' });
      await load();
    } catch (e) {
      showApiError(e);
    }
  };

  const r = last?.report;
  const rows = r
    ? [
        html`<${Stat} label=${t('storage.scrub.checked')} value=${r.checked} />`,
        html`<${Stat} label=${t('storage.scrub.ok')} value=${r.ok} />`,
        html`<${Stat} label=${t('storage.scrub.corrupt')} value=${r.corrupt.length} />`,
        html`<${Stat} label=${t('storage.scrub.missing')} value=${r.missing_files.length} />`,
        html`<${Stat} label=${t('storage.scrub.drift')} value=${r.path_drift.length} />`,
        html`<${Stat} label=${t('storage.scrub.unhashed')} value=${r.unhashed} />`,
        html`<${Stat} label=${t('storage.scrub.duplicates')} value=${r.exact_duplicates.length} />`,
        html`<${Stat} label=${t('storage.integrity.cover_mismatches')} value=${r.cover_mismatches.length} />`,
        html`<${Stat} label=${t('storage.scrub.orphaned')} value=${orphans.length} />`,
      ]
    : null;

  return html`
    <${SettingsGroup} label=${t('storage.group.integrity')}>
      <p class="text-xs text-text-muted px-4 py-2">${t('storage.scrub.desc')}</p>

      <div class="flex flex-wrap items-center gap-2 px-4 py-3 border-t border-border-subtle">
        <select
          class="input text-sm w-auto"
          value=${depth}
          disabled=${running}
          onChange=${(/** @type {any} */ e) => setDepth(e.target.value)}
        >
          <option value="quick">${t('storage.scrub.depth.quick')}</option>
          <option value="deep">${t('storage.scrub.depth.deep')}</option>
        </select>
        <label class="flex items-center gap-2 text-sm cursor-pointer">
          <input
            type="checkbox"
            class="accent-accent cursor-pointer"
            checked=${fix}
            disabled=${running}
            onChange=${(/** @type {any} */ e) => setFix(e.target.checked)}
          />
          ${t('storage.scrub.repair')}
        </label>
        <button type="button" class="btn-secondary btn-sm" disabled=${running} onClick=${run}>
          ${running ? t('storage.scrub.running') : t('storage.scrub.run')}
        </button>
      </div>

      <p class="text-xs text-text-muted px-4 pb-2">${t('storage.scrub.repair.desc')}</p>

      ${last === undefined
        ? null
        : last === null
          ? html`<p class="px-4 py-3 text-sm text-text-muted">${t('storage.scrub.never')}</p>`
          : html`
              <div class="px-4 py-2 text-xs text-text-muted border-t border-border-subtle">
                ${t('storage.scrub.last', {
                  depth: t(`storage.scrub.depth.${last.depth}`),
                  when: formatRelativeTime(new Date(last.created_at * 1000)),
                })}
              </div>
              <div class="flex flex-col gap-0 divide-y divide-border-subtle">${rows}</div>
              ${orphans.length > 0
                ? html`<div
                    class="flex items-center justify-between gap-2 px-4 py-3 border-t border-border-subtle"
                  >
                    <span class="text-xs text-text-muted">${t('storage.scrub.orphans.desc')}</span>
                    <button type="button" class="btn-ghost btn-sm text-danger" onClick=${removeOrphans}>
                      ${t('storage.scrub.orphans.delete')}
                    </button>
                  </div>`
                : null}
            `}
    <//>
  `;
}

function HistoryGroup() {
  const [state, setState] = useState(
    /** @type {{ status: string, rows: any[], error: string }} */ ({
      status: 'loading',
      rows: [],
      error: '',
    }),
  );

  useEffect(() => {
    (async () => {
      try {
        const rows = await api.getAdminStorageStatsHistory();
        setState({ status: 'ready', rows: Array.isArray(rows) ? rows : [], error: '' });
      } catch (e) {
        setState({ status: 'error', rows: [], error: e?.message ?? t('storage.error.history_failed') });
      }
    })();
  }, []);

  return html`
    <${SettingsGroup} label=${t('storage.history.group')}>
      ${state.status === 'loading'
        ? html`<p class="px-4 py-3 text-sm text-text-muted">${t('common.loading')}</p>`
        : state.status === 'error'
        ? html`<p class="px-4 py-3 text-sm text-danger">${state.error}</p>`
        : state.rows.length === 0
        ? html`<${EmptyState} title=${t('storage.history.empty')} compact=${true} />`
        : html`
            <div class="overflow-x-auto">
              <table class="data-table">
                <thead>
                  <tr>
                    <th>${t('storage.history.date')}</th>
                    <th class="num">${t('storage.history.chapters')}</th>
                    <th class="num">${t('storage.history.covers')}</th>
                    <th class="num">${t('storage.history.free')}</th>
                  </tr>
                </thead>
                <tbody>
                  ${state.rows.slice(0, 30).map(
                    (r, i) => html`
                      <tr key=${i}>
                        <td class="muted">
                          ${r.captured_at ? new Date(r.captured_at).toLocaleDateString() : '—'}
                        </td>
                        <td class="num font-medium">${fmt(r.chapter_used_bytes)}</td>
                        <td class="num">${fmt(r.cover_used_bytes)}</td>
                        <td class="num">${fmt(r.free_bytes)}</td>
                      </tr>
                    `,
                  )}
                </tbody>
              </table>
            </div>
          `}
    <//>
  `;
}

export function StorageSection() {
  return html`
    <${UsageGroup} />
    <${ScrubGroup} />
    <${HistoryGroup} />
  `;
}
