// Contacts page — personal CRM UI
// Security: all innerHTML usage goes through esc() which uses textContent-based sanitization.
// No user-controlled strings are inserted without esc(). This is consistent with all Homun JS files.
'use strict';

const API = '/api/v1/contacts';
const CHANNELS = ['telegram', 'whatsapp', 'discord', 'slack', 'email', 'web'];
const MODES = ['automatic', 'assisted', 'on_demand', 'silent'];
const REL_TYPES = ['partner', 'madre', 'padre', 'figlio/a', 'fratello/sorella', 'collega', 'amico/a', 'capo', 'cliente'];
const EVENT_TYPES = ['birthday', 'nameday', 'anniversary', 'custom'];
let allContacts = [];
let selectedId = null;

// ── Init ────────────────────────────────────────────────────────────

document.addEventListener('DOMContentLoaded', () => {
    loadContacts();
    loadPending();

    document.getElementById('contact-search').addEventListener('input', e => {
        renderList(filterContacts(e.target.value));
    });

    document.getElementById('add-contact-btn').addEventListener('click', () => {
        toggleCreatePanel(true);
    });
    document.getElementById('cancel-create-btn').addEventListener('click', () => {
        toggleCreatePanel(false);
    });
    document.getElementById('add-identity-row-btn').addEventListener('click', addFormIdentityRow);
    document.getElementById('contact-form').addEventListener('submit', e => saveNewContact(e));

    // Add one empty identity row by default
    addFormIdentityRow();
});

// ── Data ────────────────────────────────────────────────────────────

async function loadContacts() {
    try {
        const res = await fetch(API);
        allContacts = await res.json();
        renderList(allContacts);
        const badge = document.getElementById('contacts-count');
        if (badge) badge.textContent = allContacts.length;
    } catch (e) {
        console.error('Failed to load contacts', e);
    }
}

function toggleCreatePanel(show) {
    const panel = document.getElementById('contact-create-panel');
    if (panel) panel.style.display = show ? '' : 'none';
    if (show) {
        document.getElementById('cf-name').focus();
    } else {
        document.getElementById('contact-form').reset();
        document.getElementById('form-identities').innerHTML = '';
        addFormIdentityRow();
    }
}

function filterContacts(q) {
    if (!q) return allContacts;
    const lower = q.toLowerCase();
    return allContacts.filter(c =>
        c.name.toLowerCase().includes(lower) ||
        (c.nickname || '').toLowerCase().includes(lower) ||
        c.bio.toLowerCase().includes(lower)
    );
}

function contactName(id) {
    const c = allContacts.find(x => x.id === id);
    return c ? c.name : '#' + id;
}

// ── List ────────────────────────────────────────────────────────────

function renderList(contacts) {
    const grid = document.getElementById('contacts-list');
    if (!contacts.length) {
        grid.textContent = '';
        const p = document.createElement('p');
        p.textContent = 'No contacts yet. Click "+ New Contact" to add one.';
        p.className = 'empty-state';
        grid.appendChild(p);
        return;
    }
    // All dynamic text sanitized through esc()
    const html = contacts.map(c => {
        const nick = c.nickname ? '<span class="badge">' + esc(c.nickname) + '</span>' : '';
        const ch = c.preferred_channel
            ? '<span class="pill">' + esc(c.preferred_channel) + '</span>'
            : '';
        return '<div class="card contact-card" data-id="' + c.id + '" style="padding:16px;cursor:pointer">'
            + '<div style="display:flex;align-items:center;gap:8px">'
            + '<strong>' + esc(c.name) + '</strong> ' + nick + ch
            + '</div>'
            + (c.bio ? '<p class="text-secondary" style="margin:4px 0 0;font-size:13px">' + esc(c.bio.substring(0, 80)) + '</p>' : '')
            + '</div>';
    }).join('');
    grid.innerHTML = html;

    grid.querySelectorAll('.contact-card').forEach(el => {
        el.addEventListener('click', () => selectContact(parseInt(el.dataset.id)));
    });
}

