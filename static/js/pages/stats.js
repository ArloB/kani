// @ts-check
// Reading Statistics dashboard — widget registry pattern for extensibility.
// New stat blocks: implement a widget object and append it to WIDGETS.

import * as api from '../api.js';
import { navigate } from '../router.js';
import { escapeHtml } from '../utils.js';
import { setPageHeader, clearPageHeader } from '../components/app-header.js';
import { startLoading, finishLoading } from '../components/page-loading-bar.js';
import { createErrorState } from '../components/error-state.js';

// ── CSS variable resolver ─────────────────────────────────────────────────────
// Chart.js renders on <canvas> and cannot read CSS custom properties.

/** @param {string} varName e.g. '--color-text-muted' */
function cssVar(varName) {
  return getComputedStyle(document.documentElement).getPropertyValue(varName).trim() || '#888';
}

// ── Widget registry ───────────────────────────────────────────────────────────
// Each widget: { id: string, render(container, data) → { destroy() } }
// Adding a new widget requires only appending to WIDGETS — no other changes.

const WIDGETS = [
  summaryWidget(),
  dailyActivityWidget(),
  topMangaWidget(),
  genreBreakdownWidget(),
];

// ── Module state ──────────────────────────────────────────────────────────────

/** @type {AbortController | null} */       let _abort = null;
/** @type {Array<{ destroy(): void }>} */   let _widgetInstances = [];
/** @type {number} */                       let _period = 90;
/** @type {HTMLElement | null} */           let _contentEl = null;

// ── Init / Destroy ────────────────────────────────────────────────────────────

/** @param {HTMLElement} container */
export async function init(container) {
  document.title = 'Statistics - Kani';
  _period = 90;
  _widgetInstances = [];

  // Period picker action
  const picker = document.createElement('select');
  picker.className = 'input input-sm w-28';
  for (const [v, l] of [[30, '30 days'], [90, '90 days'], [180, '180 days'], [365, '1 year']]) {
    const opt = document.createElement('option');
    opt.value = String(v);
    opt.textContent = l;
    if (v === _period) opt.selected = true;
    picker.appendChild(opt);
  }
  picker.addEventListener('change', () => {
    _period = Number(picker.value);
    _load();
  });

  setPageHeader({ crumbs: [{ label: 'Statistics' }], actions: picker });

  container.innerHTML = `
    <div class="max-w-page mx-auto w-full px-4 md:px-6 py-4 md:py-6">
      <div id="stats-content"></div>
    </div>
  `;

  _contentEl = /** @type {HTMLElement} */ (container.querySelector('#stats-content'));
  await _load();
}

/** @param {HTMLElement} _c */
export function destroy(_c) {
  _abort?.abort();
  _abort = null;
  _destroyWidgets();
  clearPageHeader();
}

// ── Load ──────────────────────────────────────────────────────────────────────

async function _load() {
  if (!_contentEl) return;
  _abort?.abort();
  _abort = new AbortController();
  _destroyWidgets();
  _contentEl.innerHTML = _buildSkeleton();
  startLoading();

  try {
    const stats = await api.getReadingStats(_period);
    _contentEl.innerHTML = '';
    _renderWidgets(stats);
  } catch (err) {
    _contentEl.innerHTML = '';
    createErrorState(_contentEl, { message: err?.message ?? 'Failed to load statistics' });
  } finally {
    finishLoading();
  }
}

/** @param {any} stats */
function _renderWidgets(stats) {
  if (!_contentEl) return;
  const grid = document.createElement('div');
  grid.className = 'flex flex-col gap-6';
  _contentEl.appendChild(grid);

  for (const widget of WIDGETS) {
    const slot = document.createElement('div');
    grid.appendChild(slot);
    try {
      const instance = widget.render(slot, stats);
      _widgetInstances.push(instance);
    } catch (e) {
      console.error(`Widget ${widget.id} render error:`, e);
    }
  }
}

function _destroyWidgets() {
  for (const inst of _widgetInstances) {
    try { inst.destroy(); } catch { /* ignore */ }
  }
  _widgetInstances = [];
}

function _buildSkeleton() {
  return `
    <div class="flex flex-col gap-6 animate-pulse">
      <div class="grid grid-cols-2 md:grid-cols-4 gap-4">
        ${Array(4).fill('<div class="h-24 rounded-xl bg-surface-2"></div>').join('')}
      </div>
      <div class="h-56 rounded-xl bg-surface-2"></div>
      <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
        <div class="h-64 rounded-xl bg-surface-2"></div>
        <div class="h-64 rounded-xl bg-surface-2"></div>
      </div>
    </div>
  `;
}

