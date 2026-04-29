// @ts-check
// Accounts page — user and role management (master-detail layout).

import { h, render } from 'preact';
import { useState, useEffect } from 'preact/hooks';
import htm from 'htm';
import * as api from '../api.js';
import { hasPermission } from '../state.js';
import { escapeHtml, openConfirm, formatDate } from '../utils.js';
import { showToast } from '../components/toast.js';
import { Modal, mountIntoModalRoot } from '../components/modal.js';
import { mountMasterDetail } from '../components/master-detail.js';
import { renderTabs } from '../components/tabs.js';
import { iconPencil, iconX, iconAccounts } from '../icons.js';
import { ActivityFeed } from '../components/activity-feed.js';
import { setPageHeader, clearPageHeader } from '../components/app-header.js';
const html = htm.bind(h);

// ── Module state ──────────────────────────────────────────────────────────────

/** @type {any[]} */ let _users = [];
/** @type {any[]} */ let _roles = [];
/** @type {HTMLElement | null} */ let _container = null;
/** @type {(() => void) | null} */ let _destroyMasterDetail = null;
/** @type {'users' | 'roles'} */ let _activeTab = 'users';
/** @type {any | null} */ let _selected = null;

// All known permissions (mirrors permissions.rs)
const ALL_PERMISSIONS = [
  'library:view', 'library:add', 'library:delete', 'library:refresh', 'library:manage',
  'chapter:download', 'chapter:delete',
  'source:browse', 'source:install', 'source:delete', 'source:toggle_enabled', 'source:configure',
  'settings:view', 'settings:edit_download', 'settings:edit_scan', 'settings:edit_advanced',
  'user:manage',
  'server:manage',
];

// ── Init ──────────────────────────────────────────────────────────────────────

/** @param {HTMLElement} container */
export async function init(container) {
  document.title = 'Accounts - Kani';
  _container = container;
  _activeTab = 'users';
  _selected = null;

  // Set breadcrumb early, before data loads
  setPageHeader({ crumbs: [{ label: 'Accounts' }] });

  if (!hasPermission('user:manage')) {
    container.innerHTML = `
      <div class="flex flex-col items-center justify-center gap-3 py-20 text-text-muted">
        <p class="text-base font-medium text-text">Access denied</p>
        <p class="text-sm">You do not have permission to manage accounts.</p>
      </div>
    `;
    return;
  }

  // Content area — full height master-detail (below global header)
  const contentEl = document.createElement('div');
  contentEl.style.cssText = 'display:flex;flex-direction:column;overflow:hidden;height:calc(100vh - var(--header-h));';

  container.innerHTML = '';
  container.appendChild(contentEl);

  await _reload();

  // Restore tab + selection from URL
  const _qs = new URLSearchParams(location.search);
  const _tabParam = _qs.get('tab');
  if (_tabParam === 'roles') _activeTab = 'roles';
  const _userParam  = _qs.get('user');
  const _roleParam  = _qs.get('role');
  if (_userParam) _selected = _users.find(u => u.id === Number(_userParam)) ?? null;
  if (_roleParam) _selected = _roles.find(r => r.slug === _roleParam) ?? null;

  _mountMasterDetail(contentEl);
  _updateHeaderActions();
}

/** @param {HTMLElement} container */
export function destroy(container) {
  clearPageHeader();
  mountIntoModalRoot(null);
  _destroyMasterDetail?.();
  _destroyMasterDetail = null;
  _container = null;
  _users = [];
  _roles = [];
  _activeTab = 'users';
  _selected = null;
}

function _updateHeaderActions() {
  const addBtn = document.createElement('button');
  addBtn.type = 'button';
  addBtn.className = 'btn-primary btn-sm';
  addBtn.textContent = _activeTab === 'users' ? '+ Add user' : '+ Add role';
  addBtn.addEventListener('click', () => {
    if (_activeTab === 'users') {
      _showUserModal(null, async () => { await _reload(); _rerenderList(); });
    } else {
      _showRoleModal(null, async () => { await _reload(); _rerenderList(); });
    }
  });

  const crumbs = [{ label: 'Accounts' }];
  if (_selected) {
    crumbs.push({ label: _activeTab === 'users' ? 'Users' : 'Roles' });
    crumbs.push({ label: _activeTab === 'users' ? _selected.username : _selected.slug });
  }

  setPageHeader({ crumbs, actions: addBtn });
}

