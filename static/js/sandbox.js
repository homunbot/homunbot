// Sandbox page JavaScript

// Current state
let currentSandbox = null;
let currentSandboxStatus = null;
let currentSandboxPresets = {};
let currentSandboxImage = null;

// Initialize
document.addEventListener('DOMContentLoaded', async () => {
    await loadSandbox();
    await loadSandboxStatus();
    await loadSandboxImage();
    await loadSandboxPresets();
    setupSandboxControls();
});

// ─── API: Load ───

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

async function loadSandboxStatus() {
    try {
        const resp = await fetch('/api/v1/security/sandbox/status');
        if (!resp.ok) throw new Error('Failed to load sandbox runtime status');
        currentSandboxStatus = await resp.json();
        renderDockerStatus();
        renderRecommendation();
        updateSandboxProfileCards();
    } catch (e) {
        console.error('Error loading sandbox runtime status:', e);
        currentSandboxStatus = null;
        renderDockerStatus();
    }
}

async function loadSandboxImage() {
    try {
        const resp = await fetch('/api/v1/security/sandbox/image');
        if (!resp.ok) throw new Error('Failed to load sandbox runtime image status');
        currentSandboxImage = await resp.json();
        renderImageStatus();
    } catch (e) {
        console.error('Error loading sandbox runtime image status:', e);
        currentSandboxImage = null;
        renderImageStatus();
    }
}

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
        updatePresetLabels();
        updateSandboxProfileCards();
        renderRecommendation();
    } catch (e) {
        console.error('Error loading sandbox presets:', e);
        currentSandboxPresets = {};
    }
}

// ─── API: Save ───

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

// ─── Docker Status ───

function renderDockerStatus() {
    const iconEl = document.getElementById('sandbox-docker-status-icon');
    const textEl = document.getElementById('sandbox-docker-status-text');
    const detailEl = document.getElementById('sandbox-docker-status-detail');
    if (!iconEl || !textEl) return;

    if (!currentSandboxStatus) {
        iconEl.textContent = '⏳';
        textEl.textContent = 'Detecting available backends...';
        if (detailEl) detailEl.textContent = '';
        return;
    }

    const resolved = currentSandboxStatus.resolved_backend || 'none';
    const anyAvailable = currentSandboxStatus.any_backend_available;

    if (anyAvailable && resolved !== 'none') {
        iconEl.textContent = '✅';
        textEl.textContent = currentSandboxStatus.message || `Backend active: ${resolved}`;
        if (detailEl) {
            detailEl.textContent = currentSandboxStatus.availability_summary || '';
        }
    } else if (anyAvailable) {
        iconEl.textContent = '⚠️';
        textEl.textContent = currentSandboxStatus.message || 'Backend available but not active';
        if (detailEl) {
            detailEl.textContent = currentSandboxStatus.availability_summary || '';
        }
    } else {
        iconEl.textContent = '❌';
        textEl.textContent = 'No isolation backend available';
        if (detailEl) {
            detailEl.textContent = currentSandboxStatus.availability_summary || '';
        }
    }

    // Disable Safe/Strict cards only when no backend is available at all
    document.querySelectorAll('[data-sandbox-profile="safe"], [data-sandbox-profile="strict"]').forEach(card => {
        if (!anyAvailable) {
            card.classList.add('disabled');
            card.title = 'No isolation backend available';
        } else {
            card.classList.remove('disabled');
            card.title = '';
        }
    });
}

// ─── Recommendation ───

function renderRecommendation() {
    const section = document.getElementById('sandbox-recommendation-section');
    const textEl = document.getElementById('sandbox-recommendation-text');
    if (!section || !textEl) return;

    if (!currentSandboxStatus) {
        section.style.display = 'none';
        return;
    }

    const recommended = getRecommendedPresetName();
    const activeProfile = currentSandboxProfile();

    // Don't show if already on the recommended profile
    if (activeProfile === recommended) {
        section.style.display = 'none';
        return;
    }

    const presetLabel = currentSandboxPresets[recommended]?.label || recommended;
    const resolved = currentSandboxStatus.resolved_backend || 'none';
    const backendNote = resolved !== 'none'
        ? `Active backend: ${resolved}`
        : (currentSandboxStatus.any_backend_available ? 'Backends available' : 'No backend available');
    textEl.textContent = `Recommended: ${presetLabel} \u00b7 ${backendNote}`;
    section.style.display = '';
}

// ─── Image Status ───

