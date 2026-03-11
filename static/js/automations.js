let automations = [];
let selectedAutomationId = null;
let openEditorId = null;
let automationTargets = [];

const WEEKDAY_LABELS = {
    '1': 'Mon',
    '2': 'Tue',
    '3': 'Wed',
    '4': 'Thu',
    '5': 'Fri',
    '6': 'Sat',
    '7': 'Sun',
};

async function apiRequest(path, options = {}) {
    const res = await fetch(`/api${path}`, {
        headers: {
            'Content-Type': 'application/json',
            ...(options.headers || {}),
        },
        ...options,
    });

    if (!res.ok) {
        const text = await res.text();
        throw new Error(text || `API error ${res.status}`);
    }

    const contentType = res.headers.get('content-type') || '';
    if (contentType.includes('application/json')) {
        return res.json();
    }
    return null;
}

function escapeHtml(value) {
    return String(value || '')
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;')
        .replace(/'/g, '&#39;');
}

function shorten(value, max = 160) {
    const str = String(value || '');
    if (str.length <= max) return str;
    return `${str.slice(0, max - 1)}...`;
}

function parseStoredSchedule(schedule) {
    const raw = String(schedule || '').trim();
    if (raw.startsWith('every:')) {
        return { mode: 'every', every: raw.slice(6), cron: '', raw };
    }
    if (raw.startsWith('cron:')) {
        return { mode: 'cron', every: '', cron: raw.slice(5), raw };
    }
    return { mode: 'cron', every: '', cron: raw, raw: `cron:${raw}` };
}

function parseCronFive(expr) {
    const parts = String(expr || '').trim().split(/\s+/).filter(Boolean);
    if (parts.length !== 5) return null;
    return {
        minute: parts[0],
        hour: parts[1],
        dom: parts[2],
        mon: parts[3],
        dow: parts[4],
    };
}

function toTime(hour, minute) {
    const h = Number(hour);
    const m = Number(minute);
    if (!Number.isInteger(h) || !Number.isInteger(m) || h < 0 || h > 23 || m < 0 || m > 59) {
        return null;
    }
    return `${String(h).padStart(2, '0')}:${String(m).padStart(2, '0')}`;
}

function scheduleToUi(schedule) {
    const parsed = parseStoredSchedule(schedule);

    if (parsed.mode === 'every') {
        const secs = Number(parsed.every);
        if (Number.isFinite(secs) && secs > 0 && secs % 3600 === 0) {
            return {
                mode: 'interval',
                intervalHours: String(Math.max(1, Math.floor(secs / 3600))),
                time: '09:00',
                weekday: '1',
                custom: parsed.raw,
            };
        }
        return {
            mode: 'custom',
            intervalHours: '6',
            time: '09:00',
            weekday: '1',
            custom: parsed.raw,
        };
    }

    const cron = parseCronFive(parsed.cron);
    if (!cron) {
        return {
            mode: 'custom',
            intervalHours: '6',
            time: '09:00',
            weekday: '1',
            custom: parsed.raw,
        };
    }

    const time = toTime(cron.hour, cron.minute) || '09:00';
    if (cron.dom === '*' && cron.mon === '*') {
        if (cron.dow === '*') {
            return { mode: 'daily', time, weekday: '1', intervalHours: '6', custom: parsed.raw };
        }
        if (cron.dow === '1-5') {
            return { mode: 'weekdays', time, weekday: '1', intervalHours: '6', custom: parsed.raw };
        }
        if (/^[1-7]$/.test(cron.dow)) {
            return { mode: 'weekly', time, weekday: cron.dow, intervalHours: '6', custom: parsed.raw };
        }
    }

    return {
        mode: 'custom',
        intervalHours: '6',
        time,
        weekday: '1',
        custom: parsed.raw,
    };
}

function formatSchedule(schedule) {
    const ui = scheduleToUi(schedule);
    if (ui.mode === 'daily') return `every day at ${ui.time}`;
    if (ui.mode === 'weekdays') return `weekdays at ${ui.time}`;
    if (ui.mode === 'weekly') return `every ${WEEKDAY_LABELS[ui.weekday] || ui.weekday} at ${ui.time}`;
    if (ui.mode === 'interval') return `every ${ui.intervalHours}h`;
    return ui.custom.replace(/^cron:/, 'cron ').replace(/^every:/, 'every ');
}

function normalizeCustomSchedule(raw) {
    const value = String(raw || '').trim();
    if (!value) throw new Error('Custom schedule is required.');
    if (value.startsWith('cron:') || value.startsWith('every:')) return value;

    const parts = value.split(/\s+/).filter(Boolean);
    if (parts.length === 5 || parts.length === 6) return `cron:${value}`;

    const num = Number(value);
    if (Number.isFinite(num) && num > 0) return `every:${Math.floor(num)}`;

    throw new Error('Use cron:... or every:... in Advanced schedule.');
}

function buildStoredScheduleFromUi(mode, fields) {
    if (mode === 'daily' || mode === 'weekdays' || mode === 'weekly') {
        const timeRaw = String(fields.time || '').trim();
        if (!/^\d{2}:\d{2}$/.test(timeRaw)) {
            throw new Error('Time is required (HH:MM).');
        }
        const [hh, mm] = timeRaw.split(':').map(Number);
        if (!Number.isInteger(hh) || !Number.isInteger(mm) || hh < 0 || hh > 23 || mm < 0 || mm > 59) {
            throw new Error('Invalid time.');
        }
        const dow = mode === 'daily' ? '*' : mode === 'weekdays' ? '1-5' : String(fields.weekday || '1');
        if (mode === 'weekly' && !/^[1-7]$/.test(dow)) {
            throw new Error('Invalid week day.');
        }
        return `cron:${mm} ${hh} * * ${dow}`;
    }

    if (mode === 'interval') {
        const hours = Number(fields.intervalHours);
        if (!Number.isFinite(hours) || hours <= 0) {
            throw new Error('Interval hours must be a positive number.');
        }
        return `every:${Math.floor(hours * 3600)}`;
    }

    if (mode === 'custom') {
        return normalizeCustomSchedule(fields.custom);
    }

    throw new Error('Unknown schedule type.');
}

function formatTrigger(kind, value) {
    const k = String(kind || 'always');
    if (k === 'contains') {
        return value ? `contains: ${value}` : 'contains';
    }
    return k;
}

function formatTimestamp(ts) {
    if (!ts) return 'never';
    const date = new Date(ts.includes('T') ? ts : `${ts}Z`);
    if (Number.isNaN(date.getTime())) return ts;
    return date.toLocaleString();
}

function statusBadgeClass(status) {
    const s = String(status || '').toLowerCase();
    if (s === 'active' || s === 'success') return 'badge-success';
    if (s === 'paused' || s === 'queued') return 'badge-warning';
    if (s === 'error' || s === 'failed') return 'badge-error';
    return 'badge-neutral';
}

function parseJsonArray(value) {
    if (!value) return [];
    if (Array.isArray(value)) return value;
    if (typeof value !== 'string') return [];
    try {
        const parsed = JSON.parse(value);
        return Array.isArray(parsed) ? parsed : [];
    } catch (_) {
        return [];
    }
}

function formatDependency(dep) {
    if (!dep || typeof dep !== 'object') return '';
    const kind = String(dep.kind || '').trim();
    const name = String(dep.name || '').trim();
    if (!kind && !name) return '';
    if (!kind) return name;
    if (!name) return kind;
    return `${kind}:${name}`;
}

function showToast(message, type = 'success') {
    const existing = document.querySelector('.toast');
    if (existing) existing.remove();

    const toast = document.createElement('div');
    toast.className = `toast toast-${type}`;
    toast.textContent = message;
    document.body.appendChild(toast);

    setTimeout(() => {
        toast.classList.add('toast-out');
        setTimeout(() => toast.remove(), 300);
    }, 2600);
}

function getTargetLabel(value) {
    const found = automationTargets.find((t) => t.value === value);
    return found ? found.label : value;
}

function getDeliverToOptionsHtml(selectedValue) {
    const selected = selectedValue || 'cli:default';
    const hasSelected = automationTargets.some((t) => t.value === selected);

    let options = automationTargets
        .map((t) => `<option value="${escapeHtml(t.value)}" ${t.value === selected ? 'selected' : ''}>${escapeHtml(t.label)}</option>`)
        .join('');

    if (!hasSelected && selected) {
        options += `<option value="${escapeHtml(selected)}" selected>${escapeHtml(`${selected} (saved)`)} </option>`;
    }

    return options;
}

function renderCreateDeliverToSelect(selectedValue) {
    const el = document.getElementById('automation-deliver-to');
    if (!el) return;
    el.innerHTML = getDeliverToOptionsHtml(selectedValue || 'cli:default');
}

async function loadAutomationTargets() {
    try {
        const rows = await apiRequest('/v1/automations/targets');
        if (Array.isArray(rows) && rows.length > 0) {
            automationTargets = rows
                .map((r) => ({
                    value: String(r.value || '').trim(),
                    label: String(r.label || r.value || '').trim(),
                }))
                .filter((r) => r.value);
        }
    } catch (_) {
        // fallback handled below
    }

    if (!automationTargets.length) {
        automationTargets = [{ value: 'cli:default', label: 'CLI (default)' }];
    }

    automationTargets.sort((a, b) => a.label.localeCompare(b.label));
}

function editorWfStepHtml(idx, step) {
    const name = escapeHtml((step && step.name) || '');
    const instruction = escapeHtml((step && step.instruction) || '');
    const approval = step && step.approval_required ? 'checked' : '';
    return `<div class="wf-step-row">
        <div class="wf-step-header">
            <span class="wf-step-number">Step ${idx}</span>
            <input class="input wf-step-name" type="text" placeholder="Step name" value="${name}">
            <button type="button" class="btn btn-danger btn-sm" data-action="remove-editor-wf-step">Remove</button>
        </div>
        <textarea class="input wf-step-instruction" rows="2" placeholder="Instruction for this step">${instruction}</textarea>
        <label class="wf-step-approval"><input type="checkbox" class="wf-step-approval-check" ${approval}> Require approval</label>
    </div>`;
}

function rowEditorHtml(item) {
    const ui = scheduleToUi(item.schedule);
    const editorOpenClass = openEditorId === item.id ? ' automation-inline-editor--open' : '';
    const dependencies = parseJsonArray(item.dependencies_json).map(formatDependency).filter(Boolean);
    const validationErrors = parseJsonArray(item.validation_errors).filter(Boolean);
    const existingSteps = parseJsonArray(item.workflow_steps_json);

    return `
        <div class="automation-inline-editor${editorOpenClass}" data-editor-for="${escapeHtml(item.id)}">
            <div class="form-row--2">
                <div class="form-group">
                    <label>Name</label>
                    <input class="input" data-field="name" value="${escapeHtml(item.name)}">
                </div>
                <div class="form-group">
                    <label>Deliver To</label>
                    <select class="input" data-field="deliver_to">
                        ${getDeliverToOptionsHtml(item.deliver_to || 'cli:default')}
                    </select>
                </div>
            </div>

            <div class="form-group">
                <label>Prompt</label>
                <textarea class="input automation-textarea" data-field="prompt" rows="4">${escapeHtml(item.prompt)}</textarea>
            </div>

            <div class="form-row--2">
                <div class="form-group">
                    <label>Schedule Type</label>
                    <select class="input" data-field="schedule_mode">
                        <option value="daily" ${ui.mode === 'daily' ? 'selected' : ''}>Every day</option>
                        <option value="weekdays" ${ui.mode === 'weekdays' ? 'selected' : ''}>Weekdays</option>
                        <option value="weekly" ${ui.mode === 'weekly' ? 'selected' : ''}>Every week</option>
                        <option value="interval" ${ui.mode === 'interval' ? 'selected' : ''}>Every N hours</option>
                        <option value="custom" ${ui.mode === 'custom' ? 'selected' : ''}>Advanced</option>
                    </select>
                </div>
                <div class="form-group" data-editor-time="${escapeHtml(item.id)}" style="${ui.mode === 'interval' || ui.mode === 'custom' ? 'display:none;' : ''}">
                    <label>Time</label>
                    <input class="input" type="time" data-field="time" value="${escapeHtml(ui.time || '09:00')}">
                </div>
                <div class="form-group" data-editor-weekday="${escapeHtml(item.id)}" style="${ui.mode === 'weekly' ? '' : 'display:none;'}">
                    <label>Day of week</label>
                    <select class="input" data-field="weekday">
                        <option value="1" ${ui.weekday === '1' ? 'selected' : ''}>Monday</option>
                        <option value="2" ${ui.weekday === '2' ? 'selected' : ''}>Tuesday</option>
                        <option value="3" ${ui.weekday === '3' ? 'selected' : ''}>Wednesday</option>
                        <option value="4" ${ui.weekday === '4' ? 'selected' : ''}>Thursday</option>
                        <option value="5" ${ui.weekday === '5' ? 'selected' : ''}>Friday</option>
                        <option value="6" ${ui.weekday === '6' ? 'selected' : ''}>Saturday</option>
                        <option value="7" ${ui.weekday === '7' ? 'selected' : ''}>Sunday</option>
                    </select>
                </div>
                <div class="form-group" data-editor-interval="${escapeHtml(item.id)}" style="${ui.mode === 'interval' ? '' : 'display:none;'}">
                    <label>Every (hours)</label>
                    <input class="input" type="number" min="1" step="1" data-field="interval_hours" value="${escapeHtml(ui.intervalHours || '6')}">
                </div>
                <div class="form-group" data-editor-custom="${escapeHtml(item.id)}" style="${ui.mode === 'custom' ? '' : 'display:none;'}">
                    <label>Advanced schedule</label>
                    <input class="input" data-field="custom_schedule" value="${escapeHtml(ui.custom || '')}" placeholder="cron:0 9 * * * or every:3600">
                </div>
            </div>

            <div class="form-row--2">
                <div class="form-group">
                    <label>Trigger</label>
                    <select class="input" data-field="trigger">
                        <option value="always" ${item.trigger_kind === 'always' ? 'selected' : ''}>Always notify</option>
                        <option value="on_change" ${item.trigger_kind === 'on_change' ? 'selected' : ''}>Notify on change</option>
                        <option value="contains" ${item.trigger_kind === 'contains' ? 'selected' : ''}>Notify on contains</option>
                    </select>
                </div>
                <div class="form-group" data-editor-trigger-value="${escapeHtml(item.id)}" style="${item.trigger_kind === 'contains' ? '' : 'display:none;'}">
                    <label>Trigger Value</label>
                    <input class="input" data-field="trigger_value" value="${escapeHtml(item.trigger_value || '')}" placeholder="text to detect">
                </div>
            </div>

            <div class="form-group">
                <label class="checkbox-label">
                    <input type="checkbox" data-field="enabled" ${item.enabled ? 'checked' : ''}>
                    Enabled
                </label>
            </div>

            <div class="form-group">
                <label class="checkbox-label">
                    <input type="checkbox" data-field="is_workflow" ${existingSteps.length ? 'checked' : ''}>
                    Execute as multi-step workflow
                </label>
            </div>

            <div class="form-group" data-editor-wf-steps="${escapeHtml(item.id)}" style="${existingSteps.length ? '' : 'display:none;'}">
                <label>Workflow Steps</label>
                <div class="automation-editor-wf-list" data-wf-list="${escapeHtml(item.id)}">
                    ${existingSteps.map((s, i) => editorWfStepHtml(i + 1, s)).join('')}
                </div>
                <button type="button" class="btn btn-secondary btn-sm" data-action="add-editor-wf-step" data-id="${escapeHtml(item.id)}">+ Add Step</button>
            </div>

            <div class="form-group">
                <label>Compiled dependencies</label>
                <div class="form-hint">${escapeHtml(dependencies.length ? dependencies.join(', ') : 'No explicit dependencies detected.')}</div>
                ${validationErrors.length ? `<div class="automation-inline-error">${escapeHtml(validationErrors.join(' | '))}</div>` : ''}
            </div>

            <div class="actions">
                <button class="btn btn-primary btn-sm" data-action="save-edit" data-id="${escapeHtml(item.id)}">Save</button>
                <button class="btn btn-secondary btn-sm" data-action="cancel-edit" data-id="${escapeHtml(item.id)}">Cancel</button>
            </div>
        </div>
    `;
}

function renderAutomations() {
    const listEl = document.getElementById('automations-list');
    const countEl = document.getElementById('automations-count');
    if (!listEl || !countEl) return;

    countEl.textContent = String(automations.length);

    if (automations.length === 0) {
        listEl.innerHTML = `
            <div class="empty-state">
                <p>No automations configured yet.</p>
                <p>Create your first one above.</p>
            </div>
        `;
        return;
    }

    listEl.innerHTML = automations
        .map((item) => {
            const selectedClass = item.id === selectedAutomationId ? ' automation-row--selected' : '';
            const nextToggleLabel = item.enabled ? 'Pause' : 'Resume';
            const nextToggleStatus = item.enabled ? 'paused' : 'active';
            const isInvalidConfig = String(item.status || '').toLowerCase() === 'invalid_config';
            const runDisabledAttr = isInvalidConfig
                ? 'disabled title="Fix dependency/config errors before running"'
                : '';
            const dependencies = parseJsonArray(item.dependencies_json).map(formatDependency).filter(Boolean);
            const validationErrors = parseJsonArray(item.validation_errors).filter(Boolean);
            const resultText = shorten(item.last_result || 'No result yet');
            const validationText = validationErrors.length
                ? `Config issue: ${validationErrors.join(' | ')}`
                : '';
            const dependenciesText = dependencies.length
                ? `Dependencies: ${dependencies.join(', ')}`
                : '';

            return `
                <div class="automation-block" data-id="${escapeHtml(item.id)}">
                    <div class="item-row automation-row${selectedClass}">
                        <div class="item-info">
                            <div class="item-icon">A</div>
                            <div class="automation-main">
                                <div class="automation-top">
                                    <span class="automation-name">${escapeHtml(item.name)}</span>
                                    <span class="badge ${statusBadgeClass(item.status)}">${escapeHtml(item.status || 'unknown')}</span>
                                </div>
                                <div class="automation-meta">
                                    <span class="automation-chip">${escapeHtml(formatSchedule(item.schedule))}</span>
                                    <span class="automation-chip">next: ${escapeHtml(formatTimestamp(item.next_run))}</span>
                                    <span class="automation-chip">trigger: ${escapeHtml(formatTrigger(item.trigger_kind, item.trigger_value))}</span>
                                    <span class="automation-chip">deliver: ${escapeHtml(getTargetLabel(item.deliver_to || 'cli:default'))}</span>
                                    <span class="automation-chip">last run: ${escapeHtml(formatTimestamp(item.last_run))}</span>
                                    ${dependencies.length ? `<span class="automation-chip">deps: ${escapeHtml(String(dependencies.length))}</span>` : ''}
                                </div>
                                <div class="item-detail">${escapeHtml(resultText)}</div>
                                ${dependenciesText ? `<div class="automation-detail-line">${escapeHtml(shorten(dependenciesText, 240))}</div>` : ''}
                                ${validationText ? `<div class="automation-detail-line automation-detail-line--error">${escapeHtml(shorten(validationText, 260))}</div>` : ''}
                            </div>
                        </div>
                        <div class="item-actions automation-actions">
                            <button class="btn btn-secondary btn-sm" data-action="history" data-id="${escapeHtml(item.id)}">History</button>
                            <button class="btn btn-secondary btn-sm" data-action="run" data-id="${escapeHtml(item.id)}" ${runDisabledAttr}>Run now</button>
                            <button class="btn btn-secondary btn-sm" data-action="toggle" data-id="${escapeHtml(item.id)}" data-enabled="${item.enabled ? '1' : '0'}" data-status="${escapeHtml(nextToggleStatus)}">${nextToggleLabel}</button>
                            <button class="btn btn-secondary btn-sm" data-action="edit" data-id="${escapeHtml(item.id)}">Edit</button>
                            <button class="btn btn-danger btn-sm" data-action="delete" data-id="${escapeHtml(item.id)}">Delete</button>
                        </div>
                    </div>
                    ${rowEditorHtml(item)}
                </div>
            `;
        })
        .join('');
}

function renderHistoryRows(rows) {
    const historyEl = document.getElementById('automation-history');
    if (!historyEl) return;

    if (!rows || rows.length === 0) {
        historyEl.innerHTML = `
            <div class="empty-state">
                <p>No runs yet for this automation.</p>
            </div>
        `;
        return;
    }

    historyEl.innerHTML = rows
        .map(
            (run) => `
                <div class="automation-history-item">
                    <div class="automation-history-head">
                        <span class="automation-history-id">${escapeHtml(run.id)}</span>
                        <span class="badge ${statusBadgeClass(run.status)}">${escapeHtml(run.status || 'unknown')}</span>
                    </div>
                    <div class="automation-history-meta">
                        <span>started: ${escapeHtml(formatTimestamp(run.started_at))}</span>
                        <span>finished: ${escapeHtml(formatTimestamp(run.finished_at))}</span>
                    </div>
                    <div class="automation-history-result">${escapeHtml(run.result || 'No details')}</div>
                </div>
            `
        )
        .join('');
}

async function loadAutomations() {
    automations = await apiRequest('/v1/automations');
    renderAutomations();
}

async function loadHistory(id) {
    selectedAutomationId = id;
    renderAutomations();

    const rows = await apiRequest(`/v1/automations/${encodeURIComponent(id)}/history?limit=30`);
    renderHistoryRows(rows);
}

function onScheduleModeToggle(container, mode, id) {
    const timeGroup = container.querySelector(`[data-editor-time="${id}"]`);
    const weekdayGroup = container.querySelector(`[data-editor-weekday="${id}"]`);
    const intervalGroup = container.querySelector(`[data-editor-interval="${id}"]`);
    const customGroup = container.querySelector(`[data-editor-custom="${id}"]`);

    if (timeGroup) timeGroup.style.display = mode === 'interval' || mode === 'custom' ? 'none' : '';
    if (weekdayGroup) weekdayGroup.style.display = mode === 'weekly' ? '' : 'none';
    if (intervalGroup) intervalGroup.style.display = mode === 'interval' ? '' : 'none';
    if (customGroup) customGroup.style.display = mode === 'custom' ? '' : 'none';
}

function onTriggerToggle(container, trigger, id) {
    const triggerValueGroup = container.querySelector(`[data-editor-trigger-value="${id}"]`);
    if (!triggerValueGroup) return;
    triggerValueGroup.style.display = trigger === 'contains' ? '' : 'none';
}

function readCreateSchedule() {
    const mode = document.getElementById('automation-schedule-mode').value;
    const time = document.getElementById('automation-time').value;
    const weekday = document.getElementById('automation-weekday').value;
    const intervalHours = document.getElementById('automation-interval-hours').value;
    const custom = document.getElementById('automation-custom-schedule').value;
    return buildStoredScheduleFromUi(mode, { time, weekday, intervalHours, custom });
}

async function onCreateAutomation(event) {
    event.preventDefault();

    const name = document.getElementById('automation-name').value.trim();
    const prompt = document.getElementById('automation-prompt').value.trim();
    const deliverTo = document.getElementById('automation-deliver-to').value.trim() || 'cli:default';
    const trigger = document.getElementById('automation-trigger').value;
    const triggerValue = document.getElementById('automation-trigger-value').value.trim();

    if (!name || !prompt) {
        showToast('Name and prompt are required.', 'error');
        return;
    }

    let schedule;
    try {
        schedule = readCreateSchedule();
    } catch (err) {
        showToast(err.message || 'Invalid schedule.', 'error');
        return;
    }

    const payload = { name, prompt, deliver_to: deliverTo, schedule, trigger };

    if (trigger === 'contains') {
        if (!triggerValue) {
            showToast('Trigger value is required for contains trigger.', 'error');
            return;
        }
        payload.trigger_value = triggerValue;
    }

    // Workflow steps
    const wfToggle = document.getElementById('automation-workflow-toggle');
    if (wfToggle && wfToggle.checked) {
        const steps = collectAutomationWfSteps();
        if (steps.length === 0) {
            showToast('Add at least one workflow step.', 'error');
            return;
        }
        payload.workflow_steps = steps;
    }

    await apiRequest('/v1/automations', {
        method: 'POST',
        body: JSON.stringify(payload),
    });

    showToast('Automation created.', 'success');
    event.target.reset();
    renderCreateDeliverToSelect(deliverTo);
    document.getElementById('automation-schedule-mode').value = 'daily';
    document.getElementById('automation-time').value = '09:00';
    document.getElementById('automation-interval-hours').value = '6';
    onCreateScheduleModeChange();
    onCreateTriggerModeChange();
    // Reset workflow toggle
    const wfToggleReset = document.getElementById('automation-workflow-toggle');
    if (wfToggleReset) wfToggleReset.checked = false;
    const wfStepsReset = document.getElementById('automation-workflow-steps');
    if (wfStepsReset) wfStepsReset.style.display = 'none';
    const wfListReset = document.getElementById('automation-wf-step-list');
    if (wfListReset) wfListReset.replaceChildren();

    await loadAutomations();
}

function getEditorContainer(id) {
    const block = document.querySelector(`.automation-block[data-id="${CSS.escape(id)}"]`);
    return block;
}

function readEditorSchedule(block, id) {
    const read = (field) => block.querySelector(`[data-field="${field}"]`);
    const mode = read('schedule_mode').value;
    const time = read('time') ? read('time').value : '09:00';
    const weekday = read('weekday') ? read('weekday').value : '1';
    const intervalHours = read('interval_hours') ? read('interval_hours').value : '6';
    const custom = read('custom_schedule') ? read('custom_schedule').value : '';
    return buildStoredScheduleFromUi(mode, { time, weekday, intervalHours, custom });
}

async function saveInlineEdit(id) {
    const block = getEditorContainer(id);
    if (!block) return;

    const read = (field) => block.querySelector(`[data-field="${field}"]`);

    const name = read('name').value.trim();
    const prompt = read('prompt').value.trim();
    const deliverTo = read('deliver_to').value.trim() || 'cli:default';
    const trigger = read('trigger').value;
    const triggerValue = read('trigger_value').value.trim();
    const enabled = !!read('enabled').checked;

    if (!name || !prompt) {
        showToast('Name and prompt are required.', 'error');
        return;
    }

    let schedule;
    try {
        schedule = readEditorSchedule(block, id);
    } catch (err) {
        showToast(err.message || 'Invalid schedule.', 'error');
        return;
    }

    const payload = { name, prompt, deliver_to: deliverTo, schedule, trigger, enabled };

    if (trigger === 'contains') {
        if (!triggerValue) {
            showToast('Trigger value is required for contains trigger.', 'error');
            return;
        }
        payload.trigger_value = triggerValue;
    } else {
        payload.clear_trigger_value = true;
    }

    // Workflow steps
    const isWf = read('is_workflow');
    if (isWf && isWf.checked) {
        const wfList = block.querySelector(`[data-wf-list="${CSS.escape(id)}"]`);
        if (wfList) {
            const steps = [];
            Array.from(wfList.children).forEach(row => {
                const sName = (row.querySelector('.wf-step-name') || {}).value || '';
                const sInstr = (row.querySelector('.wf-step-instruction') || {}).value || '';
                const sAppr = (row.querySelector('.wf-step-approval-check') || {}).checked || false;
                if (sName.trim() || sInstr.trim()) {
                    steps.push({ name: sName.trim(), instruction: sInstr.trim(), approval_required: sAppr });
                }
            });
            if (steps.length === 0) {
                showToast('Add at least one workflow step.', 'error');
                return;
            }
            payload.workflow_steps = steps;
        }
    } else {
        payload.clear_workflow_steps = true;
    }

    await apiRequest(`/v1/automations/${encodeURIComponent(id)}`, {
        method: 'PATCH',
        body: JSON.stringify(payload),
    });

    openEditorId = null;
    showToast('Automation updated.', 'success');
    await loadAutomations();
    if (selectedAutomationId === id) {
        await loadHistory(id);
    }
}

async function onAutomationAction(event) {
    const target = event.target.closest('button[data-action]');
    if (!target) return;

    const action = target.dataset.action;
    const id = target.dataset.id;
    if (!id) return;

    try {
        if (action === 'history') {
            await loadHistory(id);
            return;
        }

        if (action === 'run') {
            const resp = await apiRequest(`/v1/automations/${encodeURIComponent(id)}/run`, {
                method: 'POST',
            });
            showToast(resp?.message || 'Run queued.', 'success');
            await loadAutomations();
            if (selectedAutomationId === id) {
                await loadHistory(id);
            }
            return;
        }

        if (action === 'toggle') {
            const enabled = target.dataset.enabled === '1';
            const nextStatus = target.dataset.status || (enabled ? 'paused' : 'active');
            await apiRequest(`/v1/automations/${encodeURIComponent(id)}`, {
                method: 'PATCH',
                body: JSON.stringify({
                    enabled: !enabled,
                    status: nextStatus,
                }),
            });
            showToast(enabled ? 'Automation paused.' : 'Automation resumed.', 'success');
            await loadAutomations();
            if (selectedAutomationId === id) {
                await loadHistory(id);
            }
            return;
        }

        if (action === 'edit') {
            openEditorId = openEditorId === id ? null : id;
            renderAutomations();
            return;
        }

        if (action === 'cancel-edit') {
            openEditorId = null;
            renderAutomations();
            return;
        }

        if (action === 'save-edit') {
            await saveInlineEdit(id);
            return;
        }

        if (action === 'add-editor-wf-step') {
            const block = getEditorContainer(id);
            if (!block) return;
            const wfList = block.querySelector(`[data-wf-list="${CSS.escape(id)}"]`);
            if (!wfList) return;
            const idx = wfList.children.length + 1;
            wfList.insertAdjacentHTML('beforeend', editorWfStepHtml(idx, null));
            return;
        }

        if (action === 'remove-editor-wf-step') {
            const row = target.closest('.wf-step-row');
            if (row) {
                const wfList = row.parentElement;
                row.remove();
                // Renumber
                Array.from(wfList.children).forEach((r, i) => {
                    const num = r.querySelector('.wf-step-number');
                    if (num) num.textContent = 'Step ' + (i + 1);
                });
            }
            return;
        }

        if (action === 'delete') {
            if (!window.confirm('Delete this automation?')) {
                return;
            }
            await apiRequest(`/v1/automations/${encodeURIComponent(id)}`, {
                method: 'DELETE',
            });
            showToast('Automation deleted.', 'success');
            if (selectedAutomationId === id) {
                selectedAutomationId = null;
                renderHistoryRows([]);
            }
            if (openEditorId === id) {
                openEditorId = null;
            }
            await loadAutomations();
        }
    } catch (err) {
        showToast(err.message || 'Action failed.', 'error');
    }
}

function onCreateScheduleModeChange() {
    const mode = document.getElementById('automation-schedule-mode').value;
    const timeGroup = document.getElementById('automation-time-group');
    const weekdayGroup = document.getElementById('automation-weekday-group');
    const intervalGroup = document.getElementById('automation-interval-group');
    const customGroup = document.getElementById('automation-custom-group');

    timeGroup.style.display = mode === 'interval' || mode === 'custom' ? 'none' : '';
    weekdayGroup.style.display = mode === 'weekly' ? '' : 'none';
    intervalGroup.style.display = mode === 'interval' ? '' : 'none';
    customGroup.style.display = mode === 'custom' ? '' : 'none';
}

function onCreateTriggerModeChange() {
    const trigger = document.getElementById('automation-trigger').value;
    const valueGroup = document.getElementById('automation-trigger-value-group');
    if (!valueGroup) return;
    valueGroup.style.display = trigger === 'contains' ? '' : 'none';
}

// --- Workflow step builder for automation form ---

function addAutomationWfStep() {
    const list = document.getElementById('automation-wf-step-list');
    if (!list) return;
    const idx = list.children.length + 1;
    const row = document.createElement('div');
    row.className = 'wf-step-row';
    const header = document.createElement('div');
    header.className = 'wf-step-header';
    const num = document.createElement('span');
    num.className = 'wf-step-number';
    num.textContent = 'Step ' + idx;
    const nameInput = document.createElement('input');
    nameInput.className = 'input wf-step-name';
    nameInput.type = 'text';
    nameInput.placeholder = 'Step name';
    const removeBtn = document.createElement('button');
    removeBtn.type = 'button';
    removeBtn.className = 'btn btn-danger btn-sm';
    removeBtn.textContent = 'Remove';
    removeBtn.addEventListener('click', () => {
        row.remove();
        renumberAutomationWfSteps();
    });
    header.appendChild(num);
    header.appendChild(nameInput);
    header.appendChild(removeBtn);
    const textarea = document.createElement('textarea');
    textarea.className = 'input wf-step-instruction';
    textarea.rows = 2;
    textarea.placeholder = 'Instruction for this step';
    const approvalLabel = document.createElement('label');
    approvalLabel.className = 'wf-step-approval';
    const approvalCheck = document.createElement('input');
    approvalCheck.type = 'checkbox';
    approvalCheck.className = 'wf-step-approval-check';
    approvalLabel.appendChild(approvalCheck);
    approvalLabel.appendChild(document.createTextNode(' Require approval'));
    row.appendChild(header);
    row.appendChild(textarea);
    row.appendChild(approvalLabel);
    list.appendChild(row);
}

function renumberAutomationWfSteps() {
    const list = document.getElementById('automation-wf-step-list');
    if (!list) return;
    Array.from(list.children).forEach((row, i) => {
        const num = row.querySelector('.wf-step-number');
        if (num) num.textContent = 'Step ' + (i + 1);
    });
}

function collectAutomationWfSteps() {
    const list = document.getElementById('automation-wf-step-list');
    if (!list) return [];
    const steps = [];
    Array.from(list.children).forEach((row) => {
        const name = (row.querySelector('.wf-step-name') || {}).value || '';
        const instruction = (row.querySelector('.wf-step-instruction') || {}).value || '';
        const approval = (row.querySelector('.wf-step-approval-check') || {}).checked || false;
        if (name.trim() || instruction.trim()) {
            steps.push({ name: name.trim(), instruction: instruction.trim(), approval_required: approval });
        }
    });
    return steps;
}

async function initializeAutomationsPage() {
    const createForm = document.getElementById('automation-create-form');
    const listEl = document.getElementById('automations-list');
    const refreshBtn = document.getElementById('btn-automations-refresh');
    const scheduleModeEl = document.getElementById('automation-schedule-mode');
    const triggerEl = document.getElementById('automation-trigger');

    if (!createForm || !listEl || !refreshBtn || !scheduleModeEl || !triggerEl) {
        return;
    }

    await loadAutomationTargets();
    renderCreateDeliverToSelect('cli:default');

    createForm.addEventListener('submit', async (event) => {
        try {
            await onCreateAutomation(event);
        } catch (err) {
            showToast(err.message || 'Failed to create automation.', 'error');
        }
    });

    listEl.addEventListener('click', onAutomationAction);

    listEl.addEventListener('change', (event) => {
        const target = event.target;
        if (!(target instanceof HTMLElement)) return;
        const block = target.closest('.automation-block');
        const id = block ? block.dataset.id : null;
        if (!id) return;

        if (target.matches('[data-field="schedule_mode"]')) {
            onScheduleModeToggle(block, target.value, id);
        }
        if (target.matches('[data-field="trigger"]')) {
            onTriggerToggle(block, target.value, id);
        }
        if (target.matches('[data-field="is_workflow"]')) {
            const wfGroup = block.querySelector(`[data-editor-wf-steps="${CSS.escape(id)}"]`);
            if (wfGroup) {
                wfGroup.style.display = target.checked ? '' : 'none';
                if (target.checked) {
                    const wfList = block.querySelector(`[data-wf-list="${CSS.escape(id)}"]`);
                    if (wfList && wfList.children.length === 0) {
                        wfList.insertAdjacentHTML('beforeend', editorWfStepHtml(1, null));
                    }
                }
            }
        }
    });

    refreshBtn.addEventListener('click', async () => {
        try {
            await loadAutomationTargets();
            await loadAutomations();
            if (selectedAutomationId) {
                await loadHistory(selectedAutomationId);
            }
            showToast('Automation list refreshed.', 'success');
        } catch (err) {
            showToast(err.message || 'Refresh failed.', 'error');
        }
    });

    scheduleModeEl.addEventListener('change', onCreateScheduleModeChange);
    triggerEl.addEventListener('change', onCreateTriggerModeChange);
    onCreateScheduleModeChange();
    onCreateTriggerModeChange();

    // Workflow toggle
    const wfToggle = document.getElementById('automation-workflow-toggle');
    const wfStepsContainer = document.getElementById('automation-workflow-steps');
    const wfAddBtn = document.getElementById('automation-add-wf-step');
    if (wfToggle && wfStepsContainer && wfAddBtn) {
        wfToggle.addEventListener('change', () => {
            wfStepsContainer.style.display = wfToggle.checked ? '' : 'none';
            if (wfToggle.checked && document.getElementById('automation-wf-step-list').children.length === 0) {
                addAutomationWfStep();
            }
        });
        wfAddBtn.addEventListener('click', () => addAutomationWfStep());
    }

    try {
        await loadAutomations();
    } catch (err) {
        showToast(err.message || 'Failed to load automations.', 'error');
    }

    setInterval(async () => {
        if (openEditorId) return;
        try {
            await loadAutomations();
            if (selectedAutomationId) {
                await loadHistory(selectedAutomationId);
            }
        } catch (_) {
            // Silent background refresh errors.
        }
    }, 30000);
}

document.addEventListener('DOMContentLoaded', initializeAutomationsPage);
