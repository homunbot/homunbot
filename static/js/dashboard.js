// Homun — Operational Dashboard

// ─── Helpers ───

function showToast(message, type) {
    type = type || 'success';
    var existing = document.querySelector('.toast');
    if (existing) existing.remove();
    var toast = document.createElement('div');
    toast.className = 'toast toast-' + type;
    toast.textContent = message;
    document.body.appendChild(toast);
    setTimeout(function () {
        toast.classList.add('toast-out');
        setTimeout(function () { toast.remove(); }, 300);
    }, 2500);
}

function escapeHtml(value) {
    return String(value || '').replaceAll('&', '&amp;').replaceAll('<', '&lt;')
        .replaceAll('>', '&gt;').replaceAll('"', '&quot;').replaceAll("'", '&#039;');
}

function formatInt(value) {
    return new Intl.NumberFormat('en-US').format(Number(value || 0));
}

function formatUsd(value) {
    return '$' + Number(value || 0).toFixed(4);
}

function timeAgo(iso) {
    if (!iso) return '';
    var diff = (Date.now() - new Date(iso).getTime()) / 1000;
    if (diff < 0) diff = 0;
    if (diff < 60) return 'just now';
    if (diff < 3600) return Math.floor(diff / 60) + 'm ago';
    if (diff < 86400) return Math.floor(diff / 3600) + 'h ago';
    return Math.floor(diff / 86400) + 'd ago';
}

function timeUntil(iso) {
    if (!iso) return null;
    var diff = (new Date(iso).getTime() - Date.now()) / 1000;
    if (diff < 0) return 'overdue';
    if (diff < 60) return 'now';
    if (diff < 3600) return Math.floor(diff / 60) + 'm';
    if (diff < 86400) return Math.floor(diff / 3600) + 'h ' + Math.floor((diff % 3600) / 60) + 'm';
    return Math.floor(diff / 86400) + 'd ' + Math.floor((diff % 86400) / 3600) + 'h';
}

// ─── Live Uptime Counter ───

var uptimeEl = document.querySelector('[data-live-uptime]');
if (uptimeEl) {
    var startSecs = parseInt(uptimeEl.dataset.liveUptime, 10);
    var startedAt = Date.now() - (startSecs * 1000);
    function updateUptime() {
        var secs = Math.floor((Date.now() - startedAt) / 1000);
        if (secs < 60) uptimeEl.textContent = secs + 's';
        else if (secs < 3600) uptimeEl.textContent = Math.floor(secs / 60) + 'm ' + (secs % 60) + 's';
        else if (secs < 86400) uptimeEl.textContent = Math.floor(secs / 3600) + 'h ' + Math.floor((secs % 3600) / 60) + 'm';
        else uptimeEl.textContent = Math.floor(secs / 86400) + 'd ' + Math.floor((secs % 86400) / 3600) + 'h';
    }
    updateUptime();
    setInterval(updateUptime, 1000);
}

// ─── Dashboard Data Loading ───

async function loadDashboardData() {
    var results = await Promise.allSettled([
        fetch('/api/v1/automations').then(function (r) { return r.json(); }),
        fetch('/api/v1/workflows').then(function (r) { return r.json(); }),
        fetch('/api/v1/providers/health').then(function (r) { return r.json(); }),
        fetch('/api/v1/status').then(function (r) { return r.json(); }),
        fetch('/api/v1/memory/stats').then(function (r) { return r.json(); }),
        fetch('/api/v1/knowledge/stats').then(function (r) { return r.json(); }),
        fetch('/api/v1/logs/recent?limit=50').then(function (r) { return r.json(); }),
    ]);

    var automations = results[0].status === 'fulfilled' ? results[0].value : [];
    var workflows = results[1].status === 'fulfilled' ? results[1].value : { workflows: [], stats: {} };
    var providers = results[2].status === 'fulfilled' ? results[2].value : { providers: [] };
    var status = results[3].status === 'fulfilled' ? results[3].value : { channels: [] };
    var memoryStats = results[4].status === 'fulfilled' ? results[4].value : null;
    var knowledgeStats = results[5].status === 'fulfilled' ? results[5].value : null;
    var logs = results[6].status === 'fulfilled' ? results[6].value : [];

    if (!Array.isArray(automations)) automations = [];
    if (!Array.isArray(logs)) logs = [];

    renderNextAutomation(automations);
    renderWorkflowStats(workflows);
    renderUpcomingAutomations(automations);
    renderActivityFeed(automations, logs);
    renderSystemHealth(providers, status, memoryStats, knowledgeStats);
}

