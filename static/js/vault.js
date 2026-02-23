// Homun — Vault page interactivity

// ─── Utilities ───
function showToast(message, type = 'success') {
    const el = document.getElementById('vault-toast');
    if (!el) return;
    el.textContent = message;
    el.className = `skill-toast skill-toast--${type}`;
    el.style.display = 'block';
    clearTimeout(el._timer);
    el._timer = setTimeout(() => { el.style.display = 'none'; }, 2500);
}

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

// ─── Reveal secret (modal with auto-hide) ───
async function revealSecret(key) {
    revealKeyLabel.textContent = key;
    revealValue.textContent = 'Decrypting…';
    revealTimer.textContent = '';
    openModal();

    try {
        const resp = await fetch(`/api/v1/vault/${encodeURIComponent(key)}/reveal`, {
            method: 'POST',
        });
        const data = await resp.json();
        if (data.ok && data.value != null) {
            revealValue.textContent = data.value;
            startCountdown(10);
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

function openModal() {
    revealModal.classList.add('open');
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
loadKeys();
