// Homun — Vault page interactivity

// ─── State ───
const vaultList = document.getElementById('vault-list');
const vaultCount = document.getElementById('vault-count');
const vaultForm = document.getElementById('vault-form');
const keyInput = document.getElementById('vault-key');
const valueInput = document.getElementById('vault-value');

// Reveal modal elements
const revealModal = document.getElementById('reveal-modal');
const revealKeyLabel = document.getElementById('reveal-key-label');
const revealValue = document.getElementById('reveal-value');
const revealTimer = document.getElementById('reveal-timer');
const btnCopy = document.getElementById('btn-copy-secret');
const btnCloseReveal = document.getElementById('btn-close-reveal');
let revealCountdown = null;

// 2FA state
let twoFaEnabled = false;
let pendingRevealKey = null;
let currentSessionId = null;

// ─── 2FA Functions ───

async function load2FaStatus() {
    console.log('[vault.js] load2FaStatus called');
    try {
        const resp = await fetch('/api/v1/vault/2fa/status');
        console.log('[vault.js] 2fa/status response:', resp.status);
        const data = await resp.json();
        console.log('[vault.js] 2fa/status data:', data);

        twoFaEnabled = data.enabled;
        const badge = document.getElementById('twofa-status-badge');
        const disabledView = document.getElementById('twofa-disabled-view');
        const enabledView = document.getElementById('twofa-enabled-view');

        if (!badge || !disabledView || !enabledView) {
            console.error('[vault.js] Missing DOM elements:', { badge, disabledView, enabledView });
            return;
        }

        if (data.enabled) {
            badge.textContent = 'Enabled';
            badge.className = 'badge badge-success';
            disabledView.style.display = 'none';
            enabledView.style.display = 'block';
            document.getElementById('twofa-timeout').textContent = formatDuration(data.session_timeout_secs);
            document.getElementById('twofa-recovery-count').textContent = data.recovery_codes_remaining;
        } else {
            badge.textContent = 'Disabled';
            badge.className = 'badge badge-neutral';
            disabledView.style.display = 'block';
            enabledView.style.display = 'none';
        }
    } catch (e) {
        console.error('[vault.js] load2FaStatus error:', e);
        const badge = document.getElementById('twofa-status-badge');
        if (badge) badge.textContent = 'Error';
    }
}

function formatDuration(secs) {
    if (secs < 60) return `${secs} seconds`;
    if (secs < 3600) return `${Math.round(secs / 60)} minutes`;
    return `${Math.round(secs / 3600)} hours`;
}

async function setup2Fa() {
    try {
        const resp = await fetch('/api/v1/vault/2fa/setup', { method: 'POST' });
        if (!resp.ok) {
            showToast('Failed to start 2FA setup', 'error');
            return;
        }
        const data = await resp.json();

        document.getElementById('twofa-qr-image').src = data.qr_image;
        document.getElementById('twofa-secret').textContent = data.secret;
        document.getElementById('twofa-setup-code').value = '';
        openModal('twofa-setup-modal');
    } catch (e) {
        showToast('Failed to start 2FA setup', 'error');
    }
}

async function confirm2FaSetup() {
    const code = document.getElementById('twofa-setup-code').value.trim();
    if (code.length !== 6) {
        showToast('Enter a 6-digit code', 'error');
        return;
    }

    try {
        const resp = await fetch('/api/v1/vault/2fa/confirm', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ code }),
        });
        const data = await resp.json();

        if (data.ok) {
            closeModal('twofa-setup-modal');
            load2FaStatus();

            // Show recovery codes
            if (data.recovery_codes && data.recovery_codes.length > 0) {
                const codesDiv = document.getElementById('recovery-codes-grid');
                const codesList = document.getElementById('recovery-codes-list');
                codesDiv.textContent = ''; // Clear existing
                data.recovery_codes.forEach(c => {
                    const codeEl = document.createElement('code');
                    codeEl.style.cssText = 'padding:0.5rem;background:var(--surface);border-radius:4px';
                    codeEl.textContent = c;
                    codesDiv.appendChild(codeEl);
                });
                codesList.style.display = 'block';
                document.getElementById('recovery-auth-section').style.display = 'none';
                document.getElementById('btn-show-recovery').style.display = 'inline-flex';
                document.getElementById('btn-show-recovery').textContent = 'Done';
                openModal('recovery-modal');
            }

            showToast('2FA enabled successfully');
        } else {
            showToast(data.message || 'Invalid code', 'error');
        }
    } catch (e) {
        showToast('Failed to confirm 2FA setup', 'error');
    }
}

