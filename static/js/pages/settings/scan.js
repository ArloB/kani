// @ts-check
// Settings — Scan section.

import { h } from 'preact';
import { useState, useCallback } from 'preact/hooks';
import htm from 'htm';
import * as api from '../../api.js';
import { showToast } from '../../components/toast.js';
import { SettingsGroup, ToggleRow, NumberRow } from './_shared.js';
import { useSettingsForm } from './form-bus.js';
import { t } from '../../i18n.js';

const html = htm.bind(h);

/** @param {{ settings: any }} props */
export function ScanSection({ settings }) {
  const initial = {
    auto_scan: !!settings?.auto_scan,
    scan_interval_minutes: settings?.scan_interval_minutes ?? 60,
    scan_exclude_completed: !!settings?.scan_exclude_completed,
  };
  const [form, setForm] = useState(initial);
  const [saved, setSaved] = useState(initial);

  const save = useCallback(async () => {
    await api.updateSettings({ Scan: form });
    setSaved(form);
    showToast(t('common.saved'), { type: 'success' });
  }, [form]);

  useSettingsForm({ current: form, saved, save, reset: () => setForm(saved) });

  const set = (/** @type {string} */ k, /** @type {any} */ v) =>
    setForm((f) => ({ ...f, [k]: v }));

  return html`
    <${SettingsGroup} label=${t('settings.scan.group')}>
      <${ToggleRow}
        label=${t('settings.scan.auto.label')}
        description=${t('settings.scan.auto.desc')}
        checked=${form.auto_scan}
        onChange=${(v) => set('auto_scan', v)}
      />
      ${form.auto_scan &&
      html`<${NumberRow}
        label=${t('settings.scan.interval.label')}
        description=${t('settings.scan.interval.desc')}
        value=${form.scan_interval_minutes}
        min=${1}
        onChange=${(v) => set('scan_interval_minutes', v)}
      />`}
      <${ToggleRow}
        label=${t('settings.scan.exclude.label')}
        description=${t('settings.scan.exclude.desc')}
        checked=${form.scan_exclude_completed}
        onChange=${(v) => set('scan_exclude_completed', v)}
      />
    <//>
  `;
}
