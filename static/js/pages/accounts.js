// @ts-check
// Accounts page — user and role management for admins.

import * as api from '../api.js';
import { hasPermission } from '../state.js';
import { escapeHtml } from '../utils.js';
import { showToast } from '../components/toast.js';
import { iconPencil, iconX } from '../icons.js';
import { renderTabs } from '../components/tabs.js';

// ── Module state ──────────────────────────────────────────────────────────────

/** @type {any[]} */ let _users = [];
/** @type {any[]} */ let _roles = [];
/** @type {'users' | 'roles'} */ let _activeTab = 'users';
/** @type {HTMLElement | null} */ let _container = null;

// ── Init ──────────────────────────────────────────────────────────────────────

/** @param {HTMLElement} container */
export async function init(container) {
  document.title = 'Accounts - Kani';
  _container = container;
  _activeTab = 'users';

  if (!hasPermission('user:manage')) {
    container.innerHTML = `
      <div class="max-w-2xl mx-auto px-4 py-12 text-center">
        <p class="text-text-muted">You do not have permission to manage accounts.</p>
      </div>
    `;
    return;
  }

  container.innerHTML = `
    <div class="max-w-5xl mx-auto px-4 md:px-6 py-6 flex flex-col gap-6">
      <div class="flex items-center justify-between gap-4">
        <div>
          <h1 class="text-2xl font-semibold text-text">Accounts</h1>
          <p class="text-sm text-text-muted mt-0.5">Manage users and roles.</p>
        </div>
      </div>

      <!-- Tab bar -->
      <div class="js-tabs"></div>

      <!-- Panel -->
      <div class="js-panel"></div>
    </div>
  `;

  // Tab bar
  const tabsEl = /** @type {HTMLElement} */ (container.querySelector('.js-tabs'));
  renderTabs(tabsEl, {
    tabs: [
      { id: 'users', name: 'Users' },
      { id: 'roles', name: 'Roles' },
    ],
    activeId: _activeTab,
    onSelect: (id) => {
      _activeTab = /** @type {'users' | 'roles'} */ (id);
      _renderActiveTab();
    },
  });

  // Load data and render
  await _reload();
}

/** @param {HTMLElement} container */
export function destroy(container) {
  _container = null;
  _users = [];
  _roles = [];
}

// ── Data loading ──────────────────────────────────────────────────────────────

async function _reload() {
  const [usersRes, rolesRes] = await Promise.allSettled([
    api.adminListUsers(),
    api.adminListRoles(),
  ]);
  _users = usersRes.status === 'fulfilled' ? usersRes.value ?? [] : [];
  _roles = rolesRes.status === 'fulfilled' ? rolesRes.value ?? [] : [];
  _renderActiveTab();
}

function _renderActiveTab() {
  if (!_container) return;
  const panel = /** @type {HTMLElement} */ (_container.querySelector('.js-panel'));
  if (!panel) return;

  panel.innerHTML = '';
  if (_activeTab === 'users') {
    _renderUsersPanel(panel);
  } else {
    _renderRolesPanel(panel);
  }
}

// ── Users panel ───────────────────────────────────────────────────────────────

