// @ts-check
// Category selector — manage which categories a manga belongs to.

import { h } from 'preact';
import { useState, useEffect } from 'preact/hooks';
import htm from 'htm';
import * as api from '../api.js';
import { t } from '../i18n.js';
const html = htm.bind(h);

/**
 * @param {{
 *   mangaId: number,
 * }} props
 */
export function CategorySelector({ mangaId }) {
  const [allCategories, setAllCategories] = useState(/** @type {any[]} */([]));
  const [memberIds, setMemberIds] = useState(/** @type {Set<number>} */(new Set()));
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState(/** @type {string|null} */(null));

  useEffect(() => {
    setLoading(true);
    Promise.all([api.getCategories(), api.getMangaCategories(mangaId)])
      .then(([cats, mangaCats]) => {
        setAllCategories(Array.isArray(cats) ? cats : []);
        const ids = Array.isArray(mangaCats)
          ? mangaCats.map((c) => c.id ?? c)
          : [];
        setMemberIds(new Set(ids));
      })
      .catch(() => setError(t('category_selector.error.load_failed')))
      .finally(() => setLoading(false));
  }, [mangaId]);

  async function toggle(catId) {
    const next = new Set(memberIds);
    if (next.has(catId)) next.delete(catId); else next.add(catId);
    setSaving(true);
    try {
      await api.setMangaCategories(mangaId, [...next]);
      setMemberIds(next);
    } catch {
      setError(t('category_selector.error.update_failed'));
    } finally {
      setSaving(false);
    }
  }

  if (loading) return html`<div class="flex flex-col gap-3"><p class="text-sm text-text-muted">${t('category_selector.loading')}</p></div>`;
  if (error) return html`<div class="flex flex-col gap-3"><p class="text-sm text-danger">${error}</p></div>`;

  if (allCategories.length === 0) {
    return html`
      <div class="flex flex-col gap-3">
        <p class="text-sm text-text-muted">${t('category_selector.empty')}</p>
      </div>
    `;
  }

  return html`
    <div class="flex flex-col gap-3">
      <div class="flex flex-wrap gap-2">
        ${allCategories.map(cat => html`
          <button
            key=${cat.id}
            type="button"
            class=${memberIds.has(cat.id) ? 'chip chip-active' : 'chip'}
            disabled=${saving}
            onClick=${() => toggle(cat.id)}
          >${cat.name}</button>
        `)}
      </div>
    </div>
  `;
}
