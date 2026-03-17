// Homun — Database Maintenance page

// ─── State ───
let domains = [];
let purging = null; // domain id currently being purged

// ─── Utilities ───
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
    if (ct.includes('application/json')) return res.json();
    return null;
}

function escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}

function formatRows(n) {
    if (n === 0) return 'empty';
    if (n === 1) return '1 row';
    return `${n.toLocaleString()} rows`;
}

// ─── Domain icons ───
const DOMAIN_ICONS = {
    conversations: '<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" style="width:22px;height:22px"><path d="M2 12.5V3.5A1.5 1.5 0 0 1 3.5 2h11A1.5 1.5 0 0 1 16 3.5v7a1.5 1.5 0 0 1-1.5 1.5H6L2 16V12.5z"/></svg>',
    automations: '<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" style="width:22px;height:22px"><circle cx="9" cy="9" r="6.5"/><path d="M9 5.5v4l2.8 1.8"/></svg>',
    workflows: '<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" style="width:22px;height:22px"><circle cx="5" cy="4" r="2"/><circle cx="13" cy="4" r="2"/><circle cx="9" cy="14" r="2"/><path d="M5 6v2a3 3 0 0 0 3 3h1"/><path d="M13 6v2a3 3 0 0 1-3 3h-1"/></svg>',
    knowledge: '<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" style="width:22px;height:22px"><path d="M2 3h5l2 2h7v10H2z"/><path d="M6 9h6"/></svg>',
    business: '<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" style="width:22px;height:22px"><rect x="2" y="6" width="14" height="10" rx="1.5"/><path d="M6 6V4a3 3 0 0 1 6 0v2"/></svg>',
    usage: '<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" style="width:22px;height:22px"><path d="M2 16V6l4-4h8l2 2v12H2z"/><path d="M6 2v4H2"/></svg>',
    cron: '<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" style="width:22px;height:22px"><circle cx="9" cy="9" r="7"/><path d="M9 5v4l3 2"/></svg>',
    email: '<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" style="width:22px;height:22px"><rect x="1" y="3" width="16" height="12" rx="2"/><path d="M1 5l8 5 8-5"/></svg>',
};

// ─── Load ───
async function loadStats() {
    try {
        domains = await apiRequest('/v1/maintenance/db-stats');
        render();
    } catch (e) {
        document.getElementById('maintenance-content').textContent = '';
        showErrorState('maintenance-content', 'Could not load database stats.', loadStats);
    }
}

// ─── Render ───
function render() {
    const container = document.getElementById('maintenance-content');
    container.textContent = '';

    if (!domains.length) {
        const div = document.createElement('div');
        div.className = 'empty-state';
        const p = document.createElement('p');
        p.textContent = 'No domain groups found.';
        div.appendChild(p);
        container.appendChild(div);
        return;
    }

    // Summary bar
    const totalRows = domains.reduce((s, d) => s + d.total_rows, 0);
    const totalTables = domains.reduce((s, d) => s + d.tables.length, 0);

    const summary = document.createElement('div');
    summary.className = 'maint-summary';

    [
        [totalRows.toLocaleString(), 'Total rows'],
        [totalTables.toString(), 'Tables'],
        [domains.length.toString(), 'Domains'],
    ].forEach(([value, label]) => {
        const stat = document.createElement('div');
        stat.className = 'maint-summary-stat';
        const valSpan = document.createElement('span');
        valSpan.className = 'maint-summary-value';
        valSpan.textContent = value;
        const labelSpan = document.createElement('span');
        labelSpan.className = 'maint-summary-label';
        labelSpan.textContent = label;
        stat.appendChild(valSpan);
        stat.appendChild(labelSpan);
        summary.appendChild(stat);
    });
    container.appendChild(summary);

    // Grid
    const grid = document.createElement('div');
    grid.className = 'maint-grid';

    for (const d of domains) {
        const card = buildDomainCard(d);
        grid.appendChild(card);
    }
    container.appendChild(grid);
}

function buildDomainCard(d) {
    const isEmpty = d.total_rows === 0;
    const isPurging = purging === d.id;

    const card = document.createElement('div');
    card.className = 'maint-card' + (isEmpty ? ' maint-card--empty' : '');

    // Header
    const header = document.createElement('div');
    header.className = 'maint-card-header';

    const iconDiv = document.createElement('div');
    iconDiv.className = 'maint-card-icon';
    // Icon SVGs are hardcoded constants (not user input) — safe to use innerHTML here
    iconDiv.innerHTML = DOMAIN_ICONS[d.id] || DOMAIN_ICONS.usage;

    const info = document.createElement('div');
    info.className = 'maint-card-info';
    const title = document.createElement('div');
    title.className = 'maint-card-title';
    title.textContent = d.id;
    const desc = document.createElement('div');
    desc.className = 'maint-card-desc';
    desc.textContent = d.label;
    info.appendChild(title);
    info.appendChild(desc);

    const count = document.createElement('div');
    count.className = 'maint-card-count';
    count.textContent = formatRows(d.total_rows);

    header.appendChild(iconDiv);
    header.appendChild(info);
    header.appendChild(count);
    card.appendChild(header);

    // Table rows
    const tables = document.createElement('div');
    tables.className = 'maint-card-tables';
    for (const t of d.tables) {
        const row = document.createElement('div');
        row.className = 'maint-table-row';
        const name = document.createElement('span');
        name.className = 'maint-table-name';
        name.textContent = t.name;
        const rows = document.createElement('span');
        rows.className = 'maint-table-rows';
        rows.textContent = t.rows.toLocaleString();
        row.appendChild(name);
        row.appendChild(rows);
        tables.appendChild(row);
    }
    card.appendChild(tables);

    // Actions
    const actions = document.createElement('div');
    actions.className = 'maint-card-actions';
    const btn = document.createElement('button');
    btn.className = 'btn btn-sm ' + (isEmpty ? 'btn-secondary' : 'btn-danger');
    btn.disabled = isEmpty || isPurging;
    btn.textContent = isPurging ? 'Purging...' : 'Purge';
    btn.addEventListener('click', () => confirmPurge(d.id, d.label, d.total_rows));
    actions.appendChild(btn);
    card.appendChild(actions);

    return card;
}

// ─── Purge ───
function confirmPurge(domainId, label, rows) {
    const msg = `Delete all ${rows.toLocaleString()} rows from "${domainId}" (${label})?\n\nThis cannot be undone.`;
    if (!confirm(msg)) return;
    doPurge(domainId);
}

async function doPurge(domainId) {
    purging = domainId;
    render();
    try {
        const result = await apiRequest('/v1/maintenance/purge', {
            method: 'POST',
            body: JSON.stringify({ domain: domainId }),
        });
        showToast(`Purged ${result.deleted_rows} rows from "${domainId}"`);
    } catch (e) {
        showToast('Purge failed: ' + e.message, 'error');
    }
    purging = null;
    await loadStats(); // refresh counts
}

// ─── Init ───
document.addEventListener('DOMContentLoaded', loadStats);