async function verify2Fa(code) {
    try {
        const resp = await fetch('/api/v1/vault/2fa/verify', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ code }),
        });
        const data = await resp.json();

        if (data.ok) {
            currentSessionId = data.session_id;
            return true;
        } else {
            showToast(data.message || 'Invalid code', 'error');
            return false;
        }
    } catch (e) {
        showToast('Failed to verify code', 'error');
        return false;
    }
}

async function disable2Fa() {
    if (!confirm('Disable two-factor authentication? This will make your vault less secure.')) return;

    const code = prompt('Enter your authenticator code to disable 2FA:');
    if (!code) return;

    try {
        const resp = await fetch('/api/v1/vault/2fa/disable', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ code }),
        });
        const data = await resp.json();

        if (data.ok) {
            twoFaEnabled = false;
            currentSessionId = null;
            load2FaStatus();
            showToast('2FA disabled');
        } else {
            showToast(data.message || 'Failed to disable 2FA', 'error');
        }
    } catch (e) {
        showToast('Failed to disable 2FA', 'error');
    }
}

async function viewRecoveryCodes() {
    // If we have a session, use it
    if (currentSessionId) {
        await fetchRecoveryCodes(currentSessionId);
        return;
    }

    // Otherwise, show auth input
    document.getElementById('recovery-codes-list').style.display = 'none';
    document.getElementById('recovery-auth-section').style.display = 'block';
    document.getElementById('recovery-auth-code').value = '';
    document.getElementById('btn-show-recovery').style.display = 'inline-flex';
    document.getElementById('btn-show-recovery').textContent = 'Show Codes';
    openModal('recovery-modal');
}

async function fetchRecoveryCodes(sessionId) {
    try {
        const resp = await fetch('/api/v1/vault/2fa/recovery', {
            method: 'GET',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ session_id: sessionId }),
        });

        // GET with body doesn't work well, let's use a different approach
        // We need to first verify 2FA and then get codes
    } catch (e) {
        showToast('Failed to get recovery codes', 'error');
    }
}

async function showRecoveryCodesWithAuth() {
    const code = document.getElementById('recovery-auth-code').value.trim();
    if (code.length !== 6) {
        showToast('Enter a 6-digit code', 'error');
        return;
    }

    // First verify 2FA
    const ok = await verify2Fa(code);
    if (!ok) return;

    // Now get recovery codes with session
    try {
        const resp = await fetch('/api/v1/vault/2fa/recovery', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ session_id: currentSessionId }),
        });
        const data = await resp.json();

        if (data.ok && data.codes) {
            const codesDiv = document.getElementById('recovery-codes-grid');
            codesDiv.textContent = ''; // Clear existing
            data.codes.forEach(c => {
                const codeEl = document.createElement('code');
                codeEl.style.cssText = 'padding:0.5rem;background:var(--surface);border-radius:4px';
                codeEl.textContent = c;
                codesDiv.appendChild(codeEl);
            });
            document.getElementById('recovery-codes-list').style.display = 'block';
            document.getElementById('recovery-auth-section').style.display = 'none';
            document.getElementById('btn-show-recovery').textContent = 'Done';
        } else {
            showToast(data.message || 'Failed to get recovery codes', 'error');
        }
    } catch (e) {
        showToast('Failed to get recovery codes', 'error');
    }
}

// ─── Modal helpers ───
function openModal(id) {
    document.getElementById(id)?.classList.add('open');
}

function closeModal(id) {
    document.getElementById(id)?.classList.remove('open');
}

