// @ts-check
import { h } from 'preact';
import htm from 'htm';
import { DiagCard, useDiagnostics, registerDiagnosticsCard } from '../diagnostics-cards.js';
import { StatRow } from '../../../components/stat-row.js';
import { t } from '../../../i18n.js';

const html = htm.bind(h);

function BrowserCard({ refreshToken }) {
  const { data, error } = useDiagnostics({ refreshToken });
  const b = data?.browser;
  return html`
    <${DiagCard} titleKey="diag.browser.title" loading=${!data && !error} error=${error}>
      ${b &&
      html`
        <${StatRow}
          label=${t('diag.browser.enabled')}
          value=${b.enabled ? t('common.yes') : t('common.no')}
        />
        <${StatRow} label=${t('diag.browser.calls')} value=${b.calls_total} />
        <${StatRow} label=${t('diag.browser.restarts')} value=${b.restarts} />
        <${StatRow} label=${t('diag.browser.max_instances')} value=${b.max_instances} />
      `}
    <//>
  `;
}

registerDiagnosticsCard({
  id: 'browser',
  titleKey: 'diag.browser.title',
  order: 50,
  Component: BrowserCard,
});
