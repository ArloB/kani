// @ts-check
// audit-ignore-file: this module is the colour-palette source (accent swatches,
// default accent, accent-derivation math, meta theme-colours) — its literals
// ARE the token source values, not hard-coded UI styling.

/** @type {ReadonlyArray<{color: string, label: string}>} */
export const ACCENT_SWATCHES = [
  { color: '#e8545a', label: 'Kani red'  },
  { color: '#3d8ef5', label: 'Ocean'     },
  { color: '#3a9e67', label: 'Forest'    },
  { color: '#9b6ec8', label: 'Amethyst'  },
  { color: '#d4870f', label: 'Amber'     },
  { color: '#e05585', label: 'Rose'      },
];

const _DEFAULT_ACCENT = '#e8545a';

/** @returns {{ theme: string, density: string, accent: string }} */
export function getCurrentTheme() {
  return {
    theme:   localStorage.getItem('kani-theme')   || 'system',
    density: localStorage.getItem('kani-density') || 'comfortable',
    accent:  localStorage.getItem('kani-accent')  || _DEFAULT_ACCENT,
  };
}

/**
 * Compute accent-hover (darkened ~16%) and accent-dim (15% opacity) from a hex.
 * @param {string} hex
 * @returns {{ color: string, hover: string, dim: string }}
 */
export function accentFromHex(hex) {
  const r = parseInt(hex.slice(1, 3), 16);
  const g = parseInt(hex.slice(3, 5), 16);
  const b = parseInt(hex.slice(5, 7), 16);
  const d = /** @param {number} n */ n => Math.max(0, Math.round(n * 0.84)).toString(16).padStart(2, '0');
  return { color: hex, hover: `#${d(r)}${d(g)}${d(b)}`, dim: `rgba(${r},${g},${b},0.15)` };
}

/** @param {string} theme @param {string} density @param {string} accent */
function _applyRaw(theme, density, accent) {
  const h = document.documentElement;

  if (theme === 'system') {
    const light = window.matchMedia('(prefers-color-scheme: light)').matches;
    if (light) h.setAttribute('data-theme', 'light');
    else h.removeAttribute('data-theme');
  } else if (theme === 'dark') {
    h.removeAttribute('data-theme');
  } else {
    h.setAttribute('data-theme', theme);
  }

  if (density === 'compact') h.setAttribute('data-density', 'compact');
  else h.removeAttribute('data-density');

  if (accent && accent !== _DEFAULT_ACCENT) {
    const { color, hover, dim } = accentFromHex(accent);
    h.style.setProperty('--color-accent', color);
    h.style.setProperty('--color-accent-hover', hover);
    h.style.setProperty('--color-accent-dim', dim);
  } else {
    h.style.removeProperty('--color-accent');
    h.style.removeProperty('--color-accent-hover');
    h.style.removeProperty('--color-accent-dim');
  }

  const effectiveTheme = theme === 'system'
    ? (window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark')
    : theme;
  for (const meta of document.querySelectorAll('meta[name="theme-color"]')) {
    meta.setAttribute('content', effectiveTheme === 'light' ? '#f4f5f9' : '#0f0f17');
  }
}

/**
 * Apply theme/density/accent, optionally crossfading via View Transitions.
 * @param {string} theme
 * @param {string} density
 * @param {string} accent
 */
export function applyTheme(theme, density, accent) {
  const fn = () => _applyRaw(theme, density, accent);
  if ('startViewTransition' in document && !window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
    /** @type {any} */ (document).startViewTransition(fn);
  } else {
    fn();
  }
}

/** Persist to localStorage and apply. */
export function saveAndApplyTheme(theme, density, accent) {
  localStorage.setItem('kani-theme', theme);
  localStorage.setItem('kani-density', density);
  localStorage.setItem('kani-accent', accent);
  applyTheme(theme, density, accent);
}

/** @type {MediaQueryList | null} */
let _sysMq = null;

/** Call once at app startup — wires the system-preference listener. */
export function initTheme() {
  _sysMq = window.matchMedia('(prefers-color-scheme: light)');
  _sysMq.addEventListener('change', () => {
    const cur = getCurrentTheme();
    if (cur.theme === 'system') _applyRaw('system', cur.density, cur.accent);
  });
  const { theme, density, accent } = getCurrentTheme();
  _applyRaw(theme, density, accent);
}
