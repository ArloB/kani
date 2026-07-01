// @ts-check
// Settings — General section (display, reading, notifications).

import { h, render } from 'preact';
import htm from 'htm';
import { getLocal, setLocal, resetAllConfirmDialogs } from '../../utils.js';
import { mkSettingsGroup, mkSettingsGroupCard, mkSettingsRow, mkToggleRow, mkSelectRow } from './_shared.js';
import { ACCENT_SWATCHES, getCurrentTheme, saveAndApplyTheme, getCustomThemes, deleteCustomTheme, applyCustomTheme } from '../../theme.js';
import { t } from '../../i18n.js';
import { showToast } from '../../components/toast.js';
import { ThemeEditor, ThemePreviewSwatch } from '../../components/theme-editor.js';

const html = htm.bind(h);

/**
 * @param {HTMLElement} container
 * @param {string} activeHex
 */
function _updateSwatchRings(container, activeHex) {
  for (const btn of container.querySelectorAll('button[data-swatch]')) {
    const hex = /** @type {HTMLButtonElement} */ (btn).dataset.swatch ?? '';
    btn.style.boxShadow = hex === activeHex
      ? '0 0 0 2.5px var(--color-bg), 0 0 0 4.5px var(--color-accent)'
      : '';
  }
  const custom = /** @type {HTMLElement | null} */ (container.querySelector('[data-swatch-custom]'));
  if (custom) {
    const isCustom = !ACCENT_SWATCHES.some(s => s.color === activeHex);
    custom.style.boxShadow = isCustom
      ? '0 0 0 2.5px var(--color-bg), 0 0 0 4.5px var(--color-accent)'
      : '';
  }
}

