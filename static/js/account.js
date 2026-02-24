// Homun — Account page interactivity

// ─── State ───
let owner = null;
let identities = [];
let tokens = [];

// ─── Utilities ───
function showToast(message, type = 'success') {
    let el = document.getElementById('account-toast');
    if (!el) {
        el = document.createElement('div');
        el.id = 'account-toast';
        el.className = 'skill-toast';
        el.style.cssText = 'position:fixed;bottom:1rem;right:1rem;padding:0.75rem 1rem;border-radius:0.5rem;z-index:1000;';
        document.body.appendChild(el);
    }
    el.textContent = message;
    el.className = `skill-toast skill-toast--${type}`;
    el.style.display = 'block';
    clearTimeout(el._timer);
    el._timer = setTimeout(() => { el.style.display = 'none'; }, 2500);
}

function formatDate(isoString) {
    if (!isoString) return '—';
    const d = new Date(isoString);
    return d.toLocaleDateString() + ' ' + d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

function escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}

function channelIcon(channel) {
    const icons = {
        telegram: '<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" style="width:20px;height:20px"><path d="M15.5 2.5L1.5 8l5 2m9-7.5L6.5 10m9-7.5l-3 13-5.5-5.5"/><path d="M6.5 10v4.5l2.5-2.5"/></svg>',
        discord: '<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" style="width:20px;height:20px"><path d="M6.5 3C5 3 3 3.5 2 5c-1.5 3-.5 7.5 1 9.5.5.5 1.5 1.5 3 1.5s2-1 3-1 1.5 1 3 1 2.5-1 3-1.5c1.5-2 2.5-6.5 1-9.5-1-1.5-3-2-4.5-2"/><circle cx="6.5" cy="10" r="1"/><circle cx="11.5" cy="10" r="1"/></svg>',
        whatsapp: '<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" style="width:20px;height:20px"><rect x="4" y="1" width="10" height="16" rx="2"/><line x1="9" y1="14" x2="9" y2="14"/></svg>',
        web: '<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" style="width:20px;height:20px"><circle cx="9" cy="9" r="7.5"/><path d="M1.5 9h15"/><path d="M9 1.5a11.5 11.5 0 0 1 3 7.5 11.5 11.5 0 0 1-3 7.5"/><path d="M9 1.5a11.5 11.5 0 0 0-3 7.5 11.5 11.5 0 0 0 3 7.5"/></svg>',
    };
    return icons[channel] || icons.web;
}

// ─── Owner ───
async function loadOwner() {
    try {
        const resp = await fetch('/api/v1/account');
        const data = await resp.json();

        const statusBadge = document.getElementById('account-status');
        const ownerCard = document.getElementById('owner-card');
        const noOwnerWarning = document.getElementById('no-owner-warning');

        if (!data) {
            statusBadge.textContent = 'Not configured';
            statusBadge.className = 'badge badge-warning';
            ownerCard.style.display = 'none';
            noOwnerWarning.style.display = 'block';
            return;
        }

        owner = data;
        statusBadge.textContent = 'Active';
        statusBadge.className = 'badge badge-success';
        ownerCard.style.display = 'flex';
        noOwnerWarning.style.display = 'none';

        document.getElementById('owner-username').textContent = data.username;
        document.getElementById('owner-role').textContent = data.role;

    } catch (e) {
        console.error('Failed to load owner:', e);
        document.getElementById('account-status').textContent = 'Error';
    }
}

// ─── Identities ───
async function loadIdentities() {
    try {
        const resp = await fetch('/api/v1/account/identities');
        identities = await resp.json();

        const countBadge = document.getElementById('identities-count');
        const list = document.getElementById('identities-list');
        const empty = document.getElementById('identities-empty');

        countBadge.textContent = identities.length;

        if (identities.length === 0) {
            list.innerHTML = '';
            list.appendChild(empty);
            empty.style.display = 'block';
            return;
        }

        empty.style.display = 'none';
        list.innerHTML = identities.map(id => `
            <div class="identity-item item-row">
                <div class="item-icon">${channelIcon(escapeHtml(id.channel))}</div>
                <div class="item-info">
                    <div class="item-name">${escapeHtml(id.channel)}</div>
                    <div class="item-meta">${escapeHtml(id.platform_id)}${id.display_name ? ` (${escapeHtml(id.display_name)})` : ''}</div>
                </div>
                <button class="btn btn-danger btn-sm btn-unlink" data-channel="${escapeHtml(id.channel)}" data-platform-id="${escapeHtml(id.platform_id)}">Unlink</button>
            </div>
        `).join('');

        // Bind unlink buttons
        list.querySelectorAll('.btn-unlink').forEach(btn => {
            btn.addEventListener('click', () => unlinkIdentity(btn.dataset.channel, btn.dataset.platformId));
        });

    } catch (e) {
        console.error('Failed to load identities:', e);
    }
}

