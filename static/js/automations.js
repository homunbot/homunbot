let automations = [];
let selectedAutomationId = null;
let openEditorId = null;
let automationTargets = [];

/** Get the current profile filter slug (empty = all). */
function getAutomationsProfileFilter() {
    const el = document.getElementById('automations-profile-filter');
    return el ? el.value : '';
}

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
            <div class="automation-flow-full" data-flow-full-id="${escapeHtml(item.id)}"></div>
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
                                ${item.prompt ? `<div class="automation-prompt-preview">${escapeHtml(shorten(item.prompt, 140))}</div>` : ''}
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
                    <div class="automation-flow-strip">
                        <div class="automation-flow-mini" data-flow-mini-id="${escapeHtml(item.id)}"></div>
                        <button class="automation-flow-expand-btn" data-action="expand-flow" data-id="${escapeHtml(item.id)}" title="Show flow diagram">
                            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="6 9 12 15 18 9"></polyline></svg>
                        </button>
                    </div>
                    <div class="automation-flow-canvas" data-flow-canvas-id="${escapeHtml(item.id)}" style="display:none;"></div>
                    ${rowEditorHtml(item)}
                </div>
            `;
        })
        .join('');

    // Render mini flow previews
    renderMiniFlows();
}

/**
 * Enrich flow nodes with instruction text from workflow_steps_json.
 * Adds a `description` field to each node so tooltips can show what each step does.
 */
function enrichFlowWithSteps(flow, stepsJson) {
    if (!flow || !flow.nodes || !stepsJson) return;
    var steps;
    try {
        steps = typeof stepsJson === 'string' ? JSON.parse(stepsJson) : stepsJson;
    } catch (_) { return; }
    if (!Array.isArray(steps) || steps.length === 0) return;

    // Match flow nodes to steps by label → step.name, or by order (step_0, step_1...)
    var stepsByName = {};
    steps.forEach(function (s) { if (s.name) stepsByName[s.name.toLowerCase()] = s; });

    var stepIdx = 0;
    flow.nodes.forEach(function (node) {
        // Skip trigger / deliver / condition nodes — only enrich processing steps
        if (node.kind === 'trigger' || node.kind === 'deliver') return;

        // Try match by label → step name
        var match = node.label ? stepsByName[node.label.toLowerCase()] : null;
        if (!match && stepIdx < steps.length) {
            match = steps[stepIdx++];
        }
        if (match && match.instruction) {
            node.description = match.instruction;
        }
    });
}

function renderMiniFlows() {
    if (typeof window.HomunFlow === 'undefined') return;
    automations.forEach(function (item) {
        var flow = parseFlowJson(item.flow_json);
        if (!flow) return;

        // Enrich flow nodes with workflow step instructions for richer tooltips
        enrichFlowWithSteps(flow, item.workflow_steps_json);

        // Mini strip in collapsed card
        var miniEl = document.querySelector('.automation-flow-mini[data-flow-mini-id="' + item.id + '"]');
        if (miniEl) window.HomunFlow.renderFlowMini(miniEl, flow);

        // Full flow in expanded canvas (render if already visible)
        var canvasEl = document.querySelector('.automation-flow-canvas[data-flow-canvas-id="' + item.id + '"]');
        if (canvasEl && canvasEl.style.display !== 'none') {
            window.HomunFlow.renderFlow(canvasEl, flow);
        }

        // Full flow in editor (when open)
        var fullEl = document.querySelector('.automation-flow-full[data-flow-full-id="' + item.id + '"]');
        if (fullEl) window.HomunFlow.renderFlow(fullEl, flow);
    });
}

function toggleFlowCanvas(automationId) {
    var canvasEl = document.querySelector('.automation-flow-canvas[data-flow-canvas-id="' + automationId + '"]');
    var btn = document.querySelector('.automation-flow-expand-btn[data-id="' + automationId + '"]');
    if (!canvasEl) return;

    var isHidden = canvasEl.style.display === 'none';
    canvasEl.style.display = isHidden ? 'block' : 'none';

    // Rotate chevron
    if (btn) btn.classList.toggle('automation-flow-expand-btn--open', isHidden);

    // Render flow on first expand
    if (isHidden && !canvasEl.firstChild) {
        var item = automations.find(function (a) { return a.id === automationId; });
        if (item) {
            var flow = parseFlowJson(item.flow_json);
            if (flow) window.HomunFlow.renderFlow(canvasEl, flow);
        }
    }
}

function parseFlowJson(raw) {
    if (!raw) return null;
    try { return typeof raw === 'string' ? JSON.parse(raw) : raw; } catch (_) { return null; }
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
    try {
        const pf = getAutomationsProfileFilter();
        const profileParam = pf ? '?profile=' + encodeURIComponent(pf) : '';
        automations = await apiRequest('/v1/automations' + profileParam);
    } catch (err) {
        automations = [];
        showErrorState('automations-list', 'Could not load automations.', loadAutomations);
        return;
    }
    clearErrorState('automations-list');
    renderAutomations();
}

async function loadHistory(id) {
    selectedAutomationId = id;
    renderAutomations();

    // Open side panel
    const detail = document.getElementById('auto-detail');
    const title = document.getElementById('auto-detail-title');
    if (detail) {
        detail.style.display = 'flex';
        const item = automations.find(a => a.id === id);
        if (title) title.textContent = item ? `History — ${item.name}` : 'Run History';
    }

    const rows = await apiRequest(`/v1/automations/${encodeURIComponent(id)}/history?limit=30`);
    renderHistoryRows(rows);
}

function closeHistoryPanel() {
    selectedAutomationId = null;
    const detail = document.getElementById('auto-detail');
    if (detail) detail.style.display = 'none';
    renderAutomations();
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

        if (action === 'expand-flow') {
            toggleFlowCanvas(id);
            return;
        }

        if (action === 'edit') {
            const item = automations.find(a => a.id === id);
            if (item) {
                document.getElementById('automations-list-view').style.display = 'none';
                document.getElementById('automations-builder-view').style.display = 'flex';
                Builder.loadAutomation(item);
            }
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
                closeHistoryPanel();
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
    const listEl = document.getElementById('automations-list');
    const refreshBtn = document.getElementById('btn-automations-refresh');

    if (!listEl) return;

    // Initialize profile filter dropdown
    const profileSelect = document.getElementById('automations-profile-filter');
    if (profileSelect) {
        const allOpt = document.createElement('option');
        allOpt.value = '';
        allOpt.textContent = 'All profiles';
        profileSelect.appendChild(allOpt);
        try {
            const profiles = await apiRequest('/v1/profiles');
            profiles.forEach(p => {
                const opt = document.createElement('option');
                opt.value = p.slug;
                opt.textContent = (p.avatar_emoji || '\u{1F464}') + ' ' + p.display_name;
                profileSelect.appendChild(opt);
            });
        } catch (_) {}
        profileSelect.addEventListener('change', () => loadAutomations());
    }

    await loadAutomationTargets();

    // Legacy inline create form (removed — creation now uses Builder)
    const createForm = document.getElementById('automation-create-form');
    const scheduleModeEl = document.getElementById('automation-schedule-mode');
    const triggerEl = document.getElementById('automation-trigger');
    if (createForm) {
        renderCreateDeliverToSelect('cli:default');
        createForm.addEventListener('submit', async (event) => {
            try {
                await onCreateAutomation(event);
            } catch (err) {
                showToast(err.message || 'Failed to create automation.', 'error');
            }
        });
    }

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

    if (refreshBtn) {
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
    }

    if (scheduleModeEl) scheduleModeEl.addEventListener('change', onCreateScheduleModeChange);
    if (triggerEl) triggerEl.addEventListener('change', onCreateTriggerModeChange);
    if (scheduleModeEl) onCreateScheduleModeChange();
    if (triggerEl) onCreateTriggerModeChange();

    // Workflow toggle (legacy inline form)
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

    // History panel close button
    document.getElementById('btn-auto-detail-close')?.addEventListener('click', closeHistoryPanel);

    // Prompt bar — NLP automation creation
    setupAutoPromptBar();

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

function setupAutoPromptBar() {
    const input = document.getElementById('auto-prompt-input');
    const btn = document.getElementById('btn-auto-prompt-send');
    if (!input || !btn) return;

    btn.addEventListener('click', () => submitAutoPrompt());

    input.addEventListener('keydown', e => {
        if (e.key === 'Enter' && !e.shiftKey) {
            e.preventDefault();
            submitAutoPrompt();
        }
    });

    // Auto-grow
    input.addEventListener('input', () => {
        input.style.height = 'auto';
        input.style.height = Math.min(input.scrollHeight, 100) + 'px';
    });
}

async function submitAutoPrompt() {
    const input = document.getElementById('auto-prompt-input');
    const desc = input?.value.trim();
    if (!desc) return;

    // Switch to Builder and generate from prompt
    document.getElementById('automations-list-view').style.display = 'none';
    document.getElementById('automations-builder-view').style.display = 'flex';
    Builder.reset();

    // Set the prompt in the Builder prompt bar and trigger generation
    const builderPrompt = document.getElementById('builder-prompt-input');
    if (builderPrompt) {
        builderPrompt.value = desc;
        builderPrompt.style.height = 'auto';
        builderPrompt.style.height = Math.min(builderPrompt.scrollHeight, 120) + 'px';
    }

    input.value = '';
    input.style.height = '';

    // Auto-generate the flow
    await Builder.generateFromPrompt();
}

document.addEventListener('DOMContentLoaded', initializeAutomationsPage);

// ─── Node Kind Configuration (shared source of truth for Builder) ───────────────────────
// NOTE: All values in NODE_KINDS are static configuration constants.
// Dynamic content is always escaped via escapeHtml() before DOM insertion.
const NODE_KINDS = {
    trigger: {
        label: 'Schedule Trigger',
        accent: '#E8A838',
        icon: 'M13 10V3L4 14h7v7l9-11h-7z',
        group: 'triggers',
        hasIn: false, hasOut: true,
        description: 'Starts the automation on a schedule (daily, interval, or cron expression)',
    },
    tool: {
        label: 'Tool',
        accent: '#68B984',
        icon: 'M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6-7.6 7.6-1.6-1.6a1 1 0 1 0-1.4 1.4l2.3 2.3a1 1 0 0 0 1.4 0l8.3-8.3a1 1 0 0 0 0-1.4l-2.3-2.3a1 1 0 0 0-1.4 0z',
        group: 'processing',
        hasIn: true, hasOut: true,
        description: 'Run a built-in tool (shell commands, file operations, web search, etc.)',
    },
    skill: {
        label: 'Skill',
        accent: '#E07C4F',
        icon: 'M13 10V3L4 14h7v7l9-11h-7z',
        group: 'processing',
        hasIn: true, hasOut: true,
        description: 'Execute an installed Agent Skill \u2014 extensible plugins for specialized tasks',
    },
    mcp: {
        label: 'MCP Server',
        accent: '#9B72CF',
        icon: 'M12 2C6.5 2 2 6.5 2 12s4.5 10 10 10 10-4.5 10-10S17.5 2 12 2zm0 4a2 2 0 1 1 0 4 2 2 0 0 1 0-4zm-4 8a2 2 0 1 1 0 4 2 2 0 0 1 0-4zm8 0a2 2 0 1 1 0 4 2 2 0 0 1 0-4z',
        group: 'processing',
        hasIn: true, hasOut: true,
        description: 'Call a tool from an MCP server (Gmail, GitHub, Slack, filesystem, etc.)',
    },
    llm: {
        label: 'LLM / Agent',
        accent: '#5B9BD5',
        icon: 'M12 2a9 9 0 0 0-9 9c0 3.1 1.6 5.9 4 7.5V21a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1v-2.5c2.4-1.6 4-4.4 4-7.5a9 9 0 0 0-9-9z',
        group: 'processing',
        hasIn: true, hasOut: true,
        description: 'Send a prompt to an LLM agent for reasoning, writing, analysis, or decisions',
    },
    transform: {
        label: 'Transform',
        accent: '#78909C',
        icon: 'M12 15.5A3.5 3.5 0 1 1 12 8.5a3.5 3.5 0 0 1 0 7z',
        group: 'processing',
        hasIn: true, hasOut: true,
        description: 'Transform, filter, or reshape data between steps',
    },
    condition: {
        label: 'Condition',
        accent: '#8BC34A',
        icon: 'M12 2L2 12l10 10 10-10L12 2z',
        group: 'control',
        hasIn: true, hasOut: true,
        shape: 'diamond',
        description: 'Branch the flow based on a condition (if/else). Example: "Are there new emails?" \u2192 Yes: summarize them, No: skip to deliver.',
    },
    parallel: {
        label: 'Parallel',
        accent: '#26A69A',
        icon: 'M4 6h6v2H4zm0 5h16v2H4zm10-5h6v2h-6zM4 16h6v2H4zm10 0h6v2h-6z',
        group: 'control',
        hasIn: true, hasOut: true,
        shape: 'diamond',
        description: 'Run multiple branches at the same time, then merge results. Example: check Gmail AND Slack in parallel \u2192 combine into one summary.',
    },
    loop: {
        label: 'Loop',
        accent: '#AB8F67',
        icon: 'M17.65 6.35A8 8 0 1 0 20 12h-2a6 6 0 1 1-1.76-4.24L13 11h7V4z',
        group: 'control',
        hasIn: true, hasOut: true,
        description: 'Process items one by one in a loop. Example: for each unread email \u2192 summarize it \u2192 collect all summaries.',
    },
    subprocess: {
        label: 'Sub-workflow',
        accent: '#5C7AEA',
        icon: 'M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z',
        group: 'control',
        hasIn: true, hasOut: true,
        description: 'Reuse a saved automation as a step. Example: call your "Email Summarizer" automation inside a larger "Morning Digest" flow.',
    },
    approve: {
        label: 'Require Approval',
        accent: '#FF7043',
        icon: 'M12 1L3 5v6c0 5.55 3.84 10.74 9 12 5.16-1.26 9-6.45 9-12V5l-9-4zm-2 16l-4-4 1.41-1.41L10 14.17l6.59-6.59L18 9l-8 8z',
        group: 'control',
        hasIn: true, hasOut: true,
        shape: 'diamond',
        description: 'Pause and ask for user approval before continuing. Choose which channel to send the approval request.',
    },
    require_2fa: {
        label: 'Require 2FA',
        accent: '#AB47BC',
        icon: 'M18 8h-1V6c0-2.76-2.24-5-5-5S7 3.24 7 6v2H6c-1.1 0-2 .9-2 2v10c0 1.1.9 2 2 2h12c1.1 0 2-.9 2-2V10c0-1.1-.9-2-2-2zm-6 9c-1.1 0-2-.9-2-2s.9-2 2-2 2 .9 2 2-.9 2-2 2zm3.1-9H8.9V6c0-1.71 1.39-3.1 3.1-3.1s3.1 1.39 3.1 3.1v2z',
        group: 'control',
        hasIn: true, hasOut: true,
        shape: 'diamond',
        description: 'Require two-factor authentication verification before continuing. Adds an extra security layer for sensitive operations.',
    },
    deliver: {
        label: 'Deliver',
        accent: '#42A5F5',
        icon: 'M2 21l21-9L2 3v7l15 2-15 2v7z',
        group: 'output',
        hasIn: true, hasOut: false,
        description: 'Send the result to a channel (Telegram, CLI, Discord, Web, etc.)',
    },
};

// ─── Lazy API Data Caches ──────────────────────────────────────────────────────
// Fetched once when first needed by inspector dropdowns, then reused.
let _cachedTools = null;
let _cachedSkills = null;
// _cachedMcpServers removed — now uses McpLoader (DRY)
let _cachedTargets = null;

async function getCachedTools() {
    if (!_cachedTools) {
        try {
            const res = await apiRequest('/v1/tools');
            _cachedTools = { tools: res.tools || [], missing: res.missing || [] };
        } catch (_) { _cachedTools = { tools: [], missing: [] }; }
    }
    return _cachedTools;
}
async function getCachedSkills() {
    if (!_cachedSkills) {
        try { _cachedSkills = await apiRequest('/v1/skills'); } catch (_) { _cachedSkills = []; }
    }
    return _cachedSkills;
}
// getCachedMcpServers removed — now uses McpLoader (DRY)
async function getCachedTargets() {
    if (!_cachedTargets) {
        try { _cachedTargets = await apiRequest('/v1/automations/targets'); } catch (_) { _cachedTargets = []; }
    }
    return _cachedTargets;
}

// Cache for smart parameter overrides
let _cachedEmailAccounts = null;
let _cachedModels = null;

async function getCachedEmailAccounts() {
    if (_cachedEmailAccounts) return _cachedEmailAccounts;
    try {
        const resp = await apiRequest('/v1/email-accounts');
        // API returns { accounts: [...] } — extract the array
        const arr = Array.isArray(resp) ? resp : (resp && Array.isArray(resp.accounts) ? resp.accounts : null);
        if (arr && arr.length > 0) _cachedEmailAccounts = arr;
        return arr || [];
    } catch (_) { return []; } // Don't cache errors — retry next time
}
async function getCachedModels() {
    if (_cachedModels) return _cachedModels;
    try {
        const resp = await apiRequest('/v1/providers/models');
        // API returns { ok, models: [...], current, ... } — extract the array
        const arr = Array.isArray(resp) ? resp : (resp && Array.isArray(resp.models) ? resp.models : null);
        if (arr && arr.length > 0) _cachedModels = arr;
        return arr || [];
    } catch (_) { return []; } // Don't cache errors — retry next time
}

/**
 * Resolve smart parameter overrides for a tool.
 * Returns { paramName: [{ value, label }] } for params with known values.
 */
async function resolveParamOverrides(toolName) {
    const overrides = {};
    try {
        if (toolName === 'read_email_inbox') {
            const accounts = await getCachedEmailAccounts();
            if (Array.isArray(accounts) && accounts.length > 0) {
                overrides.account = accounts
                    .filter(a => a.enabled !== false && a.configured !== false)
                    .map(a => ({
                        value: a.name,
                        label: a.name + (a.username ? ' (' + a.username + ')' : ''),
                    }));
            }
        }
        if (toolName === 'message') {
            const targets = await getCachedTargets();
            if (Array.isArray(targets) && targets.length > 0) {
                overrides.channel = targets.map(t => ({ value: t.value, label: t.label }));
            }
        }
    } catch (_) { /* ignore, no overrides */ }
    return overrides;
}

const NODE_GROUPS = [
    { key: 'triggers',   label: 'Triggers' },
    { key: 'processing', label: 'Processing' },
    { key: 'control',    label: 'Control Flow' },
    { key: 'output',     label: 'Output' },
];

// ─── Automation Templates ────────────────────────────────────────────────────────────────────
const AUTOMATION_TEMPLATES = [
    {
        id: 'email-digest',
        icon: '\u{1F4EC}',
        name: 'Daily Email Digest',
        description: 'Check inbox every morning, summarize, and send digest',
        flow: {
            nodes: [
                { id: 'n1', kind: 'trigger', label: 'Every morning', meta: 'daily 08:00' },
                { id: 'n2', kind: 'tool', label: 'read_email_inbox' },
                { id: 'n3', kind: 'llm', label: 'Summarize new emails concisely' },
                { id: 'n4', kind: 'deliver', label: 'Send digest' },
            ],
            edges: [{ from: 'n1', to: 'n2' }, { from: 'n2', to: 'n3' }, { from: 'n3', to: 'n4' }],
        },
    },
    {
        id: 'web-monitor',
        icon: '\u{1F50D}',
        name: 'Web Monitor',
        description: 'Periodically check a website for changes and notify you',
        flow: {
            nodes: [
                { id: 'n1', kind: 'trigger', label: 'Every 6 hours', meta: 'every 6h' },
                { id: 'n2', kind: 'tool', label: 'web_fetch' },
                { id: 'n3', kind: 'llm', label: 'Analyze if content changed' },
                { id: 'n4', kind: 'condition', label: 'Has changes?' },
                { id: 'n5', kind: 'deliver', label: 'Notify changes' },
            ],
            edges: [
                { from: 'n1', to: 'n2' }, { from: 'n2', to: 'n3' },
                { from: 'n3', to: 'n4' }, { from: 'n4', to: 'n5' },
            ],
        },
    },
    {
        id: 'daily-standup',
        icon: '\u{1F4CB}',
        name: 'Daily Standup',
        description: 'Generate a daily standup update every weekday morning',
        flow: {
            nodes: [
                { id: 'n1', kind: 'trigger', label: 'Weekdays 9am', meta: 'daily 09:00' },
                { id: 'n2', kind: 'llm', label: 'Prepare daily standup summary based on recent activity' },
                { id: 'n3', kind: 'deliver', label: 'Send standup' },
            ],
            edges: [{ from: 'n1', to: 'n2' }, { from: 'n2', to: 'n3' }],
        },
    },
    {
        id: 'news-briefing',
        icon: '\u{1F4F0}',
        name: 'News Briefing',
        description: 'Search for news on a topic and get a morning summary',
        flow: {
            nodes: [
                { id: 'n1', kind: 'trigger', label: 'Every morning', meta: 'daily 07:00' },
                { id: 'n2', kind: 'tool', label: 'web_search' },
                { id: 'n3', kind: 'llm', label: 'Summarize the top news into a brief' },
                { id: 'n4', kind: 'deliver', label: 'Send briefing' },
            ],
            edges: [{ from: 'n1', to: 'n2' }, { from: 'n2', to: 'n3' }, { from: 'n3', to: 'n4' }],
        },
    },
    {
        id: 'security-check',
        icon: '\u{1F6E1}',
        name: 'Security Check',
        description: 'Run a nightly security audit and alert on issues',
        flow: {
            nodes: [
                { id: 'n1', kind: 'trigger', label: 'Every night', meta: 'daily 22:00' },
                { id: 'n2', kind: 'tool', label: 'shell' },
                { id: 'n3', kind: 'llm', label: 'Analyze logs for security issues' },
                { id: 'n4', kind: 'condition', label: 'Issues found?' },
                { id: 'n5', kind: 'deliver', label: 'Alert owner' },
            ],
            edges: [
                { from: 'n1', to: 'n2' }, { from: 'n2', to: 'n3' },
                { from: 'n3', to: 'n4' }, { from: 'n4', to: 'n5' },
            ],
        },
    },
    {
        id: 'file-organizer',
        icon: '\u{1F5C2}',
        name: 'File Organizer',
        description: 'Weekly scan of a folder to identify old or unused files',
        flow: {
            nodes: [
                { id: 'n1', kind: 'trigger', label: 'Monday 8am', meta: 'daily 08:00' },
                { id: 'n2', kind: 'tool', label: 'list_files' },
                { id: 'n3', kind: 'llm', label: 'Identify old files and suggest cleanup' },
                { id: 'n4', kind: 'deliver', label: 'Send report' },
            ],
            edges: [{ from: 'n1', to: 'n2' }, { from: 'n2', to: 'n3' }, { from: 'n3', to: 'n4' }],
        },
    },
];

// ─── Automations Builder (n8n Style) ────────────────────────────────────────────────────────
const Builder = {
    nodes: [],
    edges: [],
    nodeCounter: 0,
    selectedNodeId: null,
    draggedKind: null,
    isDraggingNode: false,
    draggedNodeId: null,
    dragOffset: { x: 0, y: 0 },

    // Connection state
    isConnecting: false,
    connectionStartNode: null,
    connectionStartType: null,
    tempEdgePath: null,

    // Edit mode — null = creating new, string = editing existing automation ID
    editingId: null,

    init() {
        this.canvas = document.getElementById('builder-canvas');
        this.nodesContainer = document.getElementById('builder-canvas-nodes');
        this.edgesContainer = document.getElementById('builder-canvas-edges');
        this.inspector = document.getElementById('builder-inspector');
        this.inspectorBody = document.getElementById('inspector-body');

        if (!this.canvas) return;

        this.buildPalette();
        this.setupCanvas();
        this.setupInspector();
        this.setupPromptBar();

        // Buttons
        document.getElementById('btn-create-automation')?.addEventListener('click', () => {
            document.getElementById('automations-list-view').style.display = 'none';
            document.getElementById('automations-builder-view').style.display = 'flex';
            this.reset();
        });

        document.getElementById('btn-builder-back')?.addEventListener('click', () => {
            document.getElementById('automations-builder-view').style.display = 'none';
            document.getElementById('automations-list-view').style.display = '';
        });

        document.getElementById('btn-builder-save')?.addEventListener('click', () => this.save());
    },

    // ── Palette ──────────────────────────────────────────────────

    buildPalette() {
        const container = document.getElementById('builder-palette-items');
        if (!container) return;
        container.textContent = '';

        NODE_GROUPS.forEach(group => {
            const kinds = Object.entries(NODE_KINDS).filter(([, cfg]) => cfg.group === group.key);
            if (kinds.length === 0) return;

            // Group header
            const header = document.createElement('div');
            header.className = 'builder-palette-group-label';
            header.textContent = group.label;
            container.appendChild(header);

            // Items — built with safe DOM methods, no innerHTML with user data
            kinds.forEach(([kind, cfg]) => {
                const el = document.createElement('div');
                el.className = 'builder-node-drag';
                el.draggable = true;
                el.dataset.kind = kind;

                const iconWrap = document.createElement('div');
                iconWrap.className = 'builder-palette-icon';
                iconWrap.style.background = cfg.accent;
                // SVG icon path is a static constant from NODE_KINDS, safe for innerHTML
                const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
                svg.setAttribute('viewBox', '0 0 24 24');
                svg.setAttribute('width', '14');
                svg.setAttribute('height', '14');
                svg.setAttribute('fill', 'currentColor');
                const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
                path.setAttribute('d', cfg.icon);
                svg.appendChild(path);
                iconWrap.appendChild(svg);

                const label = document.createElement('span');
                label.className = 'builder-palette-label';
                label.textContent = cfg.label;

                el.appendChild(iconWrap);
                el.appendChild(label);

                // Tooltip description (shown on click, hidden on drag/second-click)
                if (cfg.description) {
                    const tip = document.createElement('div');
                    tip.className = 'palette-tooltip';
                    tip.textContent = cfg.description;
                    tip.style.display = 'none';
                    el.appendChild(tip);

                    el.addEventListener('click', () => {
                        // Toggle tooltip; close any other open tooltips first
                        container.querySelectorAll('.palette-tooltip').forEach(t => {
                            if (t !== tip) t.style.display = 'none';
                        });
                        tip.style.display = tip.style.display === 'none' ? 'block' : 'none';
                    });
                }

                el.addEventListener('dragstart', e => {
                    this.draggedKind = kind;
                    e.dataTransfer.setData('text/plain', kind);
                    e.dataTransfer.effectAllowed = 'copy';
                    // Hide tooltip when dragging
                    const tip = el.querySelector('.palette-tooltip');
                    if (tip) tip.style.display = 'none';
                });
                container.appendChild(el);
            });
        });
    },

    // ── Canvas ───────────────────────────────────────────────────

    setupCanvas() {
        this.canvas.addEventListener('dragover', e => {
            e.preventDefault();
            e.dataTransfer.dropEffect = 'copy';
        });

        this.canvas.addEventListener('drop', e => {
            e.preventDefault();
            const kind = this.draggedKind || e.dataTransfer.getData('text/plain');
            if (kind && NODE_KINDS[kind]) {
                const rect = this.canvas.getBoundingClientRect();
                this.addNode(kind, e.clientX - rect.left, e.clientY - rect.top);
            }
            this.draggedKind = null;
        });

        // Node dragging + connection drawing
        this.canvas.addEventListener('mousemove', e => {
            if (this.isDraggingNode && this.draggedNodeId) {
                const rect = this.canvas.getBoundingClientRect();
                const node = this.nodes.find(n => n.id === this.draggedNodeId);
                if (node) {
                    node.x = e.clientX - rect.left - this.dragOffset.x;
                    node.y = e.clientY - rect.top - this.dragOffset.y;
                    this.renderNodes();
                    this.renderEdges();
                }
            }
            if (this.isConnecting && this.tempEdgePath) {
                const rect = this.canvas.getBoundingClientRect();
                const startNode = this.nodes.find(n => n.id === this.connectionStartNode);
                if (startNode) {
                    const x1 = startNode.x + 180;
                    const y1 = startNode.y + 36;
                    const x2 = e.clientX - rect.left;
                    const y2 = e.clientY - rect.top;
                    this.tempEdgePath.setAttribute('d', this.bezierPath(x1, y1, x2, y2));
                }
            }
        });

        this.canvas.addEventListener('mouseup', () => {
            this.isDraggingNode = false;
            this.draggedNodeId = null;
            if (this.isConnecting) {
                this.isConnecting = false;
                if (this.tempEdgePath) { this.tempEdgePath.remove(); this.tempEdgePath = null; }
                this.connectionStartNode = null;
            }
        });

        this.canvas.addEventListener('click', e => {
            if (e.target === this.canvas || e.target.tagName === 'svg') {
                this.selectedNodeId = null;
                this.renderNodes();
                this.hideInspector();
            }
        });
    },

    // ── Inspector ────────────────────────────────────────────────

    setupInspector() {
        document.getElementById('btn-inspector-close')?.addEventListener('click', () => {
            this.hideInspector();
        });
        // Handle both input (text/textarea) and change (select/checkbox) events
        const handleFieldChange = (e) => {
            if (!this.selectedNodeId) return;
            const node = this.nodes.find(n => n.id === this.selectedNodeId);
            if (!node) return;
            const field = e.target.dataset.field;
            if (!field) return;

            if (field.startsWith('arg__')) {
                // Schema form field — store in node.data.arguments object
                const paramName = field.substring(5);
                if (typeof node.data.arguments !== 'object' || node.data.arguments === null) {
                    try { node.data.arguments = JSON.parse(node.data.arguments || '{}'); }
                    catch (_) { node.data.arguments = {}; }
                }
                const inputType = e.target.type;
                if (inputType === 'checkbox') {
                    node.data.arguments[paramName] = e.target.checked;
                } else if (inputType === 'number') {
                    node.data.arguments[paramName] = e.target.value === '' ? '' : Number(e.target.value);
                } else {
                    node.data.arguments[paramName] = e.target.value;
                }
            } else {
                node.data[field] = e.target.value;
            }
            // Validate field + node after change (AUTO-2)
            if (window.AutoValidate) {
                const rule = window.AutoValidate.fieldRule(node.kind, field);
                if (rule) window.AutoValidate.validateField(e.target, rule);
                window.AutoValidate.validateNode(node);
            }
            this.renderNodes();
        };
        this.inspectorBody.addEventListener('input', handleFieldChange);
        this.inspectorBody.addEventListener('change', handleFieldChange);

        // Validate on blur for immediate feedback when leaving a field (AUTO-2)
        this.inspectorBody.addEventListener('blur', (e) => {
            if (!e.target.dataset || !e.target.dataset.field) return;
            // Use selectedNodeId if still set, otherwise skip inline validation
            // (node badge is handled proactively by renderNodes)
            const nodeId = this.selectedNodeId;
            if (!nodeId) return;
            const node = this.nodes.find(n => n.id === nodeId);
            if (!node || !window.AutoValidate) return;
            const field = e.target.dataset.field;
            const rule = window.AutoValidate.fieldRule(node.kind, field);
            if (rule) window.AutoValidate.validateField(e.target, rule);
            window.AutoValidate.validateNode(node);
            this.renderNodes();
        }, true); // useCapture: blur does not bubble
    },

    // ── Prompt Bar ───────────────────────────────────────────────

    setupPromptBar() {
        const input = document.getElementById('builder-prompt-input');
        const btn = document.getElementById('btn-builder-generate');
        if (!input || !btn) return;

        btn.addEventListener('click', () => this.generateFromPrompt());
        input.addEventListener('keydown', e => {
            if (e.key === 'Enter' && !e.shiftKey) {
                e.preventDefault();
                this.generateFromPrompt();
            }
        });
        // Auto-grow textarea
        input.addEventListener('input', () => {
            input.style.height = 'auto';
            input.style.height = Math.min(input.scrollHeight, 120) + 'px';
        });
    },

    async generateFromPrompt() {
        const input = document.getElementById('builder-prompt-input');
        const status = document.getElementById('builder-prompt-status');
        const desc = input?.value.trim();
        if (!desc) return;

        status.textContent = 'Generating flow...';
        try {
            const res = await apiRequest('/v1/automations/generate-flow', {
                method: 'POST',
                body: JSON.stringify({ description: desc }),
            });
            if (res && res.flow) {
                this.loadFlowData(res.flow, res.name);
                input.value = '';
                input.style.height = 'auto';
                showToast('Flow generated! Review and save.', 'success');
            }
        } catch (err) {
            showToast(err.message || 'Failed to generate flow.', 'error');
        } finally {
            status.textContent = '';
        }
    },

    loadFlowData(flowData, name) {
        this.reset();
        if (name) document.getElementById('builder-automation-name').value = name;

        // Position nodes in a row with spacing
        const startX = 80;
        const startY = 120;
        const spacingX = 220;

        (flowData.nodes || []).forEach((n, i) => {
            this.nodeCounter++;
            const cfg = NODE_KINDS[n.kind] || NODE_KINDS.llm;
            this.nodes.push({
                id: n.id || ('node_' + this.nodeCounter),
                kind: n.kind || 'llm',
                x: startX + i * spacingX,
                y: startY,
                title: n.label || cfg.label,
                data: this.flowNodeToData(n),
            });
        });

        this.edges = (flowData.edges || []).map(e => ({
            id: 'edge_' + e.from + '_' + e.to,
            from: e.from,
            to: e.to,
        }));

        this.render();
    },

    flowNodeToData(flowNode) {
        const base = this.defaultDataForKind(flowNode.kind);
        // Populate data from flow node label/meta
        switch (flowNode.kind) {
            case 'trigger':
                if (flowNode.meta) base.time = flowNode.meta;
                break;
            case 'llm':
                if (flowNode.label) base.prompt = flowNode.label;
                break;
            case 'tool':
                if (flowNode.label) base.tool_name = flowNode.label;
                break;
            case 'skill':
                if (flowNode.label) base.skill_name = flowNode.label;
                break;
            case 'mcp':
                if (flowNode.label) base.server = flowNode.label;
                if (flowNode.meta) base.tool = flowNode.meta;
                break;
            case 'condition':
                if (flowNode.label) base.expression = flowNode.label;
                break;
            case 'deliver':
                if (flowNode.meta) base.target = flowNode.meta;
                else if (flowNode.label) base.target = flowNode.label;
                break;
            case 'transform':
                if (flowNode.label) base.template = flowNode.label;
                break;
            case 'loop':
                if (flowNode.meta) base.condition = flowNode.meta;
                break;
            case 'subprocess':
                if (flowNode.label) base.workflow_ref = flowNode.label;
                break;
            case 'approve':
                if (flowNode.label) base.approve_message = flowNode.label;
                if (flowNode.meta) base.approve_channel = flowNode.meta;
                break;
        }
        return base;
    },

    // ── Node CRUD ────────────────────────────────────────────────

    reset() {
        this.nodes = [];
        this.edges = [];
        this.nodeCounter = 0;
        this.selectedNodeId = null;
        this.editingId = null;
        document.getElementById('builder-automation-name').value = '';
        this.render();
        this.hideInspector();
    },

    /// Load an existing automation into the Builder for editing.
    /// Always reconstructs from actual data (prompt, schedule, deliver_to) —
    /// flow_json is only used for visual layout hints, not as data source.
    loadAutomation(item) {
        this.reset();
        this.editingId = item.id;
        document.getElementById('builder-automation-name').value = item.name || '';

        const startX = 80, startY = 120, spacingX = 220;

        // 1. Trigger node from schedule
        const ui = scheduleToUi(item.schedule || 'cron:0 9 * * *');
        const triggerData = Object.assign({}, this.defaultDataForKind('trigger'));
        if (ui.mode === 'interval') {
            triggerData.mode = 'interval';
            triggerData.intervalHours = Number(ui.intervalHours) || 6;
        } else if (ui.mode === 'custom') {
            triggerData.mode = 'cron';
        } else {
            triggerData.mode = ui.mode || 'daily';
            triggerData.time = ui.time || '09:00';
            if (ui.mode === 'weekly') triggerData.weekday = ui.weekday;
        }
        this.nodeCounter++;
        this.nodes.push({
            id: 'trigger', kind: 'trigger',
            x: startX, y: startY,
            title: 'Schedule Trigger',
            data: triggerData,
        });

        // 2. Middle nodes — from workflow steps or single LLM prompt
        const steps = parseJsonArray(item.workflow_steps_json);
        let lastId = 'trigger';
        if (steps.length > 0) {
            steps.forEach((step, i) => {
                this.nodeCounter++;
                const nodeId = 'node_' + this.nodeCounter;
                this.nodes.push({
                    id: nodeId, kind: step.approval_required ? 'approve' : 'llm',
                    x: startX + (i + 1) * spacingX, y: startY,
                    title: step.name || ('Step ' + (i + 1)),
                    data: { prompt: step.instruction || '' },
                });
                this.edges.push({ id: 'edge_' + lastId + '_' + nodeId, from: lastId, to: nodeId });
                lastId = nodeId;
            });
        } else {
            this.nodeCounter++;
            const taskId = 'node_' + this.nodeCounter;
            this.nodes.push({
                id: taskId, kind: 'llm',
                x: startX + spacingX, y: startY,
                title: 'LLM Task',
                data: { prompt: item.prompt || '' },
            });
            this.edges.push({ id: 'edge_trigger_' + taskId, from: 'trigger', to: taskId });
            lastId = taskId;
        }

        // 3. Deliver node
        this.nodeCounter++;
        const deliverId = 'node_' + this.nodeCounter;
        this.nodes.push({
            id: deliverId, kind: 'deliver',
            x: startX + (this.nodes.length) * spacingX, y: startY,
            title: 'Deliver',
            data: { target: item.deliver_to || 'cli:default' },
        });
        this.edges.push({ id: 'edge_' + lastId + '_' + deliverId, from: lastId, to: deliverId });

        this.render();
    },

    addNode(kind, x, y) {
        const cfg = NODE_KINDS[kind];
        if (!cfg) return;
        this.nodeCounter++;
        const node = {
            id: 'node_' + this.nodeCounter,
            kind,
            x, y,
            title: cfg.label,
            data: this.defaultDataForKind(kind),
        };
        this.nodes.push(node);
        this.selectNode(node.id);
        this.render();
    },

    defaultDataForKind(kind) {
        switch (kind) {
            case 'trigger':    return {
                mode: 'daily', time: '09:00',
                intervalHours: 6, weekdays: ['mon','tue','wed','thu','fri'],
                cronMinute: '0', cronHour: '9', cronDom: '*', cronMonth: '*', cronDow: '*',
            };
            case 'tool':       return { tool_name: '', arguments: {} };
            case 'skill':      return { skill_name: '' };
            case 'mcp':        return { server: '', tool: '', arguments: {} };
            case 'llm':        return { prompt: '', model: '' };
            case 'condition':  return { expression: '', true_label: 'Yes', false_label: 'No' };
            case 'parallel':   return { branches: 2 };
            case 'loop':       return { max_iterations: 10, condition: '' };
            case 'subprocess': return { workflow_ref: '' };
            case 'transform':  return { template: '' };
            case 'approve':    return { approve_channel: '', approve_message: '' };
            case 'require_2fa': return {};
            case 'deliver':    return { target: 'cli:default' };
            default:           return {};
        }
    },

    selectNode(id) {
        this.selectedNodeId = id;
        this.renderNodes();
        this.showInspector(id);
    },

    removeNode(id) {
        this.nodes = this.nodes.filter(n => n.id !== id);
        this.edges = this.edges.filter(e => e.from !== id && e.to !== id);
        if (this.selectedNodeId === id) {
            this.selectedNodeId = null;
            this.hideInspector();
        }
        this.render();
    },

    // ── Connections ──────────────────────────────────────────────

    startConnection(nodeId, type, e) {
        e.stopPropagation();
        this.isConnecting = true;
        this.connectionStartNode = nodeId;
        this.connectionStartType = type;
        this.tempEdgePath = document.createElementNS('http://www.w3.org/2000/svg', 'path');
        this.tempEdgePath.setAttribute('class', 'builder-edge-path');
        this.tempEdgePath.style.pointerEvents = 'none';
        this.edgesContainer.appendChild(this.tempEdgePath);
    },

    finishConnection(nodeId, type, e) {
        e.stopPropagation();
        if (this.isConnecting && this.connectionStartNode !== nodeId && this.connectionStartType !== type) {
            const from = this.connectionStartType === 'out' ? this.connectionStartNode : nodeId;
            const to = this.connectionStartType === 'in' ? this.connectionStartNode : nodeId;
            if (!this.edges.find(e => e.from === from && e.to === to)) {
                this.edges.push({ id: 'edge_' + from + '_' + to, from, to });
                this.renderEdges();
            }
        }
        this.isConnecting = false;
        if (this.tempEdgePath) { this.tempEdgePath.remove(); this.tempEdgePath = null; }
    },

    bezierPath(x1, y1, x2, y2) {
        const cx = (x1 + x2) / 2;
        return 'M ' + x1 + ' ' + y1 + ' C ' + cx + ' ' + y1 + ' ' + cx + ' ' + y2 + ' ' + x2 + ' ' + y2;
    },

    // ── Rendering ────────────────────────────────────────────────

    render() {
        this.renderNodes();
        this.renderEdges();
        this.renderTemplates();
    },

    renderTemplates() {
        let container = document.getElementById('builder-templates');
        if (!container) {
            // Create template container above the canvas
            container = document.createElement('div');
            container.id = 'builder-templates';
            const canvasEl = this.canvas;
            if (canvasEl && canvasEl.parentNode) {
                canvasEl.parentNode.insertBefore(container, canvasEl);
            }
        }
        // Show templates only when canvas is empty
        if (this.nodes.length > 0) {
            container.style.display = 'none';
            return;
        }
        container.style.display = '';
        container.textContent = '';

        const heading = document.createElement('h3');
        heading.className = 'template-heading';
        heading.textContent = 'Start from a template';
        container.appendChild(heading);

        const grid = document.createElement('div');
        grid.className = 'template-grid';

        AUTOMATION_TEMPLATES.forEach(tmpl => {
            const card = document.createElement('div');
            card.className = 'template-card';
            card.addEventListener('click', () => {
                this.loadFlowData(tmpl.flow, tmpl.name);
            });

            const icon = document.createElement('div');
            icon.className = 'template-card-icon';
            icon.textContent = tmpl.icon;
            card.appendChild(icon);

            const name = document.createElement('div');
            name.className = 'template-card-name';
            name.textContent = tmpl.name;
            card.appendChild(name);

            const desc = document.createElement('div');
            desc.className = 'template-card-desc';
            desc.textContent = tmpl.description;
            card.appendChild(desc);

            grid.appendChild(card);
        });

        container.appendChild(grid);

        const orHint = document.createElement('p');
        orHint.className = 'template-or-hint';
        orHint.textContent = 'Or drag nodes from the palette to build from scratch';
        container.appendChild(orHint);
    },

    nodeDescription(node) {
        const d = node.data;
        switch (node.kind) {
            case 'trigger': {
                if (d.mode === 'cron') return 'cron ' + [d.cronMinute||'0', d.cronHour||'9', d.cronDom||'*', d.cronMonth||'*', d.cronDow||'*'].join(' ');
                if (d.mode === 'interval') return 'every ' + (d.intervalHours || 6) + 'h';
                return 'daily at ' + (d.time || '09:00');
            }
            case 'tool':       return d.tool_name || 'Select tool...';
            case 'skill':      return d.skill_name || 'Select skill...';
            case 'mcp':        return d.server ? (d.server + (d.tool ? ' \u2192 ' + d.tool : '')) : 'Configure MCP...';
            case 'llm':        return d.prompt ? shorten(d.prompt, 25) : 'Configure prompt...';
            case 'condition':  return d.expression ? shorten(d.expression, 25) : 'Set condition...';
            case 'parallel':   return (d.branches || 2) + ' branches in parallel';
            case 'loop':       return d.condition ? shorten(d.condition, 20) : ('Max ' + (d.max_iterations || 10) + ' iterations');
            case 'subprocess': return d.workflow_ref || 'Select workflow...';
            case 'transform':  return d.template ? shorten(d.template, 25) : 'Configure transform...';
            case 'approve':    return d.approve_channel ? ('via ' + d.approve_channel) : 'Configure approval...';
            case 'require_2fa': return '2FA verification gate';
            case 'deliver':    return d.target || 'cli:default';
            default:           return '';
        }
    },

    renderNodes() {
        this.nodesContainer.textContent = '';
        this.nodes.forEach(node => {
            const cfg = NODE_KINDS[node.kind] || NODE_KINDS.llm;
            const el = document.createElement('div');
            el.className = 'builder-node' + (this.selectedNodeId === node.id ? ' selected' : '');
            if (cfg.shape) el.classList.add('builder-node--' + cfg.shape);
            el.style.transform = 'translate(' + node.x + 'px, ' + node.y + 'px)';

            const desc = this.nodeDescription(node);

            // Build node DOM safely
            const headerDiv = document.createElement('div');
            headerDiv.className = 'builder-node-header';

            const iconDiv = document.createElement('div');
            iconDiv.className = 'builder-node-icon';
            iconDiv.style.background = cfg.accent;
            const iconSvg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
            iconSvg.setAttribute('viewBox', '0 0 24 24');
            iconSvg.setAttribute('width', '14');
            iconSvg.setAttribute('height', '14');
            iconSvg.setAttribute('fill', 'currentColor');
            const iconPath = document.createElementNS('http://www.w3.org/2000/svg', 'path');
            iconPath.setAttribute('d', cfg.icon);
            iconSvg.appendChild(iconPath);
            iconDiv.appendChild(iconSvg);

            const titleDiv = document.createElement('div');
            titleDiv.className = 'builder-node-title';
            titleDiv.textContent = node.title;

            headerDiv.appendChild(iconDiv);
            headerDiv.appendChild(titleDiv);

            const bodyDiv = document.createElement('div');
            bodyDiv.className = 'builder-node-body';
            bodyDiv.textContent = desc;

            el.appendChild(headerDiv);
            el.appendChild(bodyDiv);

            // Validate node and show error badge on canvas (AUTO-2)
            if (window.AutoValidate) window.AutoValidate.validateNode(node);
            if (node._errors && node._errors.length > 0) {
                el.classList.add('builder-node--error');
                const badge = document.createElement('div');
                badge.className = 'builder-node-error-badge';
                badge.textContent = node._errors.length;
                badge.title = node._errors.join('\n');
                el.appendChild(badge);
            }

            // Connection handles
            if (cfg.hasIn) {
                const hIn = document.createElement('div');
                hIn.className = 'builder-handle builder-handle-in';
                hIn.dataset.type = 'in';
                hIn.addEventListener('mousedown', e => this.startConnection(node.id, 'in', e));
                hIn.addEventListener('mouseup', e => this.finishConnection(node.id, 'in', e));
                el.appendChild(hIn);
            }
            if (cfg.hasOut) {
                const hOut = document.createElement('div');
                hOut.className = 'builder-handle builder-handle-out';
                hOut.dataset.type = 'out';
                hOut.addEventListener('mousedown', e => this.startConnection(node.id, 'out', e));
                hOut.addEventListener('mouseup', e => this.finishConnection(node.id, 'out', e));
                el.appendChild(hOut);
            }

            // Node drag
            el.addEventListener('mousedown', e => {
                if (e.target.classList.contains('builder-handle')) return;
                this.isDraggingNode = true;
                this.draggedNodeId = node.id;
                const rect = this.canvas.getBoundingClientRect();
                this.dragOffset = {
                    x: e.clientX - rect.left - node.x,
                    y: e.clientY - rect.top - node.y,
                };
                this.selectNode(node.id);
            });

            this.nodesContainer.appendChild(el);
        });
    },

    renderEdges() {
        this.edgesContainer.textContent = '';
        this.edges.forEach(edge => {
            const fromNode = this.nodes.find(n => n.id === edge.from);
            const toNode = this.nodes.find(n => n.id === edge.to);
            if (!fromNode || !toNode) return;

            const x1 = fromNode.x + 180;
            const y1 = fromNode.y + 36;
            const x2 = toNode.x;
            const y2 = toNode.y + 36;

            const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
            path.setAttribute('d', this.bezierPath(x1, y1, x2, y2));
            path.setAttribute('class', 'builder-edge-path');
            path.addEventListener('dblclick', () => {
                this.edges = this.edges.filter(e => e.id !== edge.id);
                this.renderEdges();
            });
            this.edgesContainer.appendChild(path);
        });
    },

    // ── Inspector ────────────────────────────────────────────────

    showInspector(id) {
        const node = this.nodes.find(n => n.id === id);
        if (!node) return;

        this.inspector.style.display = 'flex';
        document.getElementById('inspector-title').textContent = node.title;

        // Build inspector DOM safely
        this.inspectorBody.textContent = '';

        // Delete button row
        const delRow = document.createElement('div');
        delRow.style.cssText = 'display:flex;justify-content:flex-end';
        const delBtn = document.createElement('button');
        delBtn.className = 'btn btn-danger btn-sm';
        delBtn.textContent = 'Delete Node';
        delBtn.addEventListener('click', () => this.removeNode(id));
        delRow.appendChild(delBtn);
        this.inspectorBody.appendChild(delRow);

        // Kind-specific fields
        this.appendInspectorFields(node);
    },

    // Unique render ID to prevent stale async fills from populating the wrong inspector
    _inspectorRenderId: 0,

    appendInspectorFields(node) {
        const d = node.data;
        const body = this.inspectorBody;
        const renderId = ++this._inspectorRenderId;

        // ── Helpers ─────────────────────────────────────────────
        const addField = (labelText, fieldName, type, opts) => {
            const group = document.createElement('div');
            group.className = 'form-group';
            if (opts.containerAttr) {
                for (const [k, v] of Object.entries(opts.containerAttr)) group.setAttribute(k, v);
            }

            const lbl = document.createElement('label');
            lbl.textContent = labelText;
            group.appendChild(lbl);

            if (type === 'select') {
                const sel = document.createElement('select');
                sel.className = 'input';
                sel.dataset.field = fieldName;
                (opts.options || []).forEach(o => {
                    const opt = document.createElement('option');
                    opt.value = o.value;
                    opt.textContent = o.label;
                    if (d[fieldName] === o.value) opt.selected = true;
                    sel.appendChild(opt);
                });
                group.appendChild(sel);
                if (opts.ref) opts.ref.el = sel;
            } else if (type === 'textarea') {
                const ta = document.createElement('textarea');
                ta.className = 'input';
                ta.rows = opts.rows || 3;
                ta.dataset.field = fieldName;
                ta.placeholder = opts.placeholder || '';
                ta.value = d[fieldName] || '';
                group.appendChild(ta);
            } else if (type === 'number') {
                const inp = document.createElement('input');
                inp.type = 'number';
                inp.className = 'input';
                inp.dataset.field = fieldName;
                inp.value = d[fieldName] || (opts.defaultVal || '');
                if (opts.min !== undefined) inp.min = opts.min;
                if (opts.max !== undefined) inp.max = opts.max;
                group.appendChild(inp);
            } else if (type === 'time') {
                const inp = document.createElement('input');
                inp.type = 'time';
                inp.className = 'input';
                inp.dataset.field = fieldName;
                inp.value = d[fieldName] || (opts.defaultVal || '09:00');
                group.appendChild(inp);
            } else {
                const inp = document.createElement('input');
                inp.type = 'text';
                inp.className = 'input';
                inp.dataset.field = fieldName;
                inp.value = d[fieldName] || '';
                inp.placeholder = opts.placeholder || '';
                if (opts.list) inp.setAttribute('list', opts.list);
                group.appendChild(inp);
            }

            body.appendChild(group);
            return group;
        };

        const addHint = (text) => {
            const hint = document.createElement('p');
            hint.className = 'form-hint';
            hint.textContent = text;
            body.appendChild(hint);
            return hint;
        };

        const addLink = (text, href) => {
            const a = document.createElement('a');
            a.className = 'inspector-link';
            a.textContent = text;
            // Navigate to the actual page route (multi-page app, not SPA)
            a.href = href || '#';
            body.appendChild(a);
        };

        // Add clickable preset buttons that populate a target field
        const addPresetButtons = (presets, targetFieldName) => {
            const row = document.createElement('div');
            row.className = 'preset-buttons';
            presets.forEach(p => {
                const btn = document.createElement('button');
                btn.type = 'button';
                btn.className = 'preset-btn';
                btn.textContent = p.label;
                btn.addEventListener('click', () => {
                    const target = body.querySelector('[data-field="' + targetFieldName + '"]');
                    if (target) {
                        target.value = p.value;
                        target.dispatchEvent(new Event('input', { bubbles: true }));
                    }
                });
                row.appendChild(btn);
            });
            body.appendChild(row);
        };

        // Async-populate a <select> — checks renderId to avoid stale fills
        const asyncPopulateSelect = (sel, fetchFn, mapFn, emptyMsg) => {
            const placeholder = document.createElement('option');
            placeholder.value = '';
            placeholder.textContent = 'Loading...';
            placeholder.disabled = true;
            sel.appendChild(placeholder);

            fetchFn().then(data => {
                if (this._inspectorRenderId !== renderId) return; // stale
                sel.textContent = '';
                const empty = document.createElement('option');
                empty.value = '';
                empty.textContent = emptyMsg || '-- Select --';
                sel.appendChild(empty);
                const items = mapFn(data);
                if (items.length === 0) {
                    empty.textContent = emptyMsg || 'None available';
                }
                items.forEach(item => {
                    const opt = document.createElement('option');
                    opt.value = item.value;
                    opt.textContent = item.label;
                    if (d[sel.dataset.field] === item.value) opt.selected = true;
                    sel.appendChild(opt);
                });
            });
        };

        // ── Kind-specific forms ─────────────────────────────────

        // Node description at top of inspector
        const kindCfg = NODE_KINDS[node.kind];
        if (kindCfg && kindCfg.description) {
            addHint(kindCfg.description);
        }

        switch (node.kind) {
            // ─── TRIGGER ────────────────────────────────────────
            case 'trigger': {
                const modeRef = {};
                addField('Schedule Mode', 'mode', 'select', { options: [
                    { value: 'daily', label: 'Every day' },
                    { value: 'interval', label: 'Interval (every N hours)' },
                    { value: 'cron', label: 'Cron expression' },
                ], ref: modeRef });

                // Daily fields
                const dailyGroup = addField('Time', 'time', 'time', {
                    defaultVal: '09:00',
                    containerAttr: { 'data-trigger-mode': 'daily' },
                });

                // Interval fields
                const intGroup = document.createElement('div');
                intGroup.className = 'form-group';
                intGroup.setAttribute('data-trigger-mode', 'interval');
                const intLbl = document.createElement('label');
                intLbl.textContent = 'Every (hours)';
                intGroup.appendChild(intLbl);
                const intInp = document.createElement('input');
                intInp.type = 'number';
                intInp.className = 'input';
                intInp.dataset.field = 'intervalHours';
                intInp.value = d.intervalHours || 6;
                intInp.min = 1;
                intInp.max = 168;
                intGroup.appendChild(intInp);
                body.appendChild(intGroup);

                // Weekday checkboxes for interval
                const wdGroup = document.createElement('div');
                wdGroup.className = 'form-group';
                wdGroup.setAttribute('data-trigger-mode', 'interval');
                const wdLbl = document.createElement('label');
                wdLbl.textContent = 'Active Days';
                wdGroup.appendChild(wdLbl);
                const wdRow = document.createElement('div');
                wdRow.className = 'weekday-row';
                const days = [
                    { key: 'mon', label: 'M' }, { key: 'tue', label: 'T' },
                    { key: 'wed', label: 'W' }, { key: 'thu', label: 'T' },
                    { key: 'fri', label: 'F' }, { key: 'sat', label: 'S' },
                    { key: 'sun', label: 'S' },
                ];
                const selectedDays = d.weekdays || ['mon','tue','wed','thu','fri'];
                days.forEach(day => {
                    const btn = document.createElement('button');
                    btn.type = 'button';
                    btn.className = 'weekday-btn' + (selectedDays.includes(day.key) ? ' active' : '');
                    btn.textContent = day.label;
                    btn.title = day.key;
                    btn.addEventListener('click', () => {
                        btn.classList.toggle('active');
                        // Update node data
                        const active = Array.from(wdRow.querySelectorAll('.weekday-btn.active'))
                            .map(b => b.title);
                        node.data.weekdays = active;
                    });
                    wdRow.appendChild(btn);
                });
                wdGroup.appendChild(wdRow);
                body.appendChild(wdGroup);

                // Cron fields
                const cronGroup = document.createElement('div');
                cronGroup.className = 'form-group';
                cronGroup.setAttribute('data-trigger-mode', 'cron');
                const cronLbl = document.createElement('label');
                cronLbl.textContent = 'Cron Expression';
                cronGroup.appendChild(cronLbl);
                const cronRow = document.createElement('div');
                cronRow.className = 'cron-fields';
                const cronParts = ['Minute', 'Hour', 'Day', 'Month', 'Weekday'];
                const cronKeys = ['cronMinute', 'cronHour', 'cronDom', 'cronMonth', 'cronDow'];
                const cronDefaults = ['0', '9', '*', '*', '*'];
                cronParts.forEach((part, i) => {
                    const wrap = document.createElement('div');
                    wrap.className = 'cron-field';
                    const fieldLbl = document.createElement('span');
                    fieldLbl.className = 'cron-field-label';
                    fieldLbl.textContent = part;
                    wrap.appendChild(fieldLbl);
                    const inp = document.createElement('input');
                    inp.type = 'text';
                    inp.className = 'input cron-input';
                    inp.dataset.field = cronKeys[i];
                    inp.value = d[cronKeys[i]] || cronDefaults[i];
                    inp.placeholder = cronDefaults[i];
                    wrap.appendChild(inp);
                    cronRow.appendChild(wrap);
                });
                cronGroup.appendChild(cronRow);

                // Cron presets
                const presetRow = document.createElement('div');
                presetRow.className = 'cron-presets';
                const presets = [
                    { label: 'Every morning', values: ['0', '9', '*', '*', '*'] },
                    { label: 'Weekdays 9am', values: ['0', '9', '*', '*', '1-5'] },
                    { label: 'Every hour', values: ['0', '*', '*', '*', '*'] },
                    { label: 'Monday 8am', values: ['0', '8', '*', '*', '1'] },
                ];
                presets.forEach(p => {
                    const btn = document.createElement('button');
                    btn.type = 'button';
                    btn.className = 'btn btn-secondary btn-xs';
                    btn.textContent = p.label;
                    btn.addEventListener('click', () => {
                        const inputs = cronRow.querySelectorAll('.cron-input');
                        p.values.forEach((v, i) => {
                            inputs[i].value = v;
                            node.data[cronKeys[i]] = v;
                        });
                    });
                    presetRow.appendChild(btn);
                });
                cronGroup.appendChild(presetRow);
                body.appendChild(cronGroup);

                // Show/hide conditional fields based on mode
                const updateTriggerVisibility = () => {
                    const mode = d.mode || 'daily';
                    body.querySelectorAll('[data-trigger-mode]').forEach(el => {
                        el.style.display = el.getAttribute('data-trigger-mode') === mode ? '' : 'none';
                    });
                };
                updateTriggerVisibility();
                if (modeRef.el) {
                    modeRef.el.addEventListener('change', () => {
                        d.mode = modeRef.el.value;
                        updateTriggerVisibility();
                    });
                }
                break;
            }

            // ─── TOOL ───────────────────────────────────────────
            case 'tool': {
                const toolRef = {};
                addField('Tool', 'tool_name', 'select', { options: [], ref: toolRef });

                // Container for dynamic schema fields (replaces JSON textarea)
                const toolSchemaContainer = document.createElement('div');
                toolSchemaContainer.className = 'schema-fields-container';
                body.appendChild(toolSchemaContainer);

                const renderToolSchema = async (toolName) => {
                    toolSchemaContainer.textContent = '';
                    if (!toolName) return;
                    const [data, overrides] = await Promise.all([
                        getCachedTools(),
                        resolveParamOverrides(toolName),
                    ]);
                    if (this._inspectorRenderId !== renderId) return;
                    const tool = data.tools.find(t => t.name === toolName);
                    const currentArgs = window.SchemaForm.parseArguments(d.arguments);
                    if (currentArgs === null || !tool || !tool.parameters || !tool.parameters.properties) {
                        // Fallback: raw JSON textarea
                        const group = document.createElement('div');
                        group.className = 'form-group';
                        const lbl = document.createElement('label');
                        lbl.textContent = 'Arguments (JSON)';
                        group.appendChild(lbl);
                        const ta = document.createElement('textarea');
                        ta.className = 'input';
                        ta.rows = 3;
                        ta.dataset.field = 'arguments';
                        ta.placeholder = '{"key": "value"}';
                        ta.value = typeof d.arguments === 'string'
                            ? d.arguments
                            : JSON.stringify(d.arguments || {}, null, 2);
                        group.appendChild(ta);
                        toolSchemaContainer.appendChild(group);
                        return;
                    }
                    // Initialize arguments as object if needed
                    if (typeof d.arguments !== 'object' || d.arguments === null) {
                        d.arguments = currentArgs;
                    }
                    window.SchemaForm.render(toolSchemaContainer, tool.parameters, currentArgs, overrides);
                };

                if (toolRef.el) {
                    asyncPopulateSelect(toolRef.el, getCachedTools, data => {
                        return data.tools.map(t => ({
                            value: t.name,
                            label: t.name + (t.description ? ' \u2014 ' + t.description.substring(0, 60) : ''),
                        }));
                    }, '-- Select a tool --');

                    toolRef.el.addEventListener('change', () => {
                        renderToolSchema(toolRef.el.value);
                    });

                    if (d.tool_name) {
                        setTimeout(() => toolRef.el.dispatchEvent(new Event('change')), 500);
                    }
                }

                // Show missing tools as hints
                getCachedTools().then(data => {
                    if (this._inspectorRenderId !== renderId) return;
                    if (data.missing && data.missing.length > 0) {
                        const missingHint = document.createElement('div');
                        missingHint.className = 'form-hint form-hint--warning';
                        missingHint.textContent = 'Unavailable: ' +
                            data.missing.map(m => m.name).join(', ');
                        body.appendChild(missingHint);
                    }
                });
                break;
            }

            // ─── SKILL ──────────────────────────────────────────
            case 'skill': {
                const skillRef = {};
                addField('Skill', 'skill_name', 'select', { options: [], ref: skillRef });
                if (skillRef.el) {
                    asyncPopulateSelect(skillRef.el, getCachedSkills, skills => {
                        if (!Array.isArray(skills)) return [];
                        return skills.map(s => ({
                            value: s.name,
                            label: s.name + (s.description ? ' \u2014 ' + s.description.substring(0, 50) : ''),
                        }));
                    }, '-- Select a skill --');
                }

                // Empty state hint
                getCachedSkills().then(skills => {
                    if (this._inspectorRenderId !== renderId) return;
                    if (!Array.isArray(skills) || skills.length === 0) {
                        addHint('No skills installed yet. Visit the Skills page to browse and install.');
                    }
                });

                addLink('\u2192 Browse & Install Skills', '/skills');
                break;
            }

            // ─── MCP ────────────────────────────────────────────
            case 'mcp': {
                const serverRef = {};
                addField('MCP Server', 'server', 'select', { options: [], ref: serverRef });
                const mcpToolRef = {};
                addField('Tool', 'tool', 'select', { options: [], ref: mcpToolRef });

                // Container for MCP tool schema fields
                const mcpSchemaContainer = document.createElement('div');
                mcpSchemaContainer.className = 'schema-fields-container';
                body.appendChild(mcpSchemaContainer);

                // Render schema form when MCP tool is selected
                const renderMcpToolSchema = (serverName, toolName) => {
                    mcpSchemaContainer.textContent = '';
                    if (!serverName || !toolName) return;
                    // Use cached tools from on-demand discovery (stored on node._mcpTools)
                    const cachedTools = node._mcpTools || [];
                    const tool = cachedTools.find(t => t.name === toolName);
                    const currentArgs = window.SchemaForm.parseArguments(d.arguments);
                    if (!tool || !tool.parameters || !tool.parameters.properties) return;
                    if (typeof d.arguments !== 'object' || d.arguments === null) {
                        d.arguments = currentArgs || {};
                    }
                    window.SchemaForm.render(mcpSchemaContainer, tool.parameters, currentArgs || {});
                };

                if (mcpToolRef.el) {
                    mcpToolRef.el.addEventListener('change', () => {
                        if (this._inspectorRenderId !== renderId) return;
                        const serverName = serverRef.el ? serverRef.el.value : d.server;
                        renderMcpToolSchema(serverName, mcpToolRef.el.value);
                    });
                    // Render schema if tool already selected
                    if (d.server && d.tool) {
                        setTimeout(() => renderMcpToolSchema(d.server, d.tool), 600);
                    }
                }

                if (serverRef.el && window.McpLoader) {
                    // Use shared McpLoader for server dropdown (DRY)
                    McpLoader.populateServerSelect(serverRef.el, d.server);

                    // Cascade: when server changes, discover tools via McpLoader
                    serverRef.el.addEventListener('change', () => {
                        if (this._inspectorRenderId !== renderId) return;
                        const selectedServer = serverRef.el.value;
                        node.data.server = selectedServer;
                        node.data.tool = '';
                        if (!mcpToolRef.el) return;

                        mcpToolRef.el.textContent = '';
                        const empty = document.createElement('option');
                        empty.value = '';
                        empty.textContent = selectedServer ? 'Connecting...' : '-- Select server first --';
                        mcpToolRef.el.appendChild(empty);

                        if (selectedServer) {
                            McpLoader.discoverTools(selectedServer).then(data => {
                                if (this._inspectorRenderId !== renderId) return;
                                // Cache discovered tools for schema rendering
                                node._mcpTools = data.tools || [];

                                if (!data.ok || !data.tools || data.tools.length === 0) {
                                    empty.textContent = data.error
                                        ? 'Connection failed \u2014 ' + data.error.substring(0, 50)
                                        : 'No tools found for ' + selectedServer;
                                    return;
                                }
                                McpLoader.populateToolSelect(mcpToolRef.el, data.tools, d.tool);
                            });
                        }
                    });

                    // If server already selected, trigger cascade
                    if (d.server) {
                        setTimeout(() => {
                            if (serverRef.el) serverRef.el.dispatchEvent(new Event('change'));
                        }, 500);
                    }
                }

                // Banner: no MCP servers? Suggest Connect Services page
                if (serverRef.el && window.McpLoader) {
                    McpLoader.fetchServers().then(servers => {
                        if (this._inspectorRenderId !== renderId) return;
                        const active = Array.isArray(servers) ? servers.filter(s => s.enabled !== false) : [];
                        if (active.length === 0) {
                            const banner = document.createElement('div');
                            banner.className = 'schema-field-hint';
                            banner.style.cssText = 'margin: 8px 0; padding: 10px 12px; border: 1px solid var(--accent); border-radius: var(--r-md); background: var(--accent-light);';
                            banner.textContent = '';
                            const link = document.createElement('a');
                            link.href = '/mcp';
                            link.textContent = 'Connect a service';
                            link.style.cssText = 'font-weight: 600; color: var(--accent-text);';
                            banner.appendChild(document.createTextNode('No MCP servers configured. '));
                            banner.appendChild(link);
                            banner.appendChild(document.createTextNode(' first (GitHub, Gmail, Slack, etc.).'));
                            body.appendChild(banner);
                        }
                    });
                }

                // Search catalog section — always shown so user can discover servers
                (() => {
                    const searchWrap = document.createElement('div');
                    searchWrap.className = 'inspector-search-section';

                    const searchLbl = document.createElement('label');
                    searchLbl.textContent = 'Find & Install MCP Servers';
                    searchWrap.appendChild(searchLbl);

                    const searchRow = document.createElement('div');
                    searchRow.className = 'inspector-search-row';
                    const searchInp = document.createElement('input');
                    searchInp.type = 'text';
                    searchInp.className = 'input';
                    searchInp.placeholder = 'Search catalog (e.g. gmail, slack, github)...';
                    searchRow.appendChild(searchInp);
                    searchWrap.appendChild(searchRow);

                    const resultsList = document.createElement('div');
                    resultsList.className = 'inspector-search-results';
                    searchWrap.appendChild(resultsList);
                    body.appendChild(searchWrap);

                    let searchTimeout = null;
                    searchInp.addEventListener('input', () => {
                        clearTimeout(searchTimeout);
                        const q = searchInp.value.trim();
                        if (q.length < 2) { resultsList.textContent = ''; return; }
                        searchTimeout = setTimeout(async () => {
                            if (this._inspectorRenderId !== renderId) return;
                            resultsList.textContent = 'Searching...';
                            try {
                                const items = await apiRequest('/v1/mcp/suggest?q=' + encodeURIComponent(q));
                                if (this._inspectorRenderId !== renderId) return;
                                resultsList.textContent = '';
                                if (!items || items.length === 0) {
                                    resultsList.textContent = 'No results found.';
                                    return;
                                }
                                items.slice(0, 5).forEach(item => {
                                    const row = document.createElement('div');
                                    row.className = 'search-result-item';

                                    const info = document.createElement('div');
                                    info.className = 'search-result-info';
                                    const name = document.createElement('strong');
                                    name.textContent = item.display_name || item.id;
                                    info.appendChild(name);
                                    if (item.description) {
                                        const desc = document.createElement('span');
                                        desc.className = 'search-result-desc';
                                        desc.textContent = ' \u2014 ' + item.description.substring(0, 60);
                                        info.appendChild(desc);
                                    }
                                    row.appendChild(info);

                                    const installBtn = document.createElement('a');
                                    installBtn.className = 'btn btn-secondary btn-xs';
                                    installBtn.textContent = 'Setup';
                                    installBtn.href = '/mcp';
                                    row.appendChild(installBtn);
                                    resultsList.appendChild(row);
                                });
                            } catch (_) {
                                resultsList.textContent = 'Search failed.';
                            }
                        }, 400);
                    });
                })();

                addLink('\u2192 Full MCP Server Setup', '/mcp');

                // Empty state
                if (window.McpLoader) McpLoader.fetchServers().then(servers => {
                    if (this._inspectorRenderId !== renderId) return;
                    if (!Array.isArray(servers) || servers.length === 0) {
                        addHint('No MCP servers configured yet. Search above or visit the MCP page.');
                    }
                });
                break;
            }

            // ─── LLM ────────────────────────────────────────────
            case 'llm': {
                addField('Prompt', 'prompt', 'textarea', {
                    rows: 5, placeholder: 'What should the agent do?\n\nExample: Summarize the latest news about AI safety'
                });
                const modelRef = {};
                addField('Model', 'model', 'select', { options: [], ref: modelRef });
                if (modelRef.el && window.ModelLoader) {
                    // Use shared ModelLoader (DRY: same logic as chat.js)
                    const sel = modelRef.el;
                    sel.textContent = '';
                    const loading = document.createElement('option');
                    loading.value = '';
                    loading.textContent = 'Loading models...';
                    loading.disabled = true;
                    sel.appendChild(loading);

                    ModelLoader.fetchGrouped().then(result => {
                        if (this._inspectorRenderId !== renderId) return;
                        ModelLoader.populateSelect(sel, result.groups, d.model);
                    }).catch(() => {
                        if (this._inspectorRenderId !== renderId) return;
                        sel.textContent = '';
                        const errOpt = document.createElement('option');
                        errOpt.value = '';
                        errOpt.textContent = '-- Default model --';
                        sel.appendChild(errOpt);
                    });
                }
                break;
            }

            // ─── CONDITION ──────────────────────────────────────
            case 'condition':
                addField('Condition Expression', 'expression', 'textarea', {
                    rows: 3, placeholder: 'e.g. has_new_emails == true\n     result.count > 0'
                });
                addPresetButtons([
                    { label: 'Contains keyword', value: 'result contains "keyword"' },
                    { label: 'Is empty', value: 'result is empty' },
                    { label: 'Count > N', value: 'count of items > 5' },
                    { label: 'Success', value: 'previous step succeeded' },
                ], 'expression');
                addField('True Branch Label', 'true_label', 'text', { placeholder: 'Yes' });
                addField('False Branch Label', 'false_label', 'text', { placeholder: 'No' });
                break;

            // ─── PARALLEL ───────────────────────────────────────
            case 'parallel': {
                addField('Number of Branches', 'branches', 'number', { defaultVal: 2, min: 2, max: 10 });
                addHint('How it works: add the parallel node, then add one node for each branch below it. ' +
                    'Connect the parallel node to each branch node. All branches run simultaneously, ' +
                    'and results are merged before the next step.');

                // Visual example
                const example = document.createElement('div');
                example.className = 'schema-field-hint';
                example.style.whiteSpace = 'pre';
                example.style.fontFamily = 'var(--ff-mono, monospace)';
                example.style.lineHeight = '1.5';
                example.style.marginTop = '8px';
                example.textContent =
                    '         ┌─ Check Gmail\n' +
                    'Parallel ┤\n' +
                    '         └─ Check Slack\n' +
                    '              ↓\n' +
                    '         Merge & Deliver';
                body.appendChild(example);
                break;
            }

            // ─── LOOP ───────────────────────────────────────────
            case 'loop':
                addField('Max Iterations', 'max_iterations', 'number', { defaultVal: 10, min: 1, max: 100 });
                addField('Break Condition', 'condition', 'text', { placeholder: 'e.g. no_more_items' });
                addPresetButtons([
                    { label: 'All processed', value: 'all items processed' },
                    { label: 'Error found', value: 'an error occurs' },
                    { label: 'No more results', value: 'no more results' },
                ], 'condition');
                break;

            // ─── SUBPROCESS ─────────────────────────────────────
            case 'subprocess': {
                const wfRef = {};
                addField('Workflow', 'workflow_ref', 'select', { options: [], ref: wfRef });
                if (wfRef.el) {
                    asyncPopulateSelect(wfRef.el,
                        () => apiRequest('/v1/automations'),
                        items => (Array.isArray(items) ? items : []).map(a => ({
                            value: a.name || a.id,
                            label: a.name || ('Automation #' + a.id),
                        })),
                        '-- Select automation --');
                }
                addHint('Select a saved automation to run as a sub-step.');
                break;
            }

            // ─── TRANSFORM ──────────────────────────────────────
            case 'transform':
                addField('Template / Code', 'template', 'textarea', {
                    rows: 5, placeholder: 'Describe how to transform the data...\n\nExample: Extract "subject" and "from" fields from email data'
                });
                addPresetButtons([
                    { label: 'Extract summary', value: 'Extract only the summary from the result' },
                    { label: 'Format as list', value: 'Format as a bullet-point list' },
                    { label: 'JSON to text', value: 'Convert JSON to readable text' },
                    { label: 'First N items', value: 'Keep only the first 5 items' },
                ], 'template');
                break;

            // ─── APPROVE ───────────────────────────────────────
            case 'approve': {
                const approveRef = {};
                addField('Send Approval Request To', 'approve_channel', 'select', { options: [], ref: approveRef });
                if (approveRef.el) {
                    asyncPopulateSelect(approveRef.el, getCachedTargets, targets => {
                        if (!Array.isArray(targets)) return [];
                        return targets.map(t => ({ value: t.value, label: t.label }));
                    }, '-- Select channel --');
                }
                addField('Approval Message', 'approve_message', 'textarea', {
                    rows: 2, placeholder: 'What should the user approve?\n\nExample: Proceed with sending the report?'
                });
                addHint('The automation will pause here and wait for user approval before continuing.');
                break;
            }

            // ─── REQUIRE 2FA ──────────────────────────────────
            case 'require_2fa':
                addHint('This node requires two-factor authentication verification before the automation can continue. The user will be prompted to enter their 2FA code.');
                addHint('Make sure 2FA is enabled in Settings > Vault & 2FA.');
                addLink('\u2192 Configure 2FA', '/vault');
                break;

            // ─── DELIVER ────────────────────────────────────────
            case 'deliver': {
                const deliverRef = {};
                addField('Deliver To', 'target', 'select', { options: [], ref: deliverRef });
                if (deliverRef.el) {
                    asyncPopulateSelect(deliverRef.el, getCachedTargets, targets => {
                        if (!Array.isArray(targets)) return [];
                        return targets.map(t => ({ value: t.value, label: t.label }));
                    }, '-- Select channel --');
                }
                break;
            }
        }
    },

    hideInspector() {
        this.inspector.style.display = 'none';
        this.selectedNodeId = null;
        this.renderNodes();
    },

    // ── Save ─────────────────────────────────────────────────────

    nodeToInstruction(node) {
        const d = node.data;
        switch (node.kind) {
            case 'llm':        return d.prompt || 'Execute task';
            case 'tool': {
                const argsStr = window.SchemaForm
                    ? window.SchemaForm.serializeArguments(d.arguments)
                    : (typeof d.arguments === 'string' ? d.arguments : JSON.stringify(d.arguments || {}));
                return 'Use tool: ' + (d.tool_name || 'unknown') + (argsStr ? ' with args: ' + argsStr : '');
            }
            case 'skill':      return 'Run skill: ' + (d.skill_name || 'unknown');
            case 'mcp': {
                const mcpArgsStr = window.SchemaForm
                    ? window.SchemaForm.serializeArguments(d.arguments)
                    : (typeof d.arguments === 'string' ? d.arguments : JSON.stringify(d.arguments || {}));
                return 'Call MCP: ' + (d.server || '?') + '/' + (d.tool || '?') + (mcpArgsStr ? ' with args: ' + mcpArgsStr : '');
            }
            case 'condition':  return 'If: ' + (d.expression || '?');
            case 'transform':  return 'Transform: ' + (d.template || '?');
            case 'loop':       return 'Loop (max ' + (d.max_iterations || 10) + '): ' + (d.condition || 'until done');
            case 'subprocess': return 'Run workflow: ' + (d.workflow_ref || '?');
            case 'parallel':   return 'Execute ' + (d.branches || 2) + ' branches in parallel';
            case 'approve':    return 'Require approval' + (d.approve_channel ? ' via ' + d.approve_channel : '') + (d.approve_message ? ': ' + d.approve_message : '');
            case 'require_2fa': return 'Require 2FA verification before proceeding';
            default:           return node.title;
        }
    },

    nodeMetaString(node) {
        const d = node.data;
        switch (node.kind) {
            case 'trigger': {
                if (d.mode === 'cron') {
                    return 'cron ' + [d.cronMinute||'0', d.cronHour||'9', d.cronDom||'*', d.cronMonth||'*', d.cronDow||'*'].join(' ');
                } else if (d.mode === 'interval') {
                    return 'every ' + (d.intervalHours || 6) + 'h';
                }
                return 'daily ' + (d.time || '09:00');
            }
            case 'tool':       return d.tool_name || '';
            case 'skill':      return d.skill_name || '';
            case 'mcp':        return d.tool || '';
            case 'llm':        return d.model || '';
            case 'condition':  return d.expression || '';
            case 'deliver':    return d.target || '';
            case 'loop':       return 'max:' + (d.max_iterations || 10);
            case 'subprocess': return d.workflow_ref || '';
            case 'transform':  return '';
            case 'approve':    return d.approve_channel || '';
            case 'require_2fa': return '2fa';
            default:           return '';
        }
    },

    async save() {
        const nameInput = document.getElementById('builder-automation-name');
        const name = nameInput.value.trim();

        // Full pre-save validation (AUTO-2)
        if (window.AutoValidate) {
            const result = window.AutoValidate.validateAll(this.nodes, this.edges, name);
            this.renderNodes(); // re-render to show error badges
            if (!result.valid) {
                showToast(result.errors[0], 'error');
                if (!name) nameInput.focus();
                return;
            }
        } else {
            // Fallback: original checks if auto-validate.js not loaded
            if (!name) {
                showToast('Please enter an automation name.', 'error');
                return;
            }
            const triggerCheck = this.nodes.find(n => n.kind === 'trigger');
            const middleCheck = this.nodes.filter(n => n.kind !== 'trigger' && n.kind !== 'deliver');
            if (!triggerCheck || middleCheck.length === 0) {
                showToast('Need at least a Trigger and one processing node.', 'error');
                return;
            }
        }

        const triggerNode = this.nodes.find(n => n.kind === 'trigger');
        const deliverNode = this.nodes.find(n => n.kind === 'deliver');
        const middleNodes = this.nodes.filter(n => n.kind !== 'trigger' && n.kind !== 'deliver');

        const payload = { name, trigger: 'always' };

        // Schedule from trigger — convert to stored format (cron:... or every:...)
        const td = triggerNode.data;
        if (td.mode === 'cron') {
            payload.cron = [
                td.cronMinute || '0',
                td.cronHour || '9',
                td.cronDom || '*',
                td.cronMonth || '*',
                td.cronDow || '*',
            ].join(' ');
        } else if (td.mode === 'interval') {
            const hours = Number(td.intervalHours) || 6;
            payload.every = Math.floor(hours * 3600);
        } else {
            // daily / weekdays / weekly → convert time to cron expression
            const time = td.time || '09:00';
            const [hh, mm] = time.split(':').map(Number);
            payload.cron = (mm || 0) + ' ' + (hh || 9) + ' * * *';
        }

        // Prompt / workflow_steps from middle nodes
        if (middleNodes.length === 1 && middleNodes[0].kind === 'llm') {
            payload.prompt = middleNodes[0].data.prompt || 'Execute task';
        } else {
            // Build a composite prompt from all workflow step instructions
            const steps = middleNodes.map((n, i) => ({
                name: n.title || ('Step ' + (i + 1)),
                instruction: this.nodeToInstruction(n),
                approval_required: n.kind === 'approve',
            }));
            const stepDescriptions = steps
                .map((s, i) => `${i + 1}. ${s.name}: ${s.instruction}`)
                .join('\n');
            payload.prompt = `Multi-step automation:\n${stepDescriptions}`;
            payload.workflow_steps = steps;
        }

        // Deliver
        payload.deliver_to = deliverNode ? (deliverNode.data.target || 'cli:default') : 'cli:default';

        // Full flow graph for visual persistence
        payload.flow_json = JSON.stringify({
            nodes: this.nodes.map(n => ({
                id: n.id,
                kind: n.kind,
                label: n.title,
                meta: this.nodeMetaString(n),
            })),
            edges: this.edges.map(e => ({ from: e.from, to: e.to })),
        });

        try {
            document.getElementById('builder-status').textContent = 'Saving...';
            if (this.editingId) {
                // Update existing automation
                await apiRequest('/v1/automations/' + encodeURIComponent(this.editingId), {
                    method: 'PATCH',
                    body: JSON.stringify(payload),
                });
                showToast('Automation updated successfully!', 'success');
            } else {
                // Create new automation
                await apiRequest('/v1/automations', {
                    method: 'POST',
                    body: JSON.stringify(payload),
                });
                showToast('Automation created successfully!', 'success');
            }
            document.getElementById('automations-builder-view').style.display = 'none';
            document.getElementById('automations-list-view').style.display = '';
            await loadAutomations();
        } catch (err) {
            showToast(err.message || 'Failed to save.', 'error');
        } finally {
            document.getElementById('builder-status').textContent = '';
        }
    },
};

document.addEventListener('DOMContentLoaded', () => {
    Builder.init();
});

