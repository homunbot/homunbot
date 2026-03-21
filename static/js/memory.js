// Homun — Memory page interactivity

// ─── Profile filter ───
(async function initProfileFilter() {
    const select = document.getElementById('memory-profile-filter');
    if (!select) return;

    // Add "All profiles" option
    const allOpt = document.createElement('option');
    allOpt.value = '';
    allOpt.textContent = 'All profiles';
    select.appendChild(allOpt);

    try {
        const res = await fetch('/api/v1/profiles');
        if (!res.ok) return;
        const profiles = await res.json();
        profiles.forEach(p => {
            const opt = document.createElement('option');
            opt.value = p.slug;
            opt.textContent = (p.avatar_emoji || '\u{1F464}') + ' ' + p.display_name;
            select.appendChild(opt);
        });
    } catch (_) {}

    select.addEventListener('change', () => {
        // Reload all sections with the new profile filter
        if (typeof historyOffset !== 'undefined') historyOffset = 0;
        if (typeof loadHistory === 'function') loadHistory();
        if (typeof loadMemoryFile === 'function') loadMemoryFile();
        if (typeof loadInstructions === 'function') loadInstructions();
        reloadMemoryStats();
    });
})();

/** Get the current profile filter slug (empty = all). */
function getProfileFilter() {
    const el = document.getElementById('memory-profile-filter');
    return el ? el.value : '';
}

/** Reload memory stats cards with profile filter. */
async function reloadMemoryStats() {
    try {
        const pf = getProfileFilter();
        const profileParam = pf ? '?profile=' + encodeURIComponent(pf) : '';
        const resp = await fetch('/api/v1/memory/stats' + profileParam);
        if (!resp.ok) return;
        const data = await resp.json();
        const el = (id) => document.getElementById(id);
        if (el('mem-stat-chunks')) el('mem-stat-chunks').textContent = data.chunk_count;
        if (el('mem-stat-daily')) el('mem-stat-daily').textContent = data.daily_count;
        const fileCount = [data.has_memory_md, data.has_instructions_md].filter(Boolean).length;
        if (el('mem-stat-files')) el('mem-stat-files').textContent = fileCount;
        const parts = [];
        if (data.has_memory_md) parts.push('MEMORY.md');
        if (data.has_instructions_md) parts.push('INSTRUCTIONS.md');
        if (el('mem-stat-files-detail')) el('mem-stat-files-detail').textContent = parts.join(' + ') || 'none';
    } catch (_) {}
}

/** Escape HTML entities to prevent XSS — all dynamic content passes through this. */
function esc(s) {
    const d = document.createElement('div');
    d.textContent = s;
    return d.innerHTML;
}

// ─── Search ───
const searchInput = document.getElementById('memory-search-input');
const searchResults = document.getElementById('search-results');
let searchTimer = null;

if (searchInput) {
    searchInput.addEventListener('input', () => {
        clearTimeout(searchTimer);
        const q = searchInput.value.trim();
        if (q.length < 2) {
            searchResults.style.display = 'none';
            searchResults.textContent = '';
            return;
        }
        searchTimer = setTimeout(() => searchMemory(q), 300);
    });
}

async function searchMemory(q) {
    try {
        const resp = await fetch(`/api/v1/memory/search?q=${encodeURIComponent(q)}&limit=20`);
        const data = await resp.json();
        // Clear existing results using safe DOM methods
        searchResults.textContent = '';

        if (!data.chunks || data.chunks.length === 0) {
            const row = document.createElement('div');
            row.className = 'item-row';
            const info = document.createElement('div');
            info.className = 'item-info';
            const name = document.createElement('div');
            name.className = 'item-name';
            name.style.color = 'var(--t4)';
            name.textContent = 'No results found';
            info.appendChild(name);
            row.appendChild(info);
            searchResults.appendChild(row);
            searchResults.style.display = 'block';
            return;
        }
        data.chunks.forEach(c => {
            searchResults.appendChild(buildChunkRow(c));
        });
        searchResults.style.display = 'block';
    } catch (e) {
        showToast('Search failed', 'error');
    }
}

