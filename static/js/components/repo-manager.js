// @ts-check
// RepoManager — master-detail UI for managing signed extension repositories.

import { h, render } from 'preact';
import htm from 'htm';
import { mountMasterDetail } from './master-detail.js';
import { ListItem } from './list-item.js';
import { Modal, mountIntoModalRoot } from './modal.js';
import { showConfirm } from './modal.js';
import { showToast, showApiError } from './toast.js';
import { createEmptyState } from './empty-state.js';
import { createErrorState } from './error-state.js';
import * as api from '../api.js';
import { hasPermission } from '../state.js';
import { t } from '../i18n.js';
import { escapeHtml } from '../utils.js';
const html = htm.bind(h);

/**
 * @param {HTMLElement} container
 * @returns {{ destroy: () => void }}
 */
export function mountRepoManager(container) {
  const { listEl, detailEl, setView, destroy: destroyLayout } = mountMasterDetail(container, { listWidth: 320 });

  /** @type {any[]} */
  let _repos = [];
  /** @type {any[]} */
  let _sources = [];
  /** @type {number | null} */
  let _selectedRepoId = null;
  /** @type {any[]} */
  let _extensions = [];

  let _tofuPending = /** @type {{ fingerprint: string, repoUrl: string } | null} */ (null);
  let _tofuOpen = false;

  function _renderTofuModal() {
    if (!_tofuPending) { mountIntoModalRoot(null); return; }
    const { fingerprint, repoUrl } = _tofuPending;
    mountIntoModalRoot(html`
      <${Modal}
        open=${_tofuOpen}
        onClose=${_cancelTofu}
        title=${t('repo.tofu.title')}
        footer=${html`
          <button class="btn-ghost btn-sm" onClick=${_cancelTofu}>${t('repo.tofu.cancel')}</button>
          <button class="btn-primary btn-sm" onClick=${() => _confirmTofu(fingerprint, repoUrl)}>
            ${t('repo.tofu.trust')}
          </button>
        `}
      >
        <p class="text-sm text-text-muted">${t('repo.tofu.warning')}</p>
        <p class="text-sm font-medium mt-3">${t('repo.tofu.fingerprint_label')}</p>
        <pre class="font-mono text-xs mt-1 px-3 py-2 bg-surface-2 rounded-lg break-all">${escapeHtml(fingerprint)}</pre>
        <p class="text-xs text-text-muted mt-2 break-all">${escapeHtml(repoUrl)}</p>
      </${Modal}>
    `);
  }

  function _cancelTofu() {
    _tofuOpen = false;
    _tofuPending = null;
    mountIntoModalRoot(null);
  }

  /**
   * @param {string} fingerprint
   * @param {string} repoUrl
   */
  async function _confirmTofu(fingerprint, repoUrl) {
    _cancelTofu();
    try {
      const result = await api.addRepo(repoUrl, fingerprint);
      await _loadRepos();
      showToast(t('repo.add.success').replace('{name}', result?.name ?? repoUrl), { type: 'success' });
    } catch (err) {
      showApiError(err);
    }
  }

  /** @param {any} err @param {string} url */
  function _handleAddRepoError(err, url) {
    if (err.status === 428) {
      const fp = err.body?.fingerprint ?? '';
      _tofuPending = { fingerprint: fp, repoUrl: url };
      _tofuOpen = true;
      _renderTofuModal();
    } else if (err.status === 409) {
      const oldFp = err.body?.old_fingerprint ?? '';
      const newFp = err.body?.new_fingerprint ?? '';
      _showKeyChangedBanner(oldFp, newFp, url);
    } else {
      showApiError(err);
    }
  }

  /**
   * @param {string} oldFp
   * @param {string} newFp
   * @param {string} repoUrl
   */
  function _showKeyChangedBanner(oldFp, newFp, repoUrl) {
    let banner = listEl.querySelector('.js-key-changed-banner');
    if (banner) banner.remove();

    banner = document.createElement('div');
    banner.className = 'js-key-changed-banner mx-2 mt-2 p-3 bg-warn/10 border border-warn/30 rounded-lg text-sm flex flex-col gap-2';
    banner.innerHTML = `
      <p class="font-medium text-warn">${escapeHtml(t('repo.key_changed.title'))}</p>
      <p class="text-xs text-text-muted">${escapeHtml(t('repo.key_changed.warning'))}</p>
      <p class="text-xs"><span class="text-text-muted">${escapeHtml(t('repo.key_changed.old'))}:</span> <span class="font-mono break-all">${escapeHtml(oldFp)}</span></p>
      <p class="text-xs"><span class="text-text-muted">${escapeHtml(t('repo.key_changed.new'))}:</span> <span class="font-mono break-all">${escapeHtml(newFp)}</span></p>
      <div class="flex gap-2 justify-end">
        <button class="btn-ghost btn-sm js-dismiss-banner">Dismiss</button>
        <button class="btn-danger btn-sm js-trust-new-key">${escapeHtml(t('repo.key_changed.trust'))}</button>
      </div>
    `;

    banner.querySelector('.js-dismiss-banner')?.addEventListener('click', () => banner?.remove());
    banner.querySelector('.js-trust-new-key')?.addEventListener('click', async () => {
      banner?.remove();
      try {
        await api.addRepo(repoUrl, newFp);
        await _loadRepos();
      } catch (err) {
        showApiError(err);
      }
    });

    const listContent = listEl.querySelector('.js-repo-list-content');
    if (listContent) listEl.insertBefore(banner, listContent);
    else listEl.appendChild(banner);
  }

  // ── List pane ──────────────────────────────────────────────────────────────

  function _buildListPane() {
    listEl.innerHTML = '';

    if (hasPermission('source:install')) {
      const addForm = document.createElement('form');
      addForm.className = 'p-3 border-b border-border-subtle flex gap-2';
      addForm.innerHTML = `
        <input
          type="url"
          class="input input-sm flex-1 min-w-0"
          placeholder="${escapeHtml(t('repo.add.placeholder'))}"
          aria-label="${escapeHtml(t('repo.add.placeholder'))}"
        />
        <button type="submit" class="btn-primary btn-sm shrink-0 js-add-btn">
          ${escapeHtml(t('repo.add.button'))}
        </button>
      `;

      const input = /** @type {HTMLInputElement} */ (addForm.querySelector('input'));
      const btn = /** @type {HTMLButtonElement} */ (addForm.querySelector('.js-add-btn'));

      addForm.addEventListener('submit', async (e) => {
        e.preventDefault();
        const url = input.value.trim();
        if (!url) return;
        btn.disabled = true;
        btn.textContent = t('repo.add.adding');
        try {
          const result = await api.addRepo(url);
          input.value = '';
          await _loadRepos();
          showToast(t('repo.add.success').replace('{name}', result?.name ?? url), { type: 'success' });
        } catch (err) {
          _handleAddRepoError(err, url);
        } finally {
          btn.disabled = false;
          btn.textContent = t('repo.add.button');
        }
      });

      listEl.appendChild(addForm);
    }

    const listContent = document.createElement('div');
    listContent.className = 'js-repo-list-content flex-1 overflow-y-auto';
    listEl.appendChild(listContent);

    _renderRepoList(listContent);
  }

  /**
   * @param {HTMLElement} container
   */
  function _renderRepoList(container) {
    container.innerHTML = '';

    if (_repos.length === 0) {
      container.appendChild(createEmptyState({
        title: t('repo.empty.title'),
        subtitle: t('repo.empty.subtitle'),
      }));
      return;
    }

    for (const repo of _repos) {
      const extCount = _countExtensions(repo);
      const subtitle = t('repo.list.extension_count').replace('{count}', String(extCount));

      const item = document.createElement('div');
      render(
        html`<${ListItem}
          title=${repo.name}
          subtitle=${subtitle}
          active=${repo.id === _selectedRepoId}
          onClick=${() => _selectRepo(repo.id)}
        />`,
        item,
      );
      container.appendChild(item);
    }
  }

  /** @param {any} repo @returns {number} */
  function _countExtensions(repo) {
    if (!repo.index_cache) return 0;
    try {
      const index = JSON.parse(repo.index_cache);
      return Array.isArray(index.extensions) ? index.extensions.length : 0;
    } catch { return 0; }
  }

  // ── Detail pane ───────────────────────────────────────────────────────────

  function _renderDetailPrompt() {
    detailEl.innerHTML = '';
    detailEl.appendChild(createEmptyState({
      title: t('repo.select_prompt'),
    }));
  }

  /** @param {number} repoId */
  async function _selectRepo(repoId) {
    _selectedRepoId = repoId;
    setView('detail');

    const listContent = listEl.querySelector('.js-repo-list-content');
    if (listContent) _renderRepoList(/** @type {HTMLElement} */ (listContent));

    detailEl.innerHTML = `<div class="p-6 text-sm text-text-muted">Loading…</div>`;

    try {
      const [extensions, sources] = await Promise.all([
        api.listRepoExtensions(repoId),
        api.getSources(),
      ]);
      _extensions = Array.isArray(extensions) ? extensions : [];
      _sources = Array.isArray(sources) ? sources : [];
      _renderDetail(repoId);
    } catch (err) {
      detailEl.innerHTML = '';
      detailEl.appendChild(createErrorState({
        message: 'Failed to load extensions.',
        onRetry: () => _selectRepo(repoId),
      }));
    }
  }

  /** @param {number} repoId */
  function _renderDetail(repoId) {
    const repo = _repos.find(r => r.id === repoId);
    if (!repo) return;

    detailEl.innerHTML = '';

    const header = document.createElement('div');
    header.className = 'flex items-center justify-between gap-3 px-6 py-4 border-b border-border-subtle';
    header.innerHTML = `
      <div class="min-w-0">
        <p class="font-medium text-text truncate">${escapeHtml(repo.name)}</p>
        <p class="text-xs text-text-muted mt-0.5 truncate">${escapeHtml(repo.url ?? '')}</p>
      </div>
      <div class="flex gap-2 shrink-0">
        ${hasPermission('source:install') ? `
          <button class="btn-ghost btn-sm js-refresh-btn">${escapeHtml(t('repo.action.refresh'))}</button>
          <button class="btn-ghost btn-sm text-error js-remove-btn">${escapeHtml(t('repo.action.remove'))}</button>
        ` : ''}
      </div>
    `;

    header.querySelector('.js-refresh-btn')?.addEventListener('click', async (e) => {
      const btn = /** @type {HTMLButtonElement} */ (e.currentTarget);
      btn.disabled = true;
      try {
        await api.refreshRepo(repoId);
        await _loadRepos();
        _selectRepo(repoId);
      } catch (err) {
        showApiError(err);
      } finally {
        btn.disabled = false;
      }
    });

    header.querySelector('.js-remove-btn')?.addEventListener('click', async () => {
      const ok = await showConfirm(t('repo.remove.confirm'));
      if (!ok) return;
      try {
        await api.removeRepo(repoId);
        _selectedRepoId = null;
        await _loadRepos();
        _renderDetailPrompt();
        setView('list');
      } catch (err) {
        showApiError(err);
      }
    });

    detailEl.appendChild(header);

    const body = document.createElement('div');
    body.className = 'flex flex-col divide-y divide-border-subtle';

    if (_extensions.length === 0) {
      body.appendChild(createEmptyState({ title: t('repo.extensions.empty') }));
    } else {
      for (const ext of _extensions) {
        body.appendChild(_buildExtensionRow(repoId, ext));
      }
    }

    detailEl.appendChild(body);
  }

  /**
   * @param {number} repoId
   * @param {any} ext
   * @returns {HTMLElement}
   */
  function _buildExtensionRow(repoId, ext) {
    const installedSource = _sources.find(s =>
      s.name?.toLowerCase() === ext.name?.toLowerCase() || s.base_url?.includes(ext.id)
    );
    const isInstalled = !!installedSource;
    const hasUpdate = isInstalled && _isNewerVersion(ext.version, installedSource.version);

    const row = document.createElement('div');
    row.className = 'flex items-center gap-4 px-6 py-3';

    const meta = document.createElement('div');
    meta.className = 'flex-1 min-w-0 flex flex-col gap-0.5';

    const nameRow = document.createElement('div');
    nameRow.className = 'flex items-center gap-2';
    nameRow.innerHTML = `<span class="text-sm font-medium text-text">${escapeHtml(ext.name)}</span>`;

    if (hasUpdate) {
      const badge = document.createElement('span');
      badge.className = 'text-2xs px-1.5 py-0.5 rounded-full bg-accent/15 text-accent font-medium';
      badge.textContent = t('repo.extensions.update_available');
      nameRow.appendChild(badge);
    } else if (isInstalled) {
      const badge = document.createElement('span');
      badge.className = 'text-2xs px-1.5 py-0.5 rounded-full bg-surface-2 text-text-muted';
      badge.textContent = t('repo.extensions.installed');
      nameRow.appendChild(badge);
    }

    if (ext.nsfw) {
      const badge = document.createElement('span');
      badge.className = 'text-2xs px-1.5 py-0.5 rounded-full bg-error/15 text-error font-medium';
      badge.textContent = t('repo.extensions.nsfw');
      nameRow.appendChild(badge);
    }

    meta.appendChild(nameRow);

    const sub = document.createElement('p');
    sub.className = 'text-xs text-text-muted';
    const parts = [`v${ext.version}`];
    if (ext.language) parts.push(ext.language);
    if (ext.format) parts.push(ext.format.toUpperCase());
    sub.textContent = parts.join(' · ');
    meta.appendChild(sub);

    if (ext.description) {
      const desc = document.createElement('p');
      desc.className = 'text-xs text-text-faint mt-0.5 line-clamp-1';
      desc.textContent = ext.description;
      meta.appendChild(desc);
    }

    row.appendChild(meta);

    if (hasPermission('source:install')) {
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.className = 'btn-primary btn-sm shrink-0';

      if (hasUpdate) {
        btn.textContent = t('repo.extensions.update');
        btn.addEventListener('click', async () => {
          btn.disabled = true;
          btn.textContent = t('repo.extensions.updating');
          try {
            await api.updateFromRepo(repoId, ext.id, installedSource.id);
            showToast(t('repo.extensions.update_success').replace('{name}', ext.name), { type: 'success' });
            const [extensions, sources] = await Promise.all([
              api.listRepoExtensions(repoId),
              api.getSources(),
            ]);
            _extensions = Array.isArray(extensions) ? extensions : [];
            _sources = Array.isArray(sources) ? sources : [];
            _renderDetail(repoId);
          } catch (err) {
            showApiError(err);
            btn.disabled = false;
            btn.textContent = t('repo.extensions.update');
          }
        });
      } else if (isInstalled) {
        btn.textContent = t('repo.extensions.installed');
        btn.disabled = true;
        btn.className = 'btn-ghost btn-sm shrink-0';
      } else {
        btn.textContent = t('repo.extensions.install');
        btn.addEventListener('click', async () => {
          btn.disabled = true;
          btn.textContent = t('repo.extensions.installing');
          try {
            await api.installFromRepo(repoId, ext.id);
            showToast(t('repo.extensions.install_success').replace('{name}', ext.name), { type: 'success' });
            const [extensions, sources] = await Promise.all([
              api.listRepoExtensions(repoId),
              api.getSources(),
            ]);
            _extensions = Array.isArray(extensions) ? extensions : [];
            _sources = Array.isArray(sources) ? sources : [];
            _renderDetail(repoId);
          } catch (err) {
            showApiError(err);
            btn.disabled = false;
            btn.textContent = t('repo.extensions.install');
          }
        });
      }

      row.appendChild(btn);
    }

    return row;
  }

  /**
   * Basic semver comparison: returns true if versionA is strictly greater than versionB.
   * @param {string} versionA
   * @param {string} versionB
   */
  function _isNewerVersion(versionA, versionB) {
    const parse = (v) => String(v ?? '0').split('.').map(n => parseInt(n, 10) || 0);
    const a = parse(versionA);
    const b = parse(versionB);
    for (let i = 0; i < Math.max(a.length, b.length); i++) {
      const diff = (a[i] ?? 0) - (b[i] ?? 0);
      if (diff !== 0) return diff > 0;
    }
    return false;
  }

  // ── Load ──────────────────────────────────────────────────────────────────

  async function _loadRepos() {
    try {
      _repos = await api.listRepos();
    } catch { _repos = []; }

    const listContent = listEl.querySelector('.js-repo-list-content');
    if (listContent) _renderRepoList(/** @type {HTMLElement} */ (listContent));
  }

  // ── Init ──────────────────────────────────────────────────────────────────

  /** @param {Event} e */
  function _onSse(e) {
    const d = /** @type {any} */ (e).detail;
    if (!d?.type) return;
    if (d.type === 'repo_refreshed' || d.type === 'update_available' || d.type === 'source_installed') {
      _loadRepos();
      if (_selectedRepoId !== null) _selectRepo(_selectedRepoId);
    }
  }
  window.addEventListener('kani:sse', _onSse);

  _buildListPane();
  _renderDetailPrompt();
  _loadRepos();

  return {
    destroy() {
      window.removeEventListener('kani:sse', _onSse);
      mountIntoModalRoot(null);
      destroyLayout();
    },
  };
}
