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
    upgrade_detection_enabled: settings?.upgrade_detection_enabled ?? true,
    upgrade_min_res_gain: Number(settings?.upgrade_min_res_gain ?? 1.2),
    upgrade_confirm_fetches: Number(settings?.upgrade_confirm_fetches ?? 3),
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

    <${SettingsGroup} label=${t('settings.upgrades.group')}>
      <${ToggleRow}
        label=${t('settings.upgrades.enabled.label')}
        description=${t('settings.upgrades.enabled.desc')}
        checked=${form.upgrade_detection_enabled}
        onChange=${(v) => set('upgrade_detection_enabled', v)}
      />
      ${form.upgrade_detection_enabled &&
      html`<${NumberRow}
          label=${t('settings.upgrades.min_gain.label')}
          description=${t('settings.upgrades.min_gain.desc')}
          value=${form.upgrade_min_res_gain}
          min=${1}
          max=${5}
          step=${0.1}
          onChange=${(v) => set('upgrade_min_res_gain', v)}
        />
        <${NumberRow}
          label=${t('settings.upgrades.confirm_fetches.label')}
          description=${t('settings.upgrades.confirm_fetches.desc')}
          value=${form.upgrade_confirm_fetches}
          min=${0}
          stepper=${true}
          onChange=${(v) => set('upgrade_confirm_fetches', v)}
        />`}
    <//>
  `;
}
