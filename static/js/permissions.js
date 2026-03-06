// Permissions page JavaScript

// Current state
let currentPermissions = null;
let currentOs = 'macos';
let currentSandbox = null;
let currentSandboxStatus = null;
let currentSandboxPresets = {};
let currentSandboxImage = null;
let currentSandboxEvents = [];

// Initialize
document.addEventListener('DOMContentLoaded', async () => {
    await loadPermissions();
    await loadSandbox();
    await loadSandboxStatus();
    await loadSandboxImage();
    await loadSandboxEvents();
    await loadSandboxPresets();
    setupModeCards();
    setupAclList();
    setupShellTabs();
    setupSandboxControls();
    setupPresets();
    setupTestPath();
    setupAclModal();
    setupPathBrowser();
});

// Load permissions from API
async function loadPermissions() {
    try {
        const resp = await fetch('/api/v1/permissions');
        if (!resp.ok) throw new Error('Failed to load permissions');
        currentPermissions = await resp.json();
        renderAclList();
        renderShellProfile();
        updateModeSelection();
        updateDefaultCheckboxes();
    } catch (e) {
        console.error('Error loading permissions:', e);
        showToast('Failed to load permissions', 'error');
    }
}

// Load execution sandbox config from API
async function loadSandbox() {
    try {
        const resp = await fetch('/api/v1/security/sandbox');
        if (!resp.ok) throw new Error('Failed to load sandbox settings');
        currentSandbox = await resp.json();
        renderSandbox();
    } catch (e) {
        console.error('Error loading sandbox settings:', e);
        showToast('Failed to load sandbox settings', 'error');
    }
}

// Load execution sandbox runtime status from API
async function loadSandboxStatus() {
    try {
        const resp = await fetch('/api/v1/security/sandbox/status');
        if (!resp.ok) throw new Error('Failed to load sandbox runtime status');
        currentSandboxStatus = await resp.json();
        renderSandboxStatus();
    } catch (e) {
        console.error('Error loading sandbox runtime status:', e);
        currentSandboxStatus = null;
        const statusEl = document.getElementById('sandbox-runtime-status');
        const badgeEl = document.getElementById('sandbox-runtime-backend');
        if (badgeEl) {
            badgeEl.textContent = 'unknown';
            badgeEl.classList.remove('badge-success', 'badge-warning', 'badge-error');
            badgeEl.classList.add('badge-neutral');
        }
        if (statusEl) {
            statusEl.textContent = 'Unable to check sandbox backend availability.';
        }
        updateRecommendedPresetButton(null);
    }
}

// Load runtime image status from API
async function loadSandboxImage() {
    try {
        const resp = await fetch('/api/v1/security/sandbox/image');
        if (!resp.ok) throw new Error('Failed to load sandbox runtime image status');
        currentSandboxImage = await resp.json();
        renderSandboxImage();
    } catch (e) {
        console.error('Error loading sandbox runtime image status:', e);
        currentSandboxImage = null;
        renderSandboxImage();
    }
}

// Load recent sandbox events from API
async function loadSandboxEvents() {
    try {
        const resp = await fetch('/api/v1/security/sandbox/events?limit=12');
        if (!resp.ok) throw new Error('Failed to load sandbox events');
        const events = await resp.json();
        currentSandboxEvents = Array.isArray(events) ? events : [];
        renderSandboxEvents();
    } catch (e) {
        console.error('Error loading sandbox events:', e);
        currentSandboxEvents = [];
        renderSandboxEvents('Unable to load sandbox events.');
    }
}

// Load sandbox presets from API
async function loadSandboxPresets() {
    try {
        const resp = await fetch('/api/v1/security/sandbox/presets');
        if (!resp.ok) throw new Error('Failed to load sandbox presets');
        const presets = await resp.json();
        currentSandboxPresets = {};
        if (Array.isArray(presets)) {
            presets.forEach((preset) => {
                if (!preset || !preset.id || !preset.config) return;
                currentSandboxPresets[String(preset.id).toLowerCase()] = preset;
            });
        }
        updatePresetButtonsFromCatalog();
        updateSandboxProfileCards();
        renderSandboxGuide(currentSandboxStatus);
    } catch (e) {
        console.error('Error loading sandbox presets:', e);
        currentSandboxPresets = {};
    }
}

// Setup mode selection cards
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
    document.getElementById('current-mode').value = currentPermissions?.mode || 'workspace';
}

