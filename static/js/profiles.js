// Security: all innerHTML usage goes through esc() which uses textContent-based sanitization.
// This follows the same security pattern as contacts.js — no raw user data in innerHTML.
'use strict';

const API = '/api/v1/profiles';

let allProfiles = [];
let selectedId = null;

/** Escape HTML entities using textContent-based sanitization (XSS-safe). */
function esc(s) {
    if (!s) return '';
    const div = document.createElement('div');
    div.textContent = String(s);
    return div.innerHTML;
}

// ── Init ────────────────────────────────────────────────────────────

document.addEventListener('DOMContentLoaded', () => {
    loadProfiles();
    document.getElementById('profile-search').addEventListener('input', e => {
        filterProfiles(e.target.value);
    });
    document.getElementById('add-profile-btn').addEventListener('click', () => {
        showCreateForm();
    });
});

// ── Data ────────────────────────────────────────────────────────────

async function loadProfiles() {
    try {
        const res = await fetch(API);
        if (!res.ok) throw new Error(res.statusText);
        allProfiles = await res.json();
        document.getElementById('profiles-count').textContent = allProfiles.length;
        renderList(allProfiles);
        if (selectedId) {
            const still = allProfiles.find(p => p.id === selectedId);
            if (still) selectProfile(still.id);
            else clearDetail();
        }
    } catch (e) {
        console.error('Failed to load profiles:', e);
    }
}

function filterProfiles(q) {
    if (!q) return renderList(allProfiles);
    const lower = q.toLowerCase();
    renderList(allProfiles.filter(p =>
        p.display_name.toLowerCase().includes(lower) ||
        p.slug.toLowerCase().includes(lower)
    ));
}

// ── Sidebar list ────────────────────────────────────────────────────

function renderList(profiles) {
    const list = document.getElementById('profiles-list');
    if (!profiles.length) {
        list.textContent = '';
        const empty = document.createElement('div');
        empty.className = 'contact-no-data';
        empty.textContent = 'No profiles found';
        list.appendChild(empty);
        return;
    }
    list.textContent = '';
    profiles.forEach(p => {
        const active = p.id === selectedId ? ' active' : '';
        const row = document.createElement('div');
        row.className = 'contact-row' + active;
        row.dataset.id = p.id;
        row.addEventListener('click', () => selectProfile(p.id));

        const avatar = document.createElement('div');
        avatar.className = 'contact-avatar';
        avatar.style.fontSize = '20px';
        avatar.textContent = p.avatar_emoji || '\u{1F464}';
        row.appendChild(avatar);

        const info = document.createElement('div');
        info.className = 'contact-row-info';

        const nameEl = document.createElement('div');
        nameEl.className = 'contact-row-name';
        nameEl.textContent = p.display_name;
        if (p.is_default) {
            const badge = document.createElement('span');
            badge.className = 'badge badge-accent';
            badge.style.cssText = 'margin-left:6px;font-size:10px';
            badge.textContent = 'Default';
            nameEl.appendChild(badge);
        }
        info.appendChild(nameEl);

        const sub = document.createElement('div');
        sub.className = 'contact-row-sub';
        sub.textContent = p.slug;
        info.appendChild(sub);

        row.appendChild(info);
        list.appendChild(row);
    });
}

// ── Detail pane ─────────────────────────────────────────────────────

async function selectProfile(id) {
    selectedId = id;
    renderList(allProfiles.filter(p => {
        const q = document.getElementById('profile-search').value.toLowerCase();
        if (!q) return true;
        return p.display_name.toLowerCase().includes(q) || p.slug.toLowerCase().includes(q);
    }));

    try {
        const [profileRes, soulRes] = await Promise.all([
            fetch(API + '/' + id),
            fetch(API + '/' + id + '/soul'),
        ]);
        if (!profileRes.ok) throw new Error('Profile not found');
        const profile = await profileRes.json();
        const soulData = soulRes.ok ? await soulRes.json() : { content: '' };
        showDetail(profile, soulData.content || '');
    } catch (e) {
        console.error('Failed to load profile:', e);
    }
}

