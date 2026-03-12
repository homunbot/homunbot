// ─── Workflows page ─────────────────────────────────────────────
'use strict';

let workflows = [];
let selectedWorkflowId = null;
let stepCounter = 0;
let deliveryTargets = [];

// ─── Helpers ────────────────────────────────────────────────────

async function apiRequest(path, options = {}) {
    const res = await fetch(`/api${path}`, {
        headers: { 'Content-Type': 'application/json', ...(options.headers || {}) },
        ...options,
    });
    if (!res.ok) {
        const text = await res.text();
        throw new Error(text || `API error ${res.status}`);
    }
    const ct = res.headers.get('content-type') || '';
    return ct.includes('application/json') ? res.json() : null;
}

function escapeHtml(s) {
    if (!s) return '';
    return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;').replace(/'/g, '&#39;');
}

function showToast(msg, type = 'success') {
    const el = document.createElement('div');
    el.className = `toast toast-${type}`;
    el.textContent = msg;
    document.body.appendChild(el);
    setTimeout(() => { el.classList.add('toast-out'); }, 2800);
    setTimeout(() => el.remove(), 3200);
}

function statusBadge(status) {
    const cls = {
        pending: 'badge-neutral',
        running: 'badge-info',
        paused: 'badge-warning',
        completed: 'badge-success',
        failed: 'badge-error',
        cancelled: 'badge-neutral',
    }[status] || 'badge-neutral';
    return `<span class="badge ${cls}">${escapeHtml(status)}</span>`;
}

function stepStatusIcon(status) {
    const icons = {
        pending: '\u25CB',   // ○
        running: '\u25CE',   // ◎
        completed: '\u25CF', // ●
        failed: '\u2715',    // ✕
        skipped: '\u2298',   // ⊘
    };
    return icons[status] || '\u25CB';
}

function truncate(s, max) {
    if (!s || s.length <= max) return s || '';
    return s.substring(0, max - 1) + '\u2026';
}