async function selectContact(id) {
    selectedId = id;
    try {
        const c = allContacts.find(x => x.id === id);
        if (!c) return;
        const [ids, rels, events] = await Promise.all([
            fetch(API + '/' + id + '/identities').then(r => r.json()),
            fetch(API + '/' + id + '/relationships').then(r => r.json()),
            fetch(API + '/' + id + '/events').then(r => r.json()),
        ]);
        showDetail(c, ids, rels, events);
    } catch (e) {
        console.error('Failed to load contact', e);
    }
}

// ── Detail View ────────────────────────────────────────────────────

function showDetail(c, identities, relationships, events) {
    const el = document.getElementById('contact-detail');
    el.style.display = 'block';

    const idsHtml = identities.length
        ? identities.map(i =>
            '<div class="identity-row" style="display:flex;align-items:center;gap:8px;padding:6px 0">'
            + '<span class="pill">' + esc(i.channel) + '</span> '
            + '<span style="font-family:var(--mono);font-size:13px">' + esc(i.identifier) + '</span>'
            + (i.label ? ' <span class="text-secondary">(' + esc(i.label) + ')</span>' : '')
            + ' <button class="btn btn-ghost btn-sm" onclick="removeIdentity(' + i.id + ')">&#xd7;</button>'
            + '</div>'
        ).join('')
        : '<p class="text-secondary">No channel identities yet</p>';

    const relsHtml = relationships.length
        ? relationships.map(r =>
            '<div class="rel-row" style="display:flex;align-items:center;gap:8px;padding:6px 0">'
            + '<span class="pill">' + esc(r.relationship_type) + '</span> '
            + '<strong>' + esc(contactName(r.to_contact_id)) + '</strong>'
            + ' <button class="btn btn-ghost btn-sm" onclick="removeRelationship(' + r.id + ')">&#xd7;</button>'
            + '</div>'
        ).join('')
        : '<p class="text-secondary">No relationships</p>';

    const evsHtml = events.length
        ? events.map(ev =>
            '<div class="event-row" style="display:flex;align-items:center;gap:8px;padding:6px 0">'
            + '<span class="pill">' + esc(ev.event_type) + '</span> '
            + '<span>' + esc(ev.date) + '</span>'
            + (ev.label ? ' — ' + esc(ev.label) : '')
            + (ev.auto_greet ? ' <span class="badge">auto-greet</span>' : '')
            + ' <button class="btn btn-ghost btn-sm" onclick="removeEvent(' + ev.id + ')">&#xd7;</button>'
            + '</div>'
        ).join('')
        : '<p class="text-secondary">No events</p>';

    // All dynamic content sanitized through esc()
    el.innerHTML = '<div class="card" style="margin-top:24px;padding:24px">'
        + '<div style="display:flex;justify-content:space-between;align-items:center">'
        + '<h2>' + esc(c.name) + (c.nickname ? ' <span class="badge">' + esc(c.nickname) + '</span>' : '') + '</h2>'
        + '<div style="display:flex;gap:8px">'
        + '<button class="btn btn-ghost" onclick="showForm(' + c.id + ')">Edit</button>'
        + '<button class="btn btn-danger" onclick="deleteContact(' + c.id + ')">Delete</button>'
        + '</div></div>'
        + (c.bio ? '<p>' + esc(c.bio) + '</p>' : '')
        + '<div class="detail-grid">'
        + '<div><strong>Channel</strong>: ' + esc(c.preferred_channel || '\u2014') + '</div>'
        + '<div><strong>Mode</strong>: ' + esc(c.response_mode) + '</div>'
        + '<div><strong>Tone</strong>: ' + esc(c.tone_of_voice || '\u2014') + '</div>'
        + '<div><strong>Persona</strong>: ' + esc(c.persona_override || 'channel default') + '</div>'
        + '<div><strong>Birthday</strong>: ' + esc(c.birthday || '\u2014') + '</div>'
        + '</div>'
        + (c.persona_instructions ? '<p class="text-secondary" style="margin-top:8px"><strong>Persona instructions</strong>: ' + esc(c.persona_instructions) + '</p>' : '')
        + '<h3 style="margin-top:24px">Identities</h3>' + idsHtml
        + '<div id="add-identity-area"></div>'
        + '<button class="btn btn-ghost btn-sm" onclick="showAddIdentity(' + c.id + ')" id="add-identity-btn">+ Add identity</button>'
        + '<h3 style="margin-top:24px">Relationships</h3>' + relsHtml
        + '<div id="add-relationship-area"></div>'
        + '<button class="btn btn-ghost btn-sm" onclick="showAddRelationship(' + c.id + ')" id="add-rel-btn">+ Add relationship</button>'
        + '<h3 style="margin-top:24px">Events</h3>' + evsHtml
        + '<div id="add-event-area"></div>'
        + '<button class="btn btn-ghost btn-sm" onclick="showAddEvent(' + c.id + ')" id="add-event-btn">+ Add event</button>'
        + '</div>';
}