/** Build a DOM row for a memory chunk (safe, no innerHTML). */
function buildChunkRow(c) {
    const row = document.createElement('div');
    row.className = 'item-row';

    const info = document.createElement('div');
    info.className = 'item-info';
    info.style.minWidth = '0';

    const wrap = document.createElement('div');
    const name = document.createElement('div');
    name.className = 'item-name';
    name.textContent = c.heading || c.memory_type;

    const detail = document.createElement('div');
    detail.className = 'item-detail';
    detail.textContent = c.content.length > 120
        ? c.content.slice(0, 120) + '…'
        : c.content;

    wrap.appendChild(name);
    wrap.appendChild(detail);
    info.appendChild(wrap);

    const badges = document.createElement('div');
    badges.style.cssText = 'display:flex;gap:6px;align-items:center;flex-shrink:0';

    // Show relevance score badge when available (hybrid search)
    if (c.score != null) {
        const scoreBadge = document.createElement('span');
        const pct = Math.round(c.score * 100);
        scoreBadge.className = pct >= 50 ? 'badge badge-success' : 'badge badge-warning';
        scoreBadge.textContent = pct + '%';
        scoreBadge.title = 'Relevance score (hybrid vector + keyword search)';
        badges.appendChild(scoreBadge);
    }

    const dateBadge = document.createElement('span');
    dateBadge.className = 'badge badge-neutral';
    dateBadge.textContent = c.date;
    badges.appendChild(dateBadge);

    row.appendChild(info);
    row.appendChild(badges);
    return row;
}

// ─── MEMORY.md Editor ───
const textarea = document.getElementById('memory-textarea');
const btnSave = document.getElementById('btn-save-memory');
const btnReload = document.getElementById('btn-reload-memory');
const memStatus = document.getElementById('memory-status');

async function loadMemoryFile() {
    try {
        const pf = getProfileFilter();
        const profileParam = pf ? '&profile=' + encodeURIComponent(pf) : '';
        const resp = await fetch('/api/v1/memory/content?file=memory' + profileParam);
        const data = await resp.json();
        if (textarea) textarea.value = data.content || '';
    } catch (e) { /* ignore */ }
}

if (btnSave) {
    btnSave.addEventListener('click', async () => {
        try {
            const pf = getProfileFilter();
            await fetch('/api/v1/memory/content', {
                method: 'PUT',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ file: 'memory', content: textarea.value, profile: pf || undefined }),
            });
            showToast('MEMORY.md saved');
            if (memStatus) memStatus.textContent = 'Saved ✓';
        } catch (e) {
            showToast('Failed to save', 'error');
        }
    });
}

if (btnReload) {
    btnReload.addEventListener('click', () => {
        loadMemoryFile();
        if (memStatus) memStatus.textContent = 'Reloaded';
    });
}

// ─── Instructions ───
const instructionsList = document.getElementById('instructions-list');
const instructionInput = document.getElementById('instruction-input');
const btnAddInstruction = document.getElementById('btn-add-instruction');
const instructionsHeader = document.getElementById('instructions-header');
const instructionsWrapper = document.getElementById('instructions-wrapper');
const instructionsCollapseIcon = document.getElementById('instructions-collapse-icon');
const instructionsCount = document.getElementById('instructions-count');
const btnDeduplicate = document.getElementById('btn-deduplicate-instructions');
let currentInstructions = [];
let instructionsCollapsed = false;

// Collapse/expand toggle
if (instructionsHeader) {
    instructionsHeader.addEventListener('click', () => {
        instructionsCollapsed = !instructionsCollapsed;
        if (instructionsWrapper) {
            instructionsWrapper.style.display = instructionsCollapsed ? 'none' : 'block';
        }
        if (instructionsCollapseIcon) {
            instructionsCollapseIcon.textContent = instructionsCollapsed ? '▶' : '▼';
        }
    });
}

// Update count badge
function updateInstructionsCount() {
    if (instructionsCount) {
        instructionsCount.textContent = currentInstructions.length > 0 ? currentInstructions.length : '';
    }
}

