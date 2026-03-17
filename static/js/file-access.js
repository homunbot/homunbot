// File Access page JavaScript

// Current state
let currentPermissions = null;
let currentEditIdx = null;
let browserCurrentPath = '~';
let browserSelectedPath = null;

// Initialize
document.addEventListener('DOMContentLoaded', async () => {
    await loadPermissions();
    setupModeCards();
    setupAclList();
    setupPresets();
    setupTestPath();
    setupAclModal();
    setupPathBrowser();
    setupDefaultCheckboxListeners();
});

// ─── API ───

async function loadPermissions() {
    try {
        const resp = await fetch('/api/v1/permissions');
        if (!resp.ok) throw new Error('Failed to load permissions');
        currentPermissions = await resp.json();
        renderAclList();
        updateModeSelection();
        updateDefaultCheckboxes();
    } catch (e) {
        showErrorState('acl-list', 'Could not load file permissions.', loadPermissions);
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
        showToast('Permissions saved', 'success');
    } catch (e) {
        console.error('Error saving permissions:', e);
        showToast('Failed to save permissions', 'error');
    }
}

// ─── Mode Cards ───

function setupModeCards() {
    document.querySelectorAll('.permission-mode-card').forEach(card => {
        card.addEventListener('click', async () => {
            const mode = card.dataset.mode;
            if (mode && currentPermissions) {
                currentPermissions.mode = mode;
                await savePermissions();
                updateModeSelection();
            }
        });
    });
}

function updateModeSelection() {
    document.querySelectorAll('.permission-mode-card').forEach(card => {
        card.classList.toggle('selected', card.dataset.mode === currentPermissions?.mode);
    });
    const modeInput = document.getElementById('current-mode');
    if (modeInput) modeInput.value = currentPermissions?.mode || 'workspace';
}

// ─── Default Permissions ───

function updateDefaultCheckboxes() {
    if (!currentPermissions) return;
    const r = document.getElementById('default-read');
    const w = document.getElementById('default-write');
    const d = document.getElementById('default-delete');
    if (r) r.checked = currentPermissions.default.read;
    if (w) w.checked = currentPermissions.default.write;
    if (d) d.checked = currentPermissions.default.delete;
}

function setupDefaultCheckboxListeners() {
    ['default-read', 'default-write', 'default-delete'].forEach(id => {
        document.getElementById(id)?.addEventListener('change', async (e) => {
            if (!currentPermissions) return;
            const key = id.replace('default-', '');
            currentPermissions.default[key] = e.target.checked;
            await savePermissions();
        });
    });
}

// ─── ACL List ───

function setupAclList() {
    document.getElementById('btn-add-acl').addEventListener('click', () => {
        openAclModal(null);
    });
}