/** @param {HTMLElement} panel */
function _renderUsersPanel(panel) {
  // Header row: heading + Add user button
  const header = document.createElement('div');
  header.className = 'flex items-center justify-between gap-4 mb-4';
  header.innerHTML = `<h2 class="text-base font-semibold text-text">Users</h2>`;
  const addBtn = document.createElement('button');
  addBtn.type = 'button';
  addBtn.className = 'btn-primary btn-sm';
  addBtn.textContent = '+ Add user';
  addBtn.addEventListener('click', () => _showUserModal(null));
  header.appendChild(addBtn);
  panel.appendChild(header);

  if (_users.length === 0) {
    panel.innerHTML += '<p class="text-sm text-text-muted">No users found.</p>';
    return;
  }

  const table = document.createElement('div');
  table.className = 'bg-surface-2 rounded-xl overflow-hidden';

  // Table header
  const thead = document.createElement('div');
  thead.className = 'grid grid-cols-[1fr_1fr_auto_auto] gap-4 px-4 py-2 border-b border-border-subtle text-xs font-semibold uppercase tracking-wide text-text-muted';
  thead.innerHTML = `<span>Username</span><span>Email</span><span>Status</span><span></span>`;
  table.appendChild(thead);

  for (const user of _users) {
    const row = document.createElement('div');
    row.className = 'grid grid-cols-[1fr_1fr_auto_auto] gap-4 px-4 py-3 items-center border-b border-border-subtle last:border-0 hover:bg-surface-alt transition-colors';

    const usernameEl = document.createElement('div');
    usernameEl.className = 'flex flex-col gap-0.5 min-w-0';
    usernameEl.innerHTML = `
      <span class="text-sm font-medium text-text truncate">${escapeHtml(user.username)}</span>
      <span class="text-xs text-text-muted truncate">${user.roles?.join(', ') ?? ''}</span>
    `;

    const emailEl = document.createElement('span');
    emailEl.className = 'text-sm text-text-muted truncate';
    emailEl.textContent = user.email;

    const statusEl = document.createElement('span');
    statusEl.className = `text-xs px-2 py-0.5 rounded-full font-medium ${user.is_active ? 'bg-success/15 text-success' : 'bg-danger/15 text-danger'}`;
    statusEl.textContent = user.is_active ? 'Active' : 'Inactive';

    const actions = document.createElement('div');
    actions.className = 'flex items-center gap-1';

    const editBtn = document.createElement('button');
    editBtn.type = 'button';
    editBtn.className = 'btn-icon';
    editBtn.setAttribute('aria-label', `Edit ${user.username}`);
    editBtn.innerHTML = iconPencil;
    editBtn.addEventListener('click', () => _showUserModal(user));

    const deleteBtn = document.createElement('button');
    deleteBtn.type = 'button';
    deleteBtn.className = 'btn-icon text-danger';
    deleteBtn.setAttribute('aria-label', `Delete ${user.username}`);
    deleteBtn.innerHTML = iconX;
    deleteBtn.addEventListener('click', async () => {
      if (!confirm(`Delete user "${user.username}"? This cannot be undone.`)) return;
      deleteBtn.disabled = true;
      try {
        await api.adminDeleteUser(user.id);
        showToast(`User "${user.username}" deleted.`);
        await _reload();
      } catch (e) {
        showToast(e?.message ?? 'Failed to delete user.', { type: 'error' });
        deleteBtn.disabled = false;
      }
    });

    actions.appendChild(editBtn);
    actions.appendChild(deleteBtn);

    row.appendChild(usernameEl);
    row.appendChild(emailEl);
    row.appendChild(statusEl);
    row.appendChild(actions);
    table.appendChild(row);
  }

  panel.appendChild(table);
}

// ── User modal ────────────────────────────────────────────────────────────────