// ── Data loading ──────────────────────────────────────────────────────────────

async function _reload() {
  const [usersRes, rolesRes] = await Promise.allSettled([
    api.adminListUsers(),
    api.adminListRoles(),
  ]);
  _users = usersRes.status === 'fulfilled' ? usersRes.value ?? [] : [];
  _roles = rolesRes.status === 'fulfilled' ? rolesRes.value ?? [] : [];
}

// ── Master-detail shell ───────────────────────────────────────────────────────

/** @type {HTMLElement | null} */ let _listEl = null;
/** @type {HTMLElement | null} */ let _detailEl = null;

/** Re-renders the list pane (called after data changes). */
function _rerenderList() {
  if (_listEl) _renderList(_listEl);
}

/** @param {HTMLElement} el */
function _mountMasterDetail(el) {
  const { listEl, detailEl, destroy } = mountMasterDetail(el);
  _destroyMasterDetail = destroy;
  _listEl = listEl;
  _detailEl = detailEl;

  _renderList(listEl);
  _renderDetail(detailEl);
}

/** @param {HTMLElement} detailEl */
function _renderDetail(detailEl) {
  if (!_selected) {
    detailEl.innerHTML = `
      <div class="flex flex-col items-center justify-center gap-3 h-full py-20 text-text-muted">
        <span class="icon-2xl opacity-30" aria-hidden="true">${iconAccounts}</span>
        <p class="text-sm">Select a ${_activeTab === 'users' ? 'user' : 'role'} to view details</p>
      </div>
    `;
    return;
  }
  if (_activeTab === 'users') {
    _renderUserDetail(detailEl, _selected);
  } else {
    _renderRoleDetail(detailEl, _selected);
  }
}

/** @param {HTMLElement} listEl */
function _renderList(listEl) {
  listEl.innerHTML = '';

  // Header with tabs
  const headerEl = document.createElement('div');
  headerEl.className = 'list-pane-header';

  const tabsEl = document.createElement('div');
  renderTabs(tabsEl, {
    tabs: [
      { id: 'users', name: 'Users', count: _users.length },
      { id: 'roles', name: 'Roles', count: _roles.length },
    ],
    activeId: _activeTab,
    stretch: true,
    onSelect: (id) => {
      _activeTab = /** @type {'users' | 'roles'} */ (id);
      _selected = null;
      history.pushState(null, '', '/accounts?tab=' + id);
      _updateHeaderActions();
      _renderList(listEl);
      if (_detailEl) _renderDetail(_detailEl);
    },
  });
  headerEl.appendChild(tabsEl);

  // Search
  const searchInput = document.createElement('input');
  searchInput.type = 'search';
  searchInput.className = 'input input-sm';
  searchInput.placeholder = `Search...`;
  headerEl.appendChild(searchInput);
  listEl.appendChild(headerEl);

  // List body
  const bodyEl = document.createElement('div');
  bodyEl.className = 'list-pane-body';

  const items = _activeTab === 'users' ? _users : _roles;
  let filter = '';

  function renderItems() {
    bodyEl.innerHTML = '';
    const filtered = filter
      ? items.filter(i => (i.username ?? i.slug ?? '').toLowerCase().includes(filter.toLowerCase()))
      : items;

    if (filtered.length === 0) {
      bodyEl.innerHTML = '<p class="text-sm text-text-muted px-3 py-4">No results.</p>';
      return;
    }

    for (const item of filtered) {
      const div = document.createElement('div');
      const isActive = _selected != null && (
        _activeTab === 'users' ? _selected.id   === item.id
                               : _selected.slug === item.slug
      );
      div.className = 'list-item' + (isActive ? ' active' : '');

      if (_activeTab === 'users') {
        const initial = (item.username ?? '?')[0].toUpperCase();
        div.innerHTML = `
          <div class="flex items-center gap-3 border-b border-border-subtle last:border-0 w-full">
            <span class="avatar" aria-hidden="true">${escapeHtml(initial)}</span>
            <span class="flex flex-col min-w-0 flex-1">
              <span class="li-title truncate">${escapeHtml(item.username)}</span>
              <span class="li-sub truncate">${item.roles?.join(', ') ?? ''}</span>
            </span>
            <span class="${item.is_active ? 'badge badge-success' : 'badge badge-danger'} shrink-0">${item.is_active ? 'Active' : 'Inactive'}</span>
          </div>
        `;
      } else {
        const isSystemRole = item.slug === 'user' || item.slug === 'admin';
        div.innerHTML = `
          <span class="flex flex-col min-w-0 flex-1">
            <span class="li-title font-mono truncate flex items-center gap-2">
              ${escapeHtml(item.slug)}
              ${isSystemRole ? '<span class="badge badge-muted text-2xs shrink-0">System</span>' : ''}
            </span>
            <span class="li-sub truncate">${escapeHtml(item.description ?? `${item.permissions?.length ?? 0} permissions`)}</span>
          </span>
        `;
      }

      div.addEventListener('click', () => {
        _selected = item;
        const params = new URLSearchParams();
        params.set('tab', _activeTab);
        if (_activeTab === 'users') params.set('user', String(item.id));
        else params.set('role', item.slug);
        history.pushState(null, '', '/accounts?' + params.toString());
        _renderList(listEl);
        _updateHeaderActions();
        if (_detailEl) _renderDetail(_detailEl);
      });
      bodyEl.appendChild(div);
    }
  }

  renderItems();
  searchInput.addEventListener('input', () => {
    filter = searchInput.value;
    renderItems();
  });

  listEl.appendChild(bodyEl);
}

