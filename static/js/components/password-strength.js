// @ts-check
// Password strength indicator — debounced server-side zxcvbn + HIBP check.

import { h } from 'preact';
import { useState, useEffect } from 'preact/hooks';
import htm from 'htm';
import * as api from '../api.js';

const html = htm.bind(h);

const SCORE_LABELS = ['Very weak', 'Weak', 'Fair', 'Strong', 'Very strong'];
const SCORE_COLORS = [
  'bg-danger',   // 0
  'bg-danger',   // 1
  'bg-warn',     // 2
  'bg-success',  // 3
  'bg-success',  // 4
];

/**
 * @param {{ password: string, identity?: string }} props
 */
export function PasswordStrength({ password, identity = '' }) {
  const [result, setResult] = useState(/** @type {any} */ (null));

  useEffect(() => {
    if (!password || password.length < 4) {
      setResult(null);
      return;
    }
    const timer = setTimeout(() => {
      api.checkPasswordStrength(password, identity)
        .then(setResult)
        .catch(() => setResult(null));
    }, 300);
    return () => clearTimeout(timer);
  }, [password, identity]);

  if (!result) return null;

  const score = result.score ?? 0;
  const label = SCORE_LABELS[score] ?? 'Unknown';
  const color = SCORE_COLORS[score] ?? 'bg-danger';

  return html`
    <div class="flex flex-col gap-1.5 mt-1.5">
      <div class="flex items-center gap-2">
        <div class="flex-1 flex gap-0.5 h-1.5">
          ${[0,1,2,3].map(i => html`
            <div key=${i} class="flex-1 rounded-full ${i <= score - 1 ? color : 'bg-surface-raised'} transition-colors"></div>
          `)}
        </div>
        <span class="text-xs text-text-muted shrink-0">${label}</span>
      </div>
      ${result.feedback?.length > 0 && html`
        <p class="text-xs text-text-muted">${result.feedback[0]}</p>
      `}
      ${result.pwned && html`
        <p class="text-xs text-danger">
          ⚠ This password has appeared in a data breach
          ${result.pwned_count ? html` (${result.pwned_count.toLocaleString()} times)` : ''}.
          Please choose a different password.
        </p>
      `}
    </div>
  `;
}
