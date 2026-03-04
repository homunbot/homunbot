// Homun — Dashboard inline editing + usage analytics

// ─── Inline Stat Card Editing ───
document.querySelectorAll('.stat-card[data-editable]').forEach((card) => {
    card.addEventListener('click', (e) => {
        if (card.classList.contains('editing') || e.target.closest('.inline-edit')) return;
        card.classList.add('editing');
        const input = card.querySelector('.inline-input');
        if (input) {
            input.focus();
            input.select();
        }
    });
});

// Save handler for inline edits
document.querySelectorAll('.inline-edit').forEach((form) => {
    const card = form.closest('.stat-card');
    const key = card?.dataset.key;
    const input = form.querySelector('.inline-input');
    const saveBtn = form.querySelector('.btn-save');
    const cancelBtn = form.querySelector('.btn-cancel');

    async function save() {
        if (!key || !input) return;
        const value = input.value.trim();
        if (!value) return cancel();

        try {
            await fetch('/api/v1/config', {
                method: 'PATCH',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ key, value }),
            });
            const valEl = card.querySelector('.stat-value');
            if (valEl) valEl.textContent = value;
            card.classList.remove('editing');
            showToast('Saved. Restart to apply.', 'success');
        } catch (_) {
            showToast('Failed to save', 'error');
        }
    }

    function cancel() {
        card.classList.remove('editing');
        const valEl = card.querySelector('.stat-value');
        if (valEl && input) input.value = valEl.textContent;
    }

    if (saveBtn) {
        saveBtn.addEventListener('click', (e) => {
            e.stopPropagation();
            save();
        });
    }

    if (cancelBtn) {
        cancelBtn.addEventListener('click', (e) => {
            e.stopPropagation();
            cancel();
        });
    }

    if (input) {
        input.addEventListener('keydown', (e) => {
            e.stopPropagation();
            if (e.key === 'Enter') save();
            if (e.key === 'Escape') cancel();
        });
        input.addEventListener('click', (e) => e.stopPropagation());
    }
});

// ─── Toast notifications ───
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
    }, 2500);
}

// ─── Live uptime counter ───
const uptimeEl = document.querySelector('[data-live-uptime]');
if (uptimeEl) {
    const startSecs = parseInt(uptimeEl.dataset.liveUptime, 10);
    const startedAt = Date.now() - (startSecs * 1000);

    function updateUptime() {
        const secs = Math.floor((Date.now() - startedAt) / 1000);
        if (secs < 60) uptimeEl.textContent = `${secs}s`;
        else if (secs < 3600) uptimeEl.textContent = `${Math.floor(secs / 60)}m ${secs % 60}s`;
        else if (secs < 86400) {
            uptimeEl.textContent = `${Math.floor(secs / 3600)}h ${Math.floor((secs % 3600) / 60)}m`;
        } else {
            uptimeEl.textContent = `${Math.floor(secs / 86400)}d ${Math.floor((secs % 86400) / 3600)}h`;
        }
    }

    updateUptime();
    setInterval(updateUptime, 1000);
}

// ─── Usage analytics ───
const usageEls = {
    since: document.getElementById('usage-since'),
    until: document.getElementById('usage-until'),
    refresh: document.getElementById('usage-refresh'),
    rangeButtons: Array.from(document.querySelectorAll('.usage-range-btn')),
    totalTokens: document.getElementById('usage-total-tokens'),
    promptTokens: document.getElementById('usage-prompt-tokens'),
    completionTokens: document.getElementById('usage-completion-tokens'),
    totalCalls: document.getElementById('usage-total-calls'),
    estimatedCost: document.getElementById('usage-estimated-cost'),
    daysCount: document.getElementById('usage-days-count'),
    chart: document.getElementById('usage-chart'),
    chartEmpty: document.getElementById('usage-chart-empty'),
    split: document.getElementById('usage-split'),
    modelsBody: document.getElementById('usage-models-body'),
};

