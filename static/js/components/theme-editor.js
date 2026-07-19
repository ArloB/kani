// @ts-check
// Theme editor modal — create and edit named custom themes.

import { h } from 'preact';
import { useState, useEffect, useRef } from 'preact/hooks';
import htm from 'htm';
import { Modal, showConfirm } from './modal.js';
import {
  CORE_TOKENS,
  getCustomThemes,
  saveCustomTheme,
  deleteCustomTheme,
  generateThemeId,
  snapshotCurrentTokens,
  accentFromHex,
  getCurrentTheme,
  applyTheme,
} from '../theme.js';
import { t } from '../i18n.js';

const html = htm.bind(h);

// Fallback values used only when a theme token is unset — these ARE token source
// values (see static/js/theme.js's audit-ignore-file header), not app UI styling.
const DEFAULT_SWATCH_COLOR = '#000000'; // audit-ignore
const DEFAULT_PREVIEW_BG = '#0f0f17'; // audit-ignore
const DEFAULT_PREVIEW_SURFACE = '#18181f'; // audit-ignore
const DEFAULT_ACCENT_COLOR = '#e8545a'; // audit-ignore
const DEFAULT_PREVIEW_TEXT = '#ddddf0'; // audit-ignore

/** @type {ReadonlyArray<{ label: string, tokens: ReadonlyArray<{ key: string, label: string }> }>} */
const TOKEN_GROUPS = [
  {
    label: t('theme.custom.group.bg'),
    tokens: [
      { key: '--color-bg',        label: t('theme.custom.token.bg') },
      { key: '--color-surface',   label: t('theme.custom.token.surface') },
      { key: '--color-surface-2', label: t('theme.custom.token.surface_2') },
      { key: '--color-surface-3', label: t('theme.custom.token.surface_3') },
    ],
  },
  {
    label: t('theme.custom.group.borders'),
    tokens: [
      { key: '--color-border',        label: t('theme.custom.token.border') },
      { key: '--color-border-subtle', label: t('theme.custom.token.border_subtle') },
    ],
  },
  {
    label: t('theme.custom.group.accent'),
    tokens: [
      { key: '--color-accent', label: t('theme.custom.token.accent') },
    ],
  },
  {
    label: t('theme.custom.group.text'),
    tokens: [
      { key: '--color-text',       label: t('theme.custom.token.text') },
      { key: '--color-text-muted', label: t('theme.custom.token.text_muted') },
      { key: '--color-text-faint', label: t('theme.custom.token.text_faint') },
    ],
  },
  {
    label: t('theme.custom.group.status'),
    tokens: [
      { key: '--color-success', label: t('theme.custom.token.success') },
      { key: '--color-warn',    label: t('theme.custom.token.warn') },
      { key: '--color-danger',  label: t('theme.custom.token.danger') },
    ],
  },
];

/**
 * Single colour token row: swatch (opens native colour picker) + hex text input.
 * @param {{ tokenKey: string, label: string, value: string, onChange: (key: string, val: string) => void }} props
 */