// ── User detail ───────────────────────────────────────────────────────────────

/**
 * @param {HTMLElement} el
 * @param {any} user
 */
function _renderUserDetail(el, user) {
  // Effective permissions: union of all roles' permissions
  const effectivePerms = new Map();
  for (const roleName of (user.roles ?? [])) {
    const role = _roles.find(r => r.slug === roleName);
    for (const perm of (role?.permissions ?? [])) {
      if (!effectivePerms.has(perm)) effectivePerms.set(perm, roleName);
    }
  }

  el.innerHTML = `
    <div class="p-6 flex flex-col gap-5 min-h-0">
      <!-- User header -->
      <div class="flex items-start gap-4">
        <span class="avatar xl" aria-hidden="true">${escapeHtml((user.username ?? '?')[0].toUpperCase())}</span>
        <div class="flex flex-col gap-1 flex-1 min-w-0">
          <h2 class="text-lg font-semibold text-text truncate">${escapeHtml(user.username)}</h2>
          <p class="meta">${escapeHtml(user.email ?? '')} · Created ${formatDate(user.created_at) || 'unknown'}</p>
        </div>
        <div class="flex items-center gap-1 shrink-0">
          <button type="button" class="btn-ghost btn-sm js-edit-user">Edit</button>
          <button type="button" class="btn-danger btn-sm js-delete-user">Delete</button>
        </div>
      </div>

      <!-- Identity card -->
      <div class="detail-card">
        <div class="detail-card-head">Identity</div>
        <div class="kv"><span class="k">Username</span><span class="v">${escapeHtml(user.username)}</span></div>
        <div class="kv"><span class="k">Email</span><span class="v">${escapeHtml(user.email ?? '—')}</span></div>
        <div class="kv"><span class="k">Status</span><span class="v">
          <span class="${user.is_active ? 'badge badge-success' : 'badge badge-danger'}">${user.is_active ? 'Active' : 'Inactive'}</span>
        </span></div>
        <div class="kv"><span class="k">Created</span><span class="v">${escapeHtml(formatDate(user.created_at) || '—')}</span></div>
      </div>

      <!-- Roles card -->
      <div class="detail-card">
        <div class="detail-card-head">
          <span>Roles</span>
        </div>
        <div class="p-3 flex flex-wrap gap-2">
          ${(user.roles ?? []).map(r => `<span class="badge badge-muted font-mono">${escapeHtml(r)}</span>`).join('') || '<span class="meta p-1">No roles assigned.</span>'}
        </div>
      </div>

      <!-- Effective permissions card -->
      <div class="detail-card">
        <div class="detail-card-head">Effective permissions — ${effectivePerms.size}</div>
        <div>
          ${[...effectivePerms.entries()].map(([perm, via]) => `
            <div class="flex items-center justify-between gap-3 px-3 py-2 border-b border-border-subtle last:border-0 text-sm">
              <span class="font-mono text-xs text-text">${escapeHtml(perm)}</span>
              <span class="meta shrink-0">via ${escapeHtml(via)}</span>
            </div>
          `).join('') || '<p class="meta px-3 py-3">No permissions.</p>'}
        </div>
      </div>

      <!-- Activity feed card -->
      <div class="detail-card">
        <div class="detail-card-head">Recent activity</div>
        <div class="js-activity-feed"></div>
      </div>
    </div>
  `;

  el.querySelector('.js-edit-user')?.addEventListener('click', () => {
    _showUserModal(user, async () => {
      await _reload();
      _selected = _users.find(u => u.id === user.id) ?? null;
      _rerenderList();
      if (_detailEl && _selected) _renderUserDetail(_detailEl, _selected);
    });
  });

  el.querySelector('.js-delete-user')?.addEventListener('click', async () => {
    if (!(await openConfirm({ title: 'Delete user', message: `Delete "${user.username}"? This cannot be undone.`, danger: true }))) return;
    try {
      await api.adminDeleteUser(user.id);
      showToast(`User "${user.username}" deleted.`);
      await _reload();
      _selected = null;
      _rerenderList();
      if (_detailEl) _renderDetail(_detailEl);
    } catch (e) {
      showToast(e?.message ?? 'Failed to delete user.', { type: 'error' });
    }
  });

  // Mount activity feed
  const feedEl = /** @type {HTMLElement} */ (el.querySelector('.js-activity-feed'));
  /** @type {Array<{ at: string, kind: string, description: string }>} */
  let _activityEvents = [];
  let _activityLoading = true;
  let _activityError = /** @type {string|null} */ (null);

  function _renderFeed() {
    render(html`<${ActivityFeed}
      events=${_activityEvents}
      loading=${_activityLoading}
      error=${_activityError}
    />`, feedEl);
  }

  async function _loadActivity() {
    _activityLoading = true;
    _renderFeed();
    try {
      const data = await api.getUserActivity(user.id, { limit: 20 });
      _activityEvents = (data.events ?? []).map(/** @param {any} e */ e => ({
        at: e.created_at,
        kind: e.action,
        description: [e.target, e.details].filter(Boolean).join(' — ') || e.action,
      }));
      _activityError = null;
    } catch {
      _activityError = 'Failed to load activity.';
    }
    _activityLoading = false;
    _renderFeed();
  }

  _renderFeed();
  _loadActivity();
}