const MODEL_PRICING_PER_1M = [
    { pattern: 'gpt-4o-mini', inUsd: 0.15, outUsd: 0.6 },
    { pattern: 'gpt-4.1-mini', inUsd: 0.4, outUsd: 1.6 },
    { pattern: 'gpt-4.1', inUsd: 2.0, outUsd: 8.0 },
    { pattern: 'gpt-4o', inUsd: 2.5, outUsd: 10.0 },
    { pattern: 'gpt-3.5', inUsd: 0.5, outUsd: 1.5 },
    { pattern: 'claude-3-5-haiku', inUsd: 0.8, outUsd: 4.0 },
    { pattern: 'claude-3-5-sonnet', inUsd: 3.0, outUsd: 15.0 },
    { pattern: 'claude-3-7-sonnet', inUsd: 3.0, outUsd: 15.0 },
    { pattern: 'claude-sonnet-4', inUsd: 3.0, outUsd: 15.0 },
    { pattern: 'claude-opus-4', inUsd: 15.0, outUsd: 75.0 },
    { pattern: 'gemini-2.0-flash', inUsd: 0.1, outUsd: 0.4 },
    { pattern: 'gemini-1.5-flash', inUsd: 0.15, outUsd: 0.6 },
    { pattern: 'gemini-1.5-pro', inUsd: 1.25, outUsd: 5.0 },
    { pattern: 'llama', inUsd: 0.2, outUsd: 0.2 },
    { pattern: 'mistral', inUsd: 0.2, outUsd: 0.6 },
];

const PROVIDER_FALLBACK_PER_1M = {
    openai: { inUsd: 1.0, outUsd: 4.0 },
    anthropic: { inUsd: 3.0, outUsd: 15.0 },
    google: { inUsd: 0.3, outUsd: 1.0 },
    openrouter: { inUsd: 1.2, outUsd: 4.5 },
};

function toLocalIsoDate(d) {
    const tzOffset = d.getTimezoneOffset() * 60000;
    return new Date(d.getTime() - tzOffset).toISOString().slice(0, 10);
}

function formatInt(value) {
    return new Intl.NumberFormat('en-US').format(Number(value || 0));
}

function formatUsd(value) {
    return `$${Number(value || 0).toFixed(4)}`;
}

function escapeHtml(value) {
    return String(value || '')
        .replaceAll('&', '&amp;')
        .replaceAll('<', '&lt;')
        .replaceAll('>', '&gt;')
        .replaceAll('"', '&quot;')
        .replaceAll("'", '&#039;');
}

function getPricingFor(model, provider) {
    const modelLc = String(model || '').toLowerCase();
    const providerLc = String(provider || '').toLowerCase();

    const explicit = MODEL_PRICING_PER_1M.find((entry) => modelLc.includes(entry.pattern));
    if (explicit) return { ...explicit, source: 'model' };

    if (PROVIDER_FALLBACK_PER_1M[providerLc]) {
        return { ...PROVIDER_FALLBACK_PER_1M[providerLc], source: 'provider' };
    }

    return null;
}

function estimateRowCost(row) {
    const pricing = getPricingFor(row.model, row.provider);
    if (!pricing) return { known: false, usd: 0 };

    const promptCost = (Number(row.prompt_tokens || 0) / 1_000_000) * pricing.inUsd;
    const completionCost = (Number(row.completion_tokens || 0) / 1_000_000) * pricing.outUsd;
    return { known: true, usd: promptCost + completionCost, pricingSource: pricing.source };
}

function setPreset(days) {
    usageEls.rangeButtons.forEach((btn) => {
        btn.classList.toggle('is-active', btn.dataset.days === String(days));
    });

    if (days === 'all') {
        usageEls.since.value = '';
        usageEls.until.value = '';
        return;
    }

    const dayCount = parseInt(days, 10);
    if (!Number.isFinite(dayCount)) return;

    const until = new Date();
    const since = new Date();
    since.setDate(until.getDate() - (dayCount - 1));

    usageEls.since.value = toLocalIsoDate(since);
    usageEls.until.value = toLocalIsoDate(until);
}