function ColorRow({ tokenKey, label, value, onChange }) {
  const [text, setText] = useState(value);
  const colorRef = useRef(/** @type {HTMLInputElement | null} */ (null));

  useEffect(() => {
    setText(value);
    if (colorRef.current) colorRef.current.value = value;
  }, [value]);

  /** @param {Event} e */
  const handleColorInput = (e) => {
    const val = /** @type {HTMLInputElement} */ (e.target).value;
    setText(val);
    onChange(tokenKey, val);
  };

  /** @param {FocusEvent} e */
  const handleTextBlur = (e) => {
    let v = /** @type {HTMLInputElement} */ (e.target).value.trim().toLowerCase();
    if (!v.startsWith('#')) v = '#' + v;
    if (/^#[0-9a-f]{6}$/.test(v)) {
      onChange(tokenKey, v);
      setText(v);
      if (colorRef.current) colorRef.current.value = v;
    } else {
      setText(value);
    }
  };

  return html`
    <div class="flex items-center gap-3 px-4 py-2.5">
      <label
        class="w-8 h-8 rounded-md cursor-pointer shrink-0 shadow-sm overflow-hidden relative border border-border-subtle"
        style=${{ background: value || DEFAULT_SWATCH_COLOR }}
        aria-label=${label}
      >
        <input
          ref=${colorRef}
          type="color"
          value=${value || DEFAULT_SWATCH_COLOR}
          class="absolute inset-0 opacity-0 cursor-pointer w-full h-full"
          onInput=${handleColorInput}
        />
      </label>
      <span class="text-sm text-text flex-1 min-w-0">${label}</span>
      <input
        type="text"
        value=${text}
        placeholder="#rrggbb"
        class="input w-28 text-xs font-mono py-1 h-8 shrink-0"
        maxLength="7"
        onInput=${(/** @type {Event} */ e) => setText(/** @type {HTMLInputElement} */ (e.target).value)}
        onBlur=${handleTextBlur}
        onKeyDown=${(/** @type {KeyboardEvent} */ e) => { if (e.key === 'Enter') /** @type {HTMLInputElement} */ (e.target).blur(); }}
      />
    </div>
  `;
}

/**
 * Mini swatch strip showing the key colours of a theme for at-a-glance preview.
 * @param {{ tokens: Record<string, string> }} props
 */
export function ThemePreviewSwatch({ tokens }) {
  const colors = [
    tokens['--color-bg']      || DEFAULT_PREVIEW_BG,
    tokens['--color-surface'] || DEFAULT_PREVIEW_SURFACE,
    tokens['--color-accent']  || DEFAULT_ACCENT_COLOR,
    tokens['--color-text']    || DEFAULT_PREVIEW_TEXT,
  ];
  return html`
    <div class="flex rounded-md overflow-hidden h-7 w-24 shrink-0 border border-border-subtle" aria-hidden="true">
      ${colors.map((c, i) => html`<div key=${i} class="flex-1" style=${{ background: c }}></div>`)}
    </div>
  `;
}

/**
 * Full-screen modal for creating or editing a custom theme with live preview.
 * @param {{
 *   themeId?: string | null,
 *   onClose: () => void,
 *   onSave: (id: string | null) => void,
 * }} props
 */
