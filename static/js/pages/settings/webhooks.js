// @ts-check
// Settings — Webhooks section.

import * as api from '../../api.js';
import { showToast, showApiError } from '../../components/toast.js';
import { mkSettingsGroup, mkSettingsGroupCard, mkSettingsRow } from './_shared.js';
import { escapeHtml, confirmDialog } from '../../utils.js';
import { t } from '../../i18n.js';
import { createErrorState } from '../../components/error-state.js';
import { createEmptyState } from '../../components/empty-state.js';
import { skeletonSettingsCards } from '../../components/skeletons.js';

const ALL_EVENTS = [
  { value: 'chapter.new',         get label() { return t('settings.webhooks.event.chapter_new.label'); },         get description() { return t('settings.webhooks.event.chapter_new.desc'); } },
  { value: 'manga.added',         get label() { return t('settings.webhooks.event.manga_added.label'); },         get description() { return t('settings.webhooks.event.manga_added.desc'); } },
  { value: 'manga.deleted',       get label() { return t('settings.webhooks.event.manga_deleted.label'); },       get description() { return t('settings.webhooks.event.manga_deleted.desc'); } },
  { value: 'chapter.downloaded',  get label() { return t('settings.webhooks.event.chapter_downloaded.label'); },  get description() { return t('settings.webhooks.event.chapter_downloaded.desc'); } },
  { value: 'scan.completed',      get label() { return t('settings.webhooks.event.scan_completed.label'); },      get description() { return t('settings.webhooks.event.scan_completed.desc'); } },
];

/**
 * @param {string} eventsJson - JSON array from DB, e.g. '["*"]' or '["chapter.new","manga.added"]'
 * @returns {{ all: boolean, selected: Set<string> }}
 */
function parseEvents(eventsJson) {
  let arr;
  try { arr = JSON.parse(eventsJson); } catch { arr = ['*']; }
  if (!Array.isArray(arr)) arr = ['*'];
  const all = arr.includes('*');
  return { all, selected: new Set(all ? ALL_EVENTS.map(e => e.value) : arr) };
}

/**
 * @param {{ all: boolean, selected: Set<string> }} state
 * @returns {string} JSON array for API
 */
function serializeEvents(state) {
  if (state.all) return '["*"]';
  const vals = ALL_EVENTS.map(e => e.value).filter(v => state.selected.has(v));
  return JSON.stringify(vals);
}