// Setup default checkboxes
function updateDefaultCheckboxes() {
    if (!currentPermissions) return;
    document.getElementById('default-read').checked = currentPermissions.default.read;
    document.getElementById('default-write').checked = currentPermissions.default.write;
    document.getElementById('default-delete').checked = currentPermissions.default.delete;
}

['default-read', 'default-write', 'default-delete'].forEach(id => {
    document.getElementById(id)?.addEventListener('change', async (e) => {
        if (!currentPermissions) return;
        const key = id.replace('default-', '');
        currentPermissions.default[key] = e.target.checked;
        await savePermissions();
    });
});

// ACL List
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

        // Action buttons
        const actionsDiv = document.createElement('div');
        actionsDiv.className = 'acl-entry-actions';

        // Edit button (for all rules)
        const editBtn = document.createElement('button');
        editBtn.className = 'btn btn-sm btn-secondary acl-edit-btn';
        editBtn.dataset.idx = idx;
        editBtn.textContent = 'Edit';
        editBtn.addEventListener('click', (e) => {
            e.stopPropagation();
            openAclModal(entry, idx);
        });
        actionsDiv.appendChild(editBtn);

        // Delete button (only for non-built-in rules)
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

// ACL Modal
let currentEditIdx = null;

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

    // Update the entry in currentPermissions
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

// Shell Profile Tabs
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

// Sandbox controls
function setupSandboxControls() {
    const saveBtn = document.getElementById('btn-save-sandbox');
    if (saveBtn) {
        saveBtn.addEventListener('click', async () => {
            await saveSandbox();
        });
    }

    const refreshStatusBtn = document.getElementById('btn-refresh-sandbox-status');
    if (refreshStatusBtn) {
        refreshStatusBtn.addEventListener('click', async () => {
            await loadSandboxStatus();
            showToast('Sandbox runtime status refreshed', 'success');
        });
    }

    const refreshImageBtn = document.getElementById('btn-refresh-sandbox-image');
    if (refreshImageBtn) {
        refreshImageBtn.addEventListener('click', async () => {
            await loadSandboxImage();
            showToast('Sandbox runtime image status refreshed', 'success');
        });
    }

    const pullImageBtn = document.getElementById('btn-pull-sandbox-image');
    if (pullImageBtn) {
        pullImageBtn.addEventListener('click', async () => {
            pullImageBtn.disabled = true;
            pullImageBtn.textContent = 'Pulling...';
            try {
                const resp = await fetch('/api/v1/security/sandbox/image/pull', {
                    method: 'POST'
                });
                const data = await resp.json().catch(() => null);
                if (!resp.ok) {
                    throw new Error(data?.message || data || 'Failed to pull sandbox runtime image');
                }
                currentSandboxImage = data?.status || null;
                renderSandboxImage();
                await loadSandboxEvents();
                showToast('Sandbox runtime image pulled', 'success');
            } catch (e) {
                console.error('Error pulling sandbox runtime image:', e);
                showToast(e.message || 'Failed to pull sandbox runtime image', 'error');
            } finally {
                pullImageBtn.disabled = false;
                pullImageBtn.textContent = 'Pull Runtime Image';
            }
        });
    }

    const refreshEventsBtn = document.getElementById('btn-refresh-sandbox-events');
    if (refreshEventsBtn) {
        refreshEventsBtn.addEventListener('click', async () => {
            await loadSandboxEvents();
            showToast('Sandbox events refreshed', 'success');
        });
    }

    const recommendedPresetBtn = document.getElementById('btn-apply-sandbox-recommended');
    if (recommendedPresetBtn) {
        recommendedPresetBtn.addEventListener('click', async () => {
            if (!currentSandboxStatus) {
                await loadSandboxStatus();
            }
            await applySandboxProfile(getRecommendedSandboxPresetName(currentSandboxStatus));
        });
    }

    document.querySelectorAll('[data-sandbox-profile]').forEach((card) => {
        card.addEventListener('click', async () => {
            const profile = (card.dataset.sandboxProfile || '').toLowerCase();
            if (!profile) return;
            await applySandboxProfile(profile);
        });
    });

    const macosPresetBtn = document.getElementById('btn-apply-sandbox-macos');
    if (macosPresetBtn) {
        macosPresetBtn.addEventListener('click', async () => {
            if (!confirm('Apply macOS safe sandbox preset? This will overwrite current sandbox fields.')) {
                return;
            }
            applyMacosSafeSandboxPreset();
            await saveSandbox();
        });
    }

    const macosStrictPresetBtn = document.getElementById('btn-apply-sandbox-macos-strict');
    if (macosStrictPresetBtn) {
        macosStrictPresetBtn.addEventListener('click', async () => {
            if (!confirm('Apply macOS strict sandbox preset? This will overwrite current sandbox fields.')) {
                return;
            }
            applyMacosStrictSandboxPreset();
            await saveSandbox();
        });
    }

    const backendSelect = document.getElementById('sandbox-backend');
    if (backendSelect) {
        backendSelect.addEventListener('change', () => {
            updateSandboxDockerVisibility();
            markSandboxStatusUnsaved();
        });
    }

    const enabledCheckbox = document.getElementById('sandbox-enabled');
    if (enabledCheckbox) {
        enabledCheckbox.addEventListener('change', () => {
            updateSandboxBadge();
            markSandboxStatusUnsaved();
        });
    }

    const strictCheckbox = document.getElementById('sandbox-strict');
    if (strictCheckbox) {
        strictCheckbox.addEventListener('change', markSandboxStatusUnsaved);
    }

    [
        'sandbox-docker-image',
        'sandbox-docker-network',
        'sandbox-docker-memory',
        'sandbox-docker-cpus',
        'sandbox-docker-readonly',
        'sandbox-docker-mount-workspace'
    ].forEach((id) => {
        const el = document.getElementById(id);
        if (!el) return;
        el.addEventListener('change', markSandboxStatusUnsaved);
        if (el.tagName === 'INPUT' && (el.type === 'text' || el.type === 'number')) {
            el.addEventListener('input', markSandboxStatusUnsaved);
        }
    });
}