/** @param {HTMLElement} el */
export function mount(el) {
  function _render() {
    el.innerHTML = '';

    // ── Display group ────────────────────────────────────────────────────────
    const { theme: curTheme, density: curDensity, accent: curAccent } = getCurrentTheme();

    const displayGroup = mkSettingsGroup(t('settings.display.group'));
    const displayCard  = mkSettingsGroupCard(displayGroup);

    // Theme selector
    displayCard.appendChild(mkSelectRow({
      label: t('settings.display.theme'),
      description: t('settings.display.theme.desc'),
      value: curTheme,
      options: [
        { value: 'light',  label: t('settings.display.theme.light') },
        { value: 'dark',   label: t('settings.display.theme.dark') },
        { value: 'black',  label: t('settings.display.theme.black') },
        { value: 'system', label: t('settings.display.theme.system') },
      ],
      onChange: (val) => { saveAndApplyTheme(val, curDensity, curAccent); _render(); },
    }));

    // Density selector
    displayCard.appendChild(mkSelectRow({
      label: t('settings.display.density'),
      description: t('settings.display.density.desc'),
      value: curDensity,
      options: [
        { value: 'comfortable', label: t('settings.display.density.comfortable') },
        { value: 'compact',     label: t('settings.display.density.compact') },
      ],
      onChange: (val) => { saveAndApplyTheme(curTheme, val, curAccent); _render(); },
    }));

    // Accent swatches
    const accentWrap = document.createElement('div');
    accentWrap.className = 'flex items-center gap-2 flex-wrap shrink-0';

    for (const { color, label } of ACCENT_SWATCHES) {
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.dataset.swatch = color;
      btn.className = 'w-6 h-6 rounded-full shrink-0 transition-[box-shadow]';
      btn.style.background = color;
      btn.title = label;
      btn.setAttribute('aria-label', label);
      btn.setAttribute('aria-pressed', String(color === curAccent));
      btn.addEventListener('click', () => {
        saveAndApplyTheme(curTheme, curDensity, color);
        _updateSwatchRings(accentWrap, color);
        accentWrap.querySelectorAll('button[data-swatch]').forEach(b => {
          b.setAttribute('aria-pressed', String(/** @type {HTMLButtonElement} */(b).dataset.swatch === color));
        });
        const inp = /** @type {HTMLInputElement | null} */ (accentWrap.querySelector('input[type="color"]'));
        if (inp) inp.value = color;
      });
      accentWrap.appendChild(btn);
    }

    // Custom colour input
    const customLabel = document.createElement('label');
    customLabel.dataset.swatchCustom = '';
    customLabel.className = 'w-6 h-6 rounded-full border-2 border-dashed border-border cursor-pointer flex items-center justify-center transition-[box-shadow] shrink-0 overflow-hidden';
    customLabel.title = t('settings.display.accent.custom');
    const colorInput = document.createElement('input');
    colorInput.type = 'color';
    colorInput.className = 'opacity-0 w-0 h-0 absolute';
    const isCustom = !ACCENT_SWATCHES.some(s => s.color === curAccent);
    colorInput.value = isCustom ? curAccent : '#e8545a'; // audit-ignore: default accent source value
    colorInput.addEventListener('input', () => {
      const hex = colorInput.value;
      customLabel.style.background = hex;
      saveAndApplyTheme(curTheme, curDensity, hex);
      _updateSwatchRings(accentWrap, hex);
    });
    customLabel.appendChild(colorInput);
    if (isCustom) customLabel.style.background = curAccent;
    accentWrap.appendChild(customLabel);

    _updateSwatchRings(accentWrap, curAccent);

    displayCard.appendChild(mkSettingsRow({
      label: t('settings.display.accent'),
      description: t('settings.display.accent.desc'),
      control: accentWrap,
    }));

    const resetBtn = document.createElement('button');
    resetBtn.type = 'button';
    resetBtn.className = 'btn-ghost btn-sm';
    resetBtn.textContent = t('settings.display.reset_confirms.action');
    resetBtn.addEventListener('click', () => {
      resetAllConfirmDialogs();
      showToast(t('settings.display.confirms_reset'), { type: 'success' });
    });
    displayCard.appendChild(mkSettingsRow({
      label: t('settings.display.reset_confirms'),
      description: t('settings.display.reset_confirms.desc'),
      control: resetBtn,
    }));

    el.appendChild(displayGroup);

    // ────────────────────────────────────────────────────────────────────────

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
      label: 'Show in-app chapter badges',
      description: 'Show a notification badge when new chapters are found during a scan.',
      checked: getLocal('kani_disable_notifications') !== 'true',
      onChange: v => setLocal('kani_disable_notifications', v ? 'false' : 'true'),
    }));

    // Browser push notifications (only show if the API is available)
    if ('Notification' in window) {
      const browserEnabled = getLocal('kani_browser_notifications') === 'true';
      const browserRow = mkSettingsRow({
        label: 'Browser notifications',
        description: Notification.permission === 'denied'
          ? 'Notifications are blocked by your browser. Update your browser settings to allow them.'
          : 'Show a browser notification when new chapters are found during a scan.',
        control: (() => {
          const label = document.createElement('label');
          label.className = 'kani-toggle';
          const input = document.createElement('input');
          input.type = 'checkbox';
          input.className = 'kani-toggle__input';
          input.checked = browserEnabled && Notification.permission === 'granted';
          input.disabled = Notification.permission === 'denied';
          const track = document.createElement('span');
          track.className = 'kani-toggle__track';
          label.appendChild(input);
          label.appendChild(track);
          input.addEventListener('change', async () => {
            if (input.checked) {
              const perm = await Notification.requestPermission();
              if (perm !== 'granted') { input.checked = false; return; }
            }
            setLocal('kani_browser_notifications', input.checked ? 'true' : 'false');
          });
          return label;
        })(),
      });
      notifCard.appendChild(browserRow);
    }

    el.appendChild(notifGroup);

    // ── Custom themes group ──────────────────────────────────────────────────
    const customThemes = getCustomThemes();
    const activeTheme = getCurrentTheme().theme;

    const customGroup = mkSettingsGroup(t('theme.custom.group'));
    const customCard  = mkSettingsGroupCard(customGroup);

    for (const ct of customThemes) {
      const isActive = activeTheme === `custom:${ct.id}`;
      const row = document.createElement('div');
      row.className = `flex items-center gap-3 px-4 py-3 cursor-pointer hover:bg-surface-3 transition-colors${isActive ? ' bg-surface-3' : ''}`;
      if (isActive) {
        row.setAttribute('aria-current', 'true');
      }

      const swatchWrap = document.createElement('div');
      render(html`<${ThemePreviewSwatch} tokens=${ct.tokens} />`, swatchWrap);
      row.appendChild(swatchWrap);

      const nameEl = document.createElement('span');
      nameEl.className = `text-sm flex-1 min-w-0 truncate${isActive ? ' font-semibold text-accent' : ' text-text'}`;
      nameEl.textContent = ct.name;
      row.appendChild(nameEl);

      const editBtn = document.createElement('button');
      editBtn.type = 'button';
      editBtn.className = 'btn-ghost btn-sm shrink-0';
      editBtn.textContent = t('theme.custom.edit_action');
      editBtn.addEventListener('click', (e) => { e.stopPropagation(); openEditor(ct.id); });
      row.appendChild(editBtn);

      row.addEventListener('click', () => {
        applyCustomTheme(ct.id);
        _render();
      });

      customCard.appendChild(row);
    }

    if (customThemes.length === 0) {
      const emptyRow = document.createElement('div');
      emptyRow.className = 'px-4 py-3';
      const emptyText = document.createElement('p');
      emptyText.className = 'text-sm text-text-muted';
      emptyText.textContent = t('theme.custom.empty');
      emptyRow.appendChild(emptyText);
      customCard.appendChild(emptyRow);
    }

    const newRow = document.createElement('div');
    newRow.className = 'flex items-center px-4 py-3';
    const newBtn = document.createElement('button');
    newBtn.type = 'button';
    newBtn.className = 'btn-ghost btn-sm';
    newBtn.textContent = t('theme.custom.new');
    newBtn.addEventListener('click', () => openEditor(null));
    newRow.appendChild(newBtn);
    customCard.appendChild(newRow);

    el.appendChild(customGroup);

    const note = document.createElement('p');
    note.className = 'text-xs text-text-muted';
    note.textContent = 'These preferences are saved to this device only.';
    el.appendChild(note);
  }

  /** @param {string | null} themeId */
  function openEditor(themeId) {
    const root = document.getElementById('modal-root');
    if (!root) return;

    /** @param {string | null} savedId */
    function handleSave(savedId) {
      render(null, root);
      if (savedId !== null) {
        applyCustomTheme(savedId);
      } else {
        // Theme was deleted — if it was active, fall back to 'dark'
        const prev = getCurrentTheme();
        if (themeId && prev.theme === `custom:${themeId}`) {
          saveAndApplyTheme('dark', prev.density, prev.accent);
        }
      }
      _render();
    }

    render(
      html`<${ThemeEditor}
        themeId=${themeId}
        onClose=${() => render(null, root)}
        onSave=${handleSave}
      />`,
      root,
    );
  }

  _render();
  return { destroy() { el.innerHTML = ''; } };
}
