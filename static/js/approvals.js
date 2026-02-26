/**
 * Approvals page - Command approval workflow (P0-4)
 */

// ─── State ─────────────────────────────────────────────────────────────

let pendingApprovals = [];
let auditLog = [];

// ─── API Helpers ─────────────────────────────────────────────────────────

async function apiGet(endpoint) {
    const res = await fetch(`/api${endpoint}`);
    if (!res.ok) throw new Error(`API error: ${res.status}`);
    return res.json();
}

async function apiPost(endpoint, body = {}) {
    const res = await fetch(`/api${endpoint}`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body)
    });
    if (!res.ok) throw new Error(`API error: ${res.status}`);
    return res.json();
}

async function apiPut(endpoint, body = {}) {
    const res = await fetch(`/api${endpoint}`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body)
    });
    if (!res.ok) throw new Error(`API error: ${res.status}`);
    return res.json();
}

// ─── Load Data ──────────────────────────────────────────────────────────

async function loadApprovals() {
    try {
        // Load all approvals data
        const data = await apiGet('/v1/approvals');
        pendingApprovals = data.pending || [];
        
        // Update pending count badge
        document.getElementById('pending-count').textContent = pendingApprovals.length;
        
        // Render pending approvals
        renderPendingApprovals();
        
        // Load audit log
        const audit = await apiGet('/v1/approvals/audit');
        auditLog = audit.log || [];
        renderAuditLog();
        
    } catch (err) {
        console.error('Failed to load approvals:', err);
    }
}

async function loadConfig() {
    try {
        const config = await apiGet('/v1/approvals/config');
        
        // Set level
        document.getElementById('approval-level').value = config.level;
        
        // Set lists
        if (config.auto_approve) {
            document.getElementById('auto-approve-list').value = config.auto_approve.join(', ');
        }
        if (config.always_ask) {
            document.getElementById('always-ask-list').value = config.always_ask.join(', ');
        }
        
    } catch (err) {
        console.error('Failed to load config:', err);
    }
}

// ─── Render Functions ───────────────────────────────────────────────────

function renderPendingApprovals() {
    const container = document.getElementById('pending-approvals-list');
    
    if (pendingApprovals.length === 0) {
        container.innerHTML = `
            <div class="empty-state">
                <p>No pending approvals</p>
                <p class="muted">Commands requiring approval will appear here</p>
            </div>
        `;
        return;
    }
    
    container.innerHTML = pendingApprovals.map(item => `
        <div class="approval-item" data-id="${item.id}">
            <div class="approval-header">
                <span class="approval-tool">${escapeHtml(item.tool_name)}</span>
                <span class="approval-time">${formatTime(item.created_at)}</span>
            </div>
            <div class="approval-command">
                <code>${escapeHtml(item.command)}</code>
            </div>
            <div class="approval-meta">
                <span class="muted">Channel: ${escapeHtml(item.channel)}</span>
            </div>
            <div class="approval-actions">
                <button class="btn btn-primary btn-sm" onclick="approveCommand('${item.id}', false)">
                    Approve
                </button>
                <button class="btn btn-secondary btn-sm" onclick="approveCommand('${item.id}', true)">
                    Always Approve
                </button>
                <button class="btn btn-danger btn-sm" onclick="denyCommand('${item.id}')">
                    Deny
                </button>
            </div>
        </div>
    `).join('');
}

function renderAuditLog() {
    const container = document.getElementById('approval-audit-log');
    
    if (auditLog.length === 0) {
        container.innerHTML = `
            <div class="empty-state">
                <p>No activity yet</p>
            </div>
        `;
        return;
    }
    
    // Show last 20 entries, newest first
    const recent = [...auditLog].reverse().slice(0, 20);
    
    container.innerHTML = recent.map(entry => `
        <div class="audit-entry">
            <div class="audit-header">
                <span class="audit-decision decision-${entry.decision}">${entry.decision}</span>
                <span class="audit-tool">${escapeHtml(entry.tool_name)}</span>
                <span class="audit-time">${formatTime(entry.timestamp)}</span>
            </div>
            <div class="audit-summary muted">${escapeHtml(entry.arguments_summary)}</div>
        </div>
    `).join('');
}

// ─── Actions ────────────────────────────────────────────────────────────

async function approveCommand(id, always) {
    try {
        const result = await apiPost(`/v1/approvals/${id}/approve`, { always });
        if (result.success) {
            showToast(result.message, 'success');
            await loadApprovals();
        } else {
            showToast(result.message, 'error');
        }
    } catch (err) {
        showToast('Failed to approve: ' + err.message, 'error');
    }
}

async function denyCommand(id) {
    try {
        const result = await apiPost(`/v1/approvals/${id}/deny`);
        if (result.success) {
            showToast(result.message, 'success');
            await loadApprovals();
        } else {
            showToast(result.message, 'error');
        }
    } catch (err) {
        showToast('Failed to deny: ' + err.message, 'error');
    }
}

async function saveConfig() {
    const level = document.getElementById('approval-level').value;
    const autoApprove = document.getElementById('auto-approve-list').value
        .split(',')
        .map(s => s.trim())
        .filter(s => s);
    const alwaysAsk = document.getElementById('always-ask-list').value
        .split(',')
        .map(s => s.trim())
        .filter(s => s);
    
    try {
        const result = await apiPut('/v1/approvals/config', {
            level,
            auto_approve: autoApprove,
            always_ask: alwaysAsk
        });
        showToast('Configuration saved', 'success');
    } catch (err) {
        showToast('Failed to save config: ' + err.message, 'error');
    }
}

// ─── Helpers ────────────────────────────────────────────────────────────

function escapeHtml(str) {
    if (!str) return '';
    return str
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;');
}

function formatTime(isoString) {
    if (!isoString) return '';
    const date = new Date(isoString);
    return date.toLocaleString();
}

function showToast(message, type = 'info') {
    // Create toast element
    const toast = document.createElement('div');
    toast.className = `toast toast-${type}`;
    toast.textContent = message;
    toast.style.cssText = `
        position: fixed;
        bottom: 20px;
        right: 20px;
        padding: 12px 20px;
        border-radius: 8px;
        background: ${type === 'success' ? 'var(--success)' : type === 'error' ? 'var(--danger)' : 'var(--primary)'};
        color: white;
        z-index: 1000;
        animation: slideIn 0.3s ease;
    `;
    document.body.appendChild(toast);
    
    setTimeout(() => {
        toast.style.animation = 'slideOut 0.3s ease';
        setTimeout(() => toast.remove(), 300);
    }, 3000);
}

// ─── Init ───────────────────────────────────────────────────────────────

document.addEventListener('DOMContentLoaded', () => {
    loadConfig();
    loadApprovals();
    
    // Save config button
    document.getElementById('save-approval-config')?.addEventListener('click', saveConfig);
    
    // Auto-refresh pending approvals every 5 seconds
    setInterval(loadApprovals, 5000);
});

// Make functions available globally
window.approveCommand = approveCommand;
window.denyCommand = denyCommand;