// ── Role detail ───────────────────────────────────────────────────────────────

/**
 * @param {HTMLElement} el
 * @param {any} role
 */
function _renderRoleDetail(el, role) {
  const usersWithRole = _users.filter(u => u.roles?.includes(role.slug));
  const isProtected = role.slug === 'user' || role.slug === 'admin';

  // Build inherited permissions from parent role chain
  const directPerms = new Set(role.permissions ?? []);
  /** @type {Map<string, string>} */
  const inheritedPerms = new Map();
  let parentSlug = role.parent;
  while (parentSlug) {
    const parentRole = _roles.find(r => r.slug === parentSlug);
    if (!parentRole) break;
    for (const perm of (parentRole.permissions ?? [])) {
      if (!directPerms.has(perm) && !inheritedPerms.has(perm)) {
        inheritedPerms.set(perm, parentSlug);
      }
    }
    parentSlug = parentRole.parent;
  }

  const directChips = [...directPerms].map(p =>
    `<span class="badge badge-muted font-mono">${escapeHtml(p)}</span>`
  ).join('');

  const inheritedChips = [...inheritedPerms.entries()].map(([p, via]) =>
    `<span class="badge badge-muted font-mono opacity-60 italic" title="Inherited from ${escapeHtml(via)}">${escapeHtml(p)}</span>`
  ).join('');

  const inheritedSection = inheritedChips ? `
    <div class="flex items-center gap-2 px-3 py-1.5 border-t border-border-subtle">
      <span class="text-2xs uppercase tracking-wide font-semibold text-text-faint">Inherited</span>
    </div>
    <div class="px-3 pb-3 flex flex-wrap gap-1.5">${inheritedChips}</div>
  ` : '';

  const usersChips = usersWithRole.map(u => `
    <button type="button" class="badge badge-muted gap-1.5 hover:bg-surface-3 transition-colors js-user-badge"
            data-user-id="${u.id}">
      <span class="avatar" style="width:16px;height:16px;font-size:8px" aria-hidden="true">${escapeHtml((u.username ?? '?')[0].toUpperCase())}</span>
      <span class="font-mono text-xs">${escapeHtml(u.username)}</span>
    </button>
  `).join('');

  el.innerHTML = `
    <div class="p-6 flex flex-col gap-5 min-h-0">
      <!-- Role header -->
      <div class="flex items-start gap-4">
        <div class="flex flex-col gap-1 flex-1 min-w-0">
          <div class="flex items-center gap-2 flex-wrap">
            <h2 class="text-lg font-semibold text-text font-mono">${escapeHtml(role.slug)}</h2>
            ${isProtected ? '<span class="badge badge-muted text-2xs">System role</span>' : ''}
          </div>
          <p class="meta">${role.permissions?.length ?? 0} permissions${role.parent ? ` · inherits from ${escapeHtml(role.parent)}` : ''}</p>
          ${role.description ? `<p class="text-sm text-text-muted mt-0.5">${escapeHtml(role.description)}</p>` : ''}
        </div>
        <div class="flex items-center gap-1 shrink-0">
          <button type="button" class="btn-ghost btn-sm js-edit-role">Edit</button>
          ${!isProtected ? '<button type="button" class="btn-danger btn-sm js-delete-role">Delete</button>' : ''}
        </div>
      </div>

      <!-- Permissions card -->
      <div class="detail-card">
        <div class="detail-card-head">Permissions — ${directPerms.size + inheritedPerms.size}</div>
        <div class="p-3 flex flex-wrap gap-1.5">
          ${directChips || '<span class="meta p-1">No direct permissions.</span>'}
        </div>
        ${inheritedSection}
      </div>

      <!-- Users with this role -->
      <div class="detail-card">
        <div class="detail-card-head">Users with this role — ${usersWithRole.length}</div>
        <div class="p-3 flex flex-wrap gap-1.5">
          ${usersChips || '<p class="meta p-1">No users have this role.</p>'}
        </div>
      </div>
    </div>
  `;

  el.querySelector('.js-edit-role')?.addEventListener('click', () => {
    _showRoleModal(role, async () => {
      await _reload();
      const refreshed = _roles.find(r => r.slug === role.slug);
      _selected = refreshed ?? null;
      _rerenderList();
      if (_detailEl && _selected) _renderRoleDetail(_detailEl, _selected);
    });
  });

  el.querySelector('.js-delete-role')?.addEventListener('click', async () => {
    if (!(await openConfirm({ title: 'Delete role', message: `Delete role "${role.slug}"? This cannot be undone.`, danger: true }))) return;
    try {
      await api.adminDeleteRole(role.slug);
      showToast(`Role "${role.slug}" deleted.`);
      await _reload();
      _selected = null;
      _rerenderList();
      if (_detailEl) _renderDetail(_detailEl);
    } catch (e) {
      showToast(e?.message ?? 'Failed to delete role.', { type: 'error' });
    }
  });

  // Clicking a user badge jumps to Users tab and selects that user
  for (const btn of el.querySelectorAll('.js-user-badge')) {
    btn.addEventListener('click', () => {
      const userId = Number(/** @type {HTMLElement} */ (btn).dataset.userId);
      const user = _users.find(u => u.id === userId);
      if (!user) return;
      _activeTab = 'users';
      _selected = user;
      _updateHeaderActions();
      if (_listEl) _renderList(_listEl);
      if (_detailEl) _renderUserDetail(_detailEl, user);
    });
  }
}