function renderSandbox() {
    if (!currentSandbox) return;

    const enabledEl = document.getElementById('sandbox-enabled');
    const backendEl = document.getElementById('sandbox-backend');
    const strictEl = document.getElementById('sandbox-strict');
    const imageEl = document.getElementById('sandbox-docker-image');
    const networkEl = document.getElementById('sandbox-docker-network');
    const memoryEl = document.getElementById('sandbox-docker-memory');
    const cpusEl = document.getElementById('sandbox-docker-cpus');
    const readonlyEl = document.getElementById('sandbox-docker-readonly');
    const mountWsEl = document.getElementById('sandbox-docker-mount-workspace');

    if (enabledEl) enabledEl.checked = !!currentSandbox.enabled;
    if (backendEl) backendEl.value = (currentSandbox.backend || 'auto').toLowerCase();
    if (strictEl) strictEl.checked = !!currentSandbox.strict;
    if (imageEl) imageEl.value = currentSandbox.docker_image || 'node:22-alpine';
    if (networkEl) networkEl.value = (currentSandbox.docker_network || 'none').toLowerCase();
    if (memoryEl) memoryEl.value = Number(currentSandbox.docker_memory_mb || 0);
    if (cpusEl) cpusEl.value = Number(currentSandbox.docker_cpus || 0);
    if (readonlyEl) readonlyEl.checked = !!currentSandbox.docker_read_only_rootfs;
    if (mountWsEl) mountWsEl.checked = !!currentSandbox.docker_mount_workspace;

    updateSandboxBadge();
    updateSandboxDockerVisibility();
    updateSandboxProfileCards();
    renderSandboxStatus();
}

function updateSandboxBadge() {
    const badge = document.getElementById('sandbox-current-badge');
    if (!badge) return;
    const enabled = document.getElementById('sandbox-enabled')?.checked;
    badge.textContent = enabled ? 'Enabled' : 'Disabled';
    badge.classList.remove('badge-success', 'badge-neutral');
    badge.classList.add(enabled ? 'badge-success' : 'badge-neutral');
}

function updateSandboxDockerVisibility() {
    const backend = (document.getElementById('sandbox-backend')?.value || 'auto').toLowerCase();
    const wrapper = document.getElementById('sandbox-docker-fields');
    if (!wrapper) return;
    wrapper.style.display = backend === 'none' ? 'none' : 'block';
}