/** @param {HTMLElement} el */
export async function mount(el) {
  el.innerHTML = '';

  // ── Webhook list group ────────────────────────────────────────────────────
  const listGroup = mkSettingsGroup(t('settings.webhooks.group'));
  const listCard  = mkSettingsGroupCard(listGroup);

  const listEl = document.createElement('div');
  listEl.className = 'divide-y divide-border-subtle';
  listCard.appendChild(listEl);

  // Add webhook button row
  const addBtn = document.createElement('button');
  addBtn.type = 'button';
  addBtn.className = 'btn-ghost btn-sm';
  addBtn.textContent = t('settings.webhooks.add_btn');
  listCard.appendChild(mkSettingsRow({
    label: t('settings.webhooks.add.label'),
    description: t('settings.webhooks.add.desc'),
    control: addBtn,
  }));

  el.appendChild(listGroup);

  // ── Payload format reference ──────────────────────────────────────────────
  const refGroup = mkSettingsGroup(t('settings.webhooks.payload.group'));
  const refCard  = mkSettingsGroupCard(refGroup);

  const _examplePayload = escapeHtml(JSON.stringify({
    event: 'chapter.new',
    timestamp: '2026-05-19T14:30:00Z',
    data: {
      manga_id: 42,
      manga_name: 'One Piece',
      chapter_count: 3,
      chapter_ids: [101, 102, 103],
      chapter_names: ['Ch. 1101', 'Ch. 1102', 'Ch. 1103'],
    },
  }, null, 2));
  const _hmacSnippet = escapeHtml(
    '# Python\n' +
    'import hmac, hashlib\n' +
    "sig = 'sha256=' + hmac.new(secret.encode(), body, hashlib.sha256).hexdigest()\n" +
    "assert sig == request.headers['X-Kani-Signature']\n" +
    '\n' +
    '# Node.js\n' +
    "const sig = 'sha256=' + crypto.createHmac('sha256', secret).update(body).digest('hex');\n" +
    "assert.strictEqual(sig, req.headers['x-kani-signature']);"
  );

  const details = document.createElement('details');
  details.className = 'px-4 py-3';
  details.innerHTML = `
    <summary class="text-sm font-medium text-text cursor-pointer select-none">${t('settings.webhooks.payload.show')}</summary>
    <pre class="mt-3 text-xs bg-surface rounded-lg p-3 overflow-x-auto text-text-muted leading-relaxed">${_examplePayload}</pre>
    <p class="mt-3 text-xs text-text-muted">${t('settings.webhooks.payload.hmac_prefix')} <code class="bg-surface px-1 rounded">X-Kani-Signature: sha256=&lt;hex&gt;</code> ${t('settings.webhooks.payload.hmac_suffix')}</p>
    <pre class="mt-2 text-xs bg-surface rounded-lg p-3 overflow-x-auto text-text-muted leading-relaxed">${_hmacSnippet}</pre>
  `;
  refCard.appendChild(details);
  el.appendChild(refGroup);

  // ── Load and render webhook list ──────────────────────────────────────────
  async function _reload() {
    listEl.innerHTML = skeletonSettingsCards(2);
    let webhooks;
    try {
      webhooks = await api.listWebhooks();
    } catch (e) {
      listEl.innerHTML = '';
      listEl.appendChild(createErrorState({ message: t('settings.webhooks.load_failed'), onRetry: _reload }));
      return;
    }

    if (webhooks.length === 0) {
      listEl.appendChild(createEmptyState({
        title: t('settings.webhooks.empty.title'),
        subtitle: t('settings.webhooks.empty.desc'),
      }));
      return;
    }

    for (const wh of webhooks) {
      listEl.appendChild(_mkWebhookRow(wh, _reload));
    }
  }

  // ── Add form ──────────────────────────────────────────────────────────────
  /** @type {HTMLElement|null} */
  let _formEl = null;

  addBtn.addEventListener('click', () => {
    if (_formEl) { _formEl.remove(); _formEl = null; return; }
    _formEl = _mkForm(null, async (data) => {
      await api.createWebhook(data);
      _formEl?.remove(); _formEl = null;
      await _reload();
    }, () => { _formEl?.remove(); _formEl = null; });
    el.insertBefore(_formEl, refGroup);
  });

  await _reload();

  return {
    destroy() { el.innerHTML = ''; },
  };
}

// ── Webhook row ───────────────────────────────────────────────────────────────

/**
 * @param {any} wh
 * @param {() => Promise<void>} reload
 */