function renderAclList() {
    const container = document.getElementById('acl-list');
    container.textContent = '';

    if (!currentPermissions) {
        const loading = document.createElement('div');
        loading.className = 'acl-loading';
        loading.textContent = 'Loading...';
        container.appendChild(loading);
        return;
    }

    if (currentPermissions.acl.length === 0) {
        const empty = document.createElement('div');
        empty.className = 'acl-empty';
        empty.textContent = 'No ACL rules configured';
        container.appendChild(empty);
        return;
    }

    currentPermissions.acl.forEach((entry, idx) => {
        const isBuiltIn = idx < 7;
        const entryDiv = document.createElement('div');
        entryDiv.className = `acl-entry ${entry.entry_type === 'deny' ? 'acl-deny' : 'acl-allow'} ${isBuiltIn ? 'acl-builtin' : ''}`;
        entryDiv.dataset.idx = idx;

        const pathDiv = document.createElement('div');
        pathDiv.className = 'acl-entry-path';
        pathDiv.textContent = entry.path;
        entryDiv.appendChild(pathDiv);

        const metaDiv = document.createElement('div');
        metaDiv.className = 'acl-entry-meta';

        const typeBadge = document.createElement('span');
        typeBadge.className = 'acl-type-badge';
        typeBadge.textContent = entry.entry_type;
        metaDiv.appendChild(typeBadge);

        const permsBadge = document.createElement('span');
        permsBadge.className = 'acl-perms-badge';
        const perms = [];
        const pr = entry.permissions.read;
        const pw = entry.permissions.write;
        const pd = entry.permissions.delete;
        if (pr === true || pr?.Bool === true) perms.push('R');
        if (pw === true || pw?.Bool === true) perms.push('W');
        if (pd === true || pd?.Bool === true) perms.push('D');
        if (pr === "Confirm" || pr?.Confirm !== undefined) perms.push('R?');
        if (pw === "Confirm" || pw?.Confirm !== undefined) perms.push('W?');
        if (pd === "Confirm" || pd?.Confirm !== undefined) perms.push('D?');
        permsBadge.textContent = perms.join(' ');
        metaDiv.appendChild(permsBadge);

        entryDiv.appendChild(metaDiv);

        const actionsDiv = document.createElement('div');
        actionsDiv.className = 'acl-entry-actions';

        const editBtn = document.createElement('button');
        editBtn.className = 'btn btn-sm btn-secondary acl-edit-btn';
        editBtn.dataset.idx = idx;
        editBtn.textContent = 'Edit';
        editBtn.addEventListener('click', (e) => {
            e.stopPropagation();
            openAclModal(entry, idx);
        });
        actionsDiv.appendChild(editBtn);

        if (!isBuiltIn) {
            const deleteBtn = document.createElement('button');
            deleteBtn.className = 'btn btn-sm btn-danger acl-delete-btn';
            deleteBtn.dataset.idx = idx;
            deleteBtn.textContent = 'Remove';
            deleteBtn.addEventListener('click', async (e) => {
                e.stopPropagation();
                await deleteAclEntry(idx);
            });
            actionsDiv.appendChild(deleteBtn);
        }

        entryDiv.appendChild(actionsDiv);
        container.appendChild(entryDiv);
    });
}

async function deleteAclEntry(idx) {
    try {
        const resp = await fetch(`/api/v1/permissions/acl/${idx}`, { method: 'DELETE' });
        if (!resp.ok) throw new Error('Failed to delete ACL entry');
        currentPermissions.acl = await resp.json();
        renderAclList();
        showToast('ACL rule removed', 'success');
    } catch (e) {
        console.error('Error deleting ACL entry:', e);
        showToast('Failed to remove rule', 'error');
    }
}

// ─── ACL Modal ───

function setupAclModal() {
    const modal = document.getElementById('acl-modal');
    const closeBtn = modal.querySelector('.acl-modal-close');
    const cancelBtn = modal.querySelector('.acl-modal-cancel');
    const form = document.getElementById('acl-form');

    closeBtn.addEventListener('click', () => modal.classList.remove('open'));
    cancelBtn.addEventListener('click', () => modal.classList.remove('open'));
    modal.querySelector('.modal-backdrop').addEventListener('click', () => modal.classList.remove('open'));

    form.addEventListener('submit', async (e) => {
        e.preventDefault();
        if (currentEditIdx !== null) {
            await updateAclEntry(currentEditIdx);
        } else {
            await addAclEntry();
        }
    });
}

function openAclModal(entry, editIdx = null) {
    const modal = document.getElementById('acl-modal');
    const title = document.getElementById('acl-modal-title');
    currentEditIdx = editIdx;

    if (entry) {
        title.textContent = editIdx !== null ? 'Edit ACL Rule' : 'Add ACL Rule';
        document.getElementById('acl-path').value = entry.path;
        document.getElementById('acl-type').value = entry.entry_type;
        document.getElementById('acl-read').checked = entry.permissions.read === true || entry.permissions.read?.Bool === true;
        document.getElementById('acl-write').checked = entry.permissions.write === true || entry.permissions.write?.Bool === true;
        document.getElementById('acl-delete').checked = entry.permissions.delete === true || entry.permissions.delete?.Bool === true;
    } else {
        title.textContent = 'Add ACL Rule';
        document.getElementById('acl-form').reset();
        document.getElementById('acl-read').checked = true;
    }

    modal.classList.add('open');
}

async function addAclEntry() {
    const path = document.getElementById('acl-path').value.trim();
    if (!path) {
        showToast('Path is required', 'error');
        return;
    }

    const data = {
        path,
        entry_type: document.getElementById('acl-type').value,
        read: document.getElementById('acl-read').checked,
        write: document.getElementById('acl-write').checked,
        delete: document.getElementById('acl-delete').checked,
    };

    try {
        const resp = await fetch('/api/v1/permissions/acl', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(data)
        });
        if (!resp.ok) throw new Error('Failed to add ACL entry');
        currentPermissions.acl = await resp.json();
        renderAclList();
        document.getElementById('acl-modal').classList.remove('open');
        currentEditIdx = null;
        showToast('ACL rule added', 'success');
    } catch (e) {
        console.error('Error adding ACL entry:', e);
        showToast('Failed to add rule', 'error');
    }
}