function renderSandboxStatus() {
    const statusEl = document.getElementById('sandbox-runtime-status');
    const badgeEl = document.getElementById('sandbox-runtime-backend');
    if (!statusEl || !badgeEl) return;

    const status = currentSandboxStatus;
    if (!status) {
        badgeEl.textContent = 'checking...';
        badgeEl.classList.remove('badge-success', 'badge-warning', 'badge-error');
        badgeEl.classList.add('badge-neutral');
        statusEl.textContent = 'Checking sandbox backend availability...';
        updateRecommendedPresetButton(null);
        renderSandboxGuide(null);
        return;
    }

    badgeEl.textContent = status.enabled
        ? `resolved: ${status.resolved_backend}`
        : 'disabled';
    badgeEl.classList.remove('badge-success', 'badge-warning', 'badge-error', 'badge-neutral');
    if (!status.enabled) {
        badgeEl.classList.add('badge-neutral');
    } else if (!status.valid) {
        badgeEl.classList.add('badge-error');
    } else if (status.fallback_to_native) {
        badgeEl.classList.add('badge-warning');
    } else {
        badgeEl.classList.add('badge-success');
    }

    const dockerText = status.docker_available ? 'available' : 'unavailable';
    statusEl.textContent = `${status.message} Docker: ${dockerText}.`;
    updateRecommendedPresetButton(status);
    renderSandboxGuide(status);
}

function renderSandboxImage() {
    const badgeEl = document.getElementById('sandbox-image-status-badge');
    const textEl = document.getElementById('sandbox-image-status-text');
    const factsEl = document.getElementById('sandbox-image-status-facts');
    const pullBtn = document.getElementById('btn-pull-sandbox-image');
    if (!badgeEl || !textEl || !factsEl) return;

    const image = currentSandboxImage;
    if (!image) {
        badgeEl.textContent = 'unknown';
        badgeEl.className = 'badge badge-neutral';
        textEl.textContent = 'Runtime image status unavailable.';
        factsEl.innerHTML = '<span class="sandbox-guide-fact">No runtime image facts loaded yet</span>';
        if (pullBtn) pullBtn.disabled = false;
        return;
    }

    badgeEl.textContent = image.present ? 'local' : (image.docker_available ? 'missing' : 'docker unavailable');
    badgeEl.className = `badge ${image.present ? 'badge-success' : (image.docker_available ? 'badge-warning' : 'badge-neutral')}`;
    textEl.textContent = image.message || 'Runtime image status loaded.';

    const facts = [
        `Image: ${image.image || 'node:22-alpine'}`,
        `Present: ${image.present ? 'yes' : 'no'}`,
        `Size: ${formatSandboxBytes(image.size_bytes)}`,
        `Checked: ${formatSandboxTimestamp(image.checked_at)}`
    ];
    if (image.created_at) {
        facts.push(`Built: ${formatSandboxTimestamp(image.created_at)}`);
    }
    factsEl.innerHTML = facts
        .map((fact) => `<span class="sandbox-guide-fact">${fact}</span>`)
        .join('');

    if (pullBtn) {
        pullBtn.disabled = !image.docker_available;
        pullBtn.title = image.docker_available
            ? 'Pull the configured runtime image locally'
            : 'Docker is unavailable on this machine';
    }
}

function renderSandboxEvents(errorMessage = '') {
    const listEl = document.getElementById('sandbox-events-list');
    const badgeEl = document.getElementById('sandbox-events-count-badge');
    if (!listEl || !badgeEl) return;

    const events = Array.isArray(currentSandboxEvents) ? currentSandboxEvents : [];
    badgeEl.textContent = `${events.length} event${events.length === 1 ? '' : 's'}`;
    listEl.textContent = '';

    if (errorMessage) {
        const empty = document.createElement('div');
        empty.className = 'sandbox-events-empty';
        empty.textContent = errorMessage;
        listEl.appendChild(empty);
        return;
    }

    if (events.length === 0) {
        const empty = document.createElement('div');
        empty.className = 'sandbox-events-empty';
        empty.textContent = 'No sandbox events yet. Run a shell command, MCP server or skill script to populate this feed.';
        listEl.appendChild(empty);
        return;
    }

    events.forEach((event) => {
        const item = document.createElement('div');
        item.className = 'sandbox-event-item';

        const head = document.createElement('div');
        head.className = 'sandbox-event-head';

        const title = document.createElement('div');
        title.className = 'sandbox-event-title';
        title.textContent = `${String(event.execution_kind || 'process').toUpperCase()} · ${event.program || 'unknown'}`;
        head.appendChild(title);

        const badge = document.createElement('span');
        badge.className = `badge ${sandboxEventBadgeClass(event)}`;
        badge.textContent = event.status || 'event';
        head.appendChild(badge);

        const meta = document.createElement('div');
        meta.className = 'sandbox-event-meta';
        [
            `Resolved: ${event.resolved_backend || 'none'}`,
            `Requested: ${event.requested_backend || 'auto'}`,
            `When: ${formatSandboxTimestamp(event.timestamp)}`,
            event.fallback_to_native ? 'Fallback active' : '',
            event.args_preview && event.args_preview.length ? `Args: ${event.args_preview.join(' ')}` : ''
        ].filter(Boolean).forEach((fact) => {
            const chip = document.createElement('span');
            chip.className = 'sandbox-guide-fact';
            chip.textContent = fact;
            meta.appendChild(chip);
        });

        const reason = document.createElement('p');
        reason.className = 'sandbox-event-reason';
        reason.textContent = event.reason || 'No extra details';

        item.appendChild(head);
        item.appendChild(meta);
        item.appendChild(reason);
        listEl.appendChild(item);
    });
}