/** @param {any | null} user — null for create, object for edit */
function _showUserModal(user) {
  const isEdit = user != null;

  const overlay = document.createElement('div');
  overlay.className = 'fixed inset-0 z-50 flex items-center justify-center bg-bg/70 backdrop-blur-sm p-4';

  const dialog = document.createElement('div');
  dialog.className = 'bg-surface rounded-2xl shadow-xl w-full max-w-md flex flex-col gap-0 overflow-hidden';
  dialog.setAttribute('role', 'dialog');
  dialog.setAttribute('aria-modal', 'true');
  dialog.setAttribute('aria-label', isEdit ? `Edit ${user.username}` : 'Add user');

  const roleOptions = _roles.map(r =>
    `<label class="flex items-center gap-2 text-sm text-text">
      <input type="checkbox" value="${escapeHtml(r.slug)}" ${user?.roles?.includes(r.slug) ? 'checked' : ''}>
      ${escapeHtml(r.slug)}${r.description ? ` <span class="text-xs text-text-muted">— ${escapeHtml(r.description)}</span>` : ''}
    </label>`
  ).join('');

  dialog.innerHTML = `
    <div class="px-6 py-4 border-b border-border-subtle flex items-center justify-between gap-4">
      <h2 class="text-base font-semibold text-text">${isEdit ? `Edit ${escapeHtml(user.username)}` : 'Add user'}</h2>
      <button type="button" class="btn-icon js-close" aria-label="Close">${iconX}</button>
    </div>
    <div class="px-6 py-5 flex flex-col gap-4 overflow-y-auto">
      <div class="flex flex-col gap-1.5">
        <label class="text-sm font-medium text-text" for="modal-username">Username</label>
        <input type="text" id="modal-username" class="input js-username" value="${escapeHtml(user?.username ?? '')}" autocomplete="off">
      </div>
      <div class="flex flex-col gap-1.5">
        <label class="text-sm font-medium text-text" for="modal-email">Email</label>
        <input type="email" id="modal-email" class="input js-email" value="${escapeHtml(user?.email ?? '')}">
      </div>
      <div class="flex flex-col gap-1.5">
        <label class="text-sm font-medium text-text" for="modal-password">${isEdit ? 'New password (leave blank to keep)' : 'Password'}</label>
        <input type="password" id="modal-password" class="input js-password" autocomplete="new-password" placeholder="${isEdit ? 'Leave blank to keep current' : 'Min 8 characters'}">
      </div>
      ${isEdit ? `
      <div class="flex flex-col gap-1.5">
        <label class="text-sm font-medium text-text">Status</label>
        <label class="flex items-center gap-2 text-sm text-text cursor-pointer">
          <input type="checkbox" class="js-is-active" ${user.is_active ? 'checked' : ''}>
          Active
        </label>
      </div>` : ''}
      <div class="flex flex-col gap-1.5">
        <label class="text-sm font-medium text-text">Roles</label>
        <div class="flex flex-col gap-2 js-roles">${roleOptions}</div>
      </div>
      <span class="js-modal-error text-sm text-danger hidden"></span>
    </div>
    <div class="px-6 py-4 border-t border-border-subtle flex items-center justify-end gap-3">
      <button type="button" class="btn-secondary btn-sm js-cancel">Cancel</button>
      <button type="button" class="btn-primary btn-sm js-save">${isEdit ? 'Save changes' : 'Create user'}</button>
    </div>
  `;

  overlay.appendChild(dialog);
  document.body.appendChild(overlay);

  const closeModal = () => { if (overlay.parentNode) document.body.removeChild(overlay); };

  dialog.querySelector('.js-close')?.addEventListener('click', closeModal);
  dialog.querySelector('.js-cancel')?.addEventListener('click', closeModal);
  overlay.addEventListener('click', e => { if (e.target === overlay) closeModal(); });

  const usernameEl = /** @type {HTMLInputElement} */ (dialog.querySelector('.js-username'));
  const emailEl    = /** @type {HTMLInputElement} */ (dialog.querySelector('.js-email'));
  const passwordEl = /** @type {HTMLInputElement} */ (dialog.querySelector('.js-password'));
  const isActiveEl = /** @type {HTMLInputElement|null} */ (dialog.querySelector('.js-is-active'));
  const errorEl    = /** @type {HTMLElement} */ (dialog.querySelector('.js-modal-error'));
  const saveBtn    = /** @type {HTMLButtonElement} */ (dialog.querySelector('.js-save'));

  saveBtn.addEventListener('click', async () => {
    const username = usernameEl.value.trim();
    const email    = emailEl.value.trim();
    const password = passwordEl.value;

    if (!username) { _showModalError(errorEl, 'Username is required.'); return; }
    if (!email)    { _showModalError(errorEl, 'Email is required.'); return; }
    if (!isEdit && !password) { _showModalError(errorEl, 'Password is required.'); return; }
    if (password && password.length < 8) { _showModalError(errorEl, 'Password must be at least 8 characters.'); return; }

    // Collect selected roles
    const selectedRoles = [...dialog.querySelectorAll('.js-roles input:checked')]
      .map(el => /** @type {HTMLInputElement} */ (el).value);

    saveBtn.disabled = true;
    errorEl.classList.add('hidden');

    try {
      if (isEdit) {
        // Update user fields
        const patch = {};
        if (username !== user.username) patch.username = username;
        if (email !== user.email) patch.email = email;
        if (isActiveEl) patch.is_active = isActiveEl.checked;
        if (password) patch.password = password;
        if (Object.keys(patch).length) await api.adminUpdateUser(user.id, patch);

        // Sync roles: grant missing, revoke extras
        const currentRoles = user.roles ?? [];
        for (const r of selectedRoles) {
          if (!currentRoles.includes(r)) await api.adminGrantRole(user.id, r);
        }
        for (const r of currentRoles) {
          if (!selectedRoles.includes(r)) await api.adminRevokeRole(user.id, r);
        }
        showToast('User updated.');
      } else {
        const created = await api.adminCreateUser({ username, email, password, roles: selectedRoles });
        showToast(`User "${created.username}" created.`);
      }
      closeModal();
      await _reload();
    } catch (e) {
      _showModalError(errorEl, e?.message ?? 'Failed to save.');
      saveBtn.disabled = false;
    }
  });

  usernameEl.focus();
}

/** @param {HTMLElement} el @param {string} msg */
function _showModalError(el, msg) {
  el.textContent = msg;
  el.classList.remove('hidden');
}

// ── Roles panel ───────────────────────────────────────────────────────────────