// ─── 2FA Event Listeners ───
document.getElementById('btn-enable-2fa')?.addEventListener('click', setup2Fa);
document.getElementById('btn-cancel-twofa-setup')?.addEventListener('click', () => closeModal('twofa-setup-modal'));
document.getElementById('btn-confirm-twofa-setup')?.addEventListener('click', confirm2FaSetup);
document.getElementById('btn-disable-2fa')?.addEventListener('click', disable2Fa);
document.getElementById('btn-view-recovery')?.addEventListener('click', viewRecoveryCodes);
document.getElementById('btn-close-recovery')?.addEventListener('click', () => closeModal('recovery-modal'));
document.getElementById('btn-show-recovery')?.addEventListener('click', showRecoveryCodesWithAuth);
document.getElementById('btn-cancel-twofa-verify')?.addEventListener('click', () => {
    closeModal('twofa-code-modal');
    pendingRevealKey = null;
});
document.getElementById('btn-submit-twofa-verify')?.addEventListener('click', async () => {
    const code = document.getElementById('twofa-verify-code').value.trim();
    if (code.length !== 6) {
        showToast('Enter a 6-digit code', 'error');
        return;
    }

    const ok = await verify2Fa(code);
    if (ok) {
        closeModal('twofa-code-modal');
        if (pendingRevealKey) {
            doRevealSecret(pendingRevealKey);
            pendingRevealKey = null;
        }
    }
});

// Close modals on backdrop click
['twofa-setup-modal', 'twofa-code-modal', 'recovery-modal'].forEach(id => {
    document.querySelector(`#${id} .modal-backdrop`)?.addEventListener('click', () => closeModal(id));
    document.querySelector(`#${id} .modal-close`)?.addEventListener('click', () => closeModal(id));
});

// Enter key for 2FA code
document.getElementById('twofa-setup-code')?.addEventListener('keypress', (e) => {
    if (e.key === 'Enter') confirm2FaSetup();
});
document.getElementById('twofa-verify-code')?.addEventListener('keypress', (e) => {
    if (e.key === 'Enter') document.getElementById('btn-submit-twofa-verify')?.click();
});
document.getElementById('recovery-auth-code')?.addEventListener('keypress', (e) => {
    if (e.key === 'Enter') showRecoveryCodesWithAuth();
});

// ─── Load keys ───
async function loadKeys() {
    try {
        const resp = await fetch('/api/v1/vault');
        const data = await resp.json();
        const keys = data.keys || [];

        vaultCount.textContent = `${keys.length} secret${keys.length !== 1 ? 's' : ''}`;
        vaultList.textContent = '';

        if (keys.length === 0) {
            const empty = document.createElement('div');
            empty.className = 'empty-state';
            const icon = document.createElement('svg');
            icon.setAttribute('class', 'empty-state-icon');
            icon.setAttribute('viewBox', '0 0 24 24');
            icon.setAttribute('fill', 'none');
            icon.setAttribute('stroke', 'currentColor');
            icon.setAttribute('stroke-width', '1.5');
            const rect = document.createElementNS('http://www.w3.org/2000/svg', 'rect');
            rect.setAttribute('x', '3'); rect.setAttribute('y', '7');
            rect.setAttribute('width', '18'); rect.setAttribute('height', '14');
            rect.setAttribute('rx', '2');
            const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
            path.setAttribute('d', 'M7 7V5a5 5 0 0 1 10 0v2');
            icon.appendChild(rect);
            icon.appendChild(path);

            const p = document.createElement('p');
            p.textContent = 'No secrets stored yet.';
            empty.appendChild(icon);
            empty.appendChild(p);
            vaultList.appendChild(empty);
            return;
        }

        keys.forEach(key => {
            const row = document.createElement('div');
            row.className = 'item-row';

            const info = document.createElement('div');
            info.className = 'item-info';
            const nameEl = document.createElement('div');
            nameEl.className = 'item-name';
            nameEl.textContent = key;
            const detailEl = document.createElement('div');
            detailEl.className = 'item-detail';
            detailEl.textContent = '••••••••';
            info.appendChild(nameEl);
            info.appendChild(detailEl);

            const actions = document.createElement('div');
            actions.className = 'actions';

            const btnReveal = document.createElement('button');
            btnReveal.className = 'btn btn-secondary btn-sm';
            btnReveal.textContent = 'Reveal';
            btnReveal.addEventListener('click', () => revealSecret(key));

            const btnDelete = document.createElement('button');
            btnDelete.className = 'btn btn-danger btn-sm';
            btnDelete.textContent = 'Delete';
            btnDelete.addEventListener('click', () => deleteSecret(key));

            actions.appendChild(btnReveal);
            actions.appendChild(btnDelete);

            row.appendChild(info);
            row.appendChild(actions);
            vaultList.appendChild(row);
        });
    } catch (e) {
        showToast('Failed to load vault keys', 'error');
    }
}

