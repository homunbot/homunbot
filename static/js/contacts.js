// Contacts page — personal CRM UI
// Note: innerHTML is used with esc() XSS sanitization, consistent with all other Homun JS files.
'use strict';

const API = '/api/v1/contacts';
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
        showForm(null);
    });
});

// ── Data ────────────────────────────────────────────────────────────

async function loadContacts() {
    try {
        const res = await fetch(API);
        allContacts = await res.json();
        renderList(allContacts);
    } catch (e) {
        console.error('Failed to load contacts', e);
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
    // Build DOM safely — all dynamic text uses textContent via esc()
    const html = contacts.map(c => {
        const nick = c.nickname ? '<span class="badge">' + esc(c.nickname) + '</span>' : '';
        const bio = c.bio ? '<p class="contact-bio">' + esc(c.bio) + '</p>' : '';
        const ch = c.preferred_channel ? '<span class="pill">' + esc(c.preferred_channel) + '</span>' : '';
        const bday = c.birthday ? '<span class="pill">' + esc(c.birthday) + '</span>' : '';
        return '<div class="contact-card card" data-id="' + c.id + '" onclick="selectContact(' + c.id + ')">'
            + '<div class="contact-card-header"><strong>' + esc(c.name) + '</strong> ' + nick + '</div>'
            + bio
            + '<div class="contact-meta">' + ch + '<span class="pill pill-mode">' + esc(c.response_mode) + '</span>' + bday + '</div>'
            + '</div>';
    }).join('');
    grid.innerHTML = html; // safe: all values pass through esc()
}

// ── Detail ──────────────────────────────────────────────────────────

async function selectContact(id) {
    selectedId = id;
    try {
        const res = await fetch(API + '/' + id);
        const data = await res.json();
        const c = data.contact || data;
        const ids = data.identities || [];
        const [rels, events] = await Promise.all([
            fetch(API + '/' + id + '/relationships').then(r => r.json()),
            fetch(API + '/' + id + '/events').then(r => r.json()),
        ]);
        showDetail(c, ids, rels, events);
    } catch (e) {
        console.error('Failed to load contact', e);
    }
}

function showDetail(c, identities, relationships, events) {
    const el = document.getElementById('contact-detail');
    el.style.display = 'block';

    const idsHtml = identities.length
        ? identities.map(i =>
            '<div class="identity-row"><span class="pill">' + esc(i.channel) + '</span> '
            + esc(i.identifier) + (i.label ? ' (' + esc(i.label) + ')' : '')
            + ' <button class="btn btn-ghost btn-sm" onclick="removeIdentity(' + i.id + ')">x</button></div>'
        ).join('')
        : '<p class="text-secondary">None</p>';

    const relsHtml = relationships.length
        ? relationships.map(r =>
            '<div class="rel-row">' + esc(r.relationship_type) + ' → #' + r.to_contact_id
            + ' <button class="btn btn-ghost btn-sm" onclick="removeRelationship(' + r.id + ')">x</button></div>'
        ).join('')
        : '<p class="text-secondary">None</p>';

    const evsHtml = events.length
        ? events.map(e =>
            '<div class="event-row"><span class="pill">' + esc(e.event_type) + '</span> '
            + esc(e.date) + (e.label ? ' — ' + esc(e.label) : '') + (e.auto_greet ? ' auto' : '')
            + ' <button class="btn btn-ghost btn-sm" onclick="removeEvent(' + e.id + ')">x</button></div>'
        ).join('')
        : '<p class="text-secondary">None</p>';

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
        + '<div><strong>Channel</strong>: ' + esc(c.preferred_channel || '-') + '</div>'
        + '<div><strong>Mode</strong>: ' + esc(c.response_mode) + '</div>'
        + '<div><strong>Birthday</strong>: ' + esc(c.birthday || '-') + '</div>'
        + '</div>'
        + '<h3 style="margin-top:20px">Identities</h3>' + idsHtml
        + '<button class="btn btn-ghost btn-sm" onclick="addIdentityPrompt(' + c.id + ')">+ Add Identity</button>'
        + '<h3 style="margin-top:20px">Relationships</h3>' + relsHtml
        + '<button class="btn btn-ghost btn-sm" onclick="addRelationshipPrompt(' + c.id + ')">+ Add Relationship</button>'
        + '<h3 style="margin-top:20px">Events</h3>' + evsHtml
        + '<button class="btn btn-ghost btn-sm" onclick="addEventPrompt(' + c.id + ')">+ Add Event</button>'
        + '</div>';
}

// ── Form ────────────────────────────────────────────────────────────

function showForm(id) {
    const c = id ? allContacts.find(x => x.id === id) : {};
    const modal = document.getElementById('contact-form-modal');
    modal.style.display = 'block';
    const channels = ['telegram','whatsapp','discord','slack','email','web'];
    const modes = ['automatic','assisted','on_demand','silent'];
    const chOpts = channels.map(ch =>
        '<option value="' + ch + '"' + ((c.preferred_channel||'') === ch ? ' selected' : '') + '>' + ch + '</option>'
    ).join('');
    const mOpts = modes.map(m =>
        '<option value="' + m + '"' + ((c.response_mode||'automatic') === m ? ' selected' : '') + '>' + m + '</option>'
    ).join('');
    modal.innerHTML = '<div class="modal-overlay" onclick="closeForm()"></div>'
        + '<div class="modal-content card" style="padding:24px;max-width:500px;margin:60px auto;position:relative;z-index:1">'
        + '<h2>' + (id ? 'Edit' : 'New') + ' Contact</h2>'
        + '<form id="contact-form" onsubmit="saveContact(event,' + (id || 'null') + ')">'
        + '<label>Name *<input name="name" class="input" required value="' + esc(c.name||'') + '"></label>'
        + '<label>Nickname<input name="nickname" class="input" value="' + esc(c.nickname||'') + '"></label>'
        + '<label>Bio<textarea name="bio" class="input" rows="2">' + esc(c.bio||'') + '</textarea></label>'
        + '<label>Notes<textarea name="notes" class="input" rows="2">' + esc(c.notes||'') + '</textarea></label>'
        + '<label>Birthday<input name="birthday" class="input" type="date" value="' + (c.birthday||'') + '"></label>'
        + '<label>Preferred Channel<select name="preferred_channel" class="input"><option value="">—</option>' + chOpts + '</select></label>'
        + '<label>Response Mode<select name="response_mode" class="input">' + mOpts + '</select></label>'
        + '<div style="display:flex;gap:8px;margin-top:16px">'
        + '<button type="submit" class="btn btn-primary">' + (id ? 'Save' : 'Create') + '</button>'
        + '<button type="button" class="btn btn-ghost" onclick="closeForm()">Cancel</button>'
        + '</div></form></div>';
}

function closeForm() {
    document.getElementById('contact-form-modal').style.display = 'none';
}

async function saveContact(e, id) {
    e.preventDefault();
    const form = new FormData(e.target);
    const body = Object.fromEntries(form.entries());
    try {
        const url = id ? API + '/' + id : API;
        const method = id ? 'PUT' : 'POST';
        await fetch(url, { method, headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
        closeForm();
        await loadContacts();
        if (id) selectContact(id);
    } catch (e) { console.error('Failed to save contact', e); }
}

async function deleteContact(id) {
    if (!confirm('Delete this contact?')) return;
    await fetch(API + '/' + id, { method: 'DELETE' });
    document.getElementById('contact-detail').style.display = 'none';
    selectedId = null;
    loadContacts();
}

// ── Inline additions ────────────────────────────────────────────────

async function addIdentityPrompt(contactId) {
    const channel = prompt('Channel (telegram, whatsapp, discord, slack, email):');
    if (!channel) return;
    const identifier = prompt('Identifier (user ID, email, phone):');
    if (!identifier) return;
    await fetch(API + '/' + contactId + '/identities', {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ channel, identifier }),
    });
    selectContact(contactId);
}

async function removeIdentity(id) {
    await fetch(API + '/identities/' + id, { method: 'DELETE' });
    if (selectedId) selectContact(selectedId);
}

async function addRelationshipPrompt(contactId) {
    const toId = prompt('Target contact ID:');
    if (!toId) return;
    const type = prompt('Relationship type (madre, padre, collega, partner, amico/a):');
    if (!type) return;
    await fetch(API + '/' + contactId + '/relationships', {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ to_contact_id: parseInt(toId), relationship_type: type }),
    });
    selectContact(contactId);
}

