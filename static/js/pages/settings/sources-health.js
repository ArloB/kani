// @ts-check
// Settings — Sources Health section.

import * as api from '../../api.js';
import { mkSettingsGroup, mkSettingsGroupCard } from './_shared.js';
import { fmtCompactDate } from '../../utils.js';

/**
 * @param {HTMLElement} el
 */
export function mount(el) {
  const group = mkSettingsGroup('Extension health');
  const card  = mkSettingsGroupCard(group);
  el.appendChild(group);

  const loadingEl = document.createElement('p');
  loadingEl.className = 'text-sm text-text-muted px-4 py-3';
  loadingEl.textContent = 'Loading…';
  card.appendChild(loadingEl);

  api.getSourcesHealth().then((rows) => {
    card.removeChild(loadingEl);
    if (!Array.isArray(rows) || rows.length === 0) {
      const empty = document.createElement('p');
      empty.className = 'text-sm text-text-muted px-4 py-3';
      empty.textContent = 'No sources installed.';
      card.appendChild(empty);
      return;
    }
    _renderTable(card, rows);
  }).catch(() => {
    loadingEl.textContent = 'Failed to load health data.';
    loadingEl.classList.add('text-danger');
  });

  return {
    destroy() { el.innerHTML = ''; },
  };
}

/**
 * @param {HTMLElement} card
 * @param {any[]} rows
 */
function _renderTable(card, rows) {
  const table = document.createElement('table');
  table.className = 'w-full text-sm';

  const thead = document.createElement('thead');
  thead.innerHTML = `
    <tr class="border-b border-border-subtle">
      <th class="text-left text-xs font-medium text-text-muted px-4 py-2">Source</th>
      <th class="text-left text-xs font-medium text-text-muted px-4 py-2">Last success</th>
      <th class="text-left text-xs font-medium text-text-muted px-4 py-2">Last error</th>
      <th class="text-right text-xs font-medium text-text-muted px-4 py-2">Errors</th>
      <th class="text-right text-xs font-medium text-text-muted px-4 py-2">Avg ms</th>
    </tr>
  `;
  table.appendChild(thead);

  const tbody = document.createElement('tbody');
  tbody.className = 'divide-y divide-border-subtle';

  for (const row of rows) {
    const errors = row.consecutive_error_count ?? 0;
    const isUnhealthy = errors >= 3;
    const tr = document.createElement('tr');
    tr.className = isUnhealthy ? 'bg-danger/5' : '';

    const nameCell = document.createElement('td');
    nameCell.className = 'px-4 py-2.5 font-medium text-text';
    nameCell.textContent = row.source_name;

    const successCell = document.createElement('td');
    successCell.className = 'px-4 py-2.5 text-text-muted';
    successCell.textContent = row.last_success_at ? fmtCompactDate(row.last_success_at) : '—';

    const errorCell = document.createElement('td');
    errorCell.className = 'px-4 py-2.5 text-text-muted';
    errorCell.textContent = row.last_error_at ? fmtCompactDate(row.last_error_at) : '—';

    const countCell = document.createElement('td');
    countCell.className = 'px-4 py-2.5 text-right tabular-nums';
    if (errors >= 3) {
      countCell.innerHTML = `<span class="text-xs font-semibold px-1.5 py-0.5 rounded bg-danger/20 text-danger">${errors}</span>`;
    } else if (errors > 0) {
      countCell.innerHTML = `<span class="text-xs font-semibold px-1.5 py-0.5 rounded bg-warn/20 text-warn">${errors}</span>`;
    } else {
      countCell.textContent = '0';
      countCell.classList.add('text-text-muted');
    }

    const msCell = document.createElement('td');
    msCell.className = 'px-4 py-2.5 text-right tabular-nums text-text-muted';
    msCell.textContent = row.avg_response_ms != null ? `${Math.round(row.avg_response_ms)}` : '—';

    tr.appendChild(nameCell);
    tr.appendChild(successCell);
    tr.appendChild(errorCell);
    tr.appendChild(countCell);
    tr.appendChild(msCell);
    tbody.appendChild(tr);
  }

  table.appendChild(tbody);
  card.appendChild(table);
}

