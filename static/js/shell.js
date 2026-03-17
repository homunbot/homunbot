// Shell page JavaScript

// Current state
let currentPermissions = null;
let currentOs = 'macos';

// Initialize
document.addEventListener('DOMContentLoaded', async () => {
    await loadPermissions();
    setupShellTabs();
});

// ─── API ───

async function loadPermissions() {
    try {
        const resp = await fetch('/api/v1/permissions');
        if (!resp.ok) throw new Error('Failed to load permissions');
        currentPermissions = await resp.json();
        renderShellProfile();
    } catch (e) {
        console.error('Error loading permissions:', e);
        showToast('Failed to load permissions', 'error');
    }
}

async function savePermissions() {
    try {
        const resp = await fetch('/api/v1/permissions', {
            method: 'PUT',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(currentPermissions)
        });
        if (!resp.ok) throw new Error('Failed to save permissions');
        showToast('Shell settings saved', 'success');
    } catch (e) {
        console.error('Error saving permissions:', e);
        showToast('Failed to save shell settings', 'error');
    }
}

// ─── Shell Tabs ───

function setupShellTabs() {
    document.querySelectorAll('.shell-tab').forEach(tab => {
        tab.addEventListener('click', () => {
            document.querySelectorAll('.shell-tab').forEach(t => t.classList.remove('active'));
            tab.classList.add('active');
            currentOs = tab.dataset.os;
            renderShellProfile();
        });
    });

    ['shell-select', 'allow-risky', 'blocked-commands', 'allowed-commands'].forEach(id => {
        document.getElementById(id)?.addEventListener('change', saveShellProfile);
    });
}

function renderShellProfile() {
    if (!currentPermissions) return;

    const profile = currentPermissions.shell[currentOs];
    if (!profile) return;

    document.getElementById('shell-select').value = profile.shell || '';
    document.getElementById('allow-risky').checked = profile.allow_risky || false;
    document.getElementById('blocked-commands').value = (profile.blocked_commands || []).join('\n');
    document.getElementById('allowed-commands').value = (profile.allowed_commands || []).join('\n');
}

async function saveShellProfile() {
    if (!currentPermissions) return;

    currentPermissions.shell[currentOs] = {
        shell: document.getElementById('shell-select').value || null,
        allow_risky: document.getElementById('allow-risky').checked,
        blocked_commands: document.getElementById('blocked-commands').value
            .split('\n')
            .map(s => s.trim())
            .filter(s => s),
        allowed_commands: document.getElementById('allowed-commands').value
            .split('\n')
            .map(s => s.trim())
            .filter(s => s)
    };

    await savePermissions();
}