// ─── Stat Card: Next Automation ───

function renderNextAutomation(automations) {
    var valEl = document.getElementById('stat-next-auto-value');
    var nameEl = document.getElementById('stat-next-auto-name');
    if (!valEl || !nameEl) return;

    var enabled = automations.filter(function (a) { return a.enabled && a.next_run; });
    enabled.sort(function (a, b) { return new Date(a.next_run) - new Date(b.next_run); });

    if (enabled.length === 0) {
        valEl.textContent = 'None';
        nameEl.textContent = 'no automations scheduled';
        return;
    }

    var next = enabled[0];
    valEl.textContent = timeUntil(next.next_run) || '—';
    nameEl.textContent = next.name || 'Unnamed';

    setInterval(function () {
        valEl.textContent = timeUntil(next.next_run) || '—';
    }, 60000);
}

// ─── Stat Card: Workflows ───

function renderWorkflowStats(data) {
    var valEl = document.getElementById('stat-workflows-value');
    var subEl = document.getElementById('stat-workflows-sub');
    if (!valEl || !subEl) return;

    var stats = data.stats || {};
    var running = Number(stats.running || 0);
    var paused = Number(stats.paused || 0);

    if (running > 0) {
        valEl.textContent = running + ' running';
        subEl.textContent = paused > 0 ? paused + ' paused' : 'all active';
    } else if (paused > 0) {
        valEl.textContent = paused + ' paused';
        subEl.textContent = 'none running';
    } else {
        valEl.textContent = '0';
        subEl.textContent = 'all idle';
    }
}

// ─── Upcoming Automations ───

function renderUpcomingAutomations(automations) {
    var container = document.getElementById('dash-automations-list');
    if (!container) return;
    container.textContent = '';

    var enabled = automations.filter(function (a) { return a.enabled && a.next_run; });
    enabled.sort(function (a, b) { return new Date(a.next_run) - new Date(b.next_run); });
    var items = enabled.slice(0, 5);

    if (items.length === 0) {
        var empty = document.createElement('div');
        empty.className = 'dash-empty';
        empty.textContent = 'No automations scheduled. Create one in Automations.';
        container.appendChild(empty);
        return;
    }

    items.forEach(function (auto) {
        var row = document.createElement('div');
        row.className = 'item-row';

        var info = document.createElement('div');
        info.className = 'item-info';

        var icon = document.createElement('div');
        icon.className = 'item-icon';
        icon.textContent = '\u23F0';

        var text = document.createElement('div');
        var name = document.createElement('div');
        name.className = 'item-name';
        name.textContent = auto.name || 'Unnamed';
        var detail = document.createElement('div');
        detail.className = 'item-detail';
        detail.textContent = 'in ' + (timeUntil(auto.next_run) || '\u2014') + (auto.schedule ? ' \u00B7 ' + auto.schedule : '');

        text.appendChild(name);
        text.appendChild(detail);
        info.appendChild(icon);
        info.appendChild(text);

        var actions = document.createElement('div');
        actions.style.display = 'flex';
        actions.style.alignItems = 'center';
        actions.style.gap = '8px';

        if (auto.status) {
            var badge = document.createElement('span');
            var st = String(auto.status).toLowerCase();
            badge.className = 'badge ' + (st === 'success' ? 'badge-success' : st === 'error' ? 'badge-error' : 'badge-neutral');
            badge.textContent = auto.status;
            actions.appendChild(badge);
        }

        var runBtn = document.createElement('button');
        runBtn.className = 'dash-run-btn';
        runBtn.textContent = 'Run';
        runBtn.title = 'Run now';
        runBtn.addEventListener('click', function (e) {
            e.preventDefault();
            e.stopPropagation();
            runBtn.disabled = true;
            runBtn.textContent = '...';
            fetch('/api/v1/automations/' + auto.id + '/run', { method: 'POST' })
                .then(function () { showToast('Automation started', 'success'); })
                .catch(function () { showToast('Failed to start', 'error'); })
                .finally(function () { runBtn.disabled = false; runBtn.textContent = 'Run'; });
        });
        actions.appendChild(runBtn);

        row.appendChild(info);
        row.appendChild(actions);
        container.appendChild(row);
    });
}