function showDetail(profile, soulContent) {
    document.getElementById('profile-empty').style.display = 'none';
    const detail = document.getElementById('profile-detail');
    detail.style.display = '';
    detail.textContent = '';

    let pj = {};
    try { pj = JSON.parse(profile.profile_json || '{}'); } catch (_) {}

    const identity = pj.identity || {};
    const linguistics = pj.linguistics || {};
    const personality = pj.personality || {};
    const capabilities = pj.capabilities || {};
    const visibility = pj.visibility || {};

    // Inner wrapper — matches contacts detail structure
    const inner = document.createElement('div');
    inner.className = 'contact-detail-inner';

    // Header
    const header = document.createElement('div');
    header.className = 'contact-detail-header';

    const avatarEl = document.createElement('div');
    avatarEl.className = 'contact-avatar lg';
    avatarEl.style.fontSize = '32px';
    avatarEl.textContent = profile.avatar_emoji || '\u{1F464}';
    header.appendChild(avatarEl);

    const headerInfo = document.createElement('div');
    headerInfo.className = 'contact-detail-header-info';
    const h2 = document.createElement('h2');
    h2.textContent = profile.display_name;
    headerInfo.appendChild(h2);
    const subDiv = document.createElement('div');
    subDiv.className = 'contact-header-sub';
    const slugBadge = document.createElement('span');
    slugBadge.className = 'badge';
    slugBadge.textContent = profile.slug;
    subDiv.appendChild(slugBadge);
    if (profile.is_default) {
        const defBadge = document.createElement('span');
        defBadge.className = 'badge badge-accent';
        defBadge.textContent = 'Default';
        subDiv.appendChild(defBadge);
    }
    headerInfo.appendChild(subDiv);
    header.appendChild(headerInfo);

    const actions = document.createElement('div');
    actions.className = 'contact-detail-actions';
    const editBtn = document.createElement('button');
    editBtn.className = 'btn btn-sm';
    editBtn.textContent = 'Edit';
    editBtn.onclick = () => showEditForm(profile.id);
    actions.appendChild(editBtn);
    if (!profile.is_default) {
        const delBtn = document.createElement('button');
        delBtn.className = 'btn btn-sm btn-danger';
        delBtn.textContent = 'Delete';
        delBtn.onclick = () => deleteProfile(profile.id);
        actions.appendChild(delBtn);
    }
    header.appendChild(actions);
    inner.appendChild(header);

    // Sections
    appendSection(inner, 'Identity', [
        ['Name', identity.name],
        ['Bio', identity.bio],
        ['Role', identity.role],
    ]);
    appendSection(inner, 'Linguistics', [
        ['Language', linguistics.language],
        ['Formality', linguistics.formality],
        ['Style', linguistics.style],
        ['Forbidden words', (linguistics.forbidden_words || []).join(', ')],
    ]);
    appendSection(inner, 'Personality', [
        ['Traits', (personality.traits || []).join(', ')],
        ['Tone', personality.tone],
        ['Humor', personality.humor ? 'Yes' : 'No'],
    ]);
    appendSection(inner, 'Capabilities', [
        ['Domains', (capabilities.domains || []).join(', ')],
        ['Tool emphasis', (capabilities.tools_emphasis || []).join(', ')],
    ]);
    appendSection(inner, 'Visibility', [
        ['Readable from', (visibility.readable_from || []).join(', ') || 'None (isolated)'],
    ]);

    // SOUL.md editor
    const soulSection = document.createElement('div');
    soulSection.className = 'contact-section';
    const soulHeader = document.createElement('div');
    soulHeader.className = 'contact-section-header';
    const soulTitle = document.createElement('h3');
    soulTitle.textContent = 'Soul.md';
    soulHeader.appendChild(soulTitle);
    soulSection.appendChild(soulHeader);

    const textarea = document.createElement('textarea');
    textarea.id = 'soul-editor';
    textarea.className = 'input';
    textarea.rows = 8;
    textarea.style.cssText = 'width:100%;font-family:var(--font-mono);font-size:13px';
    textarea.value = soulContent;
    soulSection.appendChild(textarea);

    const saveBtn = document.createElement('button');
    saveBtn.className = 'btn btn-primary btn-sm';
    saveBtn.style.marginTop = '8px';
    saveBtn.textContent = 'Save SOUL.md';
    saveBtn.onclick = () => saveSoul(profile.id);
    soulSection.appendChild(saveBtn);

    inner.appendChild(soulSection);
    detail.appendChild(inner);
}