function _mkWebhookRow(wh, reload) {
  const row = document.createElement('div');
  row.className = 'flex flex-col gap-2 px-4 py-3';

  // Top: URL + controls
  const top = document.createElement('div');
  top.className = 'flex items-start justify-between gap-3';

  const left = document.createElement('div');
  left.className = 'flex flex-col gap-1 min-w-0';

  const urlEl = document.createElement('span');
  urlEl.className = 'text-sm font-medium text-text truncate';
  urlEl.title = wh.url;
  urlEl.textContent = wh.url.length > 60 ? wh.url.slice(0, 57) + '…' : wh.url;
  left.appendChild(urlEl);

  // Event badges
  const eventState = parseEvents(wh.events);
  const badgesEl = document.createElement('div');
  badgesEl.className = 'flex flex-wrap gap-1 mt-0.5';
  const eventLabels = eventState.all
    ? [{ label: t('settings.webhooks.event.all'), value: '*' }]
    : ALL_EVENTS.filter(e => eventState.selected.has(e.value));
  for (const ev of eventLabels) {
    const badge = document.createElement('span');
    badge.className = 'text-xs px-1.5 py-0.5 rounded bg-accent/15 text-accent font-medium';
    badge.textContent = ev.label ?? ev.value;
    badgesEl.appendChild(badge);
  }
  left.appendChild(badgesEl);
  top.appendChild(left);

  // Right-side controls
  const controls = document.createElement('div');
  controls.className = 'flex items-center gap-2 shrink-0';

  // Enabled toggle
  const toggleLabel = document.createElement('label');
  toggleLabel.className = 'kani-toggle';
  const toggleInput = document.createElement('input');
  toggleInput.type = 'checkbox';
  toggleInput.className = 'kani-toggle__input';
  toggleInput.checked = wh.enabled;
  const toggleTrack = document.createElement('span');
  toggleTrack.className = 'kani-toggle__track';
  toggleLabel.appendChild(toggleInput);
  toggleLabel.appendChild(toggleTrack);
  toggleInput.addEventListener('change', async () => {
    try {
      await api.updateWebhook(wh.id, { enabled: toggleInput.checked });
    } catch {
      toggleInput.checked = !toggleInput.checked;
    }
  });
  controls.appendChild(toggleLabel);

  // Test button
  const testBtn = document.createElement('button');
  testBtn.type = 'button';
  testBtn.className = 'btn-ghost btn-sm text-xs';
  testBtn.textContent = t('settings.webhooks.row.test');
  testBtn.addEventListener('click', async () => {
    testBtn.disabled = true;
    testBtn.textContent = '…';
    try {
      const r = await api.testWebhook(wh.id);
      showToast(r?.ok ? t('settings.webhooks.row.test.success') : t('settings.webhooks.row.test.failed', { error: r?.error ?? t('settings.webhooks.row.test.default_error') }), { type: r?.ok ? 'success' : 'error' });
    } catch (e) {
      showApiError(e);
    } finally {
      testBtn.disabled = false;
      testBtn.textContent = t('settings.webhooks.row.test');
    }
  });
  controls.appendChild(testBtn);

  // Edit button
  const editBtn = document.createElement('button');
  editBtn.type = 'button';
  editBtn.className = 'btn-ghost btn-sm text-xs';
  editBtn.textContent = t('common.edit');
  controls.appendChild(editBtn);

  // Delete button
  const delBtn = document.createElement('button');
  delBtn.type = 'button';
  delBtn.className = 'btn-ghost btn-sm text-xs text-danger';
  delBtn.textContent = t('common.delete');
  delBtn.addEventListener('click', async () => {
    const ok = await confirmDialog({ title: t('settings.webhooks.row.delete.title'), message: t('settings.webhooks.row.delete.message', { url: wh.url }), confirmLabel: t('common.delete'), danger: true });
    if (!ok) return;
    delBtn.disabled = true;
    try {
      await api.deleteWebhook(wh.id);
      await reload();
    } catch (/** @type {any} */ e) {
      showApiError(e);
      delBtn.disabled = false;
    }
  });
  controls.appendChild(delBtn);

  top.appendChild(controls);
  row.appendChild(top);

  // Delivery log toggle
  const logsToggle = document.createElement('button');
  logsToggle.type = 'button';
  logsToggle.className = 'text-xs text-text-muted hover:text-accent transition-colors self-start';
  logsToggle.textContent = t('settings.webhooks.deliveries.show');

  const logsEl = document.createElement('div');
  logsEl.className = 'hidden';

  let logsLoaded = false;
  logsToggle.addEventListener('click', async () => {
    const open = logsEl.classList.toggle('hidden');
    logsToggle.textContent = open ? t('settings.webhooks.deliveries.show') : t('settings.webhooks.deliveries.hide');
    if (!open && !logsLoaded) {
      logsLoaded = true;
      logsEl.innerHTML = skeletonSettingsCards(3);
      try {
        const deliveries = await api.listWebhookDeliveries(wh.id);
        logsEl.innerHTML = '';
        logsEl.appendChild(_mkDeliveryLog(deliveries));
      } catch {
        logsEl.innerHTML = '';
        logsEl.appendChild(createErrorState({ message: t('settings.webhooks.deliveries.load_failed') }));
      }
    }
  });

  row.appendChild(logsToggle);
  row.appendChild(logsEl);

  // Edit form inline
  /** @type {HTMLElement|null} */
  let _editForm = null;
  editBtn.addEventListener('click', () => {
    if (_editForm) { _editForm.remove(); _editForm = null; editBtn.textContent = t('common.edit'); return; }
    editBtn.textContent = t('common.cancel');
    _editForm = _mkForm(wh, async (data) => {
      await api.updateWebhook(wh.id, data);
      _editForm?.remove(); _editForm = null;
      editBtn.textContent = t('common.edit');
      await reload();
    }, () => { _editForm?.remove(); _editForm = null; editBtn.textContent = t('common.edit'); });
    row.appendChild(_editForm);
  });

  return row;
}