// ── User modal ────────────────────────────────────────────────────────────────

/**
 * @param {any | null} user
 * @param {() => void} onSaved
 */
function _showUserModal(user, onSaved) {
  const isEdit = user != null;

  function UserModal({ onClose }) {
    const [username, setUsername] = useState(user?.username ?? '');
    const [email, setEmail]       = useState(user?.email ?? '');
    const [password, setPassword] = useState('');
    const [isActive, setIsActive] = useState(user?.is_active ?? true);
    const [roles, setRoles]       = useState(/** @type {string[]} */ (user?.roles ?? []));
    const [error, setError]       = useState('');
    const [saving, setSaving]     = useState(false);

    const toggleRole = (slug) =>
      setRoles(prev => prev.includes(slug) ? prev.filter(r => r !== slug) : [...prev, slug]);

    const save = async () => {
      if (!username.trim()) { setError('Username is required.'); return; }
      if (!email.trim())    { setError('Email is required.'); return; }
      if (!isEdit && !password) { setError('Password is required.'); return; }
      if (!isEdit && password.length < 8) { setError('Password must be at least 8 characters.'); return; }
      setSaving(true); setError('');
      try {
        if (isEdit) {
          const patch = /** @type {Record<string, any>} */ ({});
          if (username.trim() !== user.username) patch.username = username.trim();
          if (email.trim() !== user.email)       patch.email    = email.trim();
          patch.is_active = isActive;
          if (Object.keys(patch).length) await api.adminUpdateUser(user.id, patch);
          const cur = user.roles ?? [];
          for (const r of roles) if (!cur.includes(r)) await api.adminGrantRole(user.id, r);
          for (const r of cur)   if (!roles.includes(r)) await api.adminRevokeRole(user.id, r);
          showToast('User updated.');
        } else {
          const created = await api.adminCreateUser({ username: username.trim(), email: email.trim(), password, roles });
          showToast(`User "${created.username}" created.`);
        }
        onClose();
        await onSaved();
      } catch (e) {
        setError(e?.message ?? 'Failed to save.');
        setSaving(false);
      }
    };

    return html`
      <${Modal} open=${true} onClose=${onClose} title=${isEdit ? `Edit ${user.username}` : 'Add user'}
        footer=${html`
          <button class="btn-ghost btn-sm" onClick=${onClose}>Cancel</button>
          <button class="btn-primary btn-sm" onClick=${save} disabled=${saving}>
            ${saving ? 'Saving…' : isEdit ? 'Save changes' : 'Create user'}
          </button>
        `}
      >
        <div class="flex flex-col gap-4">
          <div class="flex flex-col gap-1.5">
            <label class="text-sm font-medium text-text" for="modal-username">Username</label>
            <input id="modal-username" type="text" class="input" value=${username}
              onInput=${(e) => setUsername(e.target.value)} autocomplete="off" />
          </div>
          <div class="flex flex-col gap-1.5">
            <label class="text-sm font-medium text-text" for="modal-email">Email</label>
            <input id="modal-email" type="email" class="input" value=${email}
              onInput=${(e) => setEmail(e.target.value)} />
          </div>
          ${!isEdit && html`
            <div class="flex flex-col gap-1.5">
              <label class="text-sm font-medium text-text" for="modal-password">Password</label>
              <input id="modal-password" type="password" class="input" value=${password}
                onInput=${(e) => setPassword(e.target.value)} autocomplete="new-password" placeholder="Min 8 characters" />
            </div>
          `}
          ${isEdit && html`
            <div class="flex flex-col gap-1.5">
              <label class="text-sm font-medium text-text">Status</label>
              <label class="flex items-center gap-2 text-sm text-text cursor-pointer">
                <input type="checkbox" checked=${isActive} onChange=${(e) => setIsActive(e.target.checked)} />
                Active
              </label>
            </div>
          `}
          <div class="flex flex-col gap-1.5">
            <span class="text-sm font-medium text-text">Roles</span>
            <div class="flex flex-col gap-2">
              ${_roles.map(r => html`
                <label key=${r.slug} class="flex items-center gap-2 text-sm text-text cursor-pointer">
                  <input type="checkbox" checked=${roles.includes(r.slug)}
                    onChange=${() => toggleRole(r.slug)} />
                  ${r.slug}
                  ${r.description && html`<span class="text-xs text-text-muted">— ${r.description}</span>`}
                </label>
              `)}
            </div>
          </div>
          ${error && html`<p class="text-sm text-danger">${error}</p>`}
        </div>
      </${Modal}>
    `;
  }

  const unmount = mountIntoModalRoot(html`<${UserModal} onClose=${() => unmount()} />`);
}