function markSandboxStatusUnsaved() {
    const statusEl = document.getElementById('sandbox-runtime-status');
    const badgeEl = document.getElementById('sandbox-runtime-backend');
    if (!statusEl || !badgeEl) return;
    badgeEl.textContent = 'unsaved changes';
    badgeEl.classList.remove('badge-success', 'badge-error', 'badge-neutral');
    badgeEl.classList.add('badge-warning');
    statusEl.textContent = 'Save sandbox settings to refresh runtime resolution.';
    updateSandboxProfileCards();
}

function updatePresetButtonsFromCatalog() {
    const safeBtn = document.getElementById('btn-apply-sandbox-macos');
    const strictBtn = document.getElementById('btn-apply-sandbox-macos-strict');
    const safePreset = currentSandboxPresets.safe;
    const strictPreset = currentSandboxPresets.strict;
    const safeDesc = document.getElementById('sandbox-profile-safe-desc');
    const strictDesc = document.getElementById('sandbox-profile-strict-desc');
    const safeBadge = document.getElementById('sandbox-profile-safe-badge');
    const strictBadge = document.getElementById('sandbox-profile-strict-badge');

    if (safeBtn && safePreset && safePreset.label) {
        safeBtn.textContent = `Apply ${safePreset.label} Preset`;
    }
    if (strictBtn && strictPreset && strictPreset.label) {
        strictBtn.textContent = `Apply ${strictPreset.label} Preset`;
    }
    if (safeDesc && safePreset && safePreset.description) {
        safeDesc.textContent = safePreset.description;
    }
    if (strictDesc && strictPreset && strictPreset.description) {
        strictDesc.textContent = strictPreset.description;
    }
    if (safeBadge && safePreset && safePreset.label) {
        safeBadge.textContent = safePreset.label;
    }
    if (strictBadge && strictPreset && strictPreset.label) {
        strictBadge.textContent = strictPreset.label;
    }
}

function getRecommendedSandboxPresetName(status) {
    const recommendedFromCatalog = Object.values(currentSandboxPresets)
        .find((preset) => preset && preset.recommended && preset.id);
    if (recommendedFromCatalog && recommendedFromCatalog.id) {
        const id = String(recommendedFromCatalog.id).toLowerCase();
        if (id === 'safe' || id === 'strict') return id;
    }

    if (status && typeof status.recommended_preset === 'string') {
        const normalized = status.recommended_preset.trim().toLowerCase();
        if (normalized === 'strict' || normalized === 'safe') {
            return normalized;
        }
    }
    if (status && status.docker_available) {
        return 'strict';
    }
    return 'safe';
}

function updateRecommendedPresetButton(status) {
    const btn = document.getElementById('btn-apply-sandbox-recommended');
    if (!btn) return;
    const preset = getRecommendedSandboxPresetName(status);
    const recommendedPreset =
        currentSandboxPresets[preset] && currentSandboxPresets[preset].label
            ? currentSandboxPresets[preset].label
            : `${(status && status.host_os) || 'host'}: ${preset === 'strict' ? 'Strict' : 'Safe'}`;
    btn.textContent = `Apply Recommended (${recommendedPreset})`;
    btn.title = preset === 'strict'
        ? 'Docker is available: strict mode prevents unsafe fallback.'
        : 'Docker is unavailable: safe mode keeps fallback to native execution.';
}

function getDefaultSandboxPresetConfig(profile) {
    if (profile === 'strict') {
        return {
            enabled: true,
            backend: 'auto',
            strict: true,
            docker_image: 'node:22-alpine',
            docker_network: 'none',
            docker_memory_mb: 512,
            docker_cpus: 1,
            docker_read_only_rootfs: true,
            docker_mount_workspace: true
        };
    }
    return {
        enabled: true,
        backend: 'auto',
        strict: false,
        docker_image: 'node:22-alpine',
        docker_network: 'none',
        docker_memory_mb: 512,
        docker_cpus: 1,
        docker_read_only_rootfs: true,
        docker_mount_workspace: true
    };
}

