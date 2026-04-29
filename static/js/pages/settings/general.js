// @ts-check
// Settings — General section (display, reading, notifications).

import { getLocal, setLocal } from '../../utils.js';
import { mkSettingsGroup, mkSettingsGroupCard, mkSettingsRow, mkToggleRow } from './_shared.js';

/** @param {HTMLElement} el */
export function mount(el) {
  function _render() {
    el.innerHTML = '';

    const paginationPrefs = [
      { label: 'Chapter list',  key: 'kani_chapter_pagination',  desc: 'How chapters are loaded in the chapter list.' },
      { label: 'Library',       key: 'kani_library_pagination',  desc: 'How manga are loaded in the library grid.' },
      { label: 'Source browse', key: 'kani_source_pagination',   desc: 'How manga are loaded when browsing a source.' },
    ];

    const paginGroup = mkSettingsGroup('Pagination');
    const paginCard  = mkSettingsGroupCard(paginGroup);
    for (const { label, key, desc } of paginationPrefs) {
      const current = getLocal(key) || 'paginated';
      const chips = document.createElement('div');
      chips.className = 'flex gap-2 shrink-0';
      for (const [val, chipLabel] of [['paginated', 'Paginated'], ['infinite', 'Infinite scroll']]) {
        const btn = document.createElement('button');
        btn.type = 'button';
        btn.className = current === val ? 'chip chip-active' : 'chip';
        btn.textContent = chipLabel;
        btn.setAttribute('aria-pressed', String(current === val));
        btn.addEventListener('click', () => { setLocal(key, val); _render(); });
        chips.appendChild(btn);
      }
      paginCard.appendChild(mkSettingsRow({ label, description: desc, control: chips }));
    }
    el.appendChild(paginGroup);

    const readGroup = mkSettingsGroup('Reading');
    const readCard  = mkSettingsGroupCard(readGroup);
    readCard.appendChild(mkToggleRow({
      label: 'Sync read status across scanlators',
      description: 'Marking a chapter read also marks all other versions of it as read.',
      checked: getLocal('kani_coalesce_read') === 'true',
      onChange: v => setLocal('kani_coalesce_read', v ? 'true' : 'false'),
    }));
    readCard.appendChild(mkToggleRow({
      label: 'Warn before opening external links',
      description: 'Show a confirmation dialog when clicking links in manga descriptions.',
      checked: getLocal('kani_skip_external_warning') !== 'true',
      onChange: v => setLocal('kani_skip_external_warning', v ? 'false' : 'true'),
    }));
    el.appendChild(readGroup);

    const notifGroup = mkSettingsGroup('Notifications');
    const notifCard  = mkSettingsGroupCard(notifGroup);
    notifCard.appendChild(mkToggleRow({
      label: 'Show new chapter notifications',
      description: 'Show a notification badge when new chapters are found during a scan.',
      checked: getLocal('kani_disable_notifications') !== 'true',
      onChange: v => setLocal('kani_disable_notifications', v ? 'false' : 'true'),
    }));
    el.appendChild(notifGroup);

    const note = document.createElement('p');
    note.className = 'text-xs text-text-muted';
    note.textContent = 'These preferences are saved to this device only.';
    el.appendChild(note);
  }

  _render();
  return { destroy() { el.innerHTML = ''; } };
}