// ── Delivery log ──────────────────────────────────────────────────────────────

/** @param {any[]} deliveries */
function _mkDeliveryLog(deliveries) {
  if (deliveries.length === 0) {
    const p = document.createElement('p');
    p.className = 'text-xs text-text-muted';
    p.textContent = t('settings.webhooks.deliveries.empty');
    return p;
  }

  const table = document.createElement('table');
  table.className = 'w-full text-xs border-collapse mt-1';
  table.innerHTML = `
    <thead>
      <tr class="text-text-muted">
        <th class="text-left py-1 pr-3 font-medium">${t('settings.webhooks.deliveries.col.event')}</th>
        <th class="text-left py-1 pr-3 font-medium">${t('settings.webhooks.deliveries.col.status')}</th>
        <th class="text-left py-1 font-medium">${t('settings.webhooks.deliveries.col.time')}</th>
      </tr>
    </thead>
    <tbody></tbody>
  `;
  const tbody = /** @type {HTMLTableSectionElement} */ (table.querySelector('tbody'));
  for (const d of deliveries) {
    const tr = document.createElement('tr');
    const ok = d.http_status && d.http_status >= 200 && d.http_status < 300;
    tr.innerHTML = `
      <td class="py-1 pr-3 text-text">${escapeHtml(d.event_type)}</td>
      <td class="py-1 pr-3 ${ok ? 'text-success' : 'text-danger'}">${d.http_status ?? '—'}${d.error ? ` · ${escapeHtml(d.error.slice(0, 60))}` : ''}</td>
      <td class="py-1 text-text-muted">${new Date(d.delivered_at).toLocaleString()}</td>
    `;
    tbody.appendChild(tr);
  }
  return table;
}

// ── Add / Edit form ───────────────────────────────────────────────────────────

/**
 * @param {any|null} existing  - existing webhook row (edit mode) or null (create mode)
 * @param {(data: any) => Promise<void>} onSave
 * @param {() => void} onCancel
 */