async function updateAclEntry(idx) {
    const path = document.getElementById('acl-path').value.trim();
    if (!path) {
        showToast('Path is required', 'error');
        return;
    }

    currentPermissions.acl[idx] = {
        path,
        entry_type: document.getElementById('acl-type').value,
        permissions: {
            read: document.getElementById('acl-read').checked,
            write: document.getElementById('acl-write').checked,
            delete: document.getElementById('acl-delete').checked,
        }
    };

    try {
        await savePermissions();
        renderAclList();
        document.getElementById('acl-modal').classList.remove('open');
        currentEditIdx = null;
        showToast('ACL rule updated', 'success');
    } catch (e) {
        console.error('Error updating ACL entry:', e);
        showToast('Failed to update rule', 'error');
    }
}

// ─── Presets ───

function setupPresets() {
    document.querySelectorAll('[data-preset]').forEach(btn => {
        btn.addEventListener('click', async () => {
            const preset = btn.dataset.preset;
            if (confirm(`Apply "${preset}" preset? This will replace your current ACL rules.`)) {
                await applyPreset(preset);
            }
        });
    });
}

async function applyPreset(presetName) {
    try {
        const resp = await fetch('/api/v1/permissions/presets');
        const presets = await resp.json();
        const preset = presets.find(p => p.name === presetName);
        if (!preset) throw new Error('Preset not found');

        currentPermissions.mode = preset.config.mode;
        currentPermissions.default = preset.config.default;
        currentPermissions.acl = preset.config.acl;

        await savePermissions();
        renderAclList();
        updateModeSelection();
        updateDefaultCheckboxes();
        showToast(`Applied "${presetName}" preset`, 'success');
    } catch (e) {
        console.error('Error applying preset:', e);
        showToast('Failed to apply preset', 'error');
    }
}

// ─── Test Path ───

function setupTestPath() {
    document.getElementById('btn-test-path').addEventListener('click', testPath);
    document.getElementById('test-path').addEventListener('keypress', (e) => {
        if (e.key === 'Enter') testPath();
    });
}

async function testPath() {
    const path = document.getElementById('test-path').value.trim();
    const operation = document.getElementById('test-operation').value;

    if (!path) {
        showToast('Please enter a path', 'error');
        return;
    }

    try {
        const resp = await fetch('/api/v1/permissions/test', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ path, operation })
        });
        const result = await resp.json();

        const resultDiv = document.getElementById('test-result');
        resultDiv.style.display = 'block';
        resultDiv.textContent = '';

        if (result.allowed && !result.needs_confirmation) {
            resultDiv.className = 'test-result test-allowed';
            const strong = document.createElement('strong');
            strong.textContent = 'Allowed';
            resultDiv.appendChild(strong);
            resultDiv.appendChild(document.createTextNode(` - ${operation} on "${path}" is permitted`));
        } else if (result.allowed && result.needs_confirmation) {
            resultDiv.className = 'test-result test-confirm';
            const strong = document.createElement('strong');
            strong.textContent = 'Confirmation Required';
            resultDiv.appendChild(strong);
            resultDiv.appendChild(document.createTextNode(` - ${result.reason}`));
        } else {
            resultDiv.className = 'test-result test-denied';
            const strong = document.createElement('strong');
            strong.textContent = 'Denied';
            resultDiv.appendChild(strong);
            resultDiv.appendChild(document.createTextNode(` - ${result.reason}`));
        }
    } catch (e) {
        console.error('Error testing path:', e);
        showToast('Failed to test path', 'error');
    }
}

// ─── Path Browser ───

