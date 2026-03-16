// Homun — Dashboard Usage Analytics
// Pricing tables, cost estimation, chart rendering, and usage controls.
// Depends on helpers from dashboard.js (showToast, escapeHtml, formatInt, formatUsd).

// ─── Pricing Tables + Cost Helpers ───

var MODEL_PRICING_PER_1M = [
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

var PROVIDER_FALLBACK_PER_1M = {
    openai: { inUsd: 1.0, outUsd: 4.0 },
    anthropic: { inUsd: 3.0, outUsd: 15.0 },
    google: { inUsd: 0.3, outUsd: 1.0 },
    openrouter: { inUsd: 1.2, outUsd: 4.5 },
};

function toLocalIsoDate(d) {
    var tzOffset = d.getTimezoneOffset() * 60000;
    return new Date(d.getTime() - tzOffset).toISOString().slice(0, 10);
}

function getPricingFor(model, provider) {
    var modelLc = String(model || '').toLowerCase();
    var providerLc = String(provider || '').toLowerCase();
    var explicit = MODEL_PRICING_PER_1M.find(function (e) { return modelLc.includes(e.pattern); });
    if (explicit) return { inUsd: explicit.inUsd, outUsd: explicit.outUsd, source: 'model' };
    if (PROVIDER_FALLBACK_PER_1M[providerLc]) {
        return { inUsd: PROVIDER_FALLBACK_PER_1M[providerLc].inUsd, outUsd: PROVIDER_FALLBACK_PER_1M[providerLc].outUsd, source: 'provider' };
    }
    return null;
}

function estimateRowCost(row) {
    var pricing = getPricingFor(row.model, row.provider);
    if (!pricing) return { known: false, usd: 0 };
    var promptCost = (Number(row.prompt_tokens || 0) / 1000000) * pricing.inUsd;
    var completionCost = (Number(row.completion_tokens || 0) / 1000000) * pricing.outUsd;
    return { known: true, usd: promptCost + completionCost, pricingSource: pricing.source };
}

// ─── Usage Analytics ───

var usageEls = {
    since: document.getElementById('usage-since'),
    until: document.getElementById('usage-until'),
    refresh: document.getElementById('usage-refresh'),
    rangeButtons: Array.from(document.querySelectorAll('.usage-range-btn')),
    estimatedCost: document.getElementById('usage-estimated-cost'),
    totalCalls: document.getElementById('usage-total-calls'),
    chart: document.getElementById('usage-chart'),
    chartEmpty: document.getElementById('usage-chart-empty'),
    split: document.getElementById('usage-split'),
};

function setPreset(days) {
    usageEls.rangeButtons.forEach(function (btn) {
        btn.classList.toggle('is-active', btn.dataset.days === String(days));
    });
    if (days === 'all') {
        usageEls.since.value = '';
        usageEls.until.value = '';
        return;
    }
    var dayCount = parseInt(days, 10);
    if (!Number.isFinite(dayCount)) return;
    var until = new Date();
    var since = new Date();
    since.setDate(until.getDate() - (dayCount - 1));
    usageEls.since.value = toLocalIsoDate(since);
    usageEls.until.value = toLocalIsoDate(until);
}

function clearPresetSelection() {
    usageEls.rangeButtons.forEach(function (btn) { btn.classList.remove('is-active'); });
}

function buildUsageQuery() {
    var params = new URLSearchParams();
    if (usageEls.since.value) params.set('since', usageEls.since.value + ' 00:00:00');
    if (usageEls.until.value) params.set('until', usageEls.until.value + ' 23:59:59');
    return params.toString();
}

// renderSplit and renderDailyChart use innerHTML for SVG rendering only.
// These generate purely decorative chart elements from numeric data (no user content).

function renderSplit(promptTokens, completionTokens) {
    var prompt = Number(promptTokens || 0);
    var completion = Number(completionTokens || 0);
    var total = Math.max(prompt + completion, 1);
    usageEls.split.innerHTML = [
        { key: 'Prompt', cls: 'prompt', value: prompt, share: (prompt / total) * 100 },
        { key: 'Completion', cls: 'completion', value: completion, share: (completion / total) * 100 },
    ].map(function (r) {
        return '<div class="usage-split-row">'
            + '<span class="usage-split-label">' + r.key + '</span>'
            + '<div class="usage-split-bar"><div class="usage-split-fill ' + r.cls + '" style="width:' + Math.max(r.share, 1) + '%"></div></div>'
            + '<span class="usage-split-value">' + formatInt(r.value) + ' (' + r.share.toFixed(1) + '%)</span>'
            + '</div>';
    }).join('');
}

function renderDailyChart(days) {
    if (!usageEls.chart || !usageEls.chartEmpty) return;
    if (!Array.isArray(days) || days.length === 0) {
        usageEls.chart.innerHTML = '';
        usageEls.chartEmpty.hidden = false;
        return;
    }
    usageEls.chartEmpty.hidden = true;
    var width = 720, height = 220;
    var padLeft = 44, padRight = 14, padTop = 12, padBottom = 26;
    var chartW = width - padLeft - padRight;
    var chartH = height - padTop - padBottom;
    var maxValue = Math.max.apply(null, days.map(function (d) { return Number(d.total_tokens || 0); }).concat([1]));
    var barSlot = chartW / days.length;
    var barW = Math.max(3, Math.min(24, barSlot * 0.72));

    var yTicks = [0, 0.25, 0.5, 0.75, 1].map(function (ratio) {
        return { y: padTop + chartH - (ratio * chartH), value: Math.round(maxValue * ratio) };
    });

    var bars = days.map(function (d, i) {
        var v = Number(d.total_tokens || 0);
        var h = Math.max((v / maxValue) * chartH, v > 0 ? 1 : 0);
        var x = padLeft + (i * barSlot) + ((barSlot - barW) / 2);
        var y = padTop + chartH - h;
        return '<rect x="' + x.toFixed(2) + '" y="' + y.toFixed(2) + '" width="' + barW.toFixed(2) + '" height="' + h.toFixed(2) + '" rx="3" fill="var(--accent)" opacity="0.88"><title>' + escapeHtml(d.day) + ': ' + formatInt(v) + ' tokens</title></rect>';
    }).join('');

    var firstDay = days[0] ? days[0].day : '';
    var lastDay = days[days.length - 1] ? days[days.length - 1].day : '';

    usageEls.chart.innerHTML = '<g>' + yTicks.map(function (t) {
        return '<line x1="' + padLeft + '" y1="' + t.y + '" x2="' + (width - padRight) + '" y2="' + t.y + '" stroke="var(--border)" stroke-width="1"/>'
            + '<text x="' + (padLeft - 6) + '" y="' + (t.y + 4) + '" text-anchor="end" font-size="10" fill="var(--t4)" font-family="var(--mono)">' + formatInt(t.value) + '</text>';
    }).join('') + '</g><g>' + bars + '</g><g>'
        + '<text x="' + padLeft + '" y="' + (height - 6) + '" text-anchor="start" font-size="10" fill="var(--t4)" font-family="var(--mono)">' + escapeHtml(firstDay) + '</text>'
        + '<text x="' + (width - padRight) + '" y="' + (height - 6) + '" text-anchor="end" font-size="10" fill="var(--t4)" font-family="var(--mono)">' + escapeHtml(lastDay) + '</text></g>';
}

async function loadUsage() {
    if (!usageEls.refresh) return;
    usageEls.refresh.disabled = true;
    usageEls.refresh.textContent = 'Loading...';
    try {
        var query = buildUsageQuery();
        var url = query ? '/api/v1/usage?' + query : '/api/v1/usage';
        var res = await fetch(url);
        if (!res.ok) throw new Error('usage fetch failed: ' + res.status);

        var payload = await res.json();
        var totals = payload.totals || {};
        var models = Array.isArray(payload.models) ? payload.models : [];
        var days = Array.isArray(payload.days) ? payload.days : [];

        var totalEstimatedCost = 0;
        var unknownRows = 0;
        models.forEach(function (row) {
            var cost = estimateRowCost(row);
            if (cost.known) totalEstimatedCost += cost.usd;
            else unknownRows++;
        });

        usageEls.estimatedCost.textContent = formatUsd(totalEstimatedCost);
        var callSuffix = unknownRows > 0 ? ' \u00B7 ' + unknownRows + ' unpriced' : '';
        usageEls.totalCalls.textContent = formatInt(totals.call_count) + ' calls' + callSuffix;

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
    usageEls.rangeButtons.forEach(function (btn) {
        btn.addEventListener('click', async function () {
            setPreset(btn.dataset.days || '7');
            await loadUsage();
        });
    });
    usageEls.since.addEventListener('change', clearPresetSelection);
    usageEls.until.addEventListener('change', clearPresetSelection);
    usageEls.refresh.addEventListener('click', loadUsage);
    loadUsage();
}