function clearPresetSelection() {
    usageEls.rangeButtons.forEach((btn) => btn.classList.remove('is-active'));
}

function buildUsageQuery() {
    const params = new URLSearchParams();
    if (usageEls.since.value) params.set('since', `${usageEls.since.value} 00:00:00`);
    if (usageEls.until.value) params.set('until', `${usageEls.until.value} 23:59:59`);
    return params.toString();
}

function renderSplit(promptTokens, completionTokens) {
    const prompt = Number(promptTokens || 0);
    const completion = Number(completionTokens || 0);
    const total = Math.max(prompt + completion, 1);

    usageEls.split.innerHTML = [
        {
            key: 'Prompt',
            cls: 'prompt',
            value: prompt,
            share: (prompt / total) * 100,
        },
        {
            key: 'Completion',
            cls: 'completion',
            value: completion,
            share: (completion / total) * 100,
        },
    ]
        .map(
            (r) => `
                <div class="usage-split-row">
                    <span class="usage-split-label">${r.key}</span>
                    <div class="usage-split-bar">
                        <div class="usage-split-fill ${r.cls}" style="width: ${Math.max(r.share, 1)}%"></div>
                    </div>
                    <span class="usage-split-value">${formatInt(r.value)} (${r.share.toFixed(1)}%)</span>
                </div>
            `,
        )
        .join('');
}

function renderDailyChart(days) {
    if (!usageEls.chart || !usageEls.chartEmpty) return;

    if (!Array.isArray(days) || days.length === 0) {
        usageEls.chart.innerHTML = '';
        usageEls.chartEmpty.hidden = false;
        return;
    }

    usageEls.chartEmpty.hidden = true;

    const width = 720;
    const height = 220;
    const padLeft = 44;
    const padRight = 14;
    const padTop = 12;
    const padBottom = 26;
    const chartW = width - padLeft - padRight;
    const chartH = height - padTop - padBottom;
    const maxValue = Math.max(...days.map((d) => Number(d.total_tokens || 0)), 1);
    const barSlot = chartW / days.length;
    const barW = Math.max(3, Math.min(24, barSlot * 0.72));

    const yTicks = [0, 0.25, 0.5, 0.75, 1].map((ratio) => {
        const y = padTop + chartH - (ratio * chartH);
        const value = Math.round(maxValue * ratio);
        return {
            y,
            value,
        };
    });

    const bars = days
        .map((d, i) => {
            const v = Number(d.total_tokens || 0);
            const h = Math.max((v / maxValue) * chartH, v > 0 ? 1 : 0);
            const x = padLeft + (i * barSlot) + ((barSlot - barW) / 2);
            const y = padTop + chartH - h;
            return `<rect x="${x.toFixed(2)}" y="${y.toFixed(2)}" width="${barW.toFixed(2)}" height="${h.toFixed(2)}" rx="3" fill="var(--accent)" opacity="0.88"><title>${escapeHtml(d.day)}: ${formatInt(v)} tokens</title></rect>`;
        })
        .join('');

    const firstDay = days[0]?.day || '';
    const lastDay = days[days.length - 1]?.day || '';

    usageEls.chart.innerHTML = `
        <g>
            ${yTicks
                .map(
                    (t) => `
                        <line x1="${padLeft}" y1="${t.y}" x2="${width - padRight}" y2="${t.y}" stroke="var(--border)" stroke-width="1" />
                        <text x="${padLeft - 6}" y="${t.y + 4}" text-anchor="end" font-size="10" fill="var(--t4)" font-family="var(--mono)">${formatInt(t.value)}</text>
                    `,
                )
                .join('')}
        </g>
        <g>${bars}</g>
        <g>
            <text x="${padLeft}" y="${height - 6}" text-anchor="start" font-size="10" fill="var(--t4)" font-family="var(--mono)">${escapeHtml(firstDay)}</text>
            <text x="${width - padRight}" y="${height - 6}" text-anchor="end" font-size="10" fill="var(--t4)" font-family="var(--mono)">${escapeHtml(lastDay)}</text>
        </g>
    `;
}

