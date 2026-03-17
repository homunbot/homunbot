/**
 * Approvals page - Command approval workflow (P0-4)
 */

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
        // Load pending approvals with pagination
        const pendingData = await apiGet(`/v1/approvals?page=${currentPage}&limit=${itemsPerPage}`);
        pendingApprovals = pendingData.pending || [];
        
        // Update pending count badge in page header
        var countEl = document.getElementById('pending-count');
        if (countEl) {
            countEl.textContent = pendingApprovals.length + ' pending';
            countEl.style.display = pendingApprovals.length > 0 ? '' : 'none';
        }
        
        // Render pending approvals
        renderPendingApprovals();
        
        // Load audit log with pagination
        const auditData = await apiGet(`/v1/approvals/audit?page=${currentPage}&limit=${itemsPerPage}`);
        auditLog = auditData.log || [];
        renderAuditLog();
        
    } catch (err) {
        showErrorState('pending-approvals-list', 'Could not load approvals.', loadApprovals);
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

// ─── State ─────────────────────────────────────────────────────────────

let pendingApprovals = [];
let auditLog = [];
let currentPage = 1;
let itemsPerPage = 20;
let hasMorePending = true;
let hasMoreAudit = true;

// ─── Render Functions ───────────────────────────────────────────────────

function renderPendingApprovals() {
    const container = document.getElementById('pending-approvals-list');
    const startIndex = (currentPage - 1) * itemsPerPage;
    const endIndex = startIndex + itemsPerPage;
    const pageItems = pendingApprovals.slice(startIndex, endIndex);
    
    if (pendingApprovals.length === 0) {
        container.innerHTML = `
            <div class="empty-state">
                <p>No pending approvals</p>
                <p class="muted">Commands requiring approval will appear here</p>
            </div>
        `;
        container.className = 'scrollable-list';
        return;
    }
    
    // Clear existing content but keep the scroll position
    const scrollPosition = container.scrollTop;
    container.innerHTML = pageItems.map(item => `
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
    
    // Add scrollable list class to the container
    container.className = 'scrollable-list';
    
    // Restore scroll position
    container.scrollTop = scrollPosition;
    
    // Add pagination controls if needed
    renderPagination();
}

function renderAuditLog() {
    const container = document.getElementById('approval-audit-log');
    const startIndex = (currentPage - 1) * itemsPerPage;
    const endIndex = startIndex + itemsPerPage;
    const pageItems = [...auditLog].reverse().slice(startIndex, endIndex);
    
    if (auditLog.length === 0) {
        container.innerHTML = `
            <div class="empty-state">
                <p>No activity yet</p>
            </div>
        `;
        container.className = 'scrollable-list';
        return;
    }
    
    // Clear existing content but keep the scroll position
    const scrollPosition = container.scrollTop;
    container.innerHTML = pageItems.map(entry => `
        <div class="audit-entry">
            <div class="audit-header">
                <span class="audit-decision decision-${entry.decision}">${entry.decision}</span>
                <span class="audit-tool">${escapeHtml(entry.tool_name)}</span>
                <span class="audit-time">${formatTime(entry.timestamp)}</span>
            </div>
            <div class="audit-summary muted">${escapeHtml(entry.arguments_summary)}</div>
        </div>
    `).join('');
    
    // Add scrollable list class to the container
    container.className = 'scrollable-list';
    
    // Restore scroll position
    container.scrollTop = scrollPosition;
    
    // Add pagination controls if needed
    renderPagination();
}

function renderPagination() {
    const container = document.getElementById('pagination-controls');
    if (!container) return;
    
    const totalPending = pendingApprovals.length;
    const totalPages = Math.ceil(totalPending / itemsPerPage);
    
    if (totalPages <= 1) {
        container.innerHTML = '';
        return;
    }
    
    container.innerHTML = `
        <div class="pagination">
            ${currentPage > 1 ? `
                <button class="btn btn-sm btn-secondary" onclick="changePage(${currentPage - 1})">
                    ← Previous
                </button>
            ` : ''}
            
            <span class="pagination-info">
                Page ${currentPage} of ${totalPages} (${totalPending} total)
            </span>
            
            ${currentPage < totalPages ? `
                <button class="btn btn-sm btn-secondary" onclick="changePage(${currentPage + 1})">
                    Next →
                </button>
            ` : ''}
        </div>
    `;
}

function changePage(page) {
    currentPage = page;
    renderPendingApprovals();
    renderAuditLog();
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

// ─── Init ───────────────────────────────────────────────────────────────

document.addEventListener('DOMContentLoaded', () => {
    loadConfig();
    loadApprovals();
    
    // Save config button
    document.getElementById('save-approval-config')?.addEventListener('click', saveConfig);
    
    // Auto-refresh pending approvals every 5 seconds
    setInterval(loadApprovals, 5000);
    
    // Add pagination container to the DOM
    const content = document.querySelector('.content-inner');
    if (content) {
        const paginationContainer = document.createElement('div');
        paginationContainer.id = 'pagination-controls';
        paginationContainer.className = 'pagination-container';
        content.appendChild(paginationContainer);
    }
});

// Make functions available globally
window.approveCommand = approveCommand;
window.denyCommand = denyCommand;
