// @ts-check
// Settings — Offline Reading + OPDS info.

import { getLocal, setLocal } from '../../utils.js';
import { mkSettingsGroup, mkSettingsGroupCard, mkSettingsRow } from './_shared.js';
import { t } from '../../i18n.js';

/** @param {HTMLElement} el */
export function mount(el) {
  function _render() {
    el.innerHTML = '';

    const mode = getLocal('kani_offline_mode') || 'off';
    const scope = getLocal('kani_offline_scope') || 'mine';
    const filter = getLocal('kani_offline_filter') || 'unread';
    const nextN = Number(getLocal('kani_offline_next_n') || '5') || 5;
    const maxMb = getLocal('kani_offline_max_mb') || '';

    // ── Mode ────────────────────────────────────────────────────────────────
    const offlineGroup = mkSettingsGroup(t('settings.offline.group'));
    const offlineCard = mkSettingsGroupCard(offlineGroup);

    const modeChips = document.createElement('div');
    modeChips.className = 'flex gap-2 shrink-0';
    for (const [val, label] of [['off', t('settings.offline.mode.off')], ['auto', t('settings.offline.mode.auto')], ['manual', t('settings.offline.mode.manual')]]) {
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.className = mode === val ? 'chip chip-active' : 'chip';
      btn.textContent = label;
      btn.setAttribute('aria-pressed', String(mode === val));
      btn.addEventListener('click', () => { setLocal('kani_offline_mode', val); _render(); });
      modeChips.appendChild(btn);
    }
    offlineCard.appendChild(mkSettingsRow({
      label: t('settings.offline.mode.label'),
      description: t('settings.offline.mode.desc'),
      control: modeChips,
    }));

    if (mode === 'auto') {
      const scopeChips = document.createElement('div');
      scopeChips.className = 'flex gap-2 shrink-0';
      for (const [val, label] of [['mine', t('settings.offline.scope.mine')], ['all', t('settings.offline.scope.all')]]) {
        const btn = document.createElement('button');
        btn.type = 'button';
        btn.className = scope === val ? 'chip chip-active' : 'chip';
        btn.textContent = label;
        btn.setAttribute('aria-pressed', String(scope === val));
        btn.addEventListener('click', () => { setLocal('kani_offline_scope', val); _render(); });
        scopeChips.appendChild(btn);
      }
      offlineCard.appendChild(mkSettingsRow({
        label: t('settings.offline.scope.label'),
        description: t('settings.offline.scope.desc'),
        control: scopeChips,
      }));

      const filterChips = document.createElement('div');
      filterChips.className = 'flex gap-2 shrink-0';
      for (const [val, label] of [['all', t('settings.offline.filter.all')], ['unread', t('settings.offline.filter.unread')], ['next', t('settings.offline.filter.next')]]) {
        const btn = document.createElement('button');
        btn.type = 'button';
        btn.className = filter === val ? 'chip chip-active' : 'chip';
        btn.textContent = label;
        btn.setAttribute('aria-pressed', String(filter === val));
        btn.addEventListener('click', () => { setLocal('kani_offline_filter', val); _render(); });
        filterChips.appendChild(btn);
      }
      offlineCard.appendChild(mkSettingsRow({
        label: t('settings.offline.filter.label'),
        description: t('settings.offline.filter.desc'),
        control: filterChips,
      }));

      if (filter === 'next') {
        const nextInput = document.createElement('input');
        nextInput.type = 'number';
        nextInput.className = 'input w-20 text-sm';
        nextInput.min = '1';
        nextInput.max = '100';
        nextInput.value = String(nextN);
        nextInput.addEventListener('change', () => {
          const v = Math.max(1, Math.min(100, parseInt(nextInput.value, 10) || 5));
          setLocal('kani_offline_next_n', String(v));
          nextInput.value = String(v);
        });
        offlineCard.appendChild(mkSettingsRow({
          label: t('settings.offline.ahead.label'),
          description: t('settings.offline.ahead.desc'),
          control: nextInput,
        }));
      }
    }

    const maxInput = document.createElement('input');
    maxInput.type = 'number';
    maxInput.className = 'input w-24 text-sm';
    maxInput.min = '1';
    maxInput.placeholder = '∞';
    if (maxMb) maxInput.value = maxMb;
    maxInput.addEventListener('change', () => {
      const raw = maxInput.value.trim();
      if (raw === '' || isNaN(Number(raw)) || Number(raw) <= 0) {
        setLocal('kani_offline_max_mb', '');
        maxInput.value = '';
      } else {
        setLocal('kani_offline_max_mb', raw);
      }
    });
    offlineCard.appendChild(mkSettingsRow({
      label: t('settings.offline.max_mb.label'),
      description: t('settings.offline.max_mb.desc'),
      control: maxInput,
    }));

    el.appendChild(offlineGroup);

    // ── Cache stats ──────────────────────────────────────────────────────────
    const statsGroup = mkSettingsGroup(t('settings.offline.cache.group'));
    const statsCard = mkSettingsGroupCard(statsGroup);

    const usageRow = (() => {
      const usageEl = document.createElement('span');
      usageEl.className = 'text-sm text-text-muted shrink-0';
      usageEl.textContent = t('settings.offline.cache.calculating');
      _estimateCacheSize().then(mb => {
        usageEl.textContent = mb !== null ? t('settings.offline.cache.size', { mb }) : t('settings.offline.cache.unknown');
      });
      return mkSettingsRow({ label: t('settings.offline.cache.size_row.label'), description: t('settings.offline.cache.size_row.desc'), control: usageEl });
    })();
    statsCard.appendChild(usageRow);

    const clearBtn = document.createElement('button');
    clearBtn.type = 'button';
    clearBtn.className = 'btn-danger btn-sm';
    clearBtn.textContent = t('settings.offline.cache.clear_btn');
    clearBtn.addEventListener('click', async () => {
      clearBtn.disabled = true;
      clearBtn.textContent = t('settings.offline.cache.clearing');
      try {
        if ('caches' in window) await caches.delete('kani-pages-v1');
        const usageEl = /** @type {HTMLElement|null} */ (usageRow.querySelector('.shrink-0'));
        if (usageEl) usageEl.textContent = t('settings.offline.cache.size', { mb: 0 });
        clearBtn.textContent = t('settings.offline.cache.cleared');
        setTimeout(() => { clearBtn.disabled = false; clearBtn.textContent = t('settings.offline.cache.clear_btn'); }, 2000);
      } catch {
        clearBtn.disabled = false;
        clearBtn.textContent = t('settings.offline.cache.clear_btn');
      }
    });
    statsCard.appendChild(mkSettingsRow({ label: t('settings.offline.cache.clear_row.label'), description: t('settings.offline.cache.clear_row.desc'), control: clearBtn }));

    el.appendChild(statsGroup);

    // ── OPDS ─────────────────────────────────────────────────────────────────
    const opdsGroup = mkSettingsGroup(t('settings.offline.opds.group'));
    const opdsCard = mkSettingsGroupCard(opdsGroup);

    const feedUrl = `${window.location.origin}/opds`;
    const urlWrap = document.createElement('div');
    urlWrap.className = 'flex items-center gap-2 shrink-0';
    const urlCode = document.createElement('code');
    urlCode.className = 'text-xs bg-surface px-2 py-1 rounded text-text-muted break-all';
    urlCode.textContent = feedUrl;
    const copyBtn = document.createElement('button');
    copyBtn.type = 'button';
    copyBtn.className = 'btn-ghost btn-sm text-xs';
    copyBtn.textContent = t('common.copy');
    copyBtn.addEventListener('click', async () => {
      await navigator.clipboard.writeText(feedUrl).catch(() => {});
      copyBtn.textContent = t('common.copied');
      setTimeout(() => { copyBtn.textContent = t('common.copy'); }, 1500);
    });
    urlWrap.appendChild(urlCode);
    urlWrap.appendChild(copyBtn);
    opdsCard.appendChild(mkSettingsRow({ label: t('settings.offline.opds.feed.label'), description: t('settings.offline.opds.feed.desc'), control: urlWrap }));

    const authNote = document.createElement('span');
    authNote.className = 'text-sm text-text-muted shrink-0';
    authNote.textContent = t('settings.offline.opds.auth_note');
    opdsCard.appendChild(mkSettingsRow({ label: t('settings.offline.opds.auth.label'), description: t('settings.offline.opds.auth.desc'), control: authNote }));

    const appsRow = (() => {
      const wrap = document.createElement('div');
      wrap.className = 'flex flex-wrap gap-2 shrink-0';
      for (const [name, href] of [
        ['Chunky (iOS)', 'https://chunkyreader.com'],
        ['Moon+ Reader (Android)', 'https://moondownload.com'],
        ['Panels (iPad)', 'https://panels.app'],
      ]) {
        const a = document.createElement('a');
        a.href = href;
        a.target = '_blank';
        a.rel = 'noopener noreferrer';
        a.className = 'chip text-xs';
        a.textContent = name;
        wrap.appendChild(a);
      }
      return mkSettingsRow({ label: t('settings.offline.opds.apps.label'), description: '', control: wrap });
    })();
    opdsCard.appendChild(appsRow);

    el.appendChild(opdsGroup);

    const note = document.createElement('p');
    note.className = 'text-xs text-text-muted';
    note.textContent = t('settings.offline.footer');
    el.appendChild(note);
  }

  _render();
  return { destroy() { el.innerHTML = ''; } };
}

/** @returns {Promise<number|null>} */
async function _estimateCacheSize() {
  if (!('caches' in window)) return null;
  try {
    const cache = await caches.open('kani-pages-v1');
    const keys = await cache.keys();
    if (keys.length === 0) return 0;
    let total = 0;
    for (const req of keys) {
      const resp = await cache.match(req);
      if (!resp) continue;
      const buf = await resp.clone().arrayBuffer().catch(() => null);
      if (buf) total += buf.byteLength;
    }
    return Math.round(total / (1024 * 1024));
  } catch {
    return null;
  }
}
