// Homun — API Keys management page
// All dynamic content inserted via innerHTML uses esc() to prevent XSS.
// This follows the same pattern used across all Homun pages (vault.js, account.js, etc.).

// ─── DOM refs ───
const keysList = document.getElementById('keys-list');
const keysEmpty = document.getElementById('keys-empty');
const keysCount = document.getElementById('keys-count');
const createSection = document.getElementById('create-form-section');
const revealSection = document.getElementById('token-reveal-section');
const revealedToken = document.getElementById('revealed-token');
const createForm = document.getElementById('create-key-form');

// ─── State ───
let keys = [];

// ─── Load keys from API ───
async function loadKeys() {
    try {
        const resp = await fetch('/api/v1/account/tokens');
        if (!resp.ok) throw new Error('Failed to load keys');
        keys = await resp.json();
        renderKeys();
    } catch (e) {
        console.error('[api-keys] loadKeys error:', e);
        keysEmpty.textContent = 'Failed to load API keys.';
        keysEmpty.style.display = '';
    }
}

// ─── Render key list ───
function renderKeys() {
    keysCount.textContent = keys.length;

    // Clear old rows
    keysList.querySelectorAll('.key-row').forEach(el => el.remove());

    if (keys.length === 0) {
        keysEmpty.style.display = '';
        return;
    }
    keysEmpty.style.display = 'none';

    keys.forEach(k => {
        const row = document.createElement('div');
        row.className = 'key-row item-row';
        row.dataset.tokenId = k.token_id;

        const scopeClass = k.scope === 'admin' ? 'badge-info'
            : k.scope === 'write' ? 'badge-warning'
            : 'badge-neutral';

        const enabledLabel = k.enabled ? 'Enabled' : 'Disabled';
        const enabledClass = k.enabled ? 'badge-success' : 'badge-danger';
        const toggleLabel = k.enabled ? 'Disable' : 'Enable';
        const lastUsed = k.last_used ? timeAgo(k.last_used) : 'Never';

        // Build row with safe DOM — esc() sanitizes all API values
        // eslint-disable-next-line no-unsanitized/property -- esc() escapes all dynamic values
        row.innerHTML = [
            '<div style="flex:1;min-width:0">',
            '  <div style="display:flex;align-items:center;gap:var(--space-2);flex-wrap:wrap">',
            '    <strong>' + esc(k.name) + '</strong>',
            '    <span class="badge ' + scopeClass + '">' + esc(k.scope) + '</span>',
            '    <span class="badge ' + enabledClass + '">' + enabledLabel + '</span>',
            '    ' + formatExpiry(k.expires_at),
            '  </div>',
            '  <div style="margin-top:var(--space-1);color:var(--muted);font-size:0.8125rem">',
            '    <code style="font-family:var(--font-mono)">' + esc(k.display_token) + '</code>',
            '    · Last used: ' + esc(lastUsed),
            '  </div>',
            '</div>',
            '<div style="display:flex;gap:var(--space-1);align-items:center;flex-shrink:0">',
            '  <button class="btn btn-secondary btn-sm btn-toggle" data-id="' + esc(k.token_id) + '">' + toggleLabel + '</button>',
            '  <button class="btn btn-danger btn-sm btn-delete" data-id="' + esc(k.token_id) + '">Delete</button>',
            '</div>',
        ].join('\n');

        keysList.appendChild(row);
    });
}

// ─── Create key ───
createForm.addEventListener('submit', async (e) => {
    e.preventDefault();
    const name = document.getElementById('key-name').value.trim();
    if (!name) return;

    const scope = document.getElementById('key-scope').value;
    const expiresIn = document.getElementById('key-expiry').value || undefined;

    try {
        const resp = await fetch('/api/v1/account/tokens', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ name, scope, expires_in: expiresIn }),
        });

        if (!resp.ok) {
            const err = await resp.json().catch(() => ({}));
            showToast(err.error || 'Failed to create key', 'error');
            return;
        }

        const data = await resp.json();

        // Show the full token once
        revealedToken.textContent = data.token;
        revealSection.style.display = '';
        createSection.style.display = 'none';
        createForm.reset();

        showToast('API key created', 'success');
        loadKeys();
    } catch (err) {
        console.error('[api-keys] create error:', err);
        showToast('Network error', 'error');
    }
});