// ── Widget: Summary cards ─────────────────────────────────────────────────────

function summaryWidget() {
  return {
    id: 'summary',
    /** @param {HTMLElement} container @param {any} data */
    render(container, data) {
      const cards = [
        { label: 'Chapters Read',    value: data.total_chapters_read ?? 0 },
        { label: 'Manga Read',       value: data.total_manga_read    ?? 0 },
        { label: 'Completed Manga',  value: data.completed_manga     ?? 0 },
        { label: 'Current Streak',   value: `${data.current_streak ?? 0}d` },
      ];

      container.innerHTML = `
        <div class="grid grid-cols-2 md:grid-cols-4 gap-4">
          ${cards.map(c => `
            <div class="card p-4 flex flex-col gap-1">
              <span class="text-2xl font-bold text-text">${escapeHtml(String(c.value))}</span>
              <span class="text-sm text-text-muted">${escapeHtml(c.label)}</span>
            </div>
          `).join('')}
        </div>
        ${data.longest_streak ? `<p class="text-sm text-text-muted mt-2">Longest streak: <span class="font-medium text-text">${data.longest_streak}d</span></p>` : ''}
      `;
      return { destroy() { container.innerHTML = ''; } };
    },
  };
}

// ── Widget: Daily activity bar chart ──────────────────────────────────────────

function dailyActivityWidget() {
  /** @type {any} */ let _chart = null;

  return {
    id: 'daily_activity',
    /** @param {HTMLElement} container @param {any} data */
    render(container, data) {
      const rows = data.daily_activity ?? [];
      if (!rows.length) {
        container.innerHTML = `<div class="card p-4 text-text-muted text-sm">No reading activity in selected period.</div>`;
        return { destroy() { container.innerHTML = ''; } };
      }

      container.innerHTML = `
        <div class="card p-4">
          <h2 class="text-base font-semibold mb-4">Daily Activity</h2>
          <div class="relative h-48">
            <canvas id="chart-daily" class="w-full h-full"></canvas>
          </div>
        </div>
      `;

      const canvas = /** @type {HTMLCanvasElement} */ (container.querySelector('#chart-daily'));

      // Lazy-load Chart.js (vendored)
      _loadChartJs().then(Chart => {
        if (!canvas.isConnected) return;
        _chart = new Chart(canvas.getContext('2d'), {
          type: 'bar',
          data: {
            labels: rows.map((r) => r.date),
            datasets: [{
              label: 'Chapters',
              data: rows.map((r) => r.chapters_read),
              backgroundColor: 'rgba(99, 102, 241, 0.6)',
              borderRadius: 3,
            }],
          },
          options: {
            responsive: true,
            maintainAspectRatio: false,
            plugins: { legend: { display: false } },
            scales: {
              x: { ticks: { maxTicksLimit: 8, color: cssVar('--color-text-muted') }, grid: { display: false } },
              y: { beginAtZero: true, ticks: { precision: 0, color: cssVar('--color-text-muted') } },
            },
          },
        });
      }).catch(() => {
        // Chart.js unavailable — show plain text fallback
        container.innerHTML = `<div class="card p-4 text-sm text-text-muted">Chart.js not available. Run <code>kani-cli setup</code> to download vendor files.</div>`;
      });

      return {
        destroy() {
          _chart?.destroy();
          _chart = null;
          container.innerHTML = '';
        },
      };
    },
  };
}

// ── Widget: Top manga horizontal bars ─────────────────────────────────────────