function renderImageStatus() {
    const badgeEl = document.getElementById('sandbox-image-status-badge');
    const nameEl = document.getElementById('sandbox-image-name');
    const pullBtn = document.getElementById('btn-pull-sandbox-image');
    const section = document.getElementById('sandbox-image-section');
    if (!badgeEl) return;

    const image = currentSandboxImage;

    if (!image) {
        badgeEl.textContent = 'unknown';
        badgeEl.className = 'badge badge-neutral';
        if (pullBtn) pullBtn.disabled = false;
        return;
    }

    if (nameEl) nameEl.textContent = image.image || 'node:22-alpine';

    if (!image.docker_available) {
        badgeEl.textContent = 'docker unavailable';
        badgeEl.className = 'badge badge-neutral';
        if (pullBtn) pullBtn.disabled = true;
    } else if (image.present && image.acceptability === 'acceptable') {
        badgeEl.textContent = 'present';
        badgeEl.className = 'badge badge-success';
        if (pullBtn) pullBtn.disabled = false;
    } else if (image.present) {
        badgeEl.textContent = 'update available';
        badgeEl.className = 'badge badge-warning';
        if (pullBtn) pullBtn.disabled = false;
    } else {
        badgeEl.textContent = 'not found';
        badgeEl.className = 'badge badge-error';
        if (pullBtn) pullBtn.disabled = false;
    }

    // Show image section only for Docker backend
    if (section) {
        const resolved = currentSandboxStatus?.resolved_backend || '';
        const configured = currentSandboxStatus?.configured_backend || '';
        const isDocker = resolved === 'docker' || (configured === 'auto' && currentSandboxStatus?.docker_available);
        section.style.display = isDocker ? '' : 'none';
    }
}

// ─── Controls Setup ───

function setupSandboxControls() {
    // Save button
    const saveBtn = document.getElementById('btn-save-sandbox');
    if (saveBtn) {
        saveBtn.addEventListener('click', async () => {
            await saveSandbox();
        });
    }

    // Docker refresh
    const refreshBtn = document.getElementById('btn-refresh-docker-status');
    if (refreshBtn) {
        refreshBtn.addEventListener('click', async () => {
            refreshBtn.disabled = true;
            refreshBtn.textContent = 'Checking...';
            await loadSandboxStatus();
            await loadSandboxImage();
            refreshBtn.disabled = false;
            refreshBtn.textContent = 'Refresh';
            showToast('Backend status refreshed', 'success');
        });
    }

    // Pull image
    const pullBtn = document.getElementById('btn-pull-sandbox-image');
    if (pullBtn) {
        pullBtn.addEventListener('click', async () => {
            pullBtn.disabled = true;
            pullBtn.textContent = 'Pulling...';
            try {
                const resp = await fetch('/api/v1/security/sandbox/image/pull', {
                    method: 'POST'
                });
                const data = await resp.json().catch(() => null);
                if (!resp.ok) {
                    throw new Error(data?.message || 'Failed to pull runtime image');
                }
                currentSandboxImage = data?.status || null;
                renderImageStatus();
                showToast('Runtime image pulled successfully', 'success');
            } catch (e) {
                console.error('Error pulling image:', e);
                showToast(e.message || 'Failed to pull runtime image', 'error');
            } finally {
                pullBtn.disabled = false;
                pullBtn.textContent = 'Pull Image';
            }
        });
    }

    // Recommended preset button
    const recommendedBtn = document.getElementById('btn-apply-sandbox-recommended');
    if (recommendedBtn) {
        recommendedBtn.addEventListener('click', async () => {
            if (!currentSandboxStatus) {
                await loadSandboxStatus();
            }
            await applySandboxProfile(getRecommendedPresetName());
        });
    }

    // Profile cards
    document.querySelectorAll('[data-sandbox-profile]').forEach((card) => {
        card.addEventListener('click', async () => {
            if (card.classList.contains('disabled')) return;
            const profile = (card.dataset.sandboxProfile || '').toLowerCase();
            if (!profile) return;
            await applySandboxProfile(profile);
        });
    });

    // Advanced: backend select visibility
    const backendSelect = document.getElementById('sandbox-backend');
    if (backendSelect) {
        backendSelect.addEventListener('change', () => {
            updateSandboxDockerVisibility();
        });
    }

    // Advanced: enabled checkbox
    const enabledCheckbox = document.getElementById('sandbox-enabled');
    if (enabledCheckbox) {
        enabledCheckbox.addEventListener('change', () => {
            updateSandboxBadge();
            renderImageStatus();
        });
    }
}