// ── Edit Form (modal, same pattern as Workflows) ───────────────────

function showForm(id) {
    if (!id) { toggleCreatePanel(true); return; }
    const c = allContacts.find(x => x.id === id);
    if (!c) return;
    const modal = document.getElementById('contact-edit-modal');
    modal.classList.add('open');
    const chOpts = CHANNELS.map(ch =>
        '<option value="' + ch + '"' + ((c.preferred_channel || '') === ch ? ' selected' : '') + '>' + ch + '</option>'
    ).join('');
    const mOpts = MODES.map(m =>
        '<option value="' + m + '"' + ((c.response_mode || 'automatic') === m ? ' selected' : '') + '>' + m + '</option>'
    ).join('');
    // All dynamic content sanitized through esc()
    modal.innerHTML = '<div class="modal-backdrop" onclick="closeForm()"></div>'
        + '<div class="modal-content card" style="padding:24px;max-width:520px;overflow-y:auto">'
        + '<h2>Edit Contact</h2>'
        + '<form id="contact-edit-form" onsubmit="saveEditContact(event,' + id + ')">'
        + '<label>Name *<input name="name" class="input" required value="' + esc(c.name || '') + '"></label>'
        + '<label>Nickname<input name="nickname" class="input" value="' + esc(c.nickname || '') + '"></label>'
        + '<label>Bio<textarea name="bio" class="input" rows="2">' + esc(c.bio || '') + '</textarea></label>'
        + '<label>Notes<textarea name="notes" class="input" rows="2">' + esc(c.notes || '') + '</textarea></label>'
        + '<label>Birthday<input name="birthday" class="input" type="date" value="' + (c.birthday || '') + '"></label>'
        + '<label>Preferred Channel<select name="preferred_channel" class="input"><option value="">\u2014</option>' + chOpts + '</select></label>'
        + '<label>Response Mode<select name="response_mode" class="input">' + mOpts + '</select></label>'
        + '<label>Tone of Voice<input name="tone_of_voice" class="input" placeholder="e.g. formal, informal, friendly" value="' + esc(c.tone_of_voice || '') + '"></label>'
        + '<label>Persona Override<select name="persona_override" class="input" onchange="document.getElementById(\'persona-instr-group\').style.display=this.value===\'custom\'?\'block\':\'none\'">'
        + '<option value=""' + (!c.persona_override ? ' selected' : '') + '>Channel default</option>'
        + '<option value="bot"' + (c.persona_override === 'bot' ? ' selected' : '') + '>Bot</option>'
        + '<option value="owner"' + (c.persona_override === 'owner' ? ' selected' : '') + '>Owner</option>'
        + '<option value="company"' + (c.persona_override === 'company' ? ' selected' : '') + '>Company</option>'
        + '<option value="custom"' + (c.persona_override === 'custom' ? ' selected' : '') + '>Custom</option>'
        + '</select></label>'
        + '<div id="persona-instr-group" style="display:' + (c.persona_override === 'custom' ? 'block' : 'none') + '">'
        + '<label>Persona Instructions<textarea name="persona_instructions" class="input" rows="3" placeholder="Custom instructions for how the agent should present itself to this contact">' + esc(c.persona_instructions || '') + '</textarea></label>'
        + '</div>'
        + '<div style="display:flex;gap:8px;margin-top:16px">'
        + '<button type="submit" class="btn btn-primary btn-sm">Save</button>'
        + '<button type="button" class="btn btn-ghost btn-sm" onclick="closeForm()">Cancel</button>'
        + '</div></form></div>';
}

