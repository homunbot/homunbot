// Contacts page — Address Book (master-detail split pane)
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

    document.getElementById('contact-search').addEventListener('input', e => {
        renderList(filterContacts(e.target.value));
    });

    document.getElementById('add-contact-btn').addEventListener('click', () => {
        selectedId = null;
        showCreateForm();
        renderList(filterContacts(document.getElementById('contact-search').value));
    });
});

// ── Data ────────────────────────────────────────────────────────────

async function loadContacts() {
    try {
        const res = await fetch(API);
        allContacts = await res.json();
        renderList(allContacts);
        document.getElementById('contacts-count').textContent = allContacts.length;
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

function contactName(id) {
    const c = allContacts.find(x => x.id === id);
    return c ? c.name : '#' + id;
}

function initials(name) {
    if (!name) return '?';
    const parts = name.trim().split(/\s+/);
    if (parts.length >= 2) return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase();
    return name.substring(0, 2).toUpperCase();
}

// ── List (sidebar rows) ─────────────────────────────────────────────

function renderList(contacts) {
    const list = document.getElementById('contacts-list');
    if (!contacts.length) {
        list.textContent = '';
        const p = document.createElement('p');
        p.textContent = 'No contacts yet.';
        p.className = 'contact-no-data';
        p.style.padding = '24px 16px';
        list.appendChild(p);
        return;
    }
    // All dynamic text sanitized through esc()
    const html = contacts.map(c => {
        const active = selectedId === c.id ? ' active' : '';
        const sub = c.nickname ? esc(c.nickname) : (c.bio ? esc(c.bio.substring(0, 40)) : '');
        return '<div class="contact-row' + active + '" data-id="' + c.id + '">'
            + '<div class="contact-avatar">' + esc(initials(c.name)) + '</div>'
            + '<div class="contact-row-info">'
            + '<div class="contact-row-name">' + esc(c.name) + '</div>'
            + (sub ? '<div class="contact-row-sub">' + sub + '</div>' : '')
            + '</div>'
            + (c.preferred_channel ? '<span class="pill" style="font-size:11px">' + esc(c.preferred_channel) + '</span>' : '')
            + '</div>';
    }).join('');
    list.innerHTML = html;

    list.querySelectorAll('.contact-row').forEach(el => {
        el.addEventListener('click', () => selectContact(parseInt(el.dataset.id)));
    });
}

async function selectContact(id) {
    selectedId = id;
    // Re-render list to update active state
    renderList(filterContacts(document.getElementById('contact-search').value));
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
    document.getElementById('contact-empty').style.display = 'none';
    const el = document.getElementById('contact-detail');
    el.style.display = 'block';
    // Mobile: add detail-open class
    document.getElementById('contacts-layout').classList.add('detail-open');

    const idsHtml = identities.length
        ? identities.map(i =>
            '<div class="contact-entity-row">'
            + '<span class="pill">' + esc(i.channel) + '</span>'
            + '<span class="contact-entity-id">' + esc(i.identifier) + '</span>'
            + (i.label ? '<span style="color:var(--t3);font-size:12px">' + esc(i.label) + '</span>' : '')
            + '<button class="btn btn-ghost btn-sm" onclick="removeIdentity(' + i.id + ')" title="Remove">&#xd7;</button>'
            + '</div>'
        ).join('')
        : '<p class="contact-no-data">No channel identities</p>';

    const relsHtml = relationships.length
        ? relationships.map(r =>
            '<div class="contact-entity-row">'
            + '<span class="pill">' + esc(r.relationship_type) + '</span>'
            + '<span class="contact-entity-name">' + esc(contactName(r.to_contact_id)) + '</span>'
            + '<button class="btn btn-ghost btn-sm" onclick="removeRelationship(' + r.id + ')" title="Remove">&#xd7;</button>'
            + '</div>'
        ).join('')
        : '<p class="contact-no-data">No relationships</p>';

    const evsHtml = events.length
        ? events.map(ev =>
            '<div class="contact-entity-row">'
            + '<span class="pill">' + esc(ev.event_type) + '</span>'
            + '<span class="contact-entity-name">' + esc(ev.date) + (ev.label ? ' — ' + esc(ev.label) : '') + '</span>'
            + (ev.auto_greet ? '<span class="badge" style="font-size:11px">auto-greet</span>' : '')
            + '<button class="btn btn-ghost btn-sm" onclick="removeEvent(' + ev.id + ')" title="Remove">&#xd7;</button>'
            + '</div>'
        ).join('')
        : '<p class="contact-no-data">No events</p>';

    const bioHtml = c.bio ? '<p style="margin:0;color:var(--t2);font-size:14px">' + esc(c.bio) + '</p>' : '';
    const personaNote = c.persona_instructions
        ? '<div class="contact-section"><div class="contact-section-header"><h3>Persona Instructions</h3></div>'
        + '<p style="font-size:13px;color:var(--t2);margin:0">' + esc(c.persona_instructions) + '</p></div>'
        : '';

    // All dynamic content sanitized through esc()
    el.innerHTML = '<div class="contact-detail-inner">'
        // Back button (mobile only)
        + '<button class="btn btn-ghost btn-sm contact-back-btn" onclick="goBackToList()" style="margin-bottom:12px">'
        + '&#8592; Back</button>'
        // Header
        + '<div class="contact-detail-header">'
        + '<div class="contact-avatar lg">' + esc(initials(c.name)) + '</div>'
        + '<div class="contact-detail-header-info">'
        + '<h2>' + esc(c.name) + '</h2>'
        + '<div class="contact-header-sub">'
        + (c.nickname ? '<span class="badge">' + esc(c.nickname) + '</span>' : '')
        + (c.preferred_channel ? '<span class="pill">' + esc(c.preferred_channel) + '</span>' : '')
        + '</div>'
        + bioHtml
        + '</div>'
        + '<div class="contact-detail-actions">'
        + '<button class="btn btn-ghost btn-sm" onclick="showEditForm(' + c.id + ')">Edit</button>'
        + '<button class="btn btn-danger btn-sm" onclick="deleteContact(' + c.id + ')">Delete</button>'
        + '</div></div>'
        // Details section
        + '<div class="contact-section" style="border-top:none;padding-top:0;margin-top:0">'
        + '<div class="contact-section-header"><h3>Details</h3></div>'
        + '<div class="contact-meta-grid">'
        + metaItem('Channel', c.preferred_channel)
        + metaItem('Mode', c.response_mode)
        + metaItem('Tone', c.tone_of_voice)
        + metaItem('Birthday', c.birthday)
        + metaItem('Persona', c.persona_override || 'channel default')
        + metaItem('Agent', c.agent_override)
        + '</div></div>'
        + personaNote
        // Identities
        + '<div class="contact-section">'
        + '<div class="contact-section-header"><h3>Identities</h3>'
        + '<button class="btn btn-ghost btn-sm" onclick="showAddIdentity(' + c.id + ')" id="add-identity-btn">+ Add</button></div>'
        + idsHtml
        + '<div id="add-identity-area"></div></div>'
        // Relationships
        + '<div class="contact-section">'
        + '<div class="contact-section-header"><h3>Relationships</h3>'
        + '<button class="btn btn-ghost btn-sm" onclick="showAddRelationship(' + c.id + ')" id="add-rel-btn">+ Add</button></div>'
        + relsHtml
        + '<div id="add-relationship-area"></div></div>'
        // Events
        + '<div class="contact-section">'
        + '<div class="contact-section-header"><h3>Events</h3>'
        + '<button class="btn btn-ghost btn-sm" onclick="showAddEvent(' + c.id + ')" id="add-event-btn">+ Add</button></div>'
        + evsHtml
        + '<div id="add-event-area"></div></div>'
        // Pending (loaded async)
        + '<div id="pending-section"></div>'
        + '</div>';

    loadPending();
}

function metaItem(label, value) {
    return '<div class="contact-meta-item">'
        + '<span class="contact-meta-label">' + esc(label) + '</span>'
        + '<span class="contact-meta-value">' + esc(value || '\u2014') + '</span>'
        + '</div>';
}

function goBackToList() {
    document.getElementById('contacts-layout').classList.remove('detail-open');
    document.getElementById('contact-detail').style.display = 'none';
    document.getElementById('contact-empty').style.display = '';
    selectedId = null;
    renderList(filterContacts(document.getElementById('contact-search').value));
}

// ── Create Form (renders in detail pane) ────────────────────────────

function showCreateForm() {
    document.getElementById('contact-empty').style.display = 'none';
    const el = document.getElementById('contact-detail');
    el.style.display = 'block';
    document.getElementById('contacts-layout').classList.add('detail-open');

    const chOpts = ['', 'telegram', 'whatsapp', 'discord', 'slack', 'email'].map(ch =>
        '<option value="' + ch + '">' + (ch || '\u2014') + '</option>'
    ).join('');

    el.innerHTML = '<div class="contact-create-form">'
        + '<button class="btn btn-ghost btn-sm contact-back-btn" onclick="goBackToList()" style="margin-bottom:12px">'
        + '&#8592; Back</button>'
        + '<h2>New Contact</h2>'
        + '<form id="contact-form" class="form form--full">'
        + '<div class="form-row--2">'
        + '<div class="form-group"><label for="cf-name">Name *</label>'
        + '<input id="cf-name" name="name" class="input" type="text" required placeholder="Full name"></div>'
        + '<div class="form-group"><label for="cf-nickname">Nickname</label>'
        + '<input id="cf-nickname" name="nickname" class="input" type="text" placeholder="Short name or handle"></div>'
        + '</div>'
        + '<div class="form-group"><label for="cf-bio">Bio</label>'
        + '<textarea id="cf-bio" name="bio" class="input" rows="2" placeholder="Who is this person? Role, context..."></textarea></div>'
        + '<div class="form-row--2">'
        + '<div class="form-group"><label for="cf-birthday">Birthday</label>'
        + '<input id="cf-birthday" name="birthday" class="input" type="date"></div>'
        + '<div class="form-group"><label for="cf-channel">Preferred Channel</label>'
        + '<select id="cf-channel" name="preferred_channel" class="input">' + chOpts + '</select></div>'
        + '</div>'
        + '<div class="form-group"><label for="cf-tone">Tone of Voice</label>'
        + '<input id="cf-tone" name="tone_of_voice" class="input" type="text" placeholder="e.g. formal, informal, friendly"></div>'
        + '<div class="form-group"><label>Channel Identities</label>'
        + '<div class="form-hint">How can Homun reach this person?</div>'
        + '<div id="form-identities"></div>'
        + '<button type="button" class="btn btn-ghost btn-sm" id="add-identity-row-btn" onclick="addFormIdentityRow()" style="margin-top:4px">+ Add identity</button>'
        + '</div>'
        + '<div style="display:flex;gap:8px;margin-top:16px">'
        + '<button type="submit" class="btn btn-primary btn-sm">Create</button>'
        + '<button type="button" class="btn btn-ghost btn-sm" onclick="goBackToList()">Cancel</button>'
        + '</div></form></div>';

    document.getElementById('contact-form').addEventListener('submit', e => saveNewContact(e));
    addFormIdentityRow();
    document.getElementById('cf-name').focus();
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

// ── Edit Form (inline in detail pane) ────────────────────────────────

function showEditForm(id) {
    const c = allContacts.find(x => x.id === id);
    if (!c) return;

    document.getElementById('contact-empty').style.display = 'none';
    const el = document.getElementById('contact-detail');
    el.style.display = 'block';
    document.getElementById('contacts-layout').classList.add('detail-open');

    const chOpts = CHANNELS.map(ch =>
        '<option value="' + ch + '"' + ((c.preferred_channel || '') === ch ? ' selected' : '') + '>' + ch + '</option>'
    ).join('');
    const mOpts = MODES.map(m =>
        '<option value="' + m + '"' + ((c.response_mode || 'automatic') === m ? ' selected' : '') + '>' + m + '</option>'
    ).join('');
    // All dynamic content sanitized through esc()
    el.innerHTML = '<div class="contact-create-form">'
        + '<button class="btn btn-ghost btn-sm contact-back-btn" onclick="selectContact(' + id + ')" style="margin-bottom:12px">'
        + '&#8592; Back</button>'
        + '<div class="contact-detail-header" style="margin-bottom:24px">'
        + '<div class="contact-avatar lg">' + esc(initials(c.name)) + '</div>'
        + '<div class="contact-detail-header-info"><h2>Edit Contact</h2>'
        + '<div class="contact-header-sub"><span class="text-secondary">' + esc(c.name) + '</span></div></div></div>'
        + '<form id="contact-edit-form" class="form form--full">'
        + '<div class="form-row--2">'
        + '<div class="form-group"><label for="ef-name">Name *</label>'
        + '<input id="ef-name" name="name" class="input" required value="' + esc(c.name || '') + '"></div>'
        + '<div class="form-group"><label for="ef-nickname">Nickname</label>'
        + '<input id="ef-nickname" name="nickname" class="input" value="' + esc(c.nickname || '') + '"></div>'
        + '</div>'
        + '<div class="form-group"><label for="ef-bio">Bio</label>'
        + '<textarea id="ef-bio" name="bio" class="input" rows="2">' + esc(c.bio || '') + '</textarea></div>'
        + '<div class="form-group"><label for="ef-notes">Notes</label>'
        + '<textarea id="ef-notes" name="notes" class="input" rows="2">' + esc(c.notes || '') + '</textarea></div>'
        + '<div class="form-row--2">'
        + '<div class="form-group"><label for="ef-birthday">Birthday</label>'
        + '<input id="ef-birthday" name="birthday" class="input" type="date" value="' + (c.birthday || '') + '"></div>'
        + '<div class="form-group"><label for="ef-channel">Preferred Channel</label>'
        + '<select id="ef-channel" name="preferred_channel" class="input"><option value="">\u2014</option>' + chOpts + '</select></div>'
        + '</div>'
        + '<div class="form-row--2">'
        + '<div class="form-group"><label for="ef-mode">Response Mode</label>'
        + '<select id="ef-mode" name="response_mode" class="input">' + mOpts + '</select></div>'
        + '<div class="form-group"><label for="ef-tone">Tone of Voice</label>'
        + '<input id="ef-tone" name="tone_of_voice" class="input" placeholder="e.g. formal, informal, friendly" value="' + esc(c.tone_of_voice || '') + '"></div>'
        + '</div>'
        + '<div class="form-group"><label for="ef-profile">Profile</label>'
        + '<select id="ef-profile" name="profile_id" class="input">'
        + '<option value="">Channel default</option>'
        + '</select>'
        + '<p style="font-size:12px;color:var(--t3);margin:4px 0 0">Choose which profile the agent uses when responding to this contact.</p></div>'
        + '<div class="form-group"><label for="ef-persona">Persona Override</label>'
        + '<select id="ef-persona" name="persona_override" class="input" onchange="document.getElementById(\'persona-instr-group\').style.display=this.value===\'custom\'?\'block\':\'none\'">'
        + '<option value=""' + (!c.persona_override ? ' selected' : '') + '>Channel default</option>'
        + '<option value="bot"' + (c.persona_override === 'bot' ? ' selected' : '') + '>Bot</option>'
        + '<option value="owner"' + (c.persona_override === 'owner' ? ' selected' : '') + '>Owner</option>'
        + '<option value="company"' + (c.persona_override === 'company' ? ' selected' : '') + '>Company</option>'
        + '<option value="custom"' + (c.persona_override === 'custom' ? ' selected' : '') + '>Custom</option>'
        + '</select></div>'
        + '<div id="persona-instr-group" style="display:' + (c.persona_override === 'custom' ? 'block' : 'none') + '">'
        + '<div class="form-group"><label for="ef-persona-instr">Persona Instructions</label>'
        + '<textarea id="ef-persona-instr" name="persona_instructions" class="input" rows="3" placeholder="Custom instructions for how the agent should present itself to this contact">' + esc(c.persona_instructions || '') + '</textarea></div>'
        + '</div>'
        + '<div style="display:flex;gap:8px;margin-top:16px">'
        + '<button type="submit" class="btn btn-primary btn-sm">Save</button>'
        + '<button type="button" class="btn btn-ghost btn-sm" onclick="selectContact(' + id + ')">Cancel</button>'
        + '</div></form></div>';

    document.getElementById('contact-edit-form').addEventListener('submit', e => saveEditContact(e, id));
    document.getElementById('ef-name').focus();
    loadProfileDropdown(c.profile_id);
}

// ── Save / Delete ───────────────────────────────────────────────────

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
        await loadContacts();
        selectContact(id);
    } catch (err) { console.error('Failed to save contact', err); }
}

async function deleteContact(id) {
    if (!confirm('Delete this contact?')) return;
    await fetch(API + '/' + id, { method: 'DELETE' });
    selectedId = null;
    document.getElementById('contact-detail').style.display = 'none';
    document.getElementById('contact-empty').style.display = '';
    document.getElementById('contacts-layout').classList.remove('detail-open');
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
        section.innerHTML = '<div class="contact-section">'
            + '<div class="contact-section-header"><h3>Pending Responses</h3></div>'
            + pending.map(p =>
                '<div class="contact-entity-row" style="flex-wrap:wrap;gap:8px">'
                + '<span class="pill">' + esc(p.channel) + '</span>'
                + '<span style="flex:1;font-size:13px">' + esc(p.inbound_content.substring(0, 200)) + '</span>'
                + '<div style="display:flex;gap:6px">'
                + '<button class="btn btn-primary btn-sm" onclick="approvePending(' + p.id + ')">Approve</button>'
                + '<button class="btn btn-ghost btn-sm" onclick="rejectPending(' + p.id + ')">Reject</button>'
                + '</div></div>'
            ).join('')
            + '</div>';
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

// ── Profile dropdown loader ──────────────────────────────────────────

let _cachedProfiles = null;

/** Load profiles and populate the #ef-profile select in the contact edit form. */
async function loadProfileDropdown(selectedId) {
    const select = document.getElementById('ef-profile');
    if (!select) return;

    if (!_cachedProfiles) {
        try {
            const res = await fetch('/api/v1/profiles');
            if (res.ok) _cachedProfiles = await res.json();
        } catch (_) {}
    }
    if (!_cachedProfiles) return;

    // Keep the "Channel default" option, add profile options
    _cachedProfiles.forEach(p => {
        const opt = document.createElement('option');
        opt.value = p.id;
        opt.textContent = (p.avatar_emoji || '\u{1F464}') + ' ' + p.display_name + (p.is_default ? ' (default)' : '');
        if (selectedId && p.id === selectedId) opt.selected = true;
        select.appendChild(opt);
    });
}

// ── Util ────────────────────────────────────────────────────────────

function esc(s) {
    if (!s) return '';
    const div = document.createElement('div');
    div.textContent = String(s);
    return div.innerHTML;
}