function getSandboxPresetConfig(profile) {
    if (currentSandboxPresets[profile] && currentSandboxPresets[profile].config) {
        return currentSandboxPresets[profile].config;
    }
    return getDefaultSandboxPresetConfig(profile);
}

function currentSandboxProfile() {
    const form = collectSandboxForm();
    if (!form.enabled) return 'disabled';

    const safeConfig = getSandboxPresetConfig('safe');
    const strictConfig = getSandboxPresetConfig('strict');
    if (sandboxConfigMatches(form, safeConfig)) return 'safe';
    if (sandboxConfigMatches(form, strictConfig)) return 'strict';
    return 'custom';
}

function sandboxConfigMatches(a, b) {
    if (!a || !b) return false;
    return !!a.enabled === !!b.enabled
        && String(a.backend || 'auto').toLowerCase() === String(b.backend || 'auto').toLowerCase()
        && !!a.strict === !!b.strict
        && String(a.docker_image || 'node:22-alpine') === String(b.docker_image || 'node:22-alpine')
        && String(a.docker_network || 'none').toLowerCase() === String(b.docker_network || 'none').toLowerCase()
        && Number(a.docker_memory_mb || 0) === Number(b.docker_memory_mb || 0)
        && Number(a.docker_cpus || 0) === Number(b.docker_cpus || 0)
        && !!a.docker_read_only_rootfs === !!b.docker_read_only_rootfs
        && !!a.docker_mount_workspace === !!b.docker_mount_workspace;
}

function updateSandboxProfileCards() {
    const activeProfile = currentSandboxProfile();
    document.querySelectorAll('[data-sandbox-profile]').forEach((card) => {
        card.classList.toggle('active', card.dataset.sandboxProfile === activeProfile);
    });
}

function renderSandboxGuide(status) {
    const badgeEl = document.getElementById('sandbox-guide-recommendation-badge');
    const titleEl = document.getElementById('sandbox-guide-title');
    const copyEl = document.getElementById('sandbox-guide-copy');
    const factsEl = document.getElementById('sandbox-guide-facts');
    if (!badgeEl || !titleEl || !copyEl || !factsEl) return;

    const recommended = getRecommendedSandboxPresetName(status);
    const presetMeta = currentSandboxPresets[recommended] || null;
    const host = (status && status.host_os) || 'host';
    const activeProfile = currentSandboxProfile();

    badgeEl.textContent = presetMeta && presetMeta.label
        ? presetMeta.label
        : `${host} ${recommended}`;

    if (!status) {
        titleEl.textContent = 'Runtime status unavailable';
        copyEl.textContent = 'Refresh runtime status before choosing a preset.';
        factsEl.innerHTML = '<span class="sandbox-guide-fact">No runtime facts loaded yet</span>';
        return;
    }

    if (!status.enabled) {
        titleEl.textContent = 'Sandbox is currently disabled';
        copyEl.textContent = recommended === 'strict'
            ? 'This machine supports a stricter sandbox. Enable it if you want MCP, shell and skill scripts isolated by default.'
            : 'This machine can still use the safe preset, which prefers isolation but keeps native fallback if Docker is missing.';
    } else if (status.fallback_to_native) {
        titleEl.textContent = 'Sandbox is on, but currently falling back';
        copyEl.textContent = 'The current backend cannot be enforced right now. Applying the recommended preset will align behavior with what this machine can actually guarantee.';
    } else if (recommended === 'strict') {
        titleEl.textContent = 'Strict sandbox is viable on this machine';
        copyEl.textContent = 'Docker is available, so you can require isolation and block execution whenever the backend disappears.';
    } else {
        titleEl.textContent = 'Safe sandbox is the practical default here';
        copyEl.textContent = 'Docker is not available, so the safest usable profile is one that keeps fallback to native execution instead of breaking tools.';
    }

    const facts = [
        `Docker: ${status.docker_available ? 'available' : 'unavailable'}`,
        `Resolved backend: ${status.resolved_backend}`,
        `Fallback: ${status.fallback_to_native ? 'active' : 'not needed'}`,
        `Current profile: ${activeProfile}`
    ];
    factsEl.innerHTML = facts.map((fact) => `<span class="sandbox-guide-fact">${fact}</span>`).join('');
}

