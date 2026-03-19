// Homun — Account page interactivity

// ─── State ───
let owner = null;
let identities = [];
let tokens = [];

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

// ─── Email Accounts Management ───
(function() {
    const modal = document.getElementById('email-account-modal');
    const form = document.getElementById('email-account-form');
    const grid = document.getElementById('email-accounts-grid');
    const addBtn = document.getElementById('btn-add-email-account');
    const testBtn = document.getElementById('btn-test-email-account');
    const deleteBtn = document.getElementById('btn-delete-email-account');
    const testResult = document.getElementById('ea-test-result');
    const modeSelect = document.getElementById('ea-mode');
    const modeHint = document.getElementById('ea-mode-hint');
    const triggerField = document.getElementById('ea-trigger-field');
    const nameInput = document.getElementById('ea-name');

    if (!modal || !form) return;

    const backdrop = modal.querySelector('.modal-backdrop');
    const closeBtn = modal.querySelector('.ea-modal-close');
    const cancelBtn = modal.querySelector('.ea-modal-cancel');

    let editingName = null;

    const MODE_HINTS = {
        assisted: 'Generates summary and draft, sends to notification channel for approval.',
        automatic: 'Agent responds directly. Escalates to assisted if lacking info or if response would include secrets.',
        on_demand: 'Only processes emails containing the trigger word or @homun.'
    };

    function updateModeVisibility() {
        const mode = modeSelect.value;
        modeHint.textContent = MODE_HINTS[mode] || '';
        triggerField.style.display = mode === 'on_demand' ? '' : 'none';
        // Auto-fetch/generate trigger word when switching to on_demand
        if (mode === 'on_demand') {
            var twInput = document.getElementById('ea-trigger-word');
            var acctName = editingName || nameInput.value.trim() || 'default';
            if (twInput && !twInput.value.trim()) {
                fetch('/api/v1/channels/email/trigger-word', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ account: acctName })
                })
                .then(function(r) { return r.json(); })
                .then(function(d) {
                    if (d.trigger_word && !twInput.value.trim()) {
                        twInput.value = d.trigger_word;
                    }
                })
                .catch(function() {});
            }
        }
    }

    function openModal(accountData) {
        if (accountData) {
            editingName = accountData.name;
            document.getElementById('email-modal-title').textContent = 'Edit: ' + accountData.name;
            nameInput.value = accountData.name;
            nameInput.readOnly = true;
            document.getElementById('ea-imap-host').value = accountData.imapHost || '';
            document.getElementById('ea-imap-port').value = accountData.imapPort || 993;
            document.getElementById('ea-smtp-host').value = accountData.smtpHost || '';
            document.getElementById('ea-smtp-port').value = accountData.smtpPort || 465;
            document.getElementById('ea-username').value = accountData.username || '';
            document.getElementById('ea-password').value = '';
            document.getElementById('ea-from').value = accountData.fromAddress || '';
            modeSelect.value = accountData.mode || 'assisted';
            document.getElementById('ea-notify-channel').value = accountData.notifyChannel || '';
            document.getElementById('ea-notify-chat-id').value = accountData.notifyChatId || '';
            document.getElementById('ea-trigger-word').value = accountData.triggerWord || '';
            document.getElementById('ea-allow-from').value = accountData.allowFrom || '';
            document.getElementById('ea-batch-threshold').value = accountData.batchThreshold || 3;
            document.getElementById('ea-batch-window').value = accountData.batchWindow || 120;
            document.getElementById('ea-send-delay').value = accountData.sendDelay || 30;
            if (deleteBtn) deleteBtn.style.display = '';
        } else {
            editingName = null;
            document.getElementById('email-modal-title').textContent = 'Add Email Account';
            form.reset();
            nameInput.readOnly = false;
            document.getElementById('ea-imap-port').value = 993;
            document.getElementById('ea-smtp-port').value = 465;
            document.getElementById('ea-batch-threshold').value = 3;
            document.getElementById('ea-batch-window').value = 120;
            document.getElementById('ea-send-delay').value = 30;
            if (deleteBtn) deleteBtn.style.display = 'none';
        }
        updateModeVisibility();
        if (eaNotifyHint) eaNotifyHint.textContent = '';
        testResult.textContent = '';
        modal.classList.add('open');
        // Auto-suggest chat ID if notify channel is set but chat ID is empty
        if (eaNotifyChannelSelect && eaNotifyChannelSelect.value &&
            eaNotifyChatIdInput && !eaNotifyChatIdInput.value.trim()) {
            eaNotifyChannelSelect.dispatchEvent(new Event('change'));
        }
    }

    function closeModal() {
        modal.classList.remove('open');
    }

    if (addBtn) addBtn.addEventListener('click', function() { openModal(null); });

    if (backdrop) backdrop.addEventListener('click', closeModal);
    if (closeBtn) closeBtn.addEventListener('click', closeModal);
    if (cancelBtn) cancelBtn.addEventListener('click', closeModal);
    document.addEventListener('keydown', function(e) {
        if (e.key === 'Escape' && modal.classList.contains('open')) closeModal();
    });

    if (modeSelect) modeSelect.addEventListener('change', updateModeVisibility);

    // --- Auto-populate Notify Chat ID from channel config ---
    var eaNotifyChannelSelect = document.getElementById('ea-notify-channel');
    var eaNotifyChatIdInput = document.getElementById('ea-notify-chat-id');
    var eaNotifyHint = document.getElementById('ea-notify-hint');
    if (eaNotifyChannelSelect) {
        eaNotifyChannelSelect.addEventListener('change', function() {
            var ch = eaNotifyChannelSelect.value;
            if (eaNotifyHint) eaNotifyHint.textContent = '';
            if (!ch) return;
            fetch('/api/v1/channels/' + ch)
                .then(function(r) { return r.ok ? r.json() : null; })
                .then(function(data) {
                    if (!data) return;
                    var id = '';
                    if (ch === 'discord') id = data.default_channel_id || (data.allow_from || [])[0] || '';
                    else if (ch === 'slack') id = data.channel_id || (data.allow_from || [])[0] || '';
                    else id = (data.allow_from || [])[0] || '';
                    if (id && eaNotifyChatIdInput && !eaNotifyChatIdInput.value.trim()) {
                        eaNotifyChatIdInput.value = id;
                    }
                    if (id && eaNotifyHint) eaNotifyHint.textContent = 'Suggested: ' + id;
                })
                .catch(function() {});
        });
    }

    if (grid) grid.addEventListener('click', function(e) {
        const card = e.target.closest('.email-account-card');
        if (!card) return;
        const d = card.dataset;
        openModal({
            name: d.emailName, imapHost: d.imapHost, imapPort: d.imapPort,
            smtpHost: d.smtpHost, smtpPort: d.smtpPort, username: d.username,
            fromAddress: d.fromAddress, allowFrom: d.allowFrom, mode: d.mode,
            notifyChannel: d.notifyChannel, notifyChatId: d.notifyChatId,
            triggerWord: d.triggerWord, batchThreshold: d.batchThreshold,
            batchWindow: d.batchWindow, sendDelay: d.sendDelay,
        });
    });

    form.addEventListener('submit', async function(e) {
        e.preventDefault();
        const name = nameInput.value.trim();
        if (!name) { alert('Account name is required'); return; }

        const allowFromRaw = document.getElementById('ea-allow-from').value.trim();
        const allowFrom = allowFromRaw ? allowFromRaw.split(',').map(s => s.trim()).filter(Boolean) : [];

        const body = {
            name,
            imap_host: document.getElementById('ea-imap-host').value.trim() || undefined,
            imap_port: parseInt(document.getElementById('ea-imap-port').value) || undefined,
            smtp_host: document.getElementById('ea-smtp-host').value.trim() || undefined,
            smtp_port: parseInt(document.getElementById('ea-smtp-port').value) || undefined,
            username: document.getElementById('ea-username').value.trim() || undefined,
            password: document.getElementById('ea-password').value || undefined,
            from_address: document.getElementById('ea-from').value.trim() || undefined,
            mode: modeSelect.value,
            notify_channel: document.getElementById('ea-notify-channel').value || undefined,
            notify_chat_id: document.getElementById('ea-notify-chat-id').value.trim() || undefined,
            trigger_word: document.getElementById('ea-trigger-word').value.trim() || undefined,
            allow_from: allowFrom.length > 0 ? allowFrom : undefined,
            batch_threshold: parseInt(document.getElementById('ea-batch-threshold').value) || undefined,
            batch_window_secs: parseInt(document.getElementById('ea-batch-window').value) || undefined,
            send_delay_secs: parseInt(document.getElementById('ea-send-delay').value) || undefined,
        };

        try {
            const res = await fetch('/api/v1/email-accounts/configure', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(body),
            });
            const data = await res.json();
            if (data.ok) { closeModal(); location.reload(); }
            else { alert(data.message || 'Failed to save'); }
        } catch (err) { alert('Error: ' + err.message); }
    });

    if (deleteBtn) deleteBtn.addEventListener('click', async function() {
        if (!editingName) return;
        if (!confirm('Delete email account "' + editingName + '"? This removes config and vault password.')) return;
        try {
            const res = await fetch('/api/v1/email-accounts/' + encodeURIComponent(editingName), { method: 'DELETE' });
            const data = await res.json();
            if (data.ok) { closeModal(); location.reload(); }
            else { alert(data.message || 'Failed to delete'); }
        } catch (err) { alert('Error: ' + err.message); }
    });

    if (testBtn) testBtn.addEventListener('click', async function() {
        const name = nameInput.value.trim();
        if (!name) { testResult.textContent = 'Enter an account name first'; return; }
        if (!editingName) { testResult.textContent = 'Save the account first, then test.'; return; }

        testResult.textContent = 'Testing IMAP connection...';
        testResult.style.color = 'var(--text-secondary)';
        try {
            const res = await fetch('/api/v1/email-accounts/test', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ name }),
            });
            const data = await res.json();
            testResult.textContent = data.message;
            testResult.style.color = data.ok ? 'var(--green)' : 'var(--red)';
        } catch (err) {
            testResult.textContent = 'Error: ' + err.message;
            testResult.style.color = 'var(--red)';
        }
    });

    console.log('[EmailAccounts] Handler initialized');
})();

