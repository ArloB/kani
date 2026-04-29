// @ts-check
// Manage tab — Download filter rules CRUD.

import { h, render } from 'preact';
import htm from 'htm';
import * as api from '../../api.js';
import { escapeHtml } from '../../utils.js';
import { showToast } from '../toast.js';
import { createEmptyState } from '../empty-state.js';
import { Combobox } from '../combobox.js';
import { iconX } from '../../icons.js';
const html = htm.bind(h);

/** @param {any} kind */
function _ruleLabel(kind) {
  if (typeof kind === 'string') return kind;
  const [k, v] = Object.entries(kind)[0] ?? ['', ''];
  const labels = /** @type {Record<string,string>} */ ({
    LanguageInclude: `Language = ${v}`,
    LanguageExclude: `Language ≠ ${v}`,
    TitleContains: `Title contains "${v}"`,
    TitleExcludes: `Title excludes "${v}"`,
    ChapterNumberMin: `Chapter ≥ ${v}`,
    ChapterNumberMax: `Chapter ≤ ${v}`,
    MaxAgeDays: `Max age: ${v} days`,
    PublishedAfter: `Published after ${new Date(Number(v) * 1000).toLocaleDateString()}`,
  });
  return labels[k] ?? `${k}: ${v}`;
}

/**
 * @param {HTMLElement} bodyEl  Card body element
 * @param {any[]} initialRules
 * @param {number} dbId
 */
export function mountDownloadRulesPanel(bodyEl, initialRules, dbId) {
  let rules = Array.isArray(initialRules) ? [...initialRules] : [];

  const wrap = document.createElement('div');
  wrap.className = 'flex flex-col gap-3';
  bodyEl.appendChild(wrap);

  let langOptions = /** @type {Array<{id:number,name:string}>} */ ([]);
  let langCmbVal = '';
  /** @type {HTMLDivElement|null} */ let langCmbMount = null;

  const renderLangCmb = () => {
    if (!langCmbMount) return;
    const used = new Set(
      rules
        .filter(r => typeof r.kind === 'object' && ('LanguageInclude' in r.kind || 'LanguageExclude' in r.kind))
        .map(r => /** @type {string} */ (Object.values(r.kind)[0]))
    );
    const opts = langOptions.filter(o => !used.has(o.name));
    if (langCmbVal && !opts.some(o => o.name === langCmbVal)) langCmbVal = '';
    render(html`<${Combobox}
      options=${opts}
      value=${opts.find(o => o.name === langCmbVal)?.id ?? null}
      onChange=${(/** @type {any} */ id) => { langCmbVal = opts.find(o => o.id === id)?.name ?? ''; }}
      placeholder="Select language…"
    />`, langCmbMount);
  };

  api.getChapterLanguages(dbId).then(langs => {
    langOptions = (Array.isArray(langs) ? langs : []).map((l, i) => ({ id: i, name: l }));
    renderLangCmb();
  }).catch(() => {});

  const rerender = () => {
    wrap.innerHTML = '';

    if (rules.length > 0) {
      const ul = document.createElement('ul');
      ul.className = 'flex flex-col divide-y divide-border-subtle';
      for (const rule of rules) {
        const li = document.createElement('li');
        li.className = 'flex items-center justify-between gap-2 py-2';
        li.innerHTML = `
          <span class="text-sm text-text">${escapeHtml(_ruleLabel(rule.kind))}</span>
          <button class="btn-icon text-danger js-rm" data-id="${rule.id}" aria-label="Remove rule">${iconX}</button>
        `;
        li.querySelector('.js-rm')?.addEventListener('click', async (e) => {
          const id = Number(/** @type {HTMLElement} */(e.currentTarget).dataset.id);
          try {
            await api.deleteDownloadRule(id);
            rules = rules.filter(r => r.id !== id);
            rerender();
          } catch { /* ignore */ }
        });
        ul.appendChild(li);
      }
      wrap.appendChild(ul);
    } else {
      wrap.appendChild(createEmptyState({ title: 'No download filters.' }));
    }

    const form = document.createElement('div');
    form.className = 'flex flex-wrap items-center gap-2 mt-2';
    form.innerHTML = `
      <select class="input w-auto text-sm js-rule-type">
        <optgroup label="Language">
          <option value="LanguageInclude">Language include</option>
          <option value="LanguageExclude">Language exclude</option>
        </optgroup>
        <optgroup label="Title">
          <option value="TitleContains">Title contains</option>
          <option value="TitleExcludes">Title excludes</option>
        </optgroup>
        <optgroup label="Chapter number">
          <option value="ChapterNumberMin">Chapter ≥ (min)</option>
          <option value="ChapterNumberMax">Chapter ≤ (max)</option>
        </optgroup>
        <optgroup label="Other">
          <option value="ExcludeFractional">Exclude fractional chapters</option>
          <option value="MaxAgeDays">Max age (days)</option>
          <option value="PublishedAfter">Published after (epoch)</option>
        </optgroup>
      </select>
      <div class="js-rule-cmb-wrap flex-1 min-w-36" style="display:none"></div>
      <input type="text" class="input flex-1 min-w-24 text-sm js-rule-val" placeholder="Value…" />
      <button type="button" class="btn-ghost btn-sm js-rule-add">Add</button>
    `;

    const typeEl = /** @type {HTMLSelectElement} */ (form.querySelector('.js-rule-type'));
    const valEl  = /** @type {HTMLInputElement} */  (form.querySelector('.js-rule-val'));
    langCmbMount = /** @type {HTMLDivElement} */    (form.querySelector('.js-rule-cmb-wrap'));

    typeEl.addEventListener('change', () => {
      const type = typeEl.value;
      if (type === 'ExcludeFractional') {
        valEl.style.display = 'none';
        if (langCmbMount) langCmbMount.style.display = 'none';
      } else if (type === 'LanguageInclude' || type === 'LanguageExclude') {
        valEl.style.display = 'none';
        if (langCmbMount) { langCmbMount.style.display = ''; renderLangCmb(); }
      } else {
        valEl.style.display = '';
        if (langCmbMount) langCmbMount.style.display = 'none';
      }
    });
    typeEl.dispatchEvent(new Event('change'));

    form.querySelector('.js-rule-add')?.addEventListener('click', async () => {
      const type = typeEl.value;
      const isCmb = type === 'LanguageInclude' || type === 'LanguageExclude';
      const valText = isCmb ? langCmbVal : valEl.value.trim();
      if ((!isCmb && type !== 'ExcludeFractional' && !valText) || (isCmb && !valText)) return;

      const kind = type === 'ExcludeFractional'
        ? 'ExcludeFractional'
        : {
          [type]: ['ChapterNumberMin', 'ChapterNumberMax', 'MaxAgeDays', 'PublishedAfter'].includes(type)
            ? Number(valText) : valText
        };
      try {
        const newRule = await api.addDownloadRule(dbId, kind);
        if (newRule && newRule.id) rules.push({ id: newRule.id, manga_id: dbId, kind });
        valEl.value = '';
        langCmbVal = '';
        rerender();
      } catch (e) {
        showToast(/** @type {any} */(e)?.hint ?? /** @type {any} */(e)?.message ?? 'Failed to add rule', { type: 'error' });
      }
    });

    wrap.appendChild(form);
    renderLangCmb();
  };

  rerender();
}