// All known permissions (mirrors permissions.rs)
const ALL_PERMISSIONS = [
  'library:view', 'library:add', 'library:delete', 'library:refresh', 'library:manage',
  'chapter:download', 'chapter:delete',
  'source:browse', 'source:install', 'source:delete', 'source:toggle_enabled', 'source:configure',
  'settings:view', 'settings:edit_download', 'settings:edit_scan', 'settings:edit_advanced',
  'user:manage',
  'server:manage',
];

/** @param {HTMLElement} panel */
function _renderRolesPanel(panel) {
  const header = document.createElement('div');
  header.className = 'flex items-center justify-between gap-4 mb-4';
  header.innerHTML = `<h2 class="text-base font-semibold text-text">Roles</h2>`;
  const addBtn = document.createElement('button');
  addBtn.type = 'button';
  addBtn.className = 'btn-primary btn-sm';
  addBtn.textContent = '+ Add role';
  addBtn.addEventListener('click', () => _showRoleModal(null));
  header.appendChild(addBtn);
  panel.appendChild(header);

  if (_roles.length === 0) {
    panel.innerHTML += '<p class="text-sm text-text-muted">No roles found.</p>';
    return;
  }

  const list = document.createElement('div');
  list.className = 'flex flex-col gap-4';

  for (const role of _roles) {
    const card = document.createElement('div');
    card.className = 'bg-surface-2 rounded-xl overflow-hidden';

    const cardHeader = document.createElement('div');
    cardHeader.className = 'flex items-center justify-between gap-4 px-4 py-3 border-b border-border-subtle';

    const nameEl = document.createElement('div');
    nameEl.className = 'flex flex-col gap-0.5';
    nameEl.innerHTML = `
      <span class="text-sm font-semibold text-text">${escapeHtml(role.slug)}</span>
      ${role.parent ? `<span class="text-xs text-text-muted">Inherits: ${escapeHtml(role.parent)}</span>` : ''}
      ${role.description ? `<span class="text-xs text-text-muted">${escapeHtml(role.description)}</span>` : ''}
    `;

    const actions = document.createElement('div');
    actions.className = 'flex items-center gap-1';

    const editBtn = document.createElement('button');
    editBtn.type = 'button';
    editBtn.className = 'btn-icon';
    editBtn.setAttribute('aria-label', `Edit role ${role.slug}`);
    editBtn.innerHTML = iconPencil;
    editBtn.addEventListener('click', () => _showRoleModal(role));

    if (role.slug !== 'user' && role.slug !== 'admin') {
      const deleteBtn = document.createElement('button');
      deleteBtn.type = 'button';
      deleteBtn.className = 'btn-icon text-danger';
      deleteBtn.setAttribute('aria-label', `Delete role ${role.slug}`);
      deleteBtn.innerHTML = iconX;
      deleteBtn.addEventListener('click', async () => {
        if (!confirm(`Delete role "${role.slug}"?`)) return;
        deleteBtn.disabled = true;
        try {
          await api.adminDeleteRole(role.slug);
          showToast(`Role "${role.slug}" deleted.`);
          await _reload();
        } catch (e) {
          showToast(e?.message ?? 'Failed to delete role.', { type: 'error' });
          deleteBtn.disabled = false;
        }
      });
      actions.appendChild(deleteBtn);
    }

    actions.insertBefore(editBtn, actions.firstChild);
    cardHeader.appendChild(nameEl);
    cardHeader.appendChild(actions);
    card.appendChild(cardHeader);

    // Permissions list
    const permsEl = document.createElement('div');
    permsEl.className = 'px-4 py-3 flex flex-wrap gap-1.5';
    if (role.permissions.length === 0) {
      permsEl.innerHTML = '<span class="text-xs text-text-faint italic">No direct permissions</span>';
    } else {
      for (const perm of role.permissions) {
        const badge = document.createElement('span');
        badge.className = 'text-xs px-2 py-0.5 rounded bg-surface-alt text-text-muted font-mono';
        badge.textContent = perm;
        permsEl.appendChild(badge);
      }
    }
    card.appendChild(permsEl);
    list.appendChild(card);
  }

  panel.appendChild(list);
}

// ── Role modal ────────────────────────────────────────────────────────────────

