// @ts-check
// Settings — Offline Reading + OPDS info.

import { getLocal, setLocal } from '../../utils.js';
import { mkSettingsGroup, mkSettingsGroupCard, mkSettingsRow } from './_shared.js';

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
    const offlineGroup = mkSettingsGroup('Offline Reading');
    const offlineCard = mkSettingsGroupCard(offlineGroup);

    const modeChips = document.createElement('div');
    modeChips.className = 'flex gap-2 shrink-0';
    for (const [val, label] of [['off', 'Off'], ['auto', 'Auto'], ['manual', 'Manual']]) {
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.className = mode === val ? 'chip chip-active' : 'chip';
      btn.textContent = label;
      btn.setAttribute('aria-pressed', String(mode === val));
      btn.addEventListener('click', () => { setLocal('kani_offline_mode', val); _render(); });
      modeChips.appendChild(btn);
    }
    offlineCard.appendChild(mkSettingsRow({
      label: 'Auto-cache mode',
      description: 'Off: manual only. Auto: cache chapters as they download. Manual: cache via the chapter list.',
      control: modeChips,
    }));

    if (mode === 'auto') {
      const scopeChips = document.createElement('div');
      scopeChips.className = 'flex gap-2 shrink-0';
      for (const [val, label] of [['mine', 'Mine only'], ['all', 'All users']]) {
        const btn = document.createElement('button');
        btn.type = 'button';
        btn.className = scope === val ? 'chip chip-active' : 'chip';
        btn.textContent = label;
        btn.setAttribute('aria-pressed', String(scope === val));
        btn.addEventListener('click', () => { setLocal('kani_offline_scope', val); _render(); });
        scopeChips.appendChild(btn);
      }
      offlineCard.appendChild(mkSettingsRow({
        label: 'Scope',
        description: 'Which downloads trigger auto-caching.',
        control: scopeChips,
      }));

      const filterChips = document.createElement('div');
      filterChips.className = 'flex gap-2 shrink-0';
      for (const [val, label] of [['all', 'All'], ['unread', 'Unread'], ['next', 'Next N']]) {
        const btn = document.createElement('button');
        btn.type = 'button';
        btn.className = filter === val ? 'chip chip-active' : 'chip';
        btn.textContent = label;
        btn.setAttribute('aria-pressed', String(filter === val));
        btn.addEventListener('click', () => { setLocal('kani_offline_filter', val); _render(); });
        filterChips.appendChild(btn);
      }
      offlineCard.appendChild(mkSettingsRow({
        label: 'Which chapters to cache',
        description: 'Filter applied when deciding whether to cache a newly downloaded chapter.',
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
          label: 'Chapters ahead',
          description: 'Cache this many unread chapters ahead of your last read position.',
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
      label: 'Max cache size (MB)',
      description: 'Limit the page image cache. Leave empty for unlimited.',
      control: maxInput,
    }));

    el.appendChild(offlineGroup);

    // ── Cache stats ──────────────────────────────────────────────────────────
    const statsGroup = mkSettingsGroup('Cache');
    const statsCard = mkSettingsGroupCard(statsGroup);

    const usageRow = (() => {
      const usageEl = document.createElement('span');
      usageEl.className = 'text-sm text-text-muted shrink-0';
      usageEl.textContent = 'Calculating…';
      _estimateCacheSize().then(mb => {
        usageEl.textContent = mb !== null ? `~${mb} MB used` : 'Unknown';
      });
      return mkSettingsRow({ label: 'Page cache size', description: 'Approximate storage used by offline chapter pages.', control: usageEl });
    })();
    statsCard.appendChild(usageRow);

    const clearBtn = document.createElement('button');
    clearBtn.type = 'button';
    clearBtn.className = 'btn-danger btn-sm';
    clearBtn.textContent = 'Clear page cache';
    clearBtn.addEventListener('click', async () => {
      clearBtn.disabled = true;
      clearBtn.textContent = 'Clearing…';
      try {
        if ('caches' in window) await caches.delete('kani-pages-v1');
        const usageEl = /** @type {HTMLElement|null} */ (usageRow.querySelector('.shrink-0'));
        if (usageEl) usageEl.textContent = '0 MB used';
        clearBtn.textContent = 'Cleared';
        setTimeout(() => { clearBtn.disabled = false; clearBtn.textContent = 'Clear page cache'; }, 2000);
      } catch {
        clearBtn.disabled = false;
        clearBtn.textContent = 'Clear page cache';
      }
    });
    statsCard.appendChild(mkSettingsRow({ label: 'Clear page cache', description: 'Remove all downloaded chapter pages from this browser.', control: clearBtn }));

    el.appendChild(statsGroup);

    // ── OPDS ─────────────────────────────────────────────────────────────────
    const opdsGroup = mkSettingsGroup('OPDS Catalog');
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
    copyBtn.textContent = 'Copy';
    copyBtn.addEventListener('click', async () => {
      await navigator.clipboard.writeText(feedUrl).catch(() => {});
      copyBtn.textContent = 'Copied!';
      setTimeout(() => { copyBtn.textContent = 'Copy'; }, 1500);
    });
    urlWrap.appendChild(urlCode);
    urlWrap.appendChild(copyBtn);
    opdsCard.appendChild(mkSettingsRow({ label: 'Feed URL', description: 'Add this to any OPDS-compatible reader app.', control: urlWrap }));

    const authNote = document.createElement('span');
    authNote.className = 'text-sm text-text-muted shrink-0';
    authNote.textContent = 'Your Kani credentials';
    opdsCard.appendChild(mkSettingsRow({ label: 'Authentication', description: 'OPDS clients use your username and password (HTTP Basic auth).', control: authNote }));

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
      return mkSettingsRow({ label: 'Compatible apps', description: '', control: wrap });
    })();
    opdsCard.appendChild(appsRow);

    el.appendChild(opdsGroup);

    const note = document.createElement('p');
    note.className = 'text-xs text-text-muted';
    note.textContent = 'Offline and cache settings are stored on this device only.';
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