function applySandboxConfigToForm(config) {
    if (!config || typeof config !== 'object') return;
    const enabledEl = document.getElementById('sandbox-enabled');
    const backendEl = document.getElementById('sandbox-backend');
    const strictEl = document.getElementById('sandbox-strict');
    const imageEl = document.getElementById('sandbox-docker-image');
    const networkEl = document.getElementById('sandbox-docker-network');
    const memoryEl = document.getElementById('sandbox-docker-memory');
    const cpusEl = document.getElementById('sandbox-docker-cpus');
    const readonlyEl = document.getElementById('sandbox-docker-readonly');
    const mountWsEl = document.getElementById('sandbox-docker-mount-workspace');

    if (enabledEl) enabledEl.checked = !!config.enabled;
    if (backendEl) backendEl.value = (config.backend || 'auto').toLowerCase();
    if (strictEl) strictEl.checked = !!config.strict;
    if (imageEl) imageEl.value = config.docker_image || 'node:22-alpine';
    if (networkEl) networkEl.value = (config.docker_network || 'none').toLowerCase();
    if (memoryEl) memoryEl.value = String(Number(config.docker_memory_mb || 0));
    if (cpusEl) cpusEl.value = String(Number(config.docker_cpus || 0));
    if (readonlyEl) readonlyEl.checked = !!config.docker_read_only_rootfs;
    if (mountWsEl) mountWsEl.checked = !!config.docker_mount_workspace;

    updateSandboxBadge();
    updateSandboxDockerVisibility();
    updateSandboxProfileCards();
    markSandboxStatusUnsaved();
}

async function applySandboxProfile(profile) {
    let label = '';
    if (profile === 'disabled') {
        label = 'disabled';
    } else if (profile === 'strict') {
        label = (currentSandboxPresets.strict && currentSandboxPresets.strict.label) || 'strict';
    } else {
        label = (currentSandboxPresets.safe && currentSandboxPresets.safe.label) || 'safe';
        profile = 'safe';
    }

    if (!confirm(`Apply sandbox profile "${label}"? This will overwrite the current sandbox fields.`)) {
        return;
    }

    if (profile === 'disabled') {
        applySandboxConfigToForm({
            enabled: false,
            backend: 'none',
            strict: false,
            docker_image: 'node:22-alpine',
            docker_network: 'none',
            docker_memory_mb: 512,
            docker_cpus: 1,
            docker_read_only_rootfs: true,
            docker_mount_workspace: true
        });
    } else if (profile === 'strict') {
        applyMacosStrictSandboxPreset();
    } else {
        applyMacosSafeSandboxPreset();
    }

    await saveSandbox();
}

function applyMacosSafeSandboxPreset() {
    if (currentSandboxPresets.safe && currentSandboxPresets.safe.config) {
        applySandboxConfigToForm(currentSandboxPresets.safe.config);
        return;
    }
    applySandboxConfigToForm({
        enabled: true,
        backend: 'auto',
        strict: false,
        docker_image: 'node:22-alpine',
        docker_network: 'none',
        docker_memory_mb: 512,
        docker_cpus: 1,
        docker_read_only_rootfs: true,
        docker_mount_workspace: true
    });
}

function applyMacosStrictSandboxPreset() {
    if (currentSandboxPresets.strict && currentSandboxPresets.strict.config) {
        applySandboxConfigToForm(currentSandboxPresets.strict.config);
        return;
    }
    applySandboxConfigToForm({
        enabled: true,
        backend: 'auto',
        strict: true,
        docker_image: 'node:22-alpine',
        docker_network: 'none',
        docker_memory_mb: 512,
        docker_cpus: 1,
        docker_read_only_rootfs: true,
        docker_mount_workspace: true
    });
}

function collectSandboxForm() {
    return {
        enabled: !!document.getElementById('sandbox-enabled')?.checked,
        backend: (document.getElementById('sandbox-backend')?.value || 'auto').toLowerCase(),
        strict: !!document.getElementById('sandbox-strict')?.checked,
        docker_image: (document.getElementById('sandbox-docker-image')?.value || 'node:22-alpine').trim(),
        docker_network: (document.getElementById('sandbox-docker-network')?.value || 'none').toLowerCase(),
        docker_memory_mb: Math.max(0, parseInt(document.getElementById('sandbox-docker-memory')?.value || '0', 10) || 0),
        docker_cpus: Math.max(0, parseFloat(document.getElementById('sandbox-docker-cpus')?.value || '0') || 0),
        docker_read_only_rootfs: !!document.getElementById('sandbox-docker-readonly')?.checked,
        docker_mount_workspace: !!document.getElementById('sandbox-docker-mount-workspace')?.checked
    };
}

