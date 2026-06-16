// @ts-check
// Admin Jobs page — background job queue and history.

import { h, render } from 'preact';
import { useState, useEffect, useRef } from 'preact/hooks';
import htm from 'htm';
import { hasPermission } from '../../state.js';
import { getJobs, cancelJob } from '../../api.js';
import { showApiError, showToast } from '../../components/toast.js';
import { setPageHeader, clearPageHeader } from '../../components/app-header.js';
import { createEmptyState } from '../../components/empty-state.js';
import { Icon } from '../../components/icon.js';
import { iconCheck, iconWarning, iconDocument } from '../../icons.js';
import { t } from '../../i18n.js';
const html = htm.bind(h);

// ── Helpers ───────────────────────────────────────────────────────────────────

/** @param {number} ts */
function _fmtTime(ts) {
  if (!ts) return '';
  return new Date(ts * 1000).toLocaleString();
}

/** @param {string} jobType */
function _jobLabel(jobType) {
  const key = `jobs.type.${jobType}`;
  const val = t(key);
  return val === key ? jobType.replace(/_/g, ' ') : val;
}

/** @param {any} job */
function _pct(job) {
  const p = job.progress;
  if (!p || !p.total) return 0;
  return Math.round((p.current / p.total) * 100);
}

// ── Empty state adapter ───────────────────────────────────────────────────────

function EmptyState({ icon, title, subtitle }) {
  const ref = useRef(/** @type {HTMLDivElement|null} */ (null));
  useEffect(() => {
    if (!ref.current) return;
    ref.current.innerHTML = '';
    ref.current.appendChild(createEmptyState({ icon, title, subtitle }));
  }, []);
  return html`<div ref=${ref} />`;
}

// ── Row components ─────────────────────────────────────────────────────────────

function ActiveJobRow({ job, onCancel }) {
  const pct = _pct(job);
  const spinning = pct === 0 || job.status === 'pending';
  const circ = 75.4;

  return html`
    <div class="flex flex-col gap-2 px-4 py-3 border-b border-border-subtle last:border-b-0">
      <div class="flex items-center gap-3">
        <div class="shrink-0 w-7 h-7 flex items-center justify-center rounded-full text-accent">
          <svg class=${'w-4 h-4 ' + (spinning ? 'dl-ring-spin' : '-rotate-90')} viewBox="0 0 32 32" aria-hidden="true">
            <circle cx="16" cy="16" r="12" fill="none" stroke="currentColor" stroke-width="2.5" opacity="0.25" />
            <circle cx="16" cy="16" r="12" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"
              stroke-dasharray=${spinning ? '56.5 18.9' : String(circ)}
              stroke-dashoffset=${spinning ? undefined : String(circ - circ * pct / 100)}
              style=${{ transition: 'stroke-dashoffset 0.3s ease' }}
            />
          </svg>
        </div>
        <div class="flex-1 min-w-0">
          <p class="text-xs text-text-muted truncate">${_jobLabel(job.job_type)}</p>
          <p class="text-sm text-text truncate" title=${job.description}>${job.description}</p>
          ${job.progress?.message && html`<p class="text-xs text-text-muted mt-0.5 truncate">${job.progress.message}</p>`}
          ${job.progress?.total > 0 && html`<p class="text-xs text-text-muted mt-0.5">${job.progress.current} / ${job.progress.total}</p>`}
        </div>
        <span class=${'text-xs px-1.5 py-0.5 rounded-full font-medium shrink-0 ' + (job.status === 'running' ? 'bg-accent/15 text-accent' : 'bg-surface-2 text-text-muted')}>
          ${job.status}
        </span>
        <button class="btn-ghost btn-sm shrink-0 text-danger" onClick=${() => onCancel(job.id)} aria-label=${t('jobs.cancel')}>
          ${t('jobs.cancel')}
        </button>
      </div>
      <div class="h-1 rounded-full bg-surface-2 overflow-hidden ml-10">
        <div class="h-full rounded-full bg-accent transition-[width] duration-300" style=${{ width: (spinning ? 30 : pct) + '%' }}></div>
      </div>
    </div>
  `;
}