function topMangaWidget() {
  /** @type {any} */ let _chart = null;

  return {
    id: 'top_manga',
    /** @param {HTMLElement} container @param {any} data */
    render(container, data) {
      const rows = (data.top_manga ?? []).slice(0, 10);
      if (!rows.length) {
        container.innerHTML = '';
        return { destroy() {} };
      }

      container.innerHTML = `
        <div class="card p-4">
          <h2 class="text-base font-semibold mb-4">Most Read Manga</h2>
          <div class="relative h-64">
            <canvas id="chart-top-manga" class="w-full h-full"></canvas>
          </div>
        </div>
      `;

      const canvas = /** @type {HTMLCanvasElement} */ (container.querySelector('#chart-top-manga'));

      _loadChartJs().then(Chart => {
        if (!canvas.isConnected) return;
        _chart = new Chart(canvas.getContext('2d'), {
          type: 'bar',
          data: {
            labels: rows.map((r) => r.manga_name),
            datasets: [{
              label: 'Chapters',
              data: rows.map((r) => r.chapters_read),
              backgroundColor: 'rgba(16, 185, 129, 0.65)',
              borderRadius: 3,
            }],
          },
          options: {
            indexAxis: 'y',
            responsive: true,
            maintainAspectRatio: false,
            plugins: { legend: { display: false } },
            scales: {
              x: { beginAtZero: true, ticks: { precision: 0, color: cssVar('--color-text-muted') } },
              y: { ticks: { color: cssVar('--color-text-muted') } },
            },
            onClick: (_evt, elements) => {
              if (elements[0]) {
                const idx = elements[0].index;
                const mangaId = rows[idx]?.manga_id;
                if (mangaId) navigate(`/manga/${mangaId}`);
              }
            },
          },
        });
      }).catch(() => {
        // Fallback: plain list
        container.innerHTML = `
          <div class="card p-4">
            <h2 class="text-base font-semibold mb-3">Most Read Manga</h2>
            <ol class="flex flex-col gap-1 text-sm">
              ${rows.map((r, i) => `<li class="flex gap-2"><span class="text-text-muted w-4">${i + 1}.</span><a href="/manga/${r.manga_id}" class="link flex-1 truncate">${escapeHtml(r.manga_name)}</a><span class="text-text-muted">${r.chapters_read}ch</span></li>`).join('')}
            </ol>
          </div>`;
      });

      return {
        destroy() {
          _chart?.destroy();
          _chart = null;
          container.innerHTML = '';
        },
      };
    },
  };
}

// ── Widget: Genre breakdown doughnut ──────────────────────────────────────────

function genreBreakdownWidget() {
  /** @type {any} */ let _chart = null;

  return {
    id: 'genre_breakdown',
    /** @param {HTMLElement} container @param {any} data */
    render(container, data) {
      const rows = (data.genre_breakdown ?? []).slice(0, 12);
      if (!rows.length) {
        container.innerHTML = '';
        return { destroy() {} };
      }

      container.innerHTML = `
        <div class="card p-4">
          <h2 class="text-base font-semibold mb-4">Genre Breakdown</h2>
          <div class="relative h-64">
            <canvas id="chart-genre" class="w-full h-full"></canvas>
          </div>
        </div>
      `;

      const canvas = /** @type {HTMLCanvasElement} */ (container.querySelector('#chart-genre'));

      const PALETTE = [
        '#6366f1','#10b981','#f59e0b','#ef4444','#3b82f6','#8b5cf6',
        '#ec4899','#14b8a6','#f97316','#84cc16','#06b6d4','#a855f7',
      ];

      _loadChartJs().then(Chart => {
        if (!canvas.isConnected) return;
        _chart = new Chart(canvas.getContext('2d'), {
          type: 'doughnut',
          data: {
            labels: rows.map((r) => r.genre),
            datasets: [{
              data: rows.map((r) => r.chapters_read),
              backgroundColor: rows.map((_, i) => PALETTE[i % PALETTE.length]),
              borderWidth: 1,
            }],
          },
          options: {
            responsive: true,
            maintainAspectRatio: false,
            plugins: {
              legend: { position: 'right', labels: { color: cssVar('--color-text-muted'), boxWidth: 12 } },
            },
          },
        });
      }).catch(() => {
        container.innerHTML = `
          <div class="card p-4">
            <h2 class="text-base font-semibold mb-3">Genre Breakdown</h2>
            <ul class="flex flex-col gap-1 text-sm">
              ${rows.map(r => `<li class="flex gap-2 justify-between"><span>${escapeHtml(r.genre)}</span><span class="text-text-muted">${r.chapters_read}</span></li>`).join('')}
            </ul>
          </div>`;
      });

      return {
        destroy() {
          _chart?.destroy();
          _chart = null;
          container.innerHTML = '';
        },
      };
    },
  };
}

// ── Chart.js loader ───────────────────────────────────────────────────────────
// Chart.js ships as a UMD bundle (sets window.Chart), not an ES module.
// We inject a script tag once and resolve when it fires onload.

/** @type {Promise<any> | null} */ let _chartJsPromise = null;

function _loadChartJs() {
  if (!_chartJsPromise) {
    _chartJsPromise = new Promise((resolve, reject) => {
      if (window.Chart) { resolve(window.Chart); return; }
      const existing = document.querySelector('script[data-chartjs]');
      if (existing) {
        // Script already injected by a previous load — wait for it
        existing.addEventListener('load',  () => resolve(window.Chart));
        existing.addEventListener('error', reject);
        return;
      }
      const s = document.createElement('script');
      s.src = '/js/vendor/chart.umd.min.js';
      s.dataset.chartjs = '1';
      s.onload  = () => resolve(window.Chart);
      s.onerror = () => reject(new Error('Chart.js not found — run: kani-cli setup'));
      document.head.appendChild(s);
    });
  }
  return _chartJsPromise;
}