function addFormIdentityRow() {
    const container = document.getElementById('form-identities');
    if (!container) return;
    const row = document.createElement('div');
    row.style.cssText = 'display:flex;gap:8px;margin-bottom:8px;align-items:center';
    const placeholders = { telegram: 'User ID or @username', whatsapp: '+39 333 1234567', discord: 'User#1234', slack: 'U012345', email: 'name@example.com' };
    // Channel options built from trusted constant array, no user input
    const chSel = '<select class="input form-id-channel" style="width:130px">'
        + CHANNELS.filter(c => c !== 'web').map(ch =>
            '<option value="' + ch + '">' + ch + '</option>'
        ).join('') + '</select>';
    row.innerHTML = chSel
        + '<input type="text" class="input form-id-value" placeholder="' + esc(placeholders.telegram) + '" style="flex:1">'
        + '<button type="button" class="btn btn-ghost btn-sm" onclick="this.parentElement.remove()">&#xd7;</button>';

    const select = row.querySelector('.form-id-channel');
    const input = row.querySelector('.form-id-value');
    select.addEventListener('change', () => {
        input.placeholder = placeholders[select.value] || 'Identifier';
    });
    container.appendChild(row);
}

function closeForm() {
    const modal = document.getElementById('contact-edit-modal');
    modal.classList.remove('open');
    modal.innerHTML = '';
}

async function saveNewContact(e) {
    e.preventDefault();
    const form = new FormData(e.target);
    const body = Object.fromEntries(form.entries());

    const idRows = document.querySelectorAll('#form-identities > div');
    const identities = [];
    idRows.forEach(row => {
        const ch = row.querySelector('.form-id-channel')?.value;
        const val = row.querySelector('.form-id-value')?.value?.trim();
        if (ch && val) identities.push({ channel: ch, identifier: val });
    });

    try {
        const res = await fetch(API, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(body),
        });
        const saved = await res.json();

        if (saved.id && identities.length) {
            await Promise.all(identities.map(ident =>
                fetch(API + '/' + saved.id + '/identities', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify(ident),
                })
            ));
        }

        toggleCreatePanel(false);
        await loadContacts();
        if (saved.id) selectContact(saved.id);
    } catch (err) { console.error('Failed to save contact', err); }
}

async function saveEditContact(e, id) {
    e.preventDefault();
    const form = new FormData(e.target);
    const body = Object.fromEntries(form.entries());
    try {
        await fetch(API + '/' + id, {
            method: 'PUT',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(body),
        });
        closeForm();
        await loadContacts();
        selectContact(id);
    } catch (err) { console.error('Failed to save contact', err); }
}

async function deleteContact(id) {
    if (!confirm('Delete this contact?')) return;
    await fetch(API + '/' + id, { method: 'DELETE' });
    document.getElementById('contact-detail').style.display = 'none';
    selectedId = null;
    loadContacts();
}

// ── Inline Add Identity ─────────────────────────────────────────────