export function ThemeEditor({ themeId, onClose, onSave }) {
  const isNew = !themeId;
  const savedThemeRef = useRef(getCurrentTheme());
  const committedRef = useRef(false);

  const [name, setName] = useState(() => {
    if (!themeId) return '';
    return getCustomThemes().find(c => c.id === themeId)?.name ?? '';
  });

  const [tokens, setTokens] = useState(() => {
    if (themeId) {
      const existing = getCustomThemes().find(c => c.id === themeId);
      if (existing) {
        /** @type {Record<string, string>} */
        const result = {};
        for (const k of CORE_TOKENS) result[k] = existing.tokens[k] ?? '';
        return result;
      }
    }
    return snapshotCurrentTokens();
  });

  // Apply all tokens on mount for live preview; restore original theme on unmount.
  useEffect(() => {
    const el = document.documentElement;
    for (const k of CORE_TOKENS) {
      if (tokens[k]) el.style.setProperty(k, tokens[k]);
    }
    const accent = tokens['--color-accent'];
    if (accent) {
      const { hover, dim } = accentFromHex(accent);
      el.style.setProperty('--color-accent-hover', hover);
      el.style.setProperty('--color-accent-dim', dim);
    }
    return () => {
      if (!committedRef.current) {
        const { theme, density, accent: a } = savedThemeRef.current;
        applyTheme(theme, density, a);
      }
    };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  /** @param {string} key @param {string} value */
  function handleTokenChange(key, value) {
    setTokens(prev => ({ ...prev, [key]: value }));
    document.documentElement.style.setProperty(key, value);
    if (key === '--color-accent') {
      const { hover, dim } = accentFromHex(value);
      document.documentElement.style.setProperty('--color-accent-hover', hover);
      document.documentElement.style.setProperty('--color-accent-dim', dim);
    }
  }

  function handleSave() {
    const trimmedName = name.trim();
    if (!trimmedName) return;
    const id = themeId || generateThemeId();
    const accent = tokens['--color-accent'] || DEFAULT_ACCENT_COLOR;
    const { hover, dim } = accentFromHex(accent);
    saveCustomTheme({
      id,
      name: trimmedName,
      tokens: { ...tokens, '--color-accent-hover': hover, '--color-accent-dim': dim },
    });
    committedRef.current = true;
    onSave(id);
  }

  async function handleDelete() {
    if (!themeId) return;
    const confirmed = await showConfirm(t('theme.custom.delete.confirm'), {
      danger: true,
      confirmLabel: t('theme.custom.delete'),
    });
    if (!confirmed) return;
    deleteCustomTheme(themeId);
    committedRef.current = true;
    onSave(null);
  }

  const canSave = name.trim().length > 0;

  const footer = html`
    <div class="flex items-center gap-3 w-full">
      ${!isNew && html`
        <button type="button" class="btn-danger btn-sm mr-auto" onClick=${handleDelete}>
          ${t('theme.custom.delete')}
        </button>
      `}
      <button type="button" class="btn-ghost btn-sm" onClick=${onClose}>
        ${t('theme.custom.cancel')}
      </button>
      <button type="button" class="btn-primary btn-sm" onClick=${handleSave} disabled=${!canSave}>
        ${t('theme.custom.save')}
      </button>
    </div>
  `;

  return html`
    <${Modal}
      open=${true}
      onClose=${onClose}
      title=${isNew ? t('theme.custom.new') : t('theme.custom.edit')}
      wide=${true}
      footer=${footer}
    >
      <div class="flex flex-col gap-5 py-1">
        <div class="flex flex-col gap-1.5">
          <label class="text-xs font-semibold uppercase tracking-wide text-text-muted">
            ${t('theme.custom.name')}
          </label>
          <input
            type="text"
            class="input w-full"
            placeholder=${t('theme.custom.name.placeholder')}
            value=${name}
            maxLength="40"
            onInput=${(/** @type {Event} */ e) => setName(/** @type {HTMLInputElement} */ (e.target).value)}
          />
          <p class="text-xs text-text-faint">${t('theme.custom.live_preview')}</p>
        </div>

        ${TOKEN_GROUPS.map(group => html`
          <div key=${group.label}>
            <p class="text-xs font-semibold uppercase tracking-wide text-text-muted mb-1.5 px-0.5">
              ${group.label}
            </p>
            ${group.tokens.length === 1
              ? html`
                <div class="bg-surface-2 rounded-xl overflow-hidden">
                  <${ColorRow}
                    key=${group.tokens[0].key}
                    tokenKey=${group.tokens[0].key}
                    label=${group.tokens[0].label}
                    value=${tokens[group.tokens[0].key] || DEFAULT_SWATCH_COLOR}
                    onChange=${handleTokenChange}
                  />
                  <p class="text-xs text-text-faint px-4 pb-2.5">${t('theme_editor.auto_derived')}</p>
                </div>
              `
              : html`
                <div class="bg-surface-2 rounded-xl divide-y divide-border-subtle overflow-hidden">
                  ${group.tokens.map(tok => html`
                    <${ColorRow}
                      key=${tok.key}
                      tokenKey=${tok.key}
                      label=${tok.label}
                      value=${tokens[tok.key] || DEFAULT_SWATCH_COLOR}
                      onChange=${handleTokenChange}
                    />
                  `)}
                </div>
              `
            }
          </div>
        `)}

        <p class="text-xs text-text-muted">${t('theme.custom.contrast_warning')}</p>
      </div>
    </${Modal}>
  `;
}