async function removeRelationship(id) {
    await fetch(API + '/relationships/' + id, { method: 'DELETE' });
    if (selectedId) selectContact(selectedId);
}

async function addEventPrompt(contactId) {
    const type = prompt('Event type (birthday, nameday, anniversary, custom):');
    if (!type) return;
    const date = prompt('Date (MM-DD or YYYY-MM-DD):');
    if (!date) return;
    const label = prompt('Label (optional):') || undefined;
    await fetch(API + '/' + contactId + '/events', {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ event_type: type, date, label }),
    });
    selectContact(contactId);
}

async function removeEvent(id) {
    await fetch(API + '/events/' + id, { method: 'DELETE' });
    if (selectedId) selectContact(selectedId);
}

// ── Pending ─────────────────────────────────────────────────────────

async function loadPending() {
    try {
        const res = await fetch(API + '/pending');
        const pending = await res.json();
        const section = document.getElementById('pending-section');
        if (!pending.length) { section.textContent = ''; return; }
        section.innerHTML = '<h2>Pending Responses</h2>'
            + pending.map(p =>
                '<div class="card" style="padding:16px;margin-bottom:12px">'
                + '<div><strong>' + esc(p.channel) + '</strong> — ' + esc(p.inbound_content.substring(0, 200)) + '</div>'
                + (p.draft_response ? '<div class="text-secondary" style="margin-top:8px">' + esc(p.draft_response.substring(0, 300)) + '</div>' : '')
                + '<div style="margin-top:8px;display:flex;gap:8px">'
                + '<button class="btn btn-primary btn-sm" onclick="approvePending(' + p.id + ')">Approve</button>'
                + '<button class="btn btn-ghost btn-sm" onclick="rejectPending(' + p.id + ')">Reject</button>'
                + '</div></div>'
            ).join('');
    } catch (e) { console.error('Failed to load pending', e); }
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