// ─── Copy token ───
document.getElementById('copy-token-btn').addEventListener('click', () => {
    const token = revealedToken.textContent;
    navigator.clipboard.writeText(token).then(() => {
        showToast('Copied to clipboard', 'success');
    }).catch(() => {
        // Fallback: select all for manual copy
        const range = document.createRange();
        range.selectNodeContents(revealedToken);
        window.getSelection().removeAllRanges();
        window.getSelection().addRange(range);
        showToast('Select All applied — press Ctrl+C', 'info');
    });
});

// ─── Dismiss reveal ───
document.getElementById('dismiss-reveal-btn').addEventListener('click', () => {
    revealSection.style.display = 'none';
    revealedToken.textContent = '';
});

// ─── Toggle create form ───
document.getElementById('create-key-btn').addEventListener('click', () => {
    const visible = createSection.style.display !== 'none';
    createSection.style.display = visible ? 'none' : '';
    if (!visible) document.getElementById('key-name').focus();
});

document.getElementById('cancel-create-btn').addEventListener('click', () => {
    createSection.style.display = 'none';
    createForm.reset();
});

// ─── Delegate toggle/delete clicks ───
keysList.addEventListener('click', async (e) => {
    const toggleBtn = e.target.closest('.btn-toggle');
    const deleteBtn = e.target.closest('.btn-delete');

    if (toggleBtn) {
        const id = toggleBtn.dataset.id;
        try {
            const resp = await fetch('/api/v1/account/tokens/' + encodeURIComponent(id), {
                method: 'POST',
            });
            if (!resp.ok) {
                const err = await resp.json().catch(() => ({}));
                showToast(err.error || 'Failed to toggle', 'error');
                return;
            }
            showToast('Key toggled', 'success');
            loadKeys();
        } catch (err) {
            showToast('Network error', 'error');
        }
    }

    if (deleteBtn) {
        const id = deleteBtn.dataset.id;
        const row = deleteBtn.closest('.key-row');
        const name = row ? (row.querySelector('strong')?.textContent || id) : id;
        if (!confirm('Delete API key "' + name + '"?')) return;

        try {
            const resp = await fetch('/api/v1/account/tokens/' + encodeURIComponent(id), {
                method: 'DELETE',
            });
            if (!resp.ok) {
                const err = await resp.json().catch(() => ({}));
                showToast(err.error || 'Failed to delete', 'error');
                return;
            }
            showToast('Key deleted', 'success');
            loadKeys();
        } catch (err) {
            showToast('Network error', 'error');
        }
    }
});

// ─── Helpers ───

function formatExpiry(expiresAt) {
    if (!expiresAt) return '<span class="badge badge-neutral">No expiry</span>';
    var exp = new Date(expiresAt);
    var now = new Date();
    if (exp <= now) {
        return '<span class="badge badge-danger">Expired</span>';
    }
    var days = Math.ceil((exp - now) / (1000 * 60 * 60 * 24));
    if (days <= 7) {
        return '<span class="badge badge-warning">Expires in ' + days + 'd</span>';
    }
    return '<span class="badge badge-neutral">Expires in ' + days + 'd</span>';
}

function timeAgo(dateStr) {
    var d = new Date(dateStr);
    var now = new Date();
    var secs = Math.floor((now - d) / 1000);
    if (secs < 60) return 'just now';
    var mins = Math.floor(secs / 60);
    if (mins < 60) return mins + 'm ago';
    var hrs = Math.floor(mins / 60);
    if (hrs < 24) return hrs + 'h ago';
    var days = Math.floor(hrs / 24);
    return days + 'd ago';
}

/** Escape HTML to prevent XSS — all API values pass through this. */
function esc(s) {
    var el = document.createElement('span');
    el.textContent = s || '';
    return el.innerHTML;
}

// ─── Init ───
loadKeys();