async function loadInstructions() {
    try {
        const pf = getProfileFilter();
        const profileParam = pf ? '?profile=' + encodeURIComponent(pf) : '';
        const resp = await fetch('/api/v1/memory/instructions' + profileParam);
        const data = await resp.json();
        currentInstructions = data.instructions || [];
        renderInstructions();
        updateInstructionsCount();
    } catch (e) { /* ignore */ }
}

function renderInstructions() {
    if (!instructionsList) return;
    instructionsList.textContent = '';
    updateInstructionsCount();

    if (currentInstructions.length === 0) {
        const row = document.createElement('div');
        row.className = 'item-row';
        const info = document.createElement('div');
        info.className = 'item-info';
        const name = document.createElement('div');
        name.className = 'item-name';
        name.style.color = 'var(--t4)';
        name.textContent = 'No instructions yet';
        info.appendChild(name);
        row.appendChild(info);
        instructionsList.appendChild(row);
        return;
    }

    currentInstructions.forEach((inst, i) => {
        const row = document.createElement('div');
        row.className = 'item-row';

        const info = document.createElement('div');
        info.className = 'item-info';
        info.style.minWidth = '0';
        const name = document.createElement('div');
        name.className = 'item-name';
        name.textContent = inst;
        info.appendChild(name);

        const btn = document.createElement('button');
        btn.className = 'btn btn-danger btn-sm';
        btn.textContent = 'Remove';
        btn.addEventListener('click', async () => {
            currentInstructions.splice(i, 1);
            await saveInstructions();
            renderInstructions();
        });

        row.appendChild(info);
        row.appendChild(btn);
        instructionsList.appendChild(row);
    });
}

// Deduplicate instructions - remove similar ones (>70% word overlap)
function deduplicateInstructions() {
    const original = currentInstructions.length;
    const deduped = [];
    const seen = new Set();
    
    for (const inst of currentInstructions) {
        const words = inst.toLowerCase().split(/\s+/).filter(w => w.length > 2);
        let isDuplicate = false;
        
        for (const existing of deduped) {
            const existingWords = existing.toLowerCase().split(/\s+/).filter(w => w.length > 2);
            const intersection = words.filter(w => existingWords.includes(w)).length;
            const union = new Set([...words, ...existingWords]).size;
            const similarity = union > 0 ? intersection / union : 0;
            
            if (similarity > 0.7) {
                isDuplicate = true;
                break;
            }
        }
        
        if (!isDuplicate) {
            deduped.push(inst);
        }
    }
    
    const removed = original - deduped.length;
    if (removed > 0) {
        currentInstructions = deduped;
        return removed;
    }
    return 0;
}

if (btnDeduplicate) {
    btnDeduplicate.addEventListener('click', async () => {
        const removed = deduplicateInstructions();
        if (removed > 0) {
            await saveInstructions();
            renderInstructions();
            showToast(`Removed ${removed} duplicate instruction${removed > 1 ? 's' : ''}`);
        } else {
            showToast('No duplicates found');
        }
    });
}

async function saveInstructions() {
    try {
        const pf = getProfileFilter();
        await fetch('/api/v1/memory/instructions', {
            method: 'PUT',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ instructions: currentInstructions, profile: pf || undefined }),
        });
        showToast('Instructions updated');
    } catch (e) {
        showToast('Failed to save instructions', 'error');
    }
}

if (btnAddInstruction) {
    btnAddInstruction.addEventListener('click', async () => {
        const val = instructionInput?.value.trim();
        if (!val) return;
        currentInstructions.push(val);
        instructionInput.value = '';
        await saveInstructions();
        renderInstructions();
    });
}

if (instructionInput) {
    instructionInput.addEventListener('keydown', (e) => {
        if (e.key === 'Enter') {
            e.preventDefault();
            btnAddInstruction?.click();
        }
    });
}

// ─── History ───
const historyList = document.getElementById('history-list');
const btnLoadMore = document.getElementById('btn-load-more');
const historyHeader = document.getElementById('history-header');
const historyWrapper = document.getElementById('history-wrapper');
const historyCollapseIcon = document.getElementById('history-collapse-icon');
const historyCount = document.getElementById('history-count');
let historyOffset = 0;
const historyLimit = 10;
let historyCollapsed = false;
let totalHistoryCount = 0;