async function saveSandbox() {
    try {
        const payload = collectSandboxForm();
        const resp = await fetch('/api/v1/security/sandbox', {
            method: 'PUT',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(payload)
        });
        if (!resp.ok) {
            const msg = (await resp.text()).trim();
            throw new Error(msg || 'Failed to save sandbox settings');
        }
        currentSandbox = await resp.json();
        renderSandbox();
        await loadSandboxStatus();
        await loadSandboxImage();
        showToast('Sandbox settings saved', 'success');
    } catch (e) {
        console.error('Error saving sandbox settings:', e);
        showToast(e.message || 'Failed to save sandbox settings', 'error');
    }
}

function sandboxEventBadgeClass(event) {
    if (!event) return 'badge-neutral';
    if (event.status === 'rejected') return 'badge-error';
    if (event.fallback_to_native) return 'badge-warning';
    if (event.resolved_backend === 'docker') return 'badge-success';
    return 'badge-neutral';
}

function formatSandboxBytes(value) {
    const bytes = Number(value || 0);
    if (!Number.isFinite(bytes) || bytes <= 0) return 'unknown';
    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    let size = bytes;
    let unit = 0;
    while (size >= 1024 && unit < units.length - 1) {
        size /= 1024;
        unit += 1;
    }
    return `${size >= 10 || unit === 0 ? Math.round(size) : size.toFixed(1)} ${units[unit]}`;
}

function formatSandboxTimestamp(value) {
    if (!value) return 'unknown';
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return String(value);
    return date.toLocaleString([], {
        year: 'numeric',
        month: 'short',
        day: 'numeric',
        hour: '2-digit',
        minute: '2-digit'
    });
}

// Presets
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

// Test Path
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

// Save permissions
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

// Toast notifications
function showToast(message, type = 'info') {
    const toast = document.getElementById('permissions-toast');
    toast.textContent = message;
    toast.className = `skill-toast toast-${type}`;
    toast.style.display = 'block';

    setTimeout(() => {
        toast.style.display = 'none';
    }, 3000);
}

// ─── Path Browser ───

let browserCurrentPath = '~';
let browserSelectedPath = null;

function setupPathBrowser() {
    // Browse button in ACL modal
    document.getElementById('btn-browse-path').addEventListener('click', () => {
        openPathBrowser();
    });

    // Path browser modal
    const modal = document.getElementById('path-browser-modal');
    modal.querySelector('.path-browser-close').addEventListener('click', () => modal.classList.remove('open'));
    modal.querySelector('.modal-backdrop').addEventListener('click', () => modal.classList.remove('open'));
    modal.querySelector('.path-browser-cancel').addEventListener('click', () => modal.classList.remove('open'));

    // Navigation buttons
    document.getElementById('btn-browser-up').addEventListener('click', async () => {
        await navigateUp();
    });

    document.getElementById('btn-browser-home').addEventListener('click', async () => {
        await navigateTo('~');
    });

    // Select button
    document.getElementById('btn-select-path').addEventListener('click', () => {
        selectCurrentPath();
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

            // Folder icon
            const icon = document.createElement('span');
            icon.className = 'browser-entry-icon';
            icon.innerHTML = '<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M2 5.5V13a1.5 1.5 0 0 0 1.5 1.5h11a1.5 1.5 0 0 0 1.5-1.5V5.5a1 1 0 0 0-1-1H8.5L7 3H3a1 1 0 0 0-1 1v1.5Z"/></svg>';

            const name = document.createElement('span');
            name.className = 'browser-entry-name';
            name.textContent = entry.name;

            entryDiv.appendChild(icon);
            entryDiv.appendChild(name);

            // Double-click to navigate, single click to select
            entryDiv.addEventListener('click', () => {
                // Select this folder
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

// Update display when recursive checkbox changes
document.getElementById('browser-recursive')?.addEventListener('change', () => {
    if (browserSelectedPath) {
        updateSelectedPathDisplay(browserSelectedPath);
    }
});

function selectCurrentPath() {
    const selectedInput = document.getElementById('browser-selected-path');
    const path = selectedInput.value;

    if (path) {
        document.getElementById('acl-path').value = path;
    }

    document.getElementById('path-browser-modal').classList.remove('open');
}