/** @param {any | null} role */
function _showRoleModal(role) {
  const isEdit = role != null;
  const isProtected = role?.slug === 'user' || role?.slug === 'admin';

  const overlay = document.createElement('div');
  overlay.className = 'fixed inset-0 z-50 flex items-center justify-center bg-bg/70 backdrop-blur-sm p-4';

  const dialog = document.createElement('div');
  dialog.className = 'bg-surface rounded-2xl shadow-xl w-full max-w-md flex flex-col gap-0 overflow-hidden max-h-[90vh]';
  dialog.setAttribute('role', 'dialog');
  dialog.setAttribute('aria-modal', 'true');
  dialog.setAttribute('aria-label', isEdit ? `Edit role ${role.slug}` : 'Add role');

  const permCheckboxes = ALL_PERMISSIONS.map(perm =>
    `<label class="flex items-center gap-2 text-sm text-text cursor-pointer">
      <input type="checkbox" value="${perm}" class="js-perm" ${role?.permissions?.includes(perm) ? 'checked' : ''}>
      <span class="font-mono text-xs">${perm}</span>
    </label>`
  ).join('');

  dialog.innerHTML = `
    <div class="px-6 py-4 border-b border-border-subtle flex items-center justify-between gap-4 shrink-0">
      <h2 class="text-base font-semibold text-text">${isEdit ? `Edit role: ${escapeHtml(role.slug)}` : 'Add role'}</h2>
      <button type="button" class="btn-icon js-close" aria-label="Close">${iconX}</button>
    </div>
    <div class="px-6 py-5 flex flex-col gap-4 overflow-y-auto">
      ${!isEdit ? `
      <div class="flex flex-col gap-1.5">
        <label class="text-sm font-medium text-text" for="modal-role-slug">Slug</label>
        <input type="text" id="modal-role-slug" class="input js-slug font-mono" placeholder="e.g. moderator" autocomplete="off">
      </div>` : ''}
      <div class="flex flex-col gap-1.5">
        <label class="text-sm font-medium text-text" for="modal-role-desc">Description</label>
        <input type="text" id="modal-role-desc" class="input js-description" value="${escapeHtml(role?.description ?? '')}" placeholder="Short description">
      </div>
      <div class="flex flex-col gap-2">
        <label class="text-sm font-medium text-text">Permissions ${isProtected ? '<span class="text-xs text-text-muted font-normal">(protected role — changes will be saved)</span>' : ''}</label>
        <div class="flex flex-col gap-1.5 js-permissions">${permCheckboxes}</div>
      </div>
      <span class="js-modal-error text-sm text-danger hidden"></span>
    </div>
    <div class="px-6 py-4 border-t border-border-subtle flex items-center justify-end gap-3 shrink-0">
      <button type="button" class="btn-secondary btn-sm js-cancel">Cancel</button>
      <button type="button" class="btn-primary btn-sm js-save">${isEdit ? 'Save changes' : 'Create role'}</button>
    </div>
  `;

  overlay.appendChild(dialog);
  document.body.appendChild(overlay);

  const closeModal = () => { if (overlay.parentNode) document.body.removeChild(overlay); };

  dialog.querySelector('.js-close')?.addEventListener('click', closeModal);
  dialog.querySelector('.js-cancel')?.addEventListener('click', closeModal);
  overlay.addEventListener('click', e => { if (e.target === overlay) closeModal(); });

  const slugEl    = /** @type {HTMLInputElement|null} */ (dialog.querySelector('.js-slug'));
  const descEl    = /** @type {HTMLInputElement} */ (dialog.querySelector('.js-description'));
  const errorEl   = /** @type {HTMLElement} */ (dialog.querySelector('.js-modal-error'));
  const saveBtn   = /** @type {HTMLButtonElement} */ (dialog.querySelector('.js-save'));

  saveBtn.addEventListener('click', async () => {
    const description = descEl.value.trim() || null;
    const selectedPerms = [...dialog.querySelectorAll('.js-perm:checked')]
      .map(el => /** @type {HTMLInputElement} */ (el).value);

    saveBtn.disabled = true;
    errorEl.classList.add('hidden');

    try {
      if (isEdit) {
        await api.adminUpdateRole(role.slug, { description: description ?? undefined, permissions: selectedPerms });
        showToast(`Role "${role.slug}" updated.`);
      } else {
        const slug = slugEl?.value.trim() ?? '';
        if (!slug) { _showModalError(errorEl, 'Slug is required.'); saveBtn.disabled = false; return; }
        await api.adminCreateRole({ slug, description: description ?? undefined, permissions: selectedPerms });
        showToast(`Role "${slug}" created.`);
      }
      closeModal();
      await _reload();
    } catch (e) {
      _showModalError(errorEl, e?.message ?? 'Failed to save.');
      saveBtn.disabled = false;
    }
  });

  (slugEl ?? descEl)?.focus();
}