function _mkForm(existing, onSave, onCancel) {
  const form = document.createElement('div');
  form.className = 'bg-surface-2 rounded-xl p-4 flex flex-col gap-3 mb-4';

  const title = document.createElement('p');
  title.className = 'text-sm font-semibold text-text';
  title.textContent = existing ? t('settings.webhooks.form.edit') : t('settings.webhooks.form.add');
  form.appendChild(title);

  // URL
  const urlLabel = document.createElement('label');
  urlLabel.className = 'flex flex-col gap-1';
  urlLabel.innerHTML = `<span class="text-xs text-text-muted font-medium">${t('settings.webhooks.form.url')} <span class="text-danger">*</span></span>`;
  const urlInput = document.createElement('input');
  urlInput.type = 'url';
  urlInput.className = 'input text-sm';
  urlInput.placeholder = 'https://example.com/hook';
  urlInput.value = existing?.url ?? '';
  urlInput.required = true;
  urlLabel.appendChild(urlInput);
  form.appendChild(urlLabel);

  // Secret
  const secretLabel = document.createElement('label');
  secretLabel.className = 'flex flex-col gap-1';
  secretLabel.innerHTML = `<span class="text-xs text-text-muted font-medium">${t('settings.webhooks.form.secret')}</span>`;
  const secretInput = document.createElement('input');
  secretInput.type = 'password';
  secretInput.className = 'input text-sm';
  secretInput.autocomplete = 'new-password';
  secretInput.placeholder = existing ? t('settings.webhooks.form.secret.unchanged') : t('settings.webhooks.form.secret.placeholder');
  secretLabel.appendChild(secretInput);
  form.appendChild(secretLabel);

  // Events
  const eventsLabel = document.createElement('div');
  eventsLabel.className = 'flex flex-col gap-1.5';
  eventsLabel.innerHTML = `<span class="text-xs text-text-muted font-medium">${t('settings.webhooks.form.events')}</span>`;

  const existingState = parseEvents(existing?.events ?? '["*"]');

  // "All events" master checkbox
  const allRow = document.createElement('label');
  allRow.className = 'flex items-center gap-2 text-sm text-text cursor-pointer';
  const allCheck = document.createElement('input');
  allCheck.type = 'checkbox';
  allCheck.className = 'rounded';
  allCheck.checked = existingState.all;
  allRow.appendChild(allCheck);
  allRow.append(t('settings.webhooks.event.all'));
  eventsLabel.appendChild(allRow);

  // Individual event checkboxes
  /** @type {Map<string, HTMLInputElement>} */
  const eventCheckboxes = new Map();
  const eventChecksWrap = document.createElement('div');
  eventChecksWrap.className = 'ml-4 flex flex-col gap-1';
  for (const ev of ALL_EVENTS) {
    const lbl = document.createElement('label');
    lbl.className = 'flex items-center gap-2 text-sm text-text cursor-pointer';
    const chk = document.createElement('input');
    chk.type = 'checkbox';
    chk.className = 'rounded';
    chk.checked = existingState.selected.has(ev.value);
    chk.disabled = existingState.all;
    eventCheckboxes.set(ev.value, chk);
    lbl.appendChild(chk);
    lbl.append(ev.label);
    const desc = document.createElement('span');
    desc.className = 'text-xs text-text-muted';
    desc.textContent = `— ${ev.description}`;
    lbl.appendChild(desc);
    eventChecksWrap.appendChild(lbl);
  }

  allCheck.addEventListener('change', () => {
    for (const [, chk] of eventCheckboxes) {
      chk.disabled = allCheck.checked;
      if (allCheck.checked) chk.checked = true;
    }
  });

  eventsLabel.appendChild(eventChecksWrap);
  form.appendChild(eventsLabel);

  // Buttons
  const btnRow = document.createElement('div');
  btnRow.className = 'flex items-center gap-2 mt-1';

  const saveBtn = document.createElement('button');
  saveBtn.type = 'button';
  saveBtn.className = 'btn-primary btn-sm';
  saveBtn.textContent = existing ? t('settings.webhooks.form.save_changes') : t('settings.webhooks.form.add');

  const cancelBtn = document.createElement('button');
  cancelBtn.type = 'button';
  cancelBtn.className = 'btn-ghost btn-sm';
  cancelBtn.textContent = t('common.cancel');

  btnRow.appendChild(saveBtn);
  btnRow.appendChild(cancelBtn);
  form.appendChild(btnRow);

  cancelBtn.addEventListener('click', onCancel);

  saveBtn.addEventListener('click', async () => {
    const url = urlInput.value.trim();
    if (!url.startsWith('http://') && !url.startsWith('https://')) {
      showToast(t('settings.webhooks.form.url_error'), { type: 'error' });
      return;
    }

    const evState = {
      all: allCheck.checked,
      selected: new Set(
        [...eventCheckboxes.entries()]
          .filter(([, chk]) => chk.checked)
          .map(([v]) => v),
      ),
    };
    const events = serializeEvents(evState);

    /** @type {any} */
    const data = { url, events };
    if (secretInput.value) data.secret = secretInput.value;

    saveBtn.disabled = true;
    try {
      await onSave(data);
    } catch (e) {
      showApiError(e);
      saveBtn.disabled = false;
    }
  });

  return form;
}