// ─── Render Sandbox Form ───

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
    renderImageStatus();
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

    // Hide Docker-only fields for native backends
    const nativeBackend = ['macos_seatbelt', 'linux_native', 'windows_native'].includes(backend);
    const dockerOnlyIds = [
        'sandbox-docker-image',
        'sandbox-docker-memory',
        'sandbox-docker-cpus',
        'sandbox-docker-readonly',
        'sandbox-docker-mount-workspace',
    ];
    for (const id of dockerOnlyIds) {
        const el = document.getElementById(id);
        if (el?.closest('.form-group')) {
            el.closest('.form-group').style.display = nativeBackend ? 'none' : '';
        }
    }
}

// ─── Preset Logic ───

function getRecommendedPresetName() {
    const recommendedFromCatalog = Object.values(currentSandboxPresets)
        .find((preset) => preset && preset.recommended && preset.id);
    if (recommendedFromCatalog) {
        const id = String(recommendedFromCatalog.id).toLowerCase();
        if (id === 'safe' || id === 'strict') return id;
    }

    if (currentSandboxStatus && typeof currentSandboxStatus.recommended_preset === 'string') {
        const normalized = currentSandboxStatus.recommended_preset.trim().toLowerCase();
        if (normalized === 'strict' || normalized === 'safe') return normalized;
    }
    if (currentSandboxStatus && currentSandboxStatus.any_backend_available) {
        return 'strict';
    }
    return 'safe';
}

function getSandboxPresetConfig(profile) {
    if (currentSandboxPresets[profile] && currentSandboxPresets[profile].config) {
        return currentSandboxPresets[profile].config;
    }
    // Fallback defaults
    const strict = profile === 'strict';
    return {
        enabled: true,
        backend: 'auto',
        strict,
        docker_image: 'homun/runtime-core:2026.03',
        runtime_image_policy: 'versioned_tag',
        runtime_image_expected_version: '2026.03',
        docker_network: 'none',
        docker_memory_mb: 512,
        docker_cpus: 1,
        docker_read_only_rootfs: true,
        docker_mount_workspace: true
    };
}

function currentSandboxProfile() {
    const form = collectSandboxForm();
    if (!form.enabled) return 'disabled';

    const safeConfig = getSandboxPresetConfig('safe');
    const strictConfig = getSandboxPresetConfig('strict');
    if (sandboxConfigMatches(form, strictConfig)) return 'strict';
    if (sandboxConfigMatches(form, safeConfig)) return 'safe';
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

function updatePresetLabels() {
    const safeDesc = document.getElementById('sandbox-profile-safe-desc');
    const strictDesc = document.getElementById('sandbox-profile-strict-desc');
    const safeBadge = document.getElementById('sandbox-profile-safe-badge');
    const strictBadge = document.getElementById('sandbox-profile-strict-badge');

    if (safeDesc && currentSandboxPresets.safe?.description) {
        safeDesc.textContent = currentSandboxPresets.safe.description;
    }
    if (strictDesc && currentSandboxPresets.strict?.description) {
        strictDesc.textContent = currentSandboxPresets.strict.description;
    }
    if (safeBadge && currentSandboxPresets.safe?.label) {
        safeBadge.textContent = currentSandboxPresets.safe.label;
    }
    if (strictBadge && currentSandboxPresets.strict?.label) {
        strictBadge.textContent = currentSandboxPresets.strict.label;
    }
}

async function applySandboxProfile(profile) {
    let label = '';
    if (profile === 'disabled') {
        label = 'disabled';
    } else if (profile === 'strict') {
        label = currentSandboxPresets.strict?.label || 'strict';
    } else {
        label = currentSandboxPresets.safe?.label || 'safe';
        profile = 'safe';
    }

    if (!confirm(`Apply sandbox profile "${label}"? This will overwrite current settings.`)) {
        return;
    }

    if (profile === 'disabled') {
        applySandboxConfigToForm({
            enabled: false,
            backend: 'none',
            strict: false,
            docker_image: 'homun/runtime-core:2026.03',
            runtime_image_policy: 'versioned_tag',
            runtime_image_expected_version: '2026.03',
            docker_network: 'none',
            docker_memory_mb: 512,
            docker_cpus: 1,
            docker_read_only_rootfs: true,
            docker_mount_workspace: true
        });
    } else {
        const config = getSandboxPresetConfig(profile);
        applySandboxConfigToForm(config);
    }

    await saveSandbox();
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

// ─── Toast ───

function showToast(message, type = 'info') {
    const toast = document.getElementById('sandbox-toast');
    toast.textContent = message;
    toast.className = `skill-toast toast-${type}`;
    toast.style.display = 'block';
    setTimeout(() => { toast.style.display = 'none'; }, 3000);
}