// ─── Store secret ───
if (vaultForm) {
    vaultForm.addEventListener('submit', async (e) => {
        e.preventDefault();
        const key = keyInput.value.trim();
        const value = valueInput.value;

        if (!key || !value) {
            showToast('Key and value are required', 'error');
            return;
        }

        if (!/^[a-z0-9_]+$/.test(key)) {
            showToast('Key must match [a-z0-9_]+', 'error');
            return;
        }

        try {
            const resp = await fetch('/api/v1/vault', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ key, value }),
            });
            const data = await resp.json();
            if (data.ok) {
                showToast(`Secret "${key}" stored`);
                keyInput.value = '';
                valueInput.value = '';
                loadKeys();
            } else {
                showToast(data.message || 'Failed to store secret', 'error');
            }
        } catch (e) {
            showToast('Failed to store secret', 'error');
        }
    });
}

// ─── Reveal secret (with 2FA support) ───
async function revealSecret(key) {
    if (twoFaEnabled && !currentSessionId) {
        // Need to verify 2FA first
        pendingRevealKey = key;
        document.getElementById('twofa-verify-code').value = '';
        openModal('twofa-code-modal');
        document.getElementById('twofa-verify-code')?.focus();
        return;
    }

    await doRevealSecret(key);
}

async function doRevealSecret(key) {
    revealKeyLabel.textContent = key;
    revealValue.textContent = 'Decrypting…';
    revealTimer.textContent = '';
    openModal('reveal-modal');

    try {
        const body = currentSessionId
            ? JSON.stringify({ session_id: currentSessionId })
            : JSON.stringify({});

        const resp = await fetch(`/api/v1/vault/${encodeURIComponent(key)}/reveal`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body,
        });
        const data = await resp.json();

        if (data.ok && data.value != null) {
            revealValue.textContent = data.value;
            startCountdown(10);
        } else if (data.requires_2fa) {
            // Shouldn't happen since we handle 2FA above, but just in case
            closeModal('reveal-modal');
            pendingRevealKey = key;
            document.getElementById('twofa-verify-code').value = '';
            openModal('twofa-code-modal');
        } else {
            revealValue.textContent = data.message || 'Failed to decrypt';
        }
    } catch (e) {
        revealValue.textContent = 'Error: could not decrypt';
    }
}

function startCountdown(seconds) {
    let remaining = seconds;
    revealTimer.textContent = `Auto-hide in ${remaining}s`;
    clearInterval(revealCountdown);
    revealCountdown = setInterval(() => {
        remaining--;
        revealTimer.textContent = `Auto-hide in ${remaining}s`;
        if (remaining <= 0) {
            closeReveal();
        }
    }, 1000);
}

function closeReveal() {
    clearInterval(revealCountdown);
    revealModal.classList.remove('open');
    revealValue.textContent = '';
    revealTimer.textContent = '';
}

if (btnCloseReveal) {
    btnCloseReveal.addEventListener('click', closeReveal);
}

// Close on backdrop click
if (revealModal) {
    revealModal.querySelector('.modal-backdrop')?.addEventListener('click', closeReveal);
    revealModal.querySelector('.modal-close')?.addEventListener('click', closeReveal);
}

// Copy to clipboard
if (btnCopy) {
    btnCopy.addEventListener('click', () => {
        const val = revealValue.textContent;
        if (val && val !== 'Decrypting…') {
            navigator.clipboard.writeText(val).then(() => {
                showToast('Copied to clipboard');
            }).catch(() => {
                showToast('Failed to copy', 'error');
            });
        }
    });
}

// ─── Delete secret ───
async function deleteSecret(key) {
    if (!confirm(`Delete secret "${key}"? This cannot be undone.`)) return;
    try {
        const resp = await fetch(`/api/v1/vault/${encodeURIComponent(key)}`, {
            method: 'DELETE',
        });
        const data = await resp.json();
        if (data.ok) {
            showToast(`Secret "${key}" deleted`);
            loadKeys();
        } else {
            showToast('Failed to delete', 'error');
        }
    } catch (e) {
        showToast('Failed to delete', 'error');
    }
}

// ─── Init ───
console.log('[vault.js] Initializing...');
loadKeys();
load2FaStatus();
console.log('[vault.js] Init complete');