// ── Role modal ────────────────────────────────────────────────────────────────

/**
 * @param {any | null} role
 * @param {() => void} onSaved
 */
function _showRoleModal(role, onSaved) {
  const isEdit = role != null;

  function RoleModal({ onClose }) {
    const [slug, setSlug]         = useState(role?.slug ?? '');
    const [description, setDesc]  = useState(role?.description ?? '');
    const [perms, setPerms]       = useState(/** @type {string[]} */ (role?.permissions ?? []));
    const [error, setError]       = useState('');
    const [saving, setSaving]     = useState(false);

    const togglePerm = (perm) =>
      setPerms(prev => prev.includes(perm) ? prev.filter(p => p !== perm) : [...prev, perm]);

    const save = async () => {
      setSaving(true); setError('');
      try {
        const desc = description.trim() || null;
        if (isEdit) {
          await api.adminUpdateRole(role.slug, { description: desc ?? undefined, permissions: perms });
          showToast(`Role "${role.slug}" updated.`);
        } else {
          const s = slug.trim();
          if (!s) { setError('Slug is required.'); setSaving(false); return; }
          await api.adminCreateRole({ slug: s, description: desc ?? undefined, permissions: perms });
          showToast(`Role "${s}" created.`);
        }
        onClose();
        await onSaved();
      } catch (e) {
        setError(e?.message ?? 'Failed to save.');
        setSaving(false);
      }
    };

    return html`
      <${Modal} open=${true} onClose=${onClose}
        title=${isEdit ? `Edit role: ${role.slug}` : 'Add role'}
        footer=${html`
          <button class="btn-ghost btn-sm" onClick=${onClose}>Cancel</button>
          <button class="btn-primary btn-sm" onClick=${save} disabled=${saving}>
            ${saving ? 'Saving…' : isEdit ? 'Save changes' : 'Create role'}
          </button>
        `}
      >
        <div class="flex flex-col gap-4">
          ${!isEdit && html`
            <div class="flex flex-col gap-1.5">
              <label class="text-sm font-medium text-text" for="modal-role-slug">Slug</label>
              <input id="modal-role-slug" type="text" class="input font-mono"
                value=${slug} onInput=${(e) => setSlug(e.target.value)}
                placeholder="e.g. moderator" autocomplete="off" />
            </div>
          `}
          <div class="flex flex-col gap-1.5">
            <label class="text-sm font-medium text-text" for="modal-role-desc">Description</label>
            <input id="modal-role-desc" type="text" class="input" value=${description}
              onInput=${(e) => setDesc(e.target.value)} placeholder="Short description" />
          </div>
          <div class="flex flex-col gap-1.5">
            <span class="text-sm font-medium text-text">Permissions</span>
            <div class="flex flex-col gap-1.5">
              ${ALL_PERMISSIONS.map(perm => html`
                <label key=${perm} class="flex items-center gap-2 text-sm text-text cursor-pointer">
                  <input type="checkbox" checked=${perms.includes(perm)}
                    onChange=${() => togglePerm(perm)} />
                  <span class="font-mono text-xs">${perm}</span>
                </label>
              `)}
            </div>
          </div>
          ${error && html`<p class="text-sm text-danger">${error}</p>`}
        </div>
      </${Modal}>
    `;
  }

  const unmount = mountIntoModalRoot(html`<${RoleModal} onClose=${() => unmount()} />`);
}