async function linkIdentity(channel, platformId, displayName) {
    try {
        const resp = await fetch('/api/v1/account/identities', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ channel, platform_id: platformId, display_name: displayName || null })
        });

        if (!resp.ok) {
            const err = await resp.json();
            throw new Error(err.error || 'Failed to link identity');
        }

        showToast('Identity linked successfully');
        loadIdentities();
    } catch (e) {
        showToast(e.message, 'error');
    }
}

async function unlinkIdentity(channel, platformId) {
    if (!confirm(`Unlink ${channel} identity?`)) return;

    try {
        const resp = await fetch(`/api/v1/account/identities/${encodeURIComponent(channel)}/${encodeURIComponent(platformId)}`, {
            method: 'DELETE'
        });

        if (!resp.ok) throw new Error('Failed to unlink');

        showToast('Identity unlinked');
        loadIdentities();
    } catch (e) {
        showToast(e.message, 'error');
    }
}

// ─── Tokens ───
async function loadTokens() {
    try {
        const resp = await fetch('/api/v1/account/tokens');
        tokens = await resp.json();

        const countBadge = document.getElementById('tokens-count');
        const list = document.getElementById('tokens-list');
        const empty = document.getElementById('tokens-empty');

        countBadge.textContent = tokens.length;

        if (tokens.length === 0) {
            list.innerHTML = '';
            list.appendChild(empty);
            empty.style.display = 'block';
            return;
        }

        empty.style.display = 'none';
        list.innerHTML = tokens.map(t => `
            <div class="token-item item-row">
                <div class="item-info">
                    <div class="item-name">${escapeHtml(t.name)}</div>
                    <div class="item-meta">
                        <code>${escapeHtml(t.token)}</code>
                        ${t.enabled ? '' : '<span class="badge badge-warning">Disabled</span>'}
                        ${t.last_used ? '· Last used: ' + formatDate(t.last_used) : ''}
                    </div>
                </div>
                <div class="item-actions">
                    <button class="btn btn-secondary btn-sm btn-toggle-token" data-token="${escapeHtml(t.token)}" data-enabled="${t.enabled}">
                        ${t.enabled ? 'Disable' : 'Enable'}
                    </button>
                    <button class="btn btn-danger btn-sm btn-delete-token" data-token="${escapeHtml(t.token)}">Delete</button>
                </div>
            </div>
        `).join('');

        // Bind toggle buttons
        list.querySelectorAll('.btn-toggle-token').forEach(btn => {
            btn.addEventListener('click', () => toggleToken(btn.dataset.token));
        });

        // Bind delete buttons
        list.querySelectorAll('.btn-delete-token').forEach(btn => {
            btn.addEventListener('click', () => deleteToken(btn.dataset.token));
        });

    } catch (e) {
        console.error('Failed to load tokens:', e);
    }
}

async function createToken(name) {
    try {
        const resp = await fetch('/api/v1/account/tokens', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ name })
        });

        if (!resp.ok) {
            const err = await resp.json();
            throw new Error(err.error || 'Failed to create token');
        }

        const data = await resp.json();
        showToast(`Token created: POST /api/v1/webhook/${data.token}`);
        loadTokens();
    } catch (e) {
        showToast(e.message, 'error');
    }
}

async function toggleToken(token) {
    try {
        const resp = await fetch(`/api/v1/account/tokens/${encodeURIComponent(token)}`, {
            method: 'POST'
        });

        if (!resp.ok) throw new Error('Failed to toggle token');

        loadTokens();
    } catch (e) {
        showToast(e.message, 'error');
    }
}

async function deleteToken(token) {
    if (!confirm('Delete this token? It will no longer work for webhook requests.')) return;

    try {
        const resp = await fetch(`/api/v1/account/tokens/${encodeURIComponent(token)}`, {
            method: 'DELETE'
        });

        if (!resp.ok) throw new Error('Failed to delete token');

        showToast('Token deleted');
        loadTokens();
    } catch (e) {
        showToast(e.message, 'error');
    }
}

// ─── Form Handlers ───
document.getElementById('link-identity-form')?.addEventListener('submit', (e) => {
    e.preventDefault();
    const channel = document.getElementById('identity-channel').value;
    const platformId = document.getElementById('identity-platform-id').value.trim();
    const displayName = document.getElementById('identity-display-name').value.trim();

    if (!platformId) {
        showToast('Please enter a platform ID', 'error');
        return;
    }

    linkIdentity(channel, platformId, displayName);
    e.target.reset();
});

document.getElementById('create-token-form')?.addEventListener('submit', (e) => {
    e.preventDefault();
    const name = document.getElementById('token-name').value.trim();

    if (!name) {
        showToast('Please enter a token name', 'error');
        return;
    }

    createToken(name);
    e.target.reset();
});

// ─── Init ───
document.addEventListener('DOMContentLoaded', () => {
    loadOwner();
    loadIdentities();
    loadTokens();
});