function CompletedJobRow({ job }) {
  return html`
    <div class="flex items-center gap-3 px-4 py-3 border-b border-border-subtle last:border-b-0">
      <div class="shrink-0 w-7 h-7 flex items-center justify-center icon-sm text-success">
        <${Icon} svg=${iconCheck} />
      </div>
      <div class="flex-1 min-w-0">
        <p class="text-xs text-text-muted truncate">${_jobLabel(job.job_type)}</p>
        <p class="text-sm text-text truncate" title=${job.description}>${job.description}</p>
        ${job.completed_at && html`<p class="text-xs text-text-muted mt-0.5">${_fmtTime(job.completed_at)}</p>`}
      </div>
    </div>
  `;
}

function FailedJobRow({ job }) {
  const [expanded, setExpanded] = useState(false);
  const errMsg = job.error?.message ?? (typeof job.error === 'string' ? job.error : null);

  return html`
    <div class="flex flex-col border-b border-border-subtle last:border-b-0">
      <div class="flex items-center gap-3 px-4 py-3">
        <div class="shrink-0 w-7 h-7 flex items-center justify-center icon-sm text-danger">
          <${Icon} svg=${iconWarning} />
        </div>
        <div class="flex-1 min-w-0">
          <p class="text-xs text-text-muted truncate">${_jobLabel(job.job_type)}</p>
          <p class="text-sm text-text truncate" title=${job.description}>${job.description}</p>
          ${errMsg && html`<p class="text-xs text-danger mt-0.5 truncate">${errMsg}</p>`}
          ${job.completed_at && html`<p class="text-xs text-text-muted mt-0.5">${_fmtTime(job.completed_at)}</p>`}
        </div>
        ${errMsg && html`
          <button class="btn-ghost btn-xs shrink-0 text-text-muted" onClick=${() => setExpanded(v => !v)}>
            ${expanded ? t('jobs.error.hide') : t('jobs.error.details')}
          </button>
        `}
      </div>
      ${expanded && errMsg && html`
        <div class="px-4 pb-3 ml-10">
          <pre class="text-xs text-danger bg-danger/5 border border-danger/20 rounded-md p-3 overflow-auto max-h-32 whitespace-pre-wrap">${errMsg}</pre>
        </div>
      `}
    </div>
  `;
}

// ── Tab button ────────────────────────────────────────────────────────────────

function TabBtn({ id, label, count, activeTab, onClick }) {
  const isActive = activeTab === id;
  return html`
    <button
      type="button"
      role="tab"
      aria-selected=${isActive}
      class=${'px-4 py-2 text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent rounded-t-md'
        + (isActive ? ' text-accent border-b-2 border-accent' : ' text-text-muted hover:text-text')}
      onClick=${() => onClick(id)}
    >
      ${label}${count > 0 ? html` <span class="ml-1 text-xs opacity-70">${count}</span>` : ''}
    </button>
  `;
}

// ── Page root ─────────────────────────────────────────────────────────────────