function renderModelsTable(models) {
    if (!Array.isArray(models) || models.length === 0) {
        usageEls.modelsBody.innerHTML = `
            <tr>
                <td colspan="7" class="usage-loading">No usage rows in selected range.</td>
            </tr>
        `;
        return { totalEstimatedCost: 0, unknownRows: 0 };
    }

    let totalEstimatedCost = 0;
    let unknownRows = 0;

    const rowsHtml = models
        .map((row) => {
            const cost = estimateRowCost(row);
            if (cost.known) totalEstimatedCost += cost.usd;
            else unknownRows += 1;

            return `
                <tr>
                    <td>${escapeHtml(row.model)}</td>
                    <td>${escapeHtml(row.provider)}</td>
                    <td class="usage-num">${formatInt(row.prompt_tokens)}</td>
                    <td class="usage-num">${formatInt(row.completion_tokens)}</td>
                    <td class="usage-num">${formatInt(row.total_tokens)}</td>
                    <td class="usage-num">${formatInt(row.call_count)}</td>
                    <td class="usage-num">${cost.known ? formatUsd(cost.usd) : 'n/a'}</td>
                </tr>
            `;
        })
        .join('');

    usageEls.modelsBody.innerHTML = rowsHtml;
    return { totalEstimatedCost, unknownRows };
}

async function loadUsage() {
    if (!usageEls.refresh) return;

    usageEls.refresh.disabled = true;
    usageEls.refresh.textContent = 'Loading...';

    try {
        const query = buildUsageQuery();
        const url = query ? `/api/v1/usage?${query}` : '/api/v1/usage';
        const res = await fetch(url);
        if (!res.ok) {
            throw new Error(`usage fetch failed: ${res.status}`);
        }

        const payload = await res.json();
        const totals = payload.totals || {};
        const models = Array.isArray(payload.models) ? payload.models : [];
        const days = Array.isArray(payload.days) ? payload.days : [];

        usageEls.totalTokens.textContent = formatInt(totals.total_tokens);
        usageEls.promptTokens.textContent = formatInt(totals.prompt_tokens);
        usageEls.completionTokens.textContent = formatInt(totals.completion_tokens);
        usageEls.daysCount.textContent = `${formatInt(days.length)} day(s) with usage`;

        const { totalEstimatedCost, unknownRows } = renderModelsTable(models);
        usageEls.estimatedCost.textContent = formatUsd(totalEstimatedCost);

        const callSuffix = unknownRows > 0 ? ` • ${unknownRows} model(s) unpriced` : '';
        usageEls.totalCalls.textContent = `${formatInt(totals.call_count)} calls${callSuffix}`;

        renderSplit(totals.prompt_tokens, totals.completion_tokens);
        renderDailyChart(days);
    } catch (err) {
        console.error(err);
        showToast('Failed to load usage data', 'error');
    } finally {
        usageEls.refresh.disabled = false;
        usageEls.refresh.textContent = 'Refresh';
    }
}

if (usageEls.refresh && usageEls.since && usageEls.until) {
    setPreset('7');

    usageEls.rangeButtons.forEach((btn) => {
        btn.addEventListener('click', async () => {
            setPreset(btn.dataset.days || '7');
            await loadUsage();
        });
    });

    usageEls.since.addEventListener('change', () => clearPresetSelection());
    usageEls.until.addEventListener('change', () => clearPresetSelection());

    usageEls.refresh.addEventListener('click', loadUsage);
    loadUsage();
}