// Collapse/expand toggle
if (historyHeader) {
    historyHeader.addEventListener('click', () => {
        historyCollapsed = !historyCollapsed;
        if (historyWrapper) {
            historyWrapper.style.display = historyCollapsed ? 'none' : 'block';
        }
        if (historyCollapseIcon) {
            historyCollapseIcon.textContent = historyCollapsed ? '▶' : '▼';
        }
    });
}

// Update count badge
function updateHistoryCount() {
    if (historyCount) {
        historyCount.textContent = totalHistoryCount > 0 ? totalHistoryCount : '';
    }
}

async function loadHistory(append = false) {
    try {
        const profileParam = getProfileFilter() ? `&profile=${encodeURIComponent(getProfileFilter())}` : '';
        const resp = await fetch(`/api/v1/memory/history?limit=${historyLimit}&offset=${historyOffset}${profileParam}`);
        const data = await resp.json();
        const chunks = data.chunks || [];

        // Update total count from API if provided
        if (data.total !== undefined) {
            totalHistoryCount = data.total;
            updateHistoryCount();
        } else if (!append) {
            // Fallback: count loaded chunks
            totalHistoryCount = chunks.length;
            updateHistoryCount();
        }

        if (!append) historyList.textContent = '';

        if (chunks.length === 0 && !append) {
            const row = document.createElement('div');
            row.className = 'item-row';
            const info = document.createElement('div');
            info.className = 'item-info';
            const name = document.createElement('div');
            name.className = 'item-name';
            name.style.color = 'var(--t4)';
            name.textContent = 'No history entries yet';
            info.appendChild(name);
            row.appendChild(info);
            historyList.appendChild(row);
            btnLoadMore.style.display = 'none';
            return;
        }

        chunks.forEach(c => {
            const row = document.createElement('div');
            row.className = 'item-row';

            const info = document.createElement('div');
            info.className = 'item-info';
            info.style.minWidth = '0';

            const wrap = document.createElement('div');
            const heading = document.createElement('div');
            heading.className = 'item-name';
            heading.textContent = c.heading || 'History entry';

            const detail = document.createElement('div');
            detail.className = 'item-detail';
            detail.textContent = c.content.length > 150
                ? c.content.slice(0, 150) + '…'
                : c.content;

            wrap.appendChild(heading);
            wrap.appendChild(detail);
            info.appendChild(wrap);

            const badge = document.createElement('span');
            badge.className = 'badge badge-neutral';
            badge.textContent = c.date;

            row.appendChild(info);
            row.appendChild(badge);
            historyList.appendChild(row);
        });

        historyOffset += chunks.length;
        btnLoadMore.style.display = chunks.length >= historyLimit ? 'inline-flex' : 'none';
    } catch (e) { /* ignore */ }
}

if (btnLoadMore) {
    btnLoadMore.addEventListener('click', () => loadHistory(true));
}

// ─── Daily Logs ───
const dailyList = document.getElementById('daily-list');
const dailyContent = document.getElementById('daily-content');
const dailyViewer = document.getElementById('daily-viewer');
const dailyBadge = document.getElementById('daily-date-badge');
const btnDailyBack = document.getElementById('btn-daily-back');
const dailyHeader = document.getElementById('daily-header');
const dailyWrapper = document.getElementById('daily-wrapper');
const dailyCollapseIcon = document.getElementById('daily-collapse-icon');
const dailyCount = document.getElementById('daily-count');
let dailyCollapsed = false;

// Collapse/expand toggle for main section
if (dailyHeader) {
    dailyHeader.addEventListener('click', () => {
        dailyCollapsed = !dailyCollapsed;
        if (dailyWrapper) {
            dailyWrapper.style.display = dailyCollapsed ? 'none' : 'block';
        }
        if (dailyCollapseIcon) {
            dailyCollapseIcon.textContent = dailyCollapsed ? '▶' : '▼';
        }
    });
}

// Month names for display
const monthNames = ['Gennaio', 'Febbraio', 'Marzo', 'Aprile', 'Maggio', 'Giugno',
                     'Luglio', 'Agosto', 'Settembre', 'Ottobre', 'Novembre', 'Dicembre'];

