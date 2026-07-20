// @ts-check
// Settings — Sources Health section. (Built; awaiting registration — plan 05.)

import { h } from 'preact';
import { useState, useEffect } from 'preact/hooks';
import htm from 'htm';
import * as api from '../../api.js';
import { SettingsGroup } from './_shared.js';
import { fmtCompactDate, errorCountAriaLabel } from '../../utils.js';
import { EmptyState } from '../../components/empty-state.js';
import { ErrorState } from '../../components/error-state.js';
import { t } from '../../i18n.js';

const html = htm.bind(h);

function CountBadge({ errors }) {
  if (errors >= 3) {
    return html`<span
      class="text-xs font-semibold px-1.5 py-0.5 rounded bg-danger/20 text-danger"
      aria-label=${errorCountAriaLabel(errors)}
      >${errors}</span
    >`;
  }
  if (errors > 0) {
    return html`<span
      class="text-xs font-semibold px-1.5 py-0.5 rounded bg-warn/20 text-warn"
      aria-label=${errorCountAriaLabel(errors)}
      >${errors}</span
    >`;
  }
  return html`<span class="text-text-muted">0</span>`;
}

export function SourcesHealthSection() {
  const [state, setState] = useState(
    /** @type {{ status: string, rows: any[] }} */ ({ status: 'loading', rows: [] }),
  );

  useEffect(() => {
    api
      .getSourcesHealth()
      .then((rows) => setState({ status: 'ready', rows: Array.isArray(rows) ? rows : [] }))
      .catch(() => setState({ status: 'error', rows: [] }));
  }, []);

  return html`
    <${SettingsGroup} label=${t('settings.health.group')}>
      ${state.status === 'loading'
        ? html`<div class="px-4 py-3 text-sm text-text-muted">${t('common.loading')}</div>`
        : state.status === 'error'
        ? html`<${ErrorState} message=${t('settings.health.error')} />`
        : state.rows.length === 0
        ? html`<${EmptyState}
            title=${t('settings.health.empty.title')}
            subtitle=${t('settings.health.empty.desc')}
          />`
        : html`
            <div class="overflow-x-auto">
              <table class="w-full text-sm">
                <thead>
                  <tr class="border-b border-border-subtle">
                    <th class="text-left text-xs font-medium text-text-muted px-4 py-2">
                      ${t('settings.health.col.source')}
                    </th>
                    <th class="text-left text-xs font-medium text-text-muted px-4 py-2">
                      ${t('settings.health.col.last_success')}
                    </th>
                    <th class="text-left text-xs font-medium text-text-muted px-4 py-2">
                      ${t('settings.health.col.last_error')}
                    </th>
                    <th class="text-right text-xs font-medium text-text-muted px-4 py-2">
                      ${t('settings.health.col.errors')}
                    </th>
                    <th class="text-right text-xs font-medium text-text-muted px-4 py-2">
                      ${t('settings.health.col.avg_ms')}
                    </th>
                  </tr>
                </thead>
                <tbody class="divide-y divide-border-subtle">
                  ${state.rows.map((row) => {
                    const errors = row.consecutive_error_count ?? 0;
                    return html`
                      <tr key=${row.source_name} class=${errors >= 3 ? 'bg-danger/5' : ''}>
                        <td class="px-4 py-2.5 font-medium text-text">${row.source_name}</td>
                        <td class="px-4 py-2.5 text-text-muted">
                          ${row.last_success_at ? fmtCompactDate(row.last_success_at) : '—'}
                        </td>
                        <td class="px-4 py-2.5 text-text-muted">
                          ${row.last_error_at ? fmtCompactDate(row.last_error_at) : '—'}
                        </td>
                        <td class="px-4 py-2.5 text-right tabular-nums">
                          <${CountBadge} errors=${errors} />
                        </td>
                        <td class="px-4 py-2.5 text-right tabular-nums text-text-muted">
                          ${row.avg_response_ms != null ? `${Math.round(row.avg_response_ms)}` : '—'}
                        </td>
                      </tr>
                    `;
                  })}
                </tbody>
              </table>
            </div>
          `}
    <//>
  `;
}