function setupPathBrowser() {
    document.getElementById('btn-browse-path').addEventListener('click', () => {
        openPathBrowser();
    });

    const modal = document.getElementById('path-browser-modal');
    modal.querySelector('.path-browser-close').addEventListener('click', () => modal.classList.remove('open'));
    modal.querySelector('.modal-backdrop').addEventListener('click', () => modal.classList.remove('open'));
    modal.querySelector('.path-browser-cancel').addEventListener('click', () => modal.classList.remove('open'));

    document.getElementById('btn-browser-up').addEventListener('click', async () => {
        await navigateUp();
    });

    document.getElementById('btn-browser-home').addEventListener('click', async () => {
        await navigateTo('~');
    });

    document.getElementById('btn-select-path').addEventListener('click', () => {
        selectCurrentPath();
    });

    document.getElementById('browser-recursive')?.addEventListener('change', () => {
        if (browserSelectedPath) {
            updateSelectedPathDisplay(browserSelectedPath);
        }
    });
}

async function openPathBrowser() {
    browserCurrentPath = '~';
    browserSelectedPath = null;
    document.getElementById('path-browser-modal').classList.add('open');
    await loadBrowserContents('~');
}

async function navigateTo(path) {
    browserCurrentPath = path;
    browserSelectedPath = null;
    await loadBrowserContents(path);
}

async function navigateUp() {
    const resp = await fetch(`/api/v1/permissions/browse?path=${encodeURIComponent(browserCurrentPath)}`);
    if (resp.ok) {
        const data = await resp.json();
        if (data.parent_path) {
            await navigateTo(data.parent_path);
        }
    }
}

async function loadBrowserContents(path) {
    const list = document.getElementById('browser-list');
    list.textContent = '';

    const loading = document.createElement('div');
    loading.className = 'browser-loading';
    loading.textContent = 'Loading...';
    list.appendChild(loading);

    document.getElementById('browser-current-path').textContent = path;
    document.getElementById('browser-selected-path').value = '';

    try {
        const resp = await fetch(`/api/v1/permissions/browse?path=${encodeURIComponent(path)}`);
        if (!resp.ok) throw new Error('Failed to browse');
        const data = await resp.json();

        list.textContent = '';

        if (data.entries.length === 0) {
            const empty = document.createElement('div');
            empty.className = 'browser-empty';
            empty.textContent = 'No folders found';
            list.appendChild(empty);
            return;
        }

        data.entries.forEach(entry => {
            const entryDiv = document.createElement('div');
            entryDiv.className = 'browser-entry';
            entryDiv.dataset.path = entry.path;

            const icon = document.createElement('span');
            icon.className = 'browser-entry-icon';
            const iconSvg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
            iconSvg.setAttribute('viewBox', '0 0 18 18');
            iconSvg.setAttribute('fill', 'none');
            iconSvg.setAttribute('stroke', 'currentColor');
            iconSvg.setAttribute('stroke-width', '1.5');
            const iconPath = document.createElementNS('http://www.w3.org/2000/svg', 'path');
            iconPath.setAttribute('d', 'M2 5.5V13a1.5 1.5 0 0 0 1.5 1.5h11a1.5 1.5 0 0 0 1.5-1.5V5.5a1 1 0 0 0-1-1H8.5L7 3H3a1 1 0 0 0-1 1v1.5Z');
            iconSvg.appendChild(iconPath);
            icon.appendChild(iconSvg);

            const name = document.createElement('span');
            name.className = 'browser-entry-name';
            name.textContent = entry.name;

            entryDiv.appendChild(icon);
            entryDiv.appendChild(name);

            entryDiv.addEventListener('click', () => {
                document.querySelectorAll('.browser-entry').forEach(e => e.classList.remove('selected'));
                entryDiv.classList.add('selected');
                browserSelectedPath = entry.path;
                updateSelectedPathDisplay(entry.path);
            });

            entryDiv.addEventListener('dblclick', async () => {
                await navigateTo(entry.path);
            });

            list.appendChild(entryDiv);
        });
    } catch (e) {
        console.error('Error browsing:', e);
        list.textContent = '';
        const error = document.createElement('div');
        error.className = 'browser-empty';
        error.textContent = 'Error loading folders';
        list.appendChild(error);
    }
}

function updateSelectedPathDisplay(path) {
    const recursive = document.getElementById('browser-recursive').checked;
    const displayPath = recursive ? `${path}/**` : path;
    document.getElementById('browser-selected-path').value = displayPath;
}

function selectCurrentPath() {
    const selectedInput = document.getElementById('browser-selected-path');
    const path = selectedInput.value;

    if (path) {
        document.getElementById('acl-path').value = path;
    }

    document.getElementById('path-browser-modal').classList.remove('open');
}