// Group dates by year-month
function groupByMonth(dates) {
    const groups = {};
    dates.forEach(d => {
        const [year, month] = d.split('-');
        const key = `${year}-${month}`;
        if (!groups[key]) {
            groups[key] = { year, month: parseInt(month), dates: [] };
        }
        groups[key].dates.push(d);
    });
    // Sort dates within each group (most recent first)
    Object.values(groups).forEach(g => g.dates.sort().reverse());
    // Return sorted by year-month (most recent first)
    return Object.values(groups).sort((a, b) => {
        if (a.year !== b.year) return b.year - a.year;
        return b.month - a.month;
    });
}

async function loadDailyList() {
    try {
        const resp = await fetch('/api/v1/memory/daily');
        const data = await resp.json();
        const dates = data.dates || [];

        // Update count badge
        if (dailyCount) {
            dailyCount.textContent = dates.length > 0 ? dates.length : '';
        }

        dailyList.textContent = '';

        if (dates.length === 0) {
            const hint = document.createElement('div');
            hint.style.cssText = 'color:var(--t4);font-size:12px;padding:8px 0';
            hint.textContent = 'No daily logs yet';
            dailyList.appendChild(hint);
            return;
        }

        const groups = groupByMonth(dates);

        groups.forEach((group, gi) => {
            const monthGroup = document.createElement('div');
            monthGroup.className = 'daily-month-group';

            // Month header (clickable to expand/collapse)
            const monthHeader = document.createElement('div');
            monthHeader.className = 'daily-month-header';
            monthHeader.style.cssText = 'display:flex;align-items:center;gap:8px;padding:8px 0;cursor:pointer;border-bottom:1px solid var(--border);margin-bottom:8px';

            // Build header content safely
            const icon = document.createElement('span');
            icon.className = 'collapse-icon';
            icon.textContent = gi === 0 ? '▼' : '▶';

            const monthName = document.createElement('span');
            monthName.className = 'month-name';
            monthName.textContent = `${group.year} ${monthNames[group.month - 1]}`;

            const badge = document.createElement('span');
            badge.className = 'badge badge-neutral';
            badge.textContent = group.dates.length;

            monthHeader.appendChild(icon);
            monthHeader.appendChild(monthName);
            monthHeader.appendChild(badge);

            // Dates container
            const datesContainer = document.createElement('div');
            datesContainer.className = 'daily-dates-container';
            datesContainer.style.cssText = 'display:' + (gi === 0 ? 'grid' : 'none') + ';grid-template-columns:repeat(auto-fill,minmax(120px,1fr));gap:8px;padding-left:24px';

            // Toggle collapse on click
            monthHeader.addEventListener('click', () => {
                const isCollapsed = datesContainer.style.display === 'none';
                datesContainer.style.display = isCollapsed ? 'grid' : 'none';
                icon.textContent = isCollapsed ? '▼' : '▶';
            });

            // Add date chips
            group.dates.forEach(d => {
                const chip = document.createElement('a');
                chip.className = 'daily-chip';
                chip.href = 'javascript:void(0)';
                chip.textContent = d;
                chip.addEventListener('click', () => openDaily(d));
                datesContainer.appendChild(chip);
            });

            monthGroup.appendChild(monthHeader);
            monthGroup.appendChild(datesContainer);
            dailyList.appendChild(monthGroup);
        });
    } catch (e) { /* ignore */ }
}

async function openDaily(date) {
    try {
        const resp = await fetch(`/api/v1/memory/daily/${encodeURIComponent(date)}`);
        if (!resp.ok) { showToast('File not found', 'error'); return; }
        const data = await resp.json();
        dailyViewer.textContent = data.content;
        dailyBadge.textContent = date;
        dailyList.style.display = 'none';
        dailyContent.style.display = 'block';
    } catch (e) {
        showToast('Failed to load daily file', 'error');
    }
}

if (btnDailyBack) {
    btnDailyBack.addEventListener('click', () => {
        dailyContent.style.display = 'none';
        dailyList.style.display = 'block';
    });
}

// ─── Init ───
loadMemoryFile();
loadInstructions();
loadHistory();
loadDailyList();