function formatTs(ts) {
    if (!ts) return '\u2014';
    try {
        const d = new Date(ts.includes('T') ? ts : ts + 'Z');
        return d.toLocaleString(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
    } catch { return ts; }
}

// ─── Step builder ───────────────────────────────────────────────

function addStepRow(name, instruction, approvalRequired) {
    const idx = stepCounter++;
    const container = document.getElementById('wf-steps-container');
    const row = document.createElement('div');
    row.className = 'wf-step-row';
    row.dataset.idx = idx;

    const header = document.createElement('div');
    header.className = 'wf-step-header';

    const numSpan = document.createElement('span');
    numSpan.className = 'wf-step-number';
    numSpan.textContent = container.children.length + 1;
    header.appendChild(numSpan);

    const nameInput = document.createElement('input');
    nameInput.className = 'input wf-step-name';
    nameInput.type = 'text';
    nameInput.placeholder = 'Step name';
    if (name) nameInput.value = name;
    header.appendChild(nameInput);

    const approvalLabel = document.createElement('label');
    approvalLabel.className = 'wf-step-approval';
    const approvalCb = document.createElement('input');
    approvalCb.type = 'checkbox';
    approvalCb.className = 'wf-step-approval-cb';
    if (approvalRequired) approvalCb.checked = true;
    approvalLabel.appendChild(approvalCb);
    approvalLabel.appendChild(document.createTextNode(' Approval'));
    header.appendChild(approvalLabel);

    const removeBtn = document.createElement('button');
    removeBtn.type = 'button';
    removeBtn.className = 'btn btn-ghost btn-sm wf-step-remove';
    removeBtn.title = 'Remove step';
    removeBtn.textContent = '\u00D7';
    header.appendChild(removeBtn);

    row.appendChild(header);

    const textarea = document.createElement('textarea');
    textarea.className = 'input wf-step-instruction';
    textarea.rows = 2;
    textarea.placeholder = 'Detailed instruction for this step...';
    if (instruction) textarea.value = instruction;
    row.appendChild(textarea);

    container.appendChild(row);
    renumberSteps();
}

function renumberSteps() {
    const rows = document.querySelectorAll('.wf-step-row');
    rows.forEach((row, i) => {
        const num = row.querySelector('.wf-step-number');
        if (num) num.textContent = i + 1;
    });
}

function collectSteps() {
    const rows = document.querySelectorAll('.wf-step-row');
    const steps = [];
    rows.forEach(row => {
        const name = row.querySelector('.wf-step-name')?.value?.trim();
        const instruction = row.querySelector('.wf-step-instruction')?.value?.trim();
        const approval = row.querySelector('.wf-step-approval-cb')?.checked || false;
        if (name && instruction) {
            steps.push({ name, instruction, approval_required: approval, max_retries: 1 });
        }
    });
    return steps;
}

// ─── Data loading ───────────────────────────────────────────────

async function loadWorkflows() {
    try {
        const data = await apiRequest('/v1/workflows');
        workflows = data.workflows || [];

        const stats = data.stats || {};
        setText('stat-total', stats.total ?? 0);
        setText('stat-running', stats.running ?? 0);
        setText('stat-completed', stats.completed ?? 0);
        setText('stat-failed', stats.failed ?? 0);
        setText('workflows-count', stats.total ?? 0);

        renderWorkflows();
    } catch (e) {
        console.error('Failed to load workflows:', e);
    }
}

function setText(id, value) {
    const el = document.getElementById(id);
    if (el) el.textContent = value;
}

// ─── Rendering ──────────────────────────────────────────────────

function renderWorkflows() {
    const list = document.getElementById('workflows-list');
    if (!workflows.length) {
        list.textContent = '';
        const empty = document.createElement('div');
        empty.className = 'empty-state';
        const p = document.createElement('p');
        p.textContent = 'No workflows yet. Click "Create Workflow" to get started.';
        empty.appendChild(p);
        list.appendChild(empty);
        return;
    }

    // Sort: active first, then by created_at desc
    const sorted = [...workflows].sort((a, b) => {
        const aActive = ['running', 'paused', 'pending'].includes(a.status);
        const bActive = ['running', 'paused', 'pending'].includes(b.status);
        if (aActive !== bActive) return aActive ? -1 : 1;
        return (b.created_at || '').localeCompare(a.created_at || '');
    });

    list.textContent = '';
    sorted.forEach(w => {
        const stepsDone = w.steps ? w.steps.filter(s => s.status === 'completed').length : 0;
        const stepsTotal = w.steps ? w.steps.length : 0;
        const progress = stepsTotal ? `${stepsDone}/${stepsTotal}` : '\u2014';
        const isSelected = w.id === selectedWorkflowId;
        const showApprove = w.status === 'paused';
        const showCancel = ['running', 'paused', 'pending'].includes(w.status);

        const row = document.createElement('div');
        row.className = 'item-row' + (isSelected ? ' selected' : '');
        row.dataset.id = w.id;

        const info = document.createElement('div');
        info.className = 'item-info';
        info.dataset.action = 'select';
        info.dataset.id = w.id;
        info.style.cursor = 'pointer';

        const nameDiv = document.createElement('div');
        nameDiv.className = 'item-name';
        nameDiv.textContent = w.name;
        info.appendChild(nameDiv);

        const detailDiv = document.createElement('div');
        detailDiv.className = 'item-detail';

        const line1 = document.createElement('span');
        line1.className = 'item-detail-line';
        line1.innerHTML = statusBadge(w.status) + ' &nbsp; Steps: ' + escapeHtml(progress);
        detailDiv.appendChild(line1);

        const line2 = document.createElement('span');
        line2.className = 'item-detail-line';
        line2.textContent = truncate(w.objective, 80);
        detailDiv.appendChild(line2);

        const line3 = document.createElement('span');
        line3.className = 'item-detail-line';
        line3.textContent = formatTs(w.created_at);
        detailDiv.appendChild(line3);

        info.appendChild(detailDiv);
        row.appendChild(info);

        const actions = document.createElement('div');
        actions.className = 'item-actions';

        if (showApprove) {
            const btn = document.createElement('button');
            btn.className = 'btn btn-primary btn-sm';
            btn.dataset.action = 'approve';
            btn.dataset.id = w.id;
            btn.textContent = 'Approve';
            actions.appendChild(btn);
        }
        if (showCancel) {
            const btn = document.createElement('button');
            btn.className = 'btn btn-danger btn-sm';
            btn.dataset.action = 'cancel';
            btn.dataset.id = w.id;
            btn.textContent = 'Cancel';
            actions.appendChild(btn);
        }

        const isTerminal = ['completed', 'failed', 'cancelled'].includes(w.status);
        if (isTerminal) {
            const restartBtn = document.createElement('button');
            restartBtn.className = 'btn btn-secondary btn-sm';
            restartBtn.dataset.action = 'restart';
            restartBtn.dataset.id = w.id;
            restartBtn.textContent = 'Restart';
            actions.appendChild(restartBtn);

            const delBtn = document.createElement('button');
            delBtn.className = 'btn btn-ghost btn-sm';
            delBtn.dataset.action = 'delete';
            delBtn.dataset.id = w.id;
            delBtn.textContent = 'Delete';
            actions.appendChild(delBtn);
        }
        row.appendChild(actions);
        list.appendChild(row);
    });
}

async function loadWorkflowDetail(id) {
    const section = document.getElementById('workflow-detail-section');
    const detail = document.getElementById('workflow-detail');
    try {
        const w = await apiRequest(`/v1/workflows/${encodeURIComponent(id)}`);
        selectedWorkflowId = id;
        section.style.display = '';
        renderWorkflows();

        detail.textContent = '';

        // Header
        const hdr = document.createElement('div');
        hdr.className = 'wf-detail-header';

        const hdrLeft = document.createElement('div');
        const strong = document.createElement('strong');
        strong.textContent = w.name;
        hdrLeft.appendChild(strong);
        hdrLeft.insertAdjacentHTML('beforeend', ' ' + statusBadge(w.status));

        const objDiv = document.createElement('div');
        objDiv.style.cssText = 'margin-top:0.25rem;opacity:0.7;font-size:0.85rem;';
        objDiv.textContent = w.objective;
        hdrLeft.appendChild(objDiv);
        hdr.appendChild(hdrLeft);

        const hdrActions = document.createElement('div');
        hdrActions.className = 'actions';
        if (w.status === 'paused') {
            const btn = document.createElement('button');
            btn.className = 'btn btn-primary btn-sm';
            btn.dataset.action = 'approve';
            btn.dataset.id = w.id;
            btn.textContent = 'Approve Next Step';
            hdrActions.appendChild(btn);
        }
        if (['running', 'paused', 'pending'].includes(w.status)) {
            const btn = document.createElement('button');
            btn.className = 'btn btn-danger btn-sm';
            btn.dataset.action = 'cancel';
            btn.dataset.id = w.id;
            btn.textContent = 'Cancel';
            hdrActions.appendChild(btn);
        }
        if (['completed', 'failed', 'cancelled'].includes(w.status)) {
            const restartBtn = document.createElement('button');
            restartBtn.className = 'btn btn-secondary btn-sm';
            restartBtn.dataset.action = 'restart';
            restartBtn.dataset.id = w.id;
            restartBtn.textContent = 'Restart';
            hdrActions.appendChild(restartBtn);

            const delBtn = document.createElement('button');
            delBtn.className = 'btn btn-ghost btn-sm';
            delBtn.dataset.action = 'delete';
            delBtn.dataset.id = w.id;
            delBtn.textContent = 'Delete';
            hdrActions.appendChild(delBtn);
        }
        hdr.appendChild(hdrActions);
        detail.appendChild(hdr);

        // Step timeline
        const timeline = document.createElement('div');
        timeline.className = 'wf-timeline';

        (w.steps || []).forEach(s => {
            const statusCls = {
                completed: 'wf-step-done',
                running: 'wf-step-active',
                failed: 'wf-step-failed',
                skipped: 'wf-step-skipped',
            }[s.status] || '';

            const step = document.createElement('div');
            step.className = 'wf-timeline-step ' + statusCls;

            const icon = document.createElement('span');
            icon.className = 'wf-timeline-icon';
            icon.textContent = stepStatusIcon(s.status);
            step.appendChild(icon);

            const body = document.createElement('div');
            body.className = 'wf-timeline-body';

            const nameRow = document.createElement('div');
            nameRow.className = 'wf-timeline-name';
            nameRow.textContent = s.name + ' ';
            if (s.approval_required) {
                nameRow.insertAdjacentHTML('beforeend', '<span class="badge badge-warning" style="font-size:0.65rem;">approval</span> ');
            }
            nameRow.insertAdjacentHTML('beforeend', '<span class="badge badge-neutral" style="font-size:0.6rem;">' + escapeHtml(s.status) + '</span>');
            body.appendChild(nameRow);

            const instrDiv = document.createElement('div');
            instrDiv.className = 'wf-timeline-instruction';
            instrDiv.textContent = truncate(s.instruction, 120);
            body.appendChild(instrDiv);

            if (s.result) {
                const resultDiv = document.createElement('div');
                resultDiv.className = 'wf-step-result';
                resultDiv.textContent = truncate(s.result, 300);
                body.appendChild(resultDiv);
            }
            if (s.error) {
                const errDiv = document.createElement('div');
                errDiv.className = 'wf-step-error';
                errDiv.textContent = s.error;
                body.appendChild(errDiv);
            }
            if (s.started_at) {
                const tsDiv = document.createElement('div');
                tsDiv.className = 'wf-timeline-ts';
                tsDiv.textContent = formatTs(s.started_at) + (s.completed_at ? ' \u2192 ' + formatTs(s.completed_at) : '');
                body.appendChild(tsDiv);
            }

            step.appendChild(body);
            timeline.appendChild(step);
        });

        detail.appendChild(timeline);
    } catch (e) {
        detail.textContent = '';
        const empty = document.createElement('div');
        empty.className = 'empty-state';
        const p = document.createElement('p');
        p.textContent = 'Error: ' + e.message;
        empty.appendChild(p);
        detail.appendChild(empty);
    }
}

// ─── Actions ────────────────────────────────────────────────────

async function onCreateWorkflow(e) {
    e.preventDefault();
    const name = document.getElementById('wf-name').value.trim();
    const objective = document.getElementById('wf-objective').value.trim();
    const deliverTo = document.getElementById('wf-deliver-to').value;
    const steps = collectSteps();

    if (!name) { showToast('Name is required', 'error'); return; }
    if (!objective) { showToast('Objective is required', 'error'); return; }
    if (!steps.length) { showToast('Add at least one step', 'error'); return; }

    try {
        const result = await apiRequest('/v1/workflows', {
            method: 'POST',
            body: JSON.stringify({ name, objective, steps, deliver_to: deliverTo }),
        });
        showToast('Workflow created: ' + result.workflow_id);
        document.getElementById('wf-name').value = '';
        document.getElementById('wf-objective').value = '';
        document.getElementById('wf-steps-container').textContent = '';
        stepCounter = 0;
        addStepRow();
        toggleCreatorPanel(false);
        await loadWorkflows();
    } catch (e) {
        showToast('Error: ' + e.message, 'error');
    }
}

async function handleAction(action, id) {
    try {
        if (action === 'approve') {
            await apiRequest(`/v1/workflows/${encodeURIComponent(id)}/approve`, { method: 'POST' });
            showToast('Workflow approved, resuming...');
        } else if (action === 'cancel') {
            await apiRequest(`/v1/workflows/${encodeURIComponent(id)}/cancel`, { method: 'POST' });
            showToast('Workflow cancelled');
        } else if (action === 'delete') {
            if (!confirm('Delete this workflow permanently?')) return;
            await apiRequest(`/v1/workflows/${encodeURIComponent(id)}/delete`, { method: 'POST' });
            showToast('Workflow deleted');
            if (selectedWorkflowId === id) {
                selectedWorkflowId = null;
                document.getElementById('workflow-detail-section').style.display = 'none';
            }
        } else if (action === 'restart') {
            const res = await apiRequest(`/v1/workflows/${encodeURIComponent(id)}/restart`, { method: 'POST' });
            showToast(res?.message || 'Workflow restarted');
        } else if (action === 'select') {
            await loadWorkflowDetail(id);
            return;
        }
        await loadWorkflows();
        if (selectedWorkflowId === id) await loadWorkflowDetail(id);
    } catch (e) {
        showToast('Error: ' + e.message, 'error');
    }
}

// ─── Init ───────────────────────────────────────────────────────

async function loadDeliveryTargets() {
    try {
        const rows = await apiRequest('/v1/automations/targets');
        if (Array.isArray(rows) && rows.length > 0) {
            deliveryTargets = rows
                .map(r => ({ value: String(r.value || '').trim(), label: String(r.label || r.value || '').trim() }))
                .filter(r => r.value);
        }
    } catch (_) { /* fallback below */ }
    // Always include Web UI
    if (!deliveryTargets.some(t => t.value === 'web:web')) {
        deliveryTargets.unshift({ value: 'web:web', label: 'Web UI' });
    }
    deliveryTargets.sort((a, b) => a.label.localeCompare(b.label));

    const sel = document.getElementById('wf-deliver-to');
    if (sel) {
        const prev = sel.value;
        while (sel.firstChild) sel.removeChild(sel.firstChild);
        for (const t of deliveryTargets) {
            const opt = document.createElement('option');
            opt.value = t.value;
            opt.textContent = t.label;
            sel.appendChild(opt);
        }
        if (prev) sel.value = prev;
    }
}

// ─── Creator panel toggle ────────────────────────────────────────
function toggleCreatorPanel(show) {
    var panel = document.getElementById('wf-creator-panel');
    var btn   = document.getElementById('wf-create-toggle');
    if (!panel) return;
    if (typeof show !== 'boolean') show = panel.style.display === 'none';
    panel.style.display = show ? '' : 'none';
    if (btn) btn.textContent = show ? 'Cancel' : '+ Create Workflow';
    if (btn) btn.classList.toggle('btn-primary', !show);
    if (btn) btn.classList.toggle('btn-ghost', show);
}

async function initWorkflowsPage() {
    // Creator panel toggle
    var createToggle = document.getElementById('wf-create-toggle');
    if (createToggle) createToggle.addEventListener('click', function() { toggleCreatorPanel(); });

    var cancelBtn = document.getElementById('wf-create-cancel');
    if (cancelBtn) cancelBtn.addEventListener('click', function() { toggleCreatorPanel(false); });

    document.getElementById('wf-add-step').addEventListener('click', () => addStepRow());
    document.getElementById('wf-steps-container').addEventListener('click', (e) => {
        if (e.target.classList.contains('wf-step-remove')) {
            e.target.closest('.wf-step-row').remove();
            renumberSteps();
        }
    });
    addStepRow();

    document.getElementById('workflow-create-form').addEventListener('submit', onCreateWorkflow);

    document.getElementById('workflows-list').addEventListener('click', (e) => {
        const btn = e.target.closest('[data-action]');
        if (btn) handleAction(btn.dataset.action, btn.dataset.id);
    });
    document.getElementById('workflow-detail').addEventListener('click', (e) => {
        const btn = e.target.closest('[data-action]');
        if (btn) handleAction(btn.dataset.action, btn.dataset.id);
    });

    document.getElementById('btn-workflows-refresh').addEventListener('click', async () => {
        await loadWorkflows();
        if (selectedWorkflowId) await loadWorkflowDetail(selectedWorkflowId);
    });

    await loadDeliveryTargets();
    await loadWorkflows();

    setInterval(async () => {
        await loadWorkflows();
        if (selectedWorkflowId) await loadWorkflowDetail(selectedWorkflowId);
    }, 15000);
}

document.addEventListener('DOMContentLoaded', initWorkflowsPage);