function JobsPage() {
  const [tab, setTab] = useState(/** @type {'active'|'completed'|'failed'} */ ('active'));
  const [active, setActive] = useState(/** @type {any[]} */ ([]));
  const [completed, setCompleted] = useState(/** @type {any[]} */ ([]));
  const [failed, setFailed] = useState(/** @type {any[]} */ ([]));

  useEffect(() => {
    getJobs({ limit: 100 }).then(items => {
      if (!Array.isArray(items)) return;
      setActive(items.filter(j => j.status === 'running' || j.status === 'pending'));
      setCompleted(items.filter(j => j.status === 'completed'));
      setFailed(items.filter(j => j.status === 'failed' || j.status === 'cancelled'));
    }).catch(showApiError);
  }, []);

  useEffect(() => {
    function onSSE(/** @type {CustomEvent} */ e) {
      const data = /** @type {any} */ (e).detail;
      const type = data.type;
      if (type === 'job_started') {
        setActive(prev => {
          if (prev.some(j => j.id === data.job_id)) return prev;
          return [{ id: data.job_id, job_type: data.job_type, status: 'running', description: data.description, progress: null }, ...prev];
        });
      } else if (type === 'job_progress') {
        setActive(prev => prev.map(j => j.id === data.job_id
          ? { ...j, status: 'running', progress: { current: data.current, total: data.total, message: data.message } }
          : j));
      } else if (type === 'job_completed') {
        setActive(prev => prev.filter(j => j.id !== data.job_id));
        setCompleted(prev => [{ id: data.job_id, job_type: data.job_type, status: 'completed', description: data.description, completed_at: Math.floor(Date.now() / 1000) }, ...prev].slice(0, 100));
      } else if (type === 'job_failed') {
        setActive(prev => prev.filter(j => j.id !== data.job_id));
        setFailed(prev => [{ id: data.job_id, job_type: data.job_type, status: 'failed', description: '', completed_at: Math.floor(Date.now() / 1000), error: { message: data.message } }, ...prev].slice(0, 100));
      } else if (type === 'job_cancelled') {
        setActive(prev => prev.filter(j => j.id !== data.job_id));
      }
    }
    window.addEventListener('kani:sse', onSSE);
    return () => window.removeEventListener('kani:sse', onSSE);
  }, []);

  async function handleCancel(id) {
    try {
      await cancelJob(id);
      showToast(t('jobs.action.cancelled'));
    } catch (err) {
      showApiError(err);
    }
  }

  return html`
    <div class="max-w-page mx-auto w-full px-4 md:px-6 py-6 flex flex-col gap-6">
      <div class="flex items-center gap-1 border-b border-border -mb-3 min-h-9" role="tablist">
        <${TabBtn} id="active"    label=${t('jobs.tab.active')}    count=${active.length}  activeTab=${tab} onClick=${setTab} />
        <${TabBtn} id="completed" label=${t('jobs.tab.completed')} count=${0}              activeTab=${tab} onClick=${setTab} />
        <${TabBtn} id="failed"    label=${t('jobs.tab.failed')}    count=${failed.length}  activeTab=${tab} onClick=${setTab} />
      </div>

      ${tab === 'active' && html`
        <div class="bg-surface border border-border rounded-xl overflow-hidden">
          ${active.length === 0
            ? html`<${EmptyState} icon=${iconDocument} title=${t('jobs.empty.active')} subtitle=${t('jobs.empty.active.desc')} />`
            : active.map(j => html`<${ActiveJobRow} key=${j.id} job=${j} onCancel=${handleCancel} />`)
          }
        </div>
      `}

      ${tab === 'completed' && html`
        <div class="bg-surface border border-border rounded-xl overflow-hidden">
          ${completed.length === 0
            ? html`<${EmptyState} icon=${iconCheck} title=${t('jobs.empty.completed')} subtitle=${t('jobs.empty.completed.desc')} />`
            : completed.map(j => html`<${CompletedJobRow} key=${j.id} job=${j} />`)
          }
        </div>
      `}

      ${tab === 'failed' && html`
        <div class="bg-surface border border-border rounded-xl overflow-hidden">
          ${failed.length === 0
            ? html`<${EmptyState} icon=${iconWarning} title=${t('jobs.empty.failed')} />`
            : failed.map(j => html`<${FailedJobRow} key=${j.id} job=${j} />`)
          }
        </div>
      `}
    </div>
  `;
}

// ── Page lifecycle ────────────────────────────────────────────────────────────

/** @param {HTMLElement} container */
export async function init(container) {
  document.title = t('jobs.title') + ' - Kani';

  if (!hasPermission('admin:jobs')) {
    container.innerHTML = `
      <div class="flex flex-col items-center justify-center gap-3 py-20 text-text-muted">
        <p class="text-base font-medium text-text">${t('jobs.denied.title')}</p>
        <p class="text-sm">${t('jobs.denied.desc')}</p>
      </div>
    `;
    return;
  }

  setPageHeader({ crumbs: [{ label: t('jobs.title') }] });
  render(html`<${JobsPage} />`, container);
}

/** @param {HTMLElement} container */
export function destroy(container) {
  clearPageHeader();
  render(null, container);
}