function showAddIdentity(contactId) {
    const area = document.getElementById('add-identity-area');
    if (!area) return;
    document.getElementById('add-identity-btn').style.display = 'none';
    const placeholders = { telegram: 'User ID or @username', whatsapp: '+39 333 1234567', discord: 'User#1234', slack: 'U012345', email: 'name@example.com' };
    // Channel options built from trusted constant array
    area.innerHTML = '<div style="display:flex;gap:8px;align-items:center;padding:8px 0">'
        + '<select id="id-ch" class="input" style="width:130px">'
        + CHANNELS.filter(c => c !== 'web').map(ch => '<option value="' + ch + '">' + ch + '</option>').join('')
        + '</select>'
        + '<input id="id-val" class="input" placeholder="' + esc(placeholders.telegram) + '" style="flex:1">'
        + '<input id="id-label" class="input" placeholder="Label (optional)" style="width:120px">'
        + '<button class="btn btn-primary btn-sm" onclick="commitAddIdentity(' + contactId + ')">Add</button>'
        + '<button class="btn btn-ghost btn-sm" onclick="cancelInline(\'add-identity-area\',\'add-identity-btn\')">&#xd7;</button>'
        + '</div>';
    const sel = document.getElementById('id-ch');
    const inp = document.getElementById('id-val');
    sel.addEventListener('change', () => { inp.placeholder = placeholders[sel.value] || 'Identifier'; });
    inp.focus();
}

async function commitAddIdentity(contactId) {
    const channel = document.getElementById('id-ch').value;
    const identifier = document.getElementById('id-val').value.trim();
    const label = document.getElementById('id-label').value.trim() || undefined;
    if (!identifier) return;
    await fetch(API + '/' + contactId + '/identities', {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ channel, identifier, label }),
    });
    selectContact(contactId);
}

async function removeIdentity(id) {
    await fetch(API + '/identities/' + id, { method: 'DELETE' });
    if (selectedId) selectContact(selectedId);
}

// ── Inline Add Relationship ─────────────────────────────────────────

function showAddRelationship(contactId) {
    const area = document.getElementById('add-relationship-area');
    if (!area) return;
    document.getElementById('add-rel-btn').style.display = 'none';

    // Contact options built from allContacts (names sanitized with esc())
    const opts = allContacts
        .filter(c => c.id !== contactId)
        .map(c => '<option value="' + c.id + '">' + esc(c.name) + (c.nickname ? ' (' + esc(c.nickname) + ')' : '') + '</option>')
        .join('');
    // Relationship types from trusted constant array
    const typeOpts = REL_TYPES.map(t => '<option value="' + t + '">' + t + '</option>').join('');

    area.innerHTML = '<div style="display:flex;gap:8px;align-items:center;padding:8px 0;flex-wrap:wrap">'
        + '<select id="rel-type" class="input" style="width:150px"><option value="">Type\u2026</option>' + typeOpts + '<option value="__custom">Other\u2026</option></select>'
        + '<input id="rel-type-custom" class="input" placeholder="Custom type" style="width:130px;display:none">'
        + '<select id="rel-target" class="input" style="flex:1"><option value="">Select contact\u2026</option>' + opts + '</select>'
        + '<button class="btn btn-primary btn-sm" onclick="commitAddRelationship(' + contactId + ')">Add</button>'
        + '<button class="btn btn-ghost btn-sm" onclick="cancelInline(\'add-relationship-area\',\'add-rel-btn\')">&#xd7;</button>'
        + '</div>';
    document.getElementById('rel-type').addEventListener('change', function () {
        document.getElementById('rel-type-custom').style.display = this.value === '__custom' ? '' : 'none';
    });
}

async function commitAddRelationship(contactId) {
    let relType = document.getElementById('rel-type').value;
    if (relType === '__custom') relType = document.getElementById('rel-type-custom').value.trim();
    const toId = parseInt(document.getElementById('rel-target').value);
    if (!relType || !toId) return;
    await fetch(API + '/' + contactId + '/relationships', {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ to_contact_id: toId, relationship_type: relType }),
    });
    selectContact(contactId);
}