function appendSection(container, title, fields) {
    const visibleFields = fields.filter(([, val]) => val);
    if (!visibleFields.length) return;

    const sec = document.createElement('div');
    sec.className = 'contact-section';
    const headerDiv = document.createElement('div');
    headerDiv.className = 'contact-section-header';
    const h3 = document.createElement('h3');
    h3.textContent = title;
    headerDiv.appendChild(h3);
    sec.appendChild(headerDiv);

    visibleFields.forEach(([label, value]) => {
        const row = document.createElement('div');
        row.className = 'contact-field';
        row.style.cssText = 'display:flex;gap:8px;padding:6px 0';
        const labelEl = document.createElement('span');
        labelEl.style.cssText = 'font-weight:600;min-width:130px;font-size:13px;color:var(--t3)';
        labelEl.textContent = label;
        row.appendChild(labelEl);
        const valueEl = document.createElement('span');
        valueEl.style.cssText = 'font-size:14px;color:var(--t1)';
        valueEl.textContent = value;
        row.appendChild(valueEl);
        sec.appendChild(row);
    });

    container.appendChild(sec);
}

function clearDetail() {
    selectedId = null;
    document.getElementById('profile-empty').style.display = '';
    document.getElementById('profile-detail').style.display = 'none';
}

// ── Create form ─────────────────────────────────────────────────────

function showCreateForm() {
    selectedId = null;
    renderList(allProfiles);
    document.getElementById('profile-empty').style.display = 'none';
    const detail = document.getElementById('profile-detail');
    detail.style.display = '';
    detail.textContent = '';

    const inner = document.createElement('div');
    inner.className = 'contact-detail-inner';

    const header = document.createElement('div');
    header.className = 'contact-detail-header';
    const headerInfo = document.createElement('div');
    headerInfo.className = 'contact-detail-header-info';
    const h2 = document.createElement('h2');
    h2.textContent = 'New Profile';
    headerInfo.appendChild(h2);
    header.appendChild(headerInfo);
    inner.appendChild(header);

    const form = document.createElement('div');

    const sec = document.createElement('div');
    sec.className = 'contact-section';

    sec.appendChild(makeLabel('Slug (identifier)'));
    const slugInput = makeInput('new-slug', 'e.g. fabio-personal');
    slugInput.pattern = '[a-z0-9-]+';
    sec.appendChild(slugInput);

    sec.appendChild(makeLabel('Display Name'));
    sec.appendChild(makeInput('new-display-name', 'e.g. Fabio Personale'));

    sec.appendChild(makeLabel('Avatar Emoji'));
    const emojiInput = makeInput('new-emoji', '');
    emojiInput.value = '\u{1F464}';
    emojiInput.style.cssText = 'width:60px;text-align:center;font-size:20px';
    sec.appendChild(emojiInput);

    const btnRow = document.createElement('div');
    btnRow.style.marginTop = '16px';
    const createBtn = document.createElement('button');
    createBtn.className = 'btn btn-primary';
    createBtn.textContent = 'Create';
    createBtn.onclick = createProfile;
    btnRow.appendChild(createBtn);
    const cancelBtn = document.createElement('button');
    cancelBtn.className = 'btn';
    cancelBtn.textContent = 'Cancel';
    cancelBtn.style.marginLeft = '8px';
    cancelBtn.onclick = clearDetail;
    btnRow.appendChild(cancelBtn);
    sec.appendChild(btnRow);

    form.appendChild(sec);
    inner.appendChild(form);
    detail.appendChild(inner);
}

function makeLabel(text) {
    const label = document.createElement('label');
    label.className = 'form-label';
    label.style.marginTop = '12px';
    label.textContent = text;
    return label;
}

function makeInput(id, placeholder) {
    const input = document.createElement('input');
    input.id = id;
    input.className = 'input';
    input.placeholder = placeholder;
    return input;
}

async function createProfile() {
    const slug = document.getElementById('new-slug').value.trim();
    const display_name = document.getElementById('new-display-name').value.trim();
    const avatar_emoji = document.getElementById('new-emoji').value.trim() || '\u{1F464}';

    if (!slug || !display_name) return alert('Slug and display name are required');
    if (!/^[a-z0-9-]+$/.test(slug)) return alert('Slug must be lowercase letters, numbers, hyphens only');

    try {
        const res = await fetch(API, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ slug, display_name, avatar_emoji }),
        });
        if (!res.ok) {
            const err = await res.json().catch(() => ({}));
            throw new Error(err.error || res.statusText);
        }
        const created = await res.json();
        selectedId = created.id;
        await loadProfiles();
    } catch (e) {
        alert('Failed to create profile: ' + e.message);
    }
}

// ── Edit form ───────────────────────────────────────────────────────