// ─── Recent Activity Feed ───

function renderActivityFeed(automations, logs) {
    var container = document.getElementById('dash-activity-list');
    if (!container) return;
    container.textContent = '';

    var events = [];

    automations.forEach(function (auto) {
        if (!auto.last_run) return;
        var st = String(auto.status || '').toLowerCase();
        events.push({
            time: auto.last_run,
            icon: st === 'error' ? '\u2715' : '\u2713',
            type: st === 'error' ? 'err' : 'ok',
            name: auto.name || 'Automation',
            detail: auto.last_result ? String(auto.last_result).slice(0, 80) : (st || 'completed'),
        });
    });

    (Array.isArray(logs) ? logs : []).forEach(function (log) {
        if (log.level !== 'error' && log.level !== 'ERROR') return;
        events.push({
            time: log.timestamp,
            icon: '\u26A0',
            type: 'err',
            name: log.target || 'system',
            detail: String(log.message || '').slice(0, 80),
        });
    });

    events.sort(function (a, b) { return new Date(b.time) - new Date(a.time); });
    var items = events.slice(0, 8);

    if (items.length === 0) {
        var empty = document.createElement('div');
        empty.className = 'dash-empty';
        empty.textContent = 'No recent activity.';
        container.appendChild(empty);
        return;
    }

    items.forEach(function (ev) {
        var row = document.createElement('div');
        row.className = 'item-row';

        var info = document.createElement('div');
        info.className = 'item-info';

        var icon = document.createElement('div');
        icon.className = 'item-icon';
        icon.textContent = ev.icon;
        if (ev.type === 'ok') icon.style.color = 'var(--ok)';
        else if (ev.type === 'err') icon.style.color = 'var(--err)';

        var text = document.createElement('div');
        var name = document.createElement('div');
        name.className = 'item-name';
        name.textContent = ev.name;
        var detail = document.createElement('div');
        detail.className = 'item-detail';
        detail.textContent = ev.detail;

        text.appendChild(name);
        text.appendChild(detail);
        info.appendChild(icon);
        info.appendChild(text);

        var timeEl = document.createElement('span');
        timeEl.className = 'dash-activity-time';
        timeEl.textContent = timeAgo(ev.time);

        row.appendChild(info);
        row.appendChild(timeEl);
        container.appendChild(row);
    });
}

// ─── System Health ───

