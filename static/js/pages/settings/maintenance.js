// @ts-check
// Settings — Maintenance & Security (server-backed admin tunables).

import * as api from '../../api.js';
import { showToast } from '../../components/toast.js';
import { mkSettingsGroup, mkSettingsGroupCard, mkNumberRow, mkSelectRow } from './_shared.js';
import { t } from '../../i18n.js';

/**
 * @param {HTMLElement} el
 * @param {any} settings
 */
export function mount(el, settings) {
  /** @type {Record<string, number|string>} */
  const maint = {
    trash_retention_days: Number(settings?.trash_retention_days ?? 30),
    audit_retention_days: Number(settings?.audit_retention_days ?? 365),
    audit_security_retention_days: Number(settings?.audit_security_retention_days ?? 0),
    disk_warn_pct: Math.round(Number(settings?.disk_warn_threshold ?? 0.1) * 100),
    thumbnail_formats: String(settings?.thumbnail_formats ?? 'jpeg'),
  };

  /** @type {Record<string, number>} */
  const sec = {
    max_login_attempts: Number(settings?.max_login_attempts ?? 5),
    max_ip_attempts: Number(settings?.max_ip_attempts ?? 20),
    login_lockout_seconds: Number(settings?.login_lockout_seconds ?? 900),
    session_timeout_secs: Number(settings?.session_timeout_secs ?? 2592000),
  };

  // ── Maintenance group ──────────────────────────────────────────────────────
  const maintGroup = mkSettingsGroup(t('settings.maintenance.group'));
  const maintCard = mkSettingsGroupCard(maintGroup);

  maintCard.appendChild(mkNumberRow({
    label: t('settings.maintenance.trash_retention'),
    description: t('settings.maintenance.trash_retention.desc'),
    id: 'maint_trash_retention',
    value: maint.trash_retention_days,
    min: 0,
    onChange: (v) => { maint.trash_retention_days = v; },
  }));
  maintCard.appendChild(mkNumberRow({
    label: t('settings.maintenance.audit_retention'),
    description: t('settings.maintenance.audit_retention.desc'),
    id: 'maint_audit_retention',
    value: maint.audit_retention_days,
    min: 0,
    onChange: (v) => { maint.audit_retention_days = v; },
  }));
  maintCard.appendChild(mkNumberRow({
    label: t('settings.maintenance.audit_security_retention'),
    description: t('settings.maintenance.audit_security_retention.desc'),
    id: 'maint_audit_security_retention',
    value: maint.audit_security_retention_days,
    min: 0,
    onChange: (v) => { maint.audit_security_retention_days = v; },
  }));
  maintCard.appendChild(mkNumberRow({
    label: t('settings.maintenance.disk_warn'),
    description: t('settings.maintenance.disk_warn.desc'),
    id: 'maint_disk_warn',
    value: maint.disk_warn_pct,
    min: 0,
    max: 100,
    onChange: (v) => { maint.disk_warn_pct = v; },
  }));
  maintCard.appendChild(mkSelectRow({
    label: t('settings.maintenance.thumbnail_formats'),
    description: t('settings.maintenance.thumbnail_formats.desc'),
    options: [{ value: 'jpeg', label: 'JPEG' }],
    value: maint.thumbnail_formats,
    onChange: (v) => { maint.thumbnail_formats = v; },
  }));

  el.appendChild(maintGroup);


  // ── Security group ─────────────────────────────────────────────────────────
  const secGroup = mkSettingsGroup(t('settings.security.group'));
  const secCard = mkSettingsGroupCard(secGroup);

  secCard.appendChild(mkNumberRow({
    label: t('settings.security.max_login_attempts'),
    description: t('settings.security.max_login_attempts.desc'),
    id: 'sec_max_login_attempts',
    value: sec.max_login_attempts,
    min: 1,
    stepper: true,
    onChange: (v) => { sec.max_login_attempts = v; },
  }));
  secCard.appendChild(mkNumberRow({
    label: t('settings.security.max_ip_attempts'),
    description: t('settings.security.max_ip_attempts.desc'),
    id: 'sec_max_ip_attempts',
    value: sec.max_ip_attempts,
    min: 1,
    stepper: true,
    onChange: (v) => { sec.max_ip_attempts = v; },
  }));
  secCard.appendChild(mkNumberRow({
    label: t('settings.security.lockout_seconds'),
    description: t('settings.security.lockout_seconds.desc'),
    id: 'sec_lockout_seconds',
    value: sec.login_lockout_seconds,
    min: 1,
    onChange: (v) => { sec.login_lockout_seconds = v; },
  }));
  secCard.appendChild(mkNumberRow({
    label: t('settings.security.session_timeout'),
    description: t('settings.security.session_timeout.desc'),
    badge: t('settings.security.restart_badge'),
    id: 'sec_session_timeout',
    value: sec.session_timeout_secs,
    min: 60,
    onChange: (v) => { sec.session_timeout_secs = v; },
  }));

  el.appendChild(secGroup);


  // ── Performance & schedules group ──────────────────────────────────────────
  /** @type {Record<string, number>} */
  const perf = {
    max_concurrent_jobs: Number(settings?.max_concurrent_jobs ?? 10),
    db_maintenance_interval_hours: Number(settings?.db_maintenance_interval_hours ?? 24),
    db_vacuum_interval_hours: Number(settings?.db_vacuum_interval_hours ?? 168),
    audit_prune_interval_hours: Number(settings?.audit_prune_interval_hours ?? 168),
    trash_purge_interval_hours: Number(settings?.trash_purge_interval_hours ?? 168),
  };

  const perfGroup = mkSettingsGroup(t('settings.performance.group'));
  const perfCard = mkSettingsGroupCard(perfGroup);

  perfCard.appendChild(mkNumberRow({
    label: t('settings.performance.max_concurrent_jobs'),
    description: t('settings.performance.max_concurrent_jobs.desc'),
    badge: t('settings.security.restart_badge'),
    id: 'perf_max_concurrent_jobs',
    value: perf.max_concurrent_jobs,
    min: 1,
    onChange: (v) => { perf.max_concurrent_jobs = v; },
  }));
  perfCard.appendChild(mkNumberRow({
    label: t('settings.performance.db_maintenance_interval'),
    description: t('settings.performance.db_maintenance_interval.desc'),
    id: 'perf_db_maintenance_interval',
    value: perf.db_maintenance_interval_hours,
    min: 1,
    onChange: (v) => { perf.db_maintenance_interval_hours = v; },
  }));
  perfCard.appendChild(mkNumberRow({
    label: t('settings.performance.db_vacuum_interval'),
    description: t('settings.performance.db_vacuum_interval.desc'),
    id: 'perf_db_vacuum_interval',
    value: perf.db_vacuum_interval_hours,
    min: 1,
    onChange: (v) => { perf.db_vacuum_interval_hours = v; },
  }));
  perfCard.appendChild(mkNumberRow({
    label: t('settings.performance.audit_prune_interval'),
    description: t('settings.performance.audit_prune_interval.desc'),
    id: 'perf_audit_prune_interval',
    value: perf.audit_prune_interval_hours,
    min: 1,
    onChange: (v) => { perf.audit_prune_interval_hours = v; },
  }));
  perfCard.appendChild(mkNumberRow({
    label: t('settings.performance.trash_purge_interval'),
    description: t('settings.performance.trash_purge_interval.desc'),
    id: 'perf_trash_purge_interval',
    value: perf.trash_purge_interval_hours,
    min: 1,
    onChange: (v) => { perf.trash_purge_interval_hours = v; },
  }));

  el.appendChild(perfGroup);


  const _snapshot = () => JSON.stringify([maint, sec, perf]);
  let lastSaved = _snapshot();

  async function _save() {
    if (_snapshot() === lastSaved) return;
    await api.updateSettings({
      Maintenance: {
        trash_retention_days: Number(maint.trash_retention_days),
        audit_retention_days: Number(maint.audit_retention_days),
        audit_security_retention_days: Number(maint.audit_security_retention_days),
        disk_warn_threshold: Math.max(0, Math.min(100, Number(maint.disk_warn_pct))) / 100,
        thumbnail_formats: String(maint.thumbnail_formats),
      },
    });
    await api.updateSettings({
      Security: {
        max_login_attempts: Number(sec.max_login_attempts),
        max_ip_attempts: Number(sec.max_ip_attempts),
        login_lockout_seconds: Number(sec.login_lockout_seconds),
        session_timeout_secs: Number(sec.session_timeout_secs),
      },
    });
    await api.updateSettings({
      Performance: {
        max_concurrent_jobs: Number(perf.max_concurrent_jobs),
        db_maintenance_interval_hours: Number(perf.db_maintenance_interval_hours),
        db_vacuum_interval_hours: Number(perf.db_vacuum_interval_hours),
        audit_prune_interval_hours: Number(perf.audit_prune_interval_hours),
        trash_purge_interval_hours: Number(perf.trash_purge_interval_hours),
      },
    });
    lastSaved = _snapshot();
    showToast(t('settings.maintenance.saved'), { type: 'success' });
  }

  return {
    destroy() { el.innerHTML = ''; },
    isDirty() { return _snapshot() !== lastSaved; },
    save: _save,
  };
}