function showEditForm(id) {
    const p = allProfiles.find(x => x.id === id);
    if (!p) return;

    let pj = {};
    try { pj = JSON.parse(p.profile_json || '{}'); } catch (_) {}

    const identity = pj.identity || {};
    const linguistics = pj.linguistics || {};
    const personality = pj.personality || {};
    const capabilities = pj.capabilities || {};
    const visibility = pj.visibility || {};

    const detail = document.getElementById('profile-detail');
    detail.textContent = '';

    const inner = document.createElement('div');
    inner.className = 'contact-detail-inner';

    const header = document.createElement('div');
    header.className = 'contact-detail-header';
    const headerInfo = document.createElement('div');
    headerInfo.className = 'contact-detail-header-info';
    const h2 = document.createElement('h2');
    h2.textContent = 'Edit: ' + p.display_name;
    headerInfo.appendChild(h2);
    header.appendChild(headerInfo);
    inner.appendChild(header);

    const form = document.createElement('div');

    // General
    const genSec = makeFormSection('General');
    genSec.appendChild(makeLabel('Display Name'));
    const dnInput = makeInput('edit-display-name', '');
    dnInput.value = p.display_name;
    genSec.appendChild(dnInput);
    genSec.appendChild(makeLabel('Avatar Emoji'));
    const emInput = makeInput('edit-emoji', '');
    emInput.value = p.avatar_emoji;
    emInput.style.cssText = 'width:60px;text-align:center;font-size:20px';
    genSec.appendChild(emInput);
    form.appendChild(genSec);

    // Identity
    const idSec = makeFormSection('Identity');
    idSec.appendChild(makeLabel('Name'));
    const nameIn = makeInput('edit-id-name', ''); nameIn.value = identity.name || '';
    idSec.appendChild(nameIn);
    idSec.appendChild(makeLabel('Bio'));
    const bioIn = makeInput('edit-id-bio', ''); bioIn.value = identity.bio || '';
    idSec.appendChild(bioIn);
    idSec.appendChild(makeLabel('Role'));
    const roleIn = makeInput('edit-id-role', 'personal, business, etc.');
    roleIn.value = identity.role || '';
    idSec.appendChild(roleIn);
    form.appendChild(idSec);

    // Linguistics
    const lingSec = makeFormSection('Linguistics');
    lingSec.appendChild(makeLabel('Language'));
    const langIn = makeInput('edit-lang', 'it, en, etc.');
    langIn.value = linguistics.language || '';
    lingSec.appendChild(langIn);
    lingSec.appendChild(makeLabel('Formality'));
    const formIn = makeInput('edit-formality', 'informal, formal, etc.');
    formIn.value = linguistics.formality || '';
    lingSec.appendChild(formIn);
    lingSec.appendChild(makeLabel('Style'));
    const styleIn = makeInput('edit-style', 'direct, warm, concise');
    styleIn.value = linguistics.style || '';
    lingSec.appendChild(styleIn);
    form.appendChild(lingSec);

    // Personality
    const persSec = makeFormSection('Personality');
    persSec.appendChild(makeLabel('Traits (comma-separated)'));
    const traitsIn = makeInput('edit-traits', '');
    traitsIn.value = (personality.traits || []).join(', ');
    persSec.appendChild(traitsIn);
    persSec.appendChild(makeLabel('Tone'));
    const toneIn = makeInput('edit-tone', '');
    toneIn.value = personality.tone || '';
    persSec.appendChild(toneIn);
    const humorLabel = document.createElement('label');
    humorLabel.className = 'form-label';
    humorLabel.style.marginTop = '8px';
    const humorCheck = document.createElement('input');
    humorCheck.type = 'checkbox';
    humorCheck.id = 'edit-humor';
    humorCheck.checked = !!personality.humor;
    humorLabel.appendChild(humorCheck);
    humorLabel.appendChild(document.createTextNode(' Humor'));
    persSec.appendChild(humorLabel);
    form.appendChild(persSec);

    // Capabilities
    const capSec = makeFormSection('Capabilities');
    capSec.appendChild(makeLabel('Domains (comma-separated)'));
    const domIn = makeInput('edit-domains', '');
    domIn.value = (capabilities.domains || []).join(', ');
    capSec.appendChild(domIn);
    capSec.appendChild(makeLabel('Tool emphasis (comma-separated)'));
    const toolsIn = makeInput('edit-tools', '');
    toolsIn.value = (capabilities.tools_emphasis || []).join(', ');
    capSec.appendChild(toolsIn);
    form.appendChild(capSec);

    // Visibility
    const visSec = makeFormSection('Visibility');
    visSec.appendChild(makeLabel('Readable from (comma-separated profile slugs)'));
    const readIn = makeInput('edit-readable', 'default, other-profile');
    readIn.value = (visibility.readable_from || []).join(', ');
    visSec.appendChild(readIn);
    form.appendChild(visSec);

    // Action buttons
    const btnRow = document.createElement('div');
    btnRow.style.cssText = 'margin-top:16px;padding:0 16px 16px';
    const saveBtn = document.createElement('button');
    saveBtn.className = 'btn btn-primary';
    saveBtn.textContent = 'Save';
    saveBtn.onclick = () => saveProfile(id);
    btnRow.appendChild(saveBtn);
    const cancelBtn = document.createElement('button');
    cancelBtn.className = 'btn';
    cancelBtn.textContent = 'Cancel';
    cancelBtn.style.marginLeft = '8px';
    cancelBtn.onclick = () => selectProfile(id);
    btnRow.appendChild(cancelBtn);
    form.appendChild(btnRow);

    inner.appendChild(form);
    detail.appendChild(inner);
}