// ─── Trusted Devices (REM-3) ───

async function loadDevices() {
    try {
        var res = await fetch('/api/v1/devices');
        if (!res.ok) return;
        var data = await res.json();
        var devices = data.devices || [];
        var list = document.getElementById('devices-list');
        var empty = document.getElementById('devices-empty');
        var badge = document.getElementById('devices-count');
        if (!list) return;
        badge.textContent = devices.length;
        if (devices.length === 0) {
            // safe: static HTML, no user content
            list.innerHTML = '<div class="empty-state"><p>No trusted devices</p></div>';
            return;
        }
        if (empty) empty.style.display = 'none';
        // Build device rows using DOM methods for safety
        list.innerHTML = '';
        devices.forEach(function (d) {
            var row = document.createElement('div');
            row.className = 'item-row';

            var info = document.createElement('div');
            info.className = 'item-info';
            var name = document.createElement('div');
            name.className = 'item-name';
            name.textContent = (d.name || d.user_agent || '').substring(0, 60);
            var meta = document.createElement('div');
            meta.className = 'item-meta';
            meta.textContent = d.ip_at_login + ' · ' + formatDate(d.approved_at || d.created_at);
            info.appendChild(name);
            info.appendChild(meta);

            var actions = document.createElement('div');
            actions.className = 'item-actions';
            var badge_el = document.createElement('span');
            badge_el.className = d.approved_at ? 'badge badge-success' : 'badge badge-warn';
            badge_el.textContent = d.approved_at ? 'Approved' : 'Pending';
            actions.appendChild(badge_el);
            actions.appendChild(document.createTextNode(' '));

            if (!d.approved_at) {
                var approveBtn = document.createElement('button');
                approveBtn.className = 'btn btn-primary btn-sm';
                approveBtn.textContent = 'Approve';
                approveBtn.addEventListener('click', function () { approveDevice(d.id); });
                actions.appendChild(approveBtn);
                actions.appendChild(document.createTextNode(' '));
            }
            var revokeBtn = document.createElement('button');
            revokeBtn.className = 'btn btn-ghost btn-sm';
            revokeBtn.textContent = d.approved_at ? 'Revoke' : 'Reject';
            revokeBtn.addEventListener('click', function () { revokeDevice(d.id); });
            actions.appendChild(revokeBtn);

            row.appendChild(info);
            row.appendChild(actions);
            list.appendChild(row);
        });
    } catch (e) {
        console.warn('[Devices] Load failed:', e);
    }
}

async function approveDevice(id) {
    try {
        await fetch('/api/v1/devices/' + encodeURIComponent(id) + '/approve', { method: 'POST' });
        loadDevices();
    } catch (e) {
        console.error('[Devices] Approve failed:', e);
    }
}

async function revokeDevice(id) {
    if (!confirm('Revoke this device? It will need re-approval on next login.')) return;
    try {
        await fetch('/api/v1/devices/' + encodeURIComponent(id), { method: 'DELETE' });
        loadDevices();
    } catch (e) {
        console.error('[Devices] Revoke failed:', e);
    }
}

// ─── Init ───
document.addEventListener('DOMContentLoaded', () => {
    loadOwner();
    loadIdentities();
    loadTokens();
    loadDevices();
});
