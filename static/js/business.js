// ─── Business Autopilot page ────────────────────────────────────
'use strict';

let businesses = [];
let selectedBizId = null;
let bizDeliveryTargets = [];

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

function statusBadge(status) {
    const cls = {
        planning: 'badge-neutral',
        active: 'badge-success',
        paused: 'badge-warning',
        closed: 'badge-error',
    }[status] || 'badge-neutral';
    return `<span class="badge ${cls}">${escapeHtml(status)}</span>`;
}

function formatTs(ts) {
    if (!ts) return '\u2014';
    try {
        const d = new Date(ts.includes('T') ? ts : ts + 'Z');
        return d.toLocaleString(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
    } catch { return ts; }
}

function fmt(n) {
    return (n || 0).toFixed(2);
}

// ─── Delivery targets ────────────────────────────────────────────

async function loadBizDeliveryTargets() {
    try {
        const rows = await apiRequest('/v1/automations/targets');
        if (Array.isArray(rows) && rows.length > 0) {
            bizDeliveryTargets = rows
                .map(r => ({ value: String(r.value || '').trim(), label: String(r.label || r.value || '').trim() }))
                .filter(r => r.value);
        }
    } catch (_) { /* fallback */ }

    if (!bizDeliveryTargets.length) {
        bizDeliveryTargets = [{ value: 'web:web', label: 'Web UI' }];
    }

    const sel = document.getElementById('biz-deliver-to');
    sel.textContent = '';
    bizDeliveryTargets.forEach(t => {
        const opt = document.createElement('option');
        opt.value = t.value;
        opt.textContent = t.label;
        sel.appendChild(opt);
    });
}

// ─── Load businesses ────────────────────────────────────────────

async function loadBusinesses() {
    try {
        const data = await apiRequest('/v1/business');
        businesses = data.businesses || [];
        renderList();
        updateStats();
    } catch (e) {
        showErrorState('biz-list', 'Could not load businesses.', loadBusinesses);
    }
}

function updateStats() {
    const active = businesses.filter(b => b.status === 'active').length;
    document.getElementById('stat-biz-active').textContent = active;
    document.getElementById('biz-count').textContent = businesses.length;
    document.getElementById('stat-biz-products').textContent = '\u2014';

    if (businesses.length === 0 || !selectedBizId) {
        document.getElementById('stat-biz-revenue').textContent = '\u2014';
        document.getElementById('stat-biz-profit').textContent = '\u2014';
    }
}

// ─── Render list ────────────────────────────────────────────────
// NOTE: All dynamic text is sanitized via escapeHtml() before insertion.
// This follows the same pattern used by workflows.js and other pages.

function renderList() {
    const container = document.getElementById('biz-list');
    if (businesses.length === 0) {
        container.textContent = '';
        const p = document.createElement('p');
        p.className = 'empty-state';
        p.textContent = 'No businesses yet. Launch one above.';
        container.appendChild(p);
        return;
    }

    container.textContent = '';
    businesses.forEach(b => {
        const card = document.createElement('div');
        card.className = 'provider-card' + (b.id === selectedBizId ? ' selected' : '');
        card.dataset.id = b.id;
        card.addEventListener('click', () => selectBusiness(b.id));

        const budget = b.budget_total
            ? `${fmt(b.budget_spent)} / ${fmt(b.budget_total)} ${escapeHtml(b.budget_currency)}`
            : 'No budget';

        const header = document.createElement('div');
        header.className = 'provider-card-header';
        const strong = document.createElement('strong');
        strong.textContent = b.name;
        header.appendChild(strong);
        const badgeSpan = document.createElement('span');
        badgeSpan.className = 'badge ' + ({
            planning: 'badge-neutral', active: 'badge-success',
            paused: 'badge-warning', closed: 'badge-error',
        }[b.status] || 'badge-neutral');
        badgeSpan.textContent = b.status;
        header.appendChild(badgeSpan);

        const body = document.createElement('div');
        body.className = 'provider-card-body';
        body.style.cssText = 'font-size:.85em;color:var(--text-dim)';

        const descDiv = document.createElement('div');
        descDiv.textContent = b.description || '';
        body.appendChild(descDiv);

        const metaDiv = document.createElement('div');
        metaDiv.style.marginTop = '.3em';
        metaDiv.textContent = `Autonomy: ${b.autonomy_level} \u00b7 Budget: ${budget}`;
        body.appendChild(metaDiv);

        const dateDiv = document.createElement('div');
        dateDiv.style.cssText = 'margin-top:.2em;font-size:.8em';
        dateDiv.textContent = formatTs(b.created_at);
        body.appendChild(dateDiv);

        card.appendChild(header);
        card.appendChild(body);
        container.appendChild(card);
    });
}

// ─── Select & detail ────────────────────────────────────────────

async function selectBusiness(id) {
    selectedBizId = id;
    renderList();
    const panel = document.getElementById('biz-detail-panel');
    panel.style.display = '';

    try {
        const data = await apiRequest(`/v1/business/${id}`);
        const biz = data.business;
        const rev = data.revenue;

        document.getElementById('biz-detail-name').textContent = biz.name;
        document.getElementById('biz-d-revenue').textContent = fmt(rev.income);
        document.getElementById('biz-d-expenses').textContent = fmt(rev.expenses);
        document.getElementById('biz-d-profit').textContent = fmt(rev.profit);
        document.getElementById('stat-biz-revenue').textContent = fmt(rev.income);
        document.getElementById('stat-biz-profit').textContent = fmt(rev.profit);

        // Show/hide pause/resume
        const btnPause = document.getElementById('btn-biz-pause');
        const btnResume = document.getElementById('btn-biz-resume');
        btnPause.style.display = biz.status === 'active' ? '' : 'none';
        btnResume.style.display = biz.status === 'paused' ? '' : 'none';

        // Info section
        const info = document.getElementById('biz-detail-info');
        info.textContent = '';
        const infoDiv = document.createElement('div');
        infoDiv.style.cssText = 'margin:.5em 0;font-size:.9em;color:var(--text-dim)';

        const budgetLine = biz.budget_total
            ? `Budget: ${fmt(biz.budget_spent)} / ${fmt(biz.budget_total)} ${biz.budget_currency} (remaining: ${fmt(biz.budget_total - biz.budget_spent)})`
            : 'No budget set';

        const badge = document.createElement('span');
        badge.className = 'badge ' + ({
            planning: 'badge-neutral', active: 'badge-success',
            paused: 'badge-warning', closed: 'badge-error',
        }[biz.status] || 'badge-neutral');
        badge.textContent = biz.status;
        infoDiv.appendChild(badge);
        infoDiv.appendChild(document.createTextNode(` \u00b7 Autonomy: `));
        const autoStrong = document.createElement('strong');
        autoStrong.textContent = biz.autonomy_level;
        infoDiv.appendChild(autoStrong);
        infoDiv.appendChild(document.createElement('br'));
        infoDiv.appendChild(document.createTextNode(budgetLine));
        infoDiv.appendChild(document.createElement('br'));
        infoDiv.appendChild(document.createTextNode(`OODA interval: ${biz.ooda_interval} \u00b7 Created: ${formatTs(biz.created_at)}`));
        info.appendChild(infoDiv);

        loadStrategies(id);
        loadProducts(id);
        loadTransactions(id);
    } catch (e) {
        showToast('Failed to load business: ' + e.message, 'error');
    }
}

async function loadStrategies(id) {
    try {
        const data = await apiRequest(`/v1/business/${id}/strategies`);
        const list = data.strategies || [];
        const el = document.getElementById('biz-strategies-list');
        el.textContent = '';
        if (list.length === 0) {
            const p = document.createElement('p');
            p.className = 'empty-state';
            p.textContent = 'No strategies yet.';
            el.appendChild(p);
            return;
        }
        list.forEach(s => {
            const row = document.createElement('div');
            row.className = 'item-row';
            const inner = document.createElement('div');
            const name = document.createElement('strong');
            name.textContent = s.name;
            inner.appendChild(name);
            inner.appendChild(document.createTextNode(' '));
            const badge = document.createElement('span');
            badge.className = 'badge ' + ({
                proposed: 'badge-neutral', approved: 'badge-info',
                active: 'badge-success', pivoted: 'badge-warning', abandoned: 'badge-error',
            }[s.status] || 'badge-neutral');
            badge.textContent = s.status;
            inner.appendChild(badge);
            const hypo = document.createElement('div');
            hypo.style.cssText = 'font-size:.85em;color:var(--text-dim)';
            hypo.textContent = s.hypothesis;
            inner.appendChild(hypo);
            row.appendChild(inner);
            el.appendChild(row);
        });
    } catch (e) {
        console.error('Failed to load strategies:', e);
    }
}

async function loadProducts(id) {
    try {
        const data = await apiRequest(`/v1/business/${id}/products`);
        const list = data.products || [];
        const el = document.getElementById('biz-products-list');
        document.getElementById('stat-biz-products').textContent = list.length;
        el.textContent = '';
        if (list.length === 0) {
            const p = document.createElement('p');
            p.className = 'empty-state';
            p.textContent = 'No products yet.';
            el.appendChild(p);
            return;
        }
        list.forEach(p => {
            const row = document.createElement('div');
            row.className = 'item-row';
            const inner = document.createElement('div');
            const name = document.createElement('strong');
            name.textContent = p.name;
            inner.appendChild(name);
            inner.appendChild(document.createTextNode(' '));
            const badge = document.createElement('span');
            badge.className = 'badge ' + ({
                draft: 'badge-neutral', active: 'badge-success', discontinued: 'badge-error',
            }[p.status] || 'badge-neutral');
            badge.textContent = p.status;
            inner.appendChild(badge);
            const meta = document.createElement('div');
            meta.style.cssText = 'font-size:.85em;color:var(--text-dim)';
            meta.textContent = `${fmt(p.price)} ${p.currency} \u00b7 ${p.units_sold} sold \u00b7 ${fmt(p.revenue_total)} revenue`;
            inner.appendChild(meta);
            row.appendChild(inner);
            el.appendChild(row);
        });
    } catch (e) {
        console.error('Failed to load products:', e);
    }
}

async function loadTransactions(id) {
    try {
        const data = await apiRequest(`/v1/business/${id}/transactions`);
        const list = data.transactions || [];
        const el = document.getElementById('biz-transactions-list');
        el.textContent = '';
        if (list.length === 0) {
            const p = document.createElement('p');
            p.className = 'empty-state';
            p.textContent = 'No transactions yet.';
            el.appendChild(p);
            return;
        }
        list.slice(0, 20).forEach(tx => {
            const sign = tx.tx_type === 'expense' ? '-' : '+';
            const color = tx.tx_type === 'expense' ? 'var(--color-error)' : 'var(--color-success)';

            const row = document.createElement('div');
            row.className = 'item-row';
            const inner = document.createElement('div');

            const amount = document.createElement('span');
            amount.style.cssText = `color:${color};font-weight:600`;
            amount.textContent = `${sign}${fmt(tx.amount)} ${tx.currency}`;
            inner.appendChild(amount);

            const typeBadge = document.createElement('span');
            typeBadge.className = 'badge badge-neutral';
            typeBadge.style.marginLeft = '.5em';
            typeBadge.textContent = tx.tx_type;
            inner.appendChild(typeBadge);

            const meta = document.createElement('div');
            meta.style.cssText = 'font-size:.85em;color:var(--text-dim)';
            meta.textContent = `${tx.description || tx.category || ''} \u00b7 ${formatTs(tx.recorded_at)}`;
            inner.appendChild(meta);

            row.appendChild(inner);
            el.appendChild(row);
        });
    } catch (e) {
        console.error('Failed to load transactions:', e);
    }
}

// ─── Actions ────────────────────────────────────────────────────

async function createBusiness(e) {
    e.preventDefault();
    const name = document.getElementById('biz-name').value.trim();
    if (!name) { showToast('Name is required', 'error'); return; }

    const body = {
        name,
        description: document.getElementById('biz-description').value.trim() || undefined,
        autonomy: document.getElementById('biz-autonomy').value,
        currency: document.getElementById('biz-currency').value.trim() || 'EUR',
        deliver_to: document.getElementById('biz-deliver-to').value || undefined,
    };
    const budgetVal = document.getElementById('biz-budget').value;
    if (budgetVal) body.budget = parseFloat(budgetVal);

    try {
        const data = await apiRequest('/v1/business', {
            method: 'POST',
            body: JSON.stringify(body),
        });
        showToast(data.message || 'Business launched');
        document.getElementById('biz-create-form').reset();
        document.getElementById('biz-currency').value = 'EUR';
        await loadBusinesses();
        if (data.business) selectBusiness(data.business.id);
    } catch (e) {
        showToast('Failed: ' + e.message, 'error');
    }
}

async function pauseBusiness() {
    if (!selectedBizId) return;
    try {
        await apiRequest(`/v1/business/${selectedBizId}/pause`, { method: 'POST' });
        showToast('Business paused');
        await loadBusinesses();
        selectBusiness(selectedBizId);
    } catch (e) {
        showToast('Failed: ' + e.message, 'error');
    }
}

async function resumeBusiness() {
    if (!selectedBizId) return;
    try {
        await apiRequest(`/v1/business/${selectedBizId}/resume`, { method: 'POST' });
        showToast('Business resumed');
        await loadBusinesses();
        selectBusiness(selectedBizId);
    } catch (e) {
        showToast('Failed: ' + e.message, 'error');
    }
}

async function closeBusiness() {
    if (!selectedBizId) return;
    if (!confirm('Close this business permanently?')) return;
    try {
        await apiRequest(`/v1/business/${selectedBizId}/close`, { method: 'POST' });
        showToast('Business closed');
        selectedBizId = null;
        document.getElementById('biz-detail-panel').style.display = 'none';
        await loadBusinesses();
    } catch (e) {
        showToast('Failed: ' + e.message, 'error');
    }
}

// ─── Init ───────────────────────────────────────────────────────

document.addEventListener('DOMContentLoaded', () => {
    document.getElementById('biz-create-form').addEventListener('submit', createBusiness);
    document.getElementById('btn-biz-refresh').addEventListener('click', loadBusinesses);
    document.getElementById('btn-biz-pause').addEventListener('click', pauseBusiness);
    document.getElementById('btn-biz-resume').addEventListener('click', resumeBusiness);
    document.getElementById('btn-biz-close').addEventListener('click', closeBusiness);

    loadBizDeliveryTargets();
    loadBusinesses();
});
