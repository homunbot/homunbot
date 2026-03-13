// Homun Connection Recipes — simplified MCP service onboarding
// Note: All user-facing strings are escaped via escapeHtml() before DOM insertion.
// This file follows the same innerHTML pattern as mcp.js and skills.js.

(function() {
    'use strict';

    // ── Helpers ──────────────────────────────────────────────────────

    function showToast(message, type) {
        var existing = document.querySelector('.toast-notification');
        if (existing) existing.remove();
        var toast = document.createElement('div');
        toast.className = 'toast-notification toast-' + (type || 'info');
        toast.textContent = message;
        document.body.appendChild(toast);
        setTimeout(function() { toast.classList.add('show'); }, 10);
        setTimeout(function() {
            toast.classList.remove('show');
            setTimeout(function() { toast.remove(); }, 250);
        }, 3500);
    }

    function escapeHtml(text) {
        return String(text || '')
            .replaceAll('&', '&amp;')
            .replaceAll('<', '&lt;')
            .replaceAll('>', '&gt;')
            .replaceAll('"', '&quot;')
            .replaceAll("'", '&#39;');
    }

    async function api(url, opts) {
        var resp = await fetch(url, opts || {});
        try { return { ok: resp.ok, status: resp.status, body: await resp.json() }; }
        catch (_) { return { ok: resp.ok, status: resp.status, body: null }; }
    }

    // ── Icons (inline SVG) ──────────────────────────────────────────

    var ICONS = {
        github: '<svg viewBox="0 0 24 24" fill="currentColor"><path d="M12 2C6.477 2 2 6.477 2 12c0 4.42 2.865 8.166 6.839 9.489.5.092.682-.217.682-.482 0-.237-.008-.866-.013-1.7-2.782.603-3.369-1.341-3.369-1.341-.454-1.155-1.11-1.462-1.11-1.462-.908-.62.069-.608.069-.608 1.003.07 1.531 1.03 1.531 1.03.892 1.529 2.341 1.088 2.91.832.092-.647.35-1.088.636-1.338-2.22-.253-4.555-1.11-4.555-4.943 0-1.091.39-1.984 1.029-2.683-.103-.253-.446-1.27.098-2.647 0 0 .84-.269 2.75 1.025A9.578 9.578 0 0 1 12 6.836c.85.004 1.705.114 2.504.336 1.909-1.294 2.747-1.025 2.747-1.025.546 1.377.203 2.394.1 2.647.64.699 1.028 1.592 1.028 2.683 0 3.842-2.339 4.687-4.566 4.935.359.309.678.919.678 1.852 0 1.336-.012 2.415-.012 2.743 0 .267.18.578.688.48C19.138 20.161 22 16.416 22 12c0-5.523-4.477-10-10-10z"/></svg>',
        gmail: '<svg viewBox="0 0 24 24" fill="currentColor"><path d="M20 18h-2V9.25L12 13 6 9.25V18H4V6h1.2l6.8 4.25L18.8 6H20v12zM20 4H4c-1.1 0-2 .9-2 2v12c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2V6c0-1.1-.9-2-2-2z"/></svg>',
        'google-calendar': '<svg viewBox="0 0 24 24" fill="currentColor"><path d="M19 4h-1V2h-2v2H8V2H6v2H5c-1.11 0-2 .9-2 2v14c0 1.1.89 2 2 2h14c1.1 0 2-.9 2-2V6c0-1.1-.9-2-2-2zm0 16H5V10h14v10zm0-12H5V6h14v2z"/></svg>',
        notion: '<svg viewBox="0 0 24 24" fill="currentColor"><path d="M4.459 4.208c.746.606 1.026.56 2.428.466l13.215-.793c.28 0 .047-.28-.046-.326L18.19 2.168c-.42-.326-.98-.7-2.055-.606L3.13 2.655c-.466.046-.56.28-.373.466l1.703 1.087zm.793 3.358v13.913c0 .746.373 1.026 1.213.98l14.523-.84c.84-.046.933-.56.933-1.166V6.63c0-.606-.233-.933-.746-.886l-15.177.886c-.56.047-.746.327-.746.933zm14.337.7c.093.42 0 .84-.42.886l-.7.14v10.264c-.606.326-1.166.513-1.633.513-.746 0-.933-.233-1.493-.933l-4.573-7.178v6.94l1.446.327s0 .84-1.166.84l-3.218.186c-.093-.186 0-.653.326-.746l.84-.233V9.854L7.822 9.76c-.093-.42.14-1.026.793-1.073l3.452-.233 4.76 7.27v-6.43l-1.213-.14c-.093-.513.28-.886.746-.933l3.228-.186z"/></svg>',
        slack: '<svg viewBox="0 0 24 24" fill="currentColor"><path d="M5.042 15.165a2.528 2.528 0 0 1-2.52 2.523A2.528 2.528 0 0 1 0 15.165a2.527 2.527 0 0 1 2.522-2.52h2.52v2.52zm1.271 0a2.527 2.527 0 0 1 2.521-2.52 2.527 2.527 0 0 1 2.521 2.52v6.313A2.528 2.528 0 0 1 8.834 24a2.528 2.528 0 0 1-2.521-2.522v-6.313zM8.834 5.042a2.528 2.528 0 0 1-2.521-2.52A2.528 2.528 0 0 1 8.834 0a2.528 2.528 0 0 1 2.521 2.522v2.52H8.834zm0 1.271a2.528 2.528 0 0 1 2.521 2.521 2.528 2.528 0 0 1-2.521 2.521H2.522A2.528 2.528 0 0 1 0 8.834a2.528 2.528 0 0 1 2.522-2.521h6.312zM18.956 8.834a2.528 2.528 0 0 1 2.522-2.521A2.528 2.528 0 0 1 24 8.834a2.528 2.528 0 0 1-2.522 2.521h-2.522V8.834zm-1.27 0a2.528 2.528 0 0 1-2.523 2.521 2.527 2.527 0 0 1-2.52-2.521V2.522A2.527 2.527 0 0 1 15.163 0a2.528 2.528 0 0 1 2.523 2.522v6.312zM15.163 18.956a2.528 2.528 0 0 1 2.523 2.522A2.528 2.528 0 0 1 15.163 24a2.527 2.527 0 0 1-2.52-2.522v-2.522h2.52zm0-1.27a2.527 2.527 0 0 1-2.52-2.523 2.527 2.527 0 0 1 2.52-2.52h6.315A2.528 2.528 0 0 1 24 15.163a2.528 2.528 0 0 1-2.522 2.523h-6.315z"/></svg>',
        default: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><path d="M12 6v6l4 2"/></svg>',
    };

    function getIcon(name) {
        return ICONS[name] || ICONS.default;
    }

    // ── State ────────────────────────────────────────────────────────

    var state = {
        recipes: [],
        selectedCategory: 'All',
        searchQuery: '',
    };

    var elView = document.getElementById('connections-view');
    var elGrid = document.getElementById('conn-grid');
    var elChips = document.getElementById('conn-category-chips');
    var elSearch = document.getElementById('conn-search-input');
    var elCount = document.getElementById('conn-count');

    if (!elView || !elGrid) return; // bail if not on MCP page

    // ── Catalog load ─────────────────────────────────────────────────

    async function loadCatalog() {
        var res = await api('/api/v1/connections/catalog');
        if (!res.ok || !res.body || !res.body.items) {
            showToast('Failed to load connection catalog', 'error');
            return;
        }
        state.recipes = res.body.items;
        renderChips();
        renderGrid();
    }

    // ── Category chips ───────────────────────────────────────────────

    function renderChips() {
        if (!elChips) return;
        var cats = ['All'];
        state.recipes.forEach(function(r) {
            if (r.category && cats.indexOf(r.category) === -1) cats.push(r.category);
        });
        var html = '';
        for (var i = 0; i < cats.length; i++) {
            var cat = cats[i];
            var active = cat === state.selectedCategory ? ' active' : '';
            html += '<button class="mcp-chip' + active + '" data-category="' + escapeHtml(cat) + '">' + escapeHtml(cat) + '</button>';
        }
        elChips.textContent = '';
        elChips.insertAdjacentHTML('beforeend', html);
    }

    // ── Grid rendering ──────────────────────────────────────────────

    function filteredRecipes() {
        var q = state.searchQuery.toLowerCase();
        return state.recipes.filter(function(r) {
            var catOk = state.selectedCategory === 'All' || r.category === state.selectedCategory;
            var qOk = !q || r.display_name.toLowerCase().indexOf(q) >= 0 || r.subtitle.toLowerCase().indexOf(q) >= 0 || (r.category || '').toLowerCase().indexOf(q) >= 0;
            return catOk && qOk;
        });
    }

    function renderGrid() {
        if (!elGrid) return;
        var items = filteredRecipes();
        if (elCount) elCount.textContent = items.length + ' service' + (items.length !== 1 ? 's' : '');

        if (items.length === 0) {
            elGrid.textContent = '';
            var empty = document.createElement('div');
            empty.className = 'empty-state';
            var p = document.createElement('p');
            p.textContent = 'No services match your filter.';
            empty.appendChild(p);
            elGrid.appendChild(empty);
            return;
        }

        var html = '';
        for (var i = 0; i < items.length; i++) {
            var item = items[i];
            var isConnected = item.connection_status && item.connection_status.status === 'connected';
            var statusBadge = isConnected
                ? '<span class="conn-status-badge conn-status-connected">Connected</span>'
                : '<span class="conn-status-badge conn-status-not-connected">Not connected</span>';
            var authLabel = item.auth_mode === 'oauth' ? 'OAuth' : 'API Key';
            var authBadge = '<span class="badge badge-neutral">' + escapeHtml(authLabel) + '</span>';
            var toolCount = isConnected && item.connection_status.tool_count ? ' \u00b7 ' + item.connection_status.tool_count + ' tools' : '';

            html += '<div class="conn-card' + (isConnected ? ' conn-card--connected' : '') + '" data-recipe-id="' + escapeHtml(item.id) + '">' +
                '<div class="conn-card-header">' +
                    '<div class="conn-card-icon">' + getIcon(item.icon) + '</div>' +
                    '<div class="conn-card-title">' +
                        '<div class="conn-card-name">' + escapeHtml(item.display_name) + '</div>' +
                        '<div class="conn-card-subtitle">' + escapeHtml(item.subtitle) + '</div>' +
                    '</div>' +
                '</div>' +
                '<div class="conn-card-body">' +
                    '<p class="conn-card-intro">' + escapeHtml(item.capability_intro) + '</p>' +
                '</div>' +
                '<div class="conn-card-footer">' +
                    '<div class="conn-card-badges">' + statusBadge + authBadge + toolCount + '</div>' +
                    '<button class="btn btn-sm ' + (isConnected ? 'btn-secondary' : 'btn-primary') + ' conn-action-btn" data-recipe-id="' + escapeHtml(item.id) + '">' +
                        (isConnected ? 'Manage' : 'Connect') +
                    '</button>' +
                '</div>' +
            '</div>';
        }
        elGrid.textContent = '';
        elGrid.insertAdjacentHTML('beforeend', html);
    }

    // ── Connect dialog ──────────────────────────────────────────────

    function openConnectDialog(recipeId) {
        var recipe = state.recipes.find(function(r) { return r.id === recipeId; });
        if (!recipe) return;

        var isConnected = recipe.connection_status && recipe.connection_status.status === 'connected';

        // Build fields HTML (all values escaped)
        var fieldsHtml = '';
        for (var i = 0; i < recipe.fields.length; i++) {
            var f = recipe.fields[i];
            var inputType = f.secret ? 'password' : (f.input || 'text');
            fieldsHtml += '<div class="form-group">' +
                '<label for="conn-field-' + escapeHtml(f.id) + '">' + escapeHtml(f.label) + (f.required ? ' *' : '') + '</label>' +
                '<input id="conn-field-' + escapeHtml(f.id) + '" class="input" type="' + escapeHtml(inputType) + '" ' +
                    'data-field-id="' + escapeHtml(f.id) + '" ' +
                    (f.required ? 'required ' : '') +
                    'placeholder="' + escapeHtml(f.help) + '">' +
                (f.source_hint ? '<div class="form-hint">' + escapeHtml(f.source_hint) + '</div>' : '') +
            '</div>';
        }

        var modalOverlay = document.getElementById('mcp-modal-overlay');
        var modalTitle = document.getElementById('mcp-modal-title');
        var modalSubtitle = document.getElementById('mcp-modal-subtitle');
        var modalMeta = document.getElementById('mcp-modal-meta');
        var modalContent = document.getElementById('mcp-modal-content');
        var modalFooter = document.getElementById('mcp-modal-footer');

        if (!modalOverlay || !modalContent) return;

        modalTitle.textContent = (isConnected ? 'Manage ' : 'Connect ') + recipe.display_name;
        modalSubtitle.textContent = recipe.subtitle;

        if (modalMeta) {
            modalMeta.textContent = '';
            modalMeta.insertAdjacentHTML('beforeend',
                '<span class="badge badge-neutral">' + escapeHtml(recipe.category) + '</span> ' +
                '<span class="badge badge-neutral">' + escapeHtml(recipe.auth_mode === 'oauth' ? 'OAuth' : 'API Key') + '</span>'
            );
        }

        var actionsHtml = '';
        if (isConnected) {
            actionsHtml = '<div class="conn-connected-actions">' +
                '<button class="btn btn-secondary btn-sm" id="conn-test-btn" data-name="' + escapeHtml(recipe.id) + '">Test Connection</button>' +
                '<button class="btn btn-secondary btn-sm" id="conn-capabilities-btn" data-name="' + escapeHtml(recipe.id) + '">View Tools</button>' +
                '</div>' +
                '<hr style="border-color: var(--border); margin: 16px 0;">' +
                '<p class="form-hint" style="margin-bottom: 12px;">Update credentials to reconnect:</p>';
        }

        modalContent.textContent = '';
        modalContent.insertAdjacentHTML('beforeend',
            actionsHtml + '<form id="conn-form" class="form">' + fieldsHtml + '</form>'
        );

        modalFooter.textContent = '';
        modalFooter.insertAdjacentHTML('beforeend',
            '<button class="btn btn-primary" id="conn-submit-btn">' + (isConnected ? 'Reconnect' : 'Connect') + '</button>'
        );

        modalOverlay.classList.add('active');

        // Bind submit
        var submitBtn = document.getElementById('conn-submit-btn');
        if (submitBtn) {
            submitBtn.addEventListener('click', async function() {
                var fields = {};
                var form = document.getElementById('conn-form');
                if (form) {
                    form.querySelectorAll('input[data-field-id]').forEach(function(input) {
                        if (input.value.trim()) fields[input.dataset.fieldId] = input.value.trim();
                    });
                }

                // Validate required
                var missing = recipe.fields.filter(function(f) { return f.required && !fields[f.id]; });
                if (missing.length > 0 && !isConnected) {
                    showToast('Missing required: ' + missing.map(function(f) { return f.label; }).join(', '), 'error');
                    return;
                }

                submitBtn.disabled = true;
                submitBtn.textContent = 'Connecting...';

                var res = await api('/api/v1/connections/recipes/' + encodeURIComponent(recipe.id) + '/connect', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ fields: fields, skip_test: false }),
                });

                submitBtn.disabled = false;
                submitBtn.textContent = isConnected ? 'Reconnect' : 'Connect';

                if (res.ok && res.body && res.body.ok) {
                    showSuccessScreen(recipe, res.body);
                } else {
                    var msg = (res.body && res.body.message) || 'Connection failed';
                    showToast(msg, 'error');
                }
            });
        }

        // Bind test
        var testBtn = document.getElementById('conn-test-btn');
        if (testBtn) {
            testBtn.addEventListener('click', async function() {
                testBtn.disabled = true;
                testBtn.textContent = 'Testing...';
                var res = await api('/api/v1/connections/' + encodeURIComponent(testBtn.dataset.name) + '/test', {
                    method: 'POST',
                });
                testBtn.disabled = false;
                testBtn.textContent = 'Test Connection';
                if (res.ok && res.body && res.body.connected) {
                    showToast('Connection OK \u2014 ' + res.body.tool_count + ' tools', 'success');
                } else {
                    showToast((res.body && res.body.error) || 'Test failed', 'error');
                }
            });
        }

        // Bind capabilities
        var capBtn = document.getElementById('conn-capabilities-btn');
        if (capBtn) {
            capBtn.addEventListener('click', async function() {
                capBtn.disabled = true;
                capBtn.textContent = 'Loading tools...';
                var res = await api('/api/v1/connections/' + encodeURIComponent(capBtn.dataset.name) + '/capabilities');
                capBtn.disabled = false;
                capBtn.textContent = 'View Tools';
                if (res.ok && res.body && Array.isArray(res.body.tools)) {
                    var toolsHtml = '';
                    if (res.body.tools.length === 0) {
                        toolsHtml = '<p class="form-hint">No tools discovered.</p>';
                    } else {
                        toolsHtml = '<ul class="conn-tools-list">';
                        for (var j = 0; j < res.body.tools.length; j++) {
                            var t = res.body.tools[j];
                            toolsHtml += '<li><strong>' + escapeHtml(t.name) + '</strong> \u2014 ' + escapeHtml(t.description) + '</li>';
                        }
                        toolsHtml += '</ul>';
                    }
                    var toolsEl = document.getElementById('conn-tools-display');
                    if (!toolsEl) {
                        toolsEl = document.createElement('div');
                        toolsEl.id = 'conn-tools-display';
                        capBtn.parentElement.parentElement.insertBefore(toolsEl, capBtn.parentElement.nextSibling);
                    }
                    toolsEl.textContent = '';
                    toolsEl.insertAdjacentHTML('beforeend', '<h3 style="font-size:14px; margin:12px 0 8px;">Available Tools</h3>' + toolsHtml);
                } else {
                    showToast('Failed to load tools', 'error');
                }
            });
        }
    }

    // ── Success screen ──────────────────────────────────────────────

    function showSuccessScreen(recipe, result) {
        var modalContent = document.getElementById('mcp-modal-content');
        var modalFooter = document.getElementById('mcp-modal-footer');
        var modalTitle = document.getElementById('mcp-modal-title');

        if (!modalContent) return;

        if (modalTitle) modalTitle.textContent = result.success ? result.success.title : 'Connected';

        var toolCountHtml = result.tool_count
            ? '<p class="conn-success-tools">' + result.tool_count + ' tool' + (result.tool_count !== 1 ? 's' : '') + ' available</p>'
            : '';

        var vaultHtml = result.stored_vault_keys && result.stored_vault_keys.length
            ? '<p class="form-hint">Secrets stored in vault: ' + escapeHtml(result.stored_vault_keys.join(', ')) + '</p>'
            : '';

        modalContent.textContent = '';
        modalContent.insertAdjacentHTML('beforeend',
            '<div class="conn-success-screen">' +
                '<div class="conn-success-icon">' + getIcon(recipe.icon) + '</div>' +
                '<p class="conn-success-body">' + escapeHtml(result.success ? result.success.body : 'Service connected successfully.') + '</p>' +
                toolCountHtml + vaultHtml +
            '</div>'
        );

        modalFooter.textContent = '';
        modalFooter.insertAdjacentHTML('beforeend',
            '<a href="/chat" class="btn btn-primary">Try in chat</a>' +
            '<button class="btn btn-secondary" id="conn-done-btn">Done</button>'
        );

        var doneBtn = document.getElementById('conn-done-btn');
        if (doneBtn) {
            doneBtn.addEventListener('click', function() {
                var overlay = document.getElementById('mcp-modal-overlay');
                if (overlay) overlay.classList.remove('active');
                loadCatalog();
            });
        }
    }

    // ── Event binding ────────────────────────────────────────────────

    if (elGrid) {
        elGrid.addEventListener('click', function(e) {
            var btn = e.target.closest('.conn-action-btn');
            if (btn) { openConnectDialog(btn.dataset.recipeId); return; }
            var card = e.target.closest('.conn-card');
            if (card) openConnectDialog(card.dataset.recipeId);
        });
    }

    if (elChips) {
        elChips.addEventListener('click', function(e) {
            var chip = e.target.closest('.mcp-chip');
            if (!chip) return;
            state.selectedCategory = chip.dataset.category || 'All';
            renderChips();
            renderGrid();
        });
    }

    if (elSearch) {
        var searchTimer = null;
        elSearch.addEventListener('input', function() {
            clearTimeout(searchTimer);
            searchTimer = setTimeout(function() {
                state.searchQuery = elSearch.value.trim();
                renderGrid();
            }, 200);
        });
    }

    // ── Init ─────────────────────────────────────────────────────────

    window._connectionsLoad = loadCatalog;
    loadCatalog();
})();