function renderSystemHealth(providers, status, memoryStats, knowledgeStats) {
    var container = document.getElementById('dash-health-grid');
    if (!container) return;
    container.textContent = '';

    container.appendChild(buildHealthCard('Providers', function (body) {
        var provList = (providers && providers.providers) || [];
        if (provList.length === 0) {
            var row = document.createElement('div');
            row.className = 'dash-status-row';
            row.textContent = 'No health data';
            body.appendChild(row);
            return;
        }
        provList.forEach(function (p) {
            var st = String(p.status || 'unknown').toLowerCase();
            body.appendChild(buildStatusRow(
                st === 'ok' || st === 'healthy' ? 'ok' : st === 'degraded' ? 'warn' : st === 'error' || st === 'down' ? 'err' : 'neutral',
                p.model || p.name || 'unknown',
                p.latency_ms ? p.latency_ms + 'ms' : ''
            ));
        });
    }));

    container.appendChild(buildHealthCard('Channels', function (body) {
        var channels = (status && status.channels) || [];
        if (channels.length === 0) {
            var row = document.createElement('div');
            row.className = 'dash-status-row';
            row.textContent = 'No channel data';
            body.appendChild(row);
            return;
        }
        channels.forEach(function (ch) {
            body.appendChild(buildStatusRow(
                ch.enabled ? 'ok' : 'neutral',
                ch.name,
                ch.enabled ? 'connected' : 'disabled'
            ));
        });
    }));

    container.appendChild(buildHealthCard('Data', function (body) {
        if (memoryStats) {
            body.appendChild(buildStatusRow('ok', 'Memories', formatInt(memoryStats.chunk_count || 0) + ' chunks'));
            body.appendChild(buildStatusRow('ok', 'Daily files', formatInt(memoryStats.daily_count || 0)));
        } else {
            body.appendChild(buildStatusRow('neutral', 'Memory', 'unavailable'));
        }
        if (knowledgeStats) {
            body.appendChild(buildStatusRow('ok', 'Knowledge', formatInt(knowledgeStats.documents_count || 0) + ' docs'));
        } else {
            body.appendChild(buildStatusRow('neutral', 'Knowledge', 'not configured'));
        }
    }));
}

function buildHealthCard(title, renderBody) {
    var card = document.createElement('div');
    card.className = 'dash-health-card';
    var h = document.createElement('div');
    h.className = 'dash-health-card-title';
    h.textContent = title;
    card.appendChild(h);
    var body = document.createElement('div');
    renderBody(body);
    card.appendChild(body);
    return card;
}

function buildStatusRow(dotType, label, meta) {
    var row = document.createElement('div');
    row.className = 'dash-status-row';
    var dot = document.createElement('span');
    dot.className = 'dash-status-dot dash-status-dot--' + dotType;
    var lbl = document.createElement('span');
    lbl.className = 'dash-status-label';
    lbl.textContent = label;
    row.appendChild(dot);
    row.appendChild(lbl);
    if (meta) {
        var m = document.createElement('span');
        m.className = 'dash-status-meta';
        m.textContent = meta;
        row.appendChild(m);
    }
    return row;
}

// ─── Emergency Stop ───

{
    var btn = document.getElementById('estop-btn');
    if (btn) {
        btn.addEventListener('click', async function () {
            if (!confirm('EMERGENCY STOP\n\nThis will immediately:\n- Stop the agent loop\n- Take the network offline\n- Close the browser\n- Shut down MCP servers\n- Cancel all subagents\n\nProceed?')) return;
            btn.disabled = true;
            btn.textContent = 'Stopping...';
            try {
                var res = await fetch('/api/v1/emergency-stop', { method: 'POST' });
                var report = await res.json();
                btn.textContent = 'STOPPED';
                btn.classList.add('estop-stopped');
                var parts = [];
                if (report.browser_closed) parts.push('Browser closed');
                if (report.mcp_shutdown) parts.push('MCP shut down');
                if (report.subagents_cancelled > 0) parts.push(report.subagents_cancelled + ' subagents cancelled');
                parts.push('Network offline');
                alert('Emergency Stop activated.\n\n' + parts.join('\n'));
            } catch (e) {
                alert('Emergency stop failed: ' + e.message);
                btn.disabled = false;
                btn.textContent = '\u26A0 Emergency Stop';
            }
        });
    }
}

// ─── Init ───
loadDashboardData();