function makeFormSection(title) {
    const sec = document.createElement('div');
    sec.className = 'contact-section';
    const headerDiv = document.createElement('div');
    headerDiv.className = 'contact-section-header';
    const h3 = document.createElement('h3');
    h3.textContent = title;
    headerDiv.appendChild(h3);
    sec.appendChild(headerDiv);
    return sec;
}

async function saveProfile(id) {
    const csvToArr = (s) => s ? s.split(',').map(x => x.trim()).filter(Boolean) : [];

    const profile_json = JSON.stringify({
        version: '1.0',
        identity: {
            name: document.getElementById('edit-id-name').value,
            display_name: document.getElementById('edit-display-name').value,
            bio: document.getElementById('edit-id-bio').value,
            role: document.getElementById('edit-id-role').value,
            avatar_emoji: document.getElementById('edit-emoji').value,
        },
        linguistics: {
            language: document.getElementById('edit-lang').value,
            formality: document.getElementById('edit-formality').value,
            style: document.getElementById('edit-style').value,
            forbidden_words: [],
            catchphrases: [],
        },
        personality: {
            traits: csvToArr(document.getElementById('edit-traits').value),
            tone: document.getElementById('edit-tone').value,
            humor: document.getElementById('edit-humor').checked,
        },
        capabilities: {
            tools_emphasis: csvToArr(document.getElementById('edit-tools').value),
            domains: csvToArr(document.getElementById('edit-domains').value),
        },
        visibility: {
            readable_from: csvToArr(document.getElementById('edit-readable').value),
        },
    });

    try {
        const res = await fetch(API + '/' + id, {
            method: 'PUT',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                display_name: document.getElementById('edit-display-name').value,
                avatar_emoji: document.getElementById('edit-emoji').value,
                profile_json,
            }),
        });
        if (!res.ok) throw new Error((await res.json().catch(() => ({}))).error || res.statusText);
        await loadProfiles();
        selectProfile(id);
    } catch (e) {
        alert('Failed to save: ' + e.message);
    }
}

// ── SOUL.md ─────────────────────────────────────────────────────────

async function saveSoul(id) {
    const content = document.getElementById('soul-editor').value;
    try {
        const res = await fetch(API + '/' + id + '/soul', {
            method: 'PUT',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ content }),
        });
        if (!res.ok) throw new Error(res.statusText);
        const btn = document.querySelector('#profile-detail .btn-primary');
        if (btn) {
            btn.textContent = 'Saved!';
            setTimeout(() => { btn.textContent = 'Save SOUL.md'; }, 1500);
        }
    } catch (e) {
        alert('Failed to save SOUL.md: ' + e.message);
    }
}

// ── Delete ──────────────────────────────────────────────────────────

async function deleteProfile(id) {
    if (!confirm('Delete this profile? This cannot be undone.')) return;
    try {
        const res = await fetch(API + '/' + id, { method: 'DELETE' });
        if (!res.ok) {
            const err = await res.json().catch(() => ({}));
            throw new Error(err.error || res.statusText);
        }
        clearDetail();
        await loadProfiles();
    } catch (e) {
        alert('Failed to delete: ' + e.message);
    }
}