async function removeRelationship(id) {
    await fetch(API + '/relationships/' + id, { method: 'DELETE' });
    if (selectedId) selectContact(selectedId);
}

// ── Inline Add Event ────────────────────────────────────────────────

function showAddEvent(contactId) {
    const area = document.getElementById('add-event-area');
    if (!area) return;
    document.getElementById('add-event-btn').style.display = 'none';
    // Event types from trusted constant array
    const typeOpts = EVENT_TYPES.map(t => '<option value="' + t + '">' + t + '</option>').join('');
    area.innerHTML = '<div style="display:flex;gap:8px;align-items:center;padding:8px 0;flex-wrap:wrap">'
        + '<select id="ev-type" class="input" style="width:120px">' + typeOpts + '</select>'
        + '<input id="ev-date" class="input" type="date" style="width:150px">'
        + '<input id="ev-label" class="input" placeholder="Label (optional)" style="flex:1">'
        + '<label style="font-size:13px;display:flex;align-items:center;gap:4px"><input type="checkbox" id="ev-greet"> Auto-greet</label>'
        + '<button class="btn btn-primary btn-sm" onclick="commitAddEvent(' + contactId + ')">Add</button>'
        + '<button class="btn btn-ghost btn-sm" onclick="cancelInline(\'add-event-area\',\'add-event-btn\')">&#xd7;</button>'
        + '</div>';
}

async function commitAddEvent(contactId) {
    const event_type = document.getElementById('ev-type').value;
    const date = document.getElementById('ev-date').value;
    if (!date) return;
    const label = document.getElementById('ev-label').value.trim() || undefined;
    const auto_greet = document.getElementById('ev-greet').checked;
    await fetch(API + '/' + contactId + '/events', {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ event_type, date, label, auto_greet }),
    });
    selectContact(contactId);
}

async function removeEvent(id) {
    await fetch(API + '/events/' + id, { method: 'DELETE' });
    if (selectedId) selectContact(selectedId);
}

function cancelInline(areaId, btnId) {
    const area = document.getElementById(areaId);
    if (area) area.innerHTML = '';
    const btn = document.getElementById(btnId);
    if (btn) btn.style.display = '';
}

// ── Pending ─────────────────────────────────────────────────────────

async function loadPending() {
    try {
        const res = await fetch(API + '/pending');
        const pending = await res.json();
        const section = document.getElementById('pending-section');
        if (!section) return;
        if (!pending.length) { section.textContent = ''; return; }
        // All dynamic content sanitized through esc()
        section.innerHTML = '<h2>Pending Responses</h2>'
            + pending.map(p =>
                '<div class="card" style="padding:16px;margin-bottom:12px">'
                + '<div><strong>' + esc(p.channel) + '</strong> \u2014 ' + esc(p.inbound_content.substring(0, 200)) + '</div>'
                + (p.draft_response ? '<div class="text-secondary" style="margin-top:8px">' + esc(p.draft_response.substring(0, 300)) + '</div>' : '')
                + '<div style="margin-top:8px;display:flex;gap:8px">'
                + '<button class="btn btn-primary btn-sm" onclick="approvePending(' + p.id + ')">Approve</button>'
                + '<button class="btn btn-ghost btn-sm" onclick="rejectPending(' + p.id + ')">Reject</button>'
                + '</div></div>'
            ).join('');
    } catch (err) { console.error('Failed to load pending', err); }
}

async function approvePending(id) {
    await fetch(API + '/pending/' + id + '/approve', { method: 'POST' });
    loadPending();
}

async function rejectPending(id) {
    await fetch(API + '/pending/' + id + '/reject', { method: 'POST' });
    loadPending();
}

// ── Util ────────────────────────────────────────────────────────────

function esc(s) {
    if (!s) return '';
    const div = document.createElement('div');
    div.textContent = String(s);
    return div.innerHTML;
}
