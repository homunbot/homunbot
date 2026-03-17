// Homun Connection Recipes — simplified MCP service onboarding
// Note: All user-facing strings are escaped via escapeHtml() before DOM insertion.
// This file follows the same innerHTML pattern as mcp.js and skills.js.

(function() {
    'use strict';

    // ── Helpers ──────────────────────────────────────────────────────

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

    // ── OAuth helpers ──────────────────────────────────────────────

    /** Map recipe → OAuth provider config. Returns null for non-OAuth recipes. */
    function oauthConfigForRecipe(recipe) {
        if (recipe.auth_mode !== 'oauth') return null;
        var map = {
            'google-workspace':  { provider: 'google', service: 'google-workspace', tokenField: 'refresh_token', tokenKey: 'refresh_token', providerLabel: 'Google' },
        };
        return map[recipe.id] || null;
    }

    /** Map recipe → MCP OAuth 2.1 config (PKCE + Dynamic Client Registration). */
    function mcpOauthConfigForRecipe(recipe) {
        if (recipe.auth_mode !== 'mcp_oauth') return null;
        var map = {
            'notion': { provider: 'notion', tokenField: 'token', tokenKey: 'access_token', providerLabel: 'Notion' },
        };
        return map[recipe.id] || null;
    }

    /** Currently active OAuth listener (removed on dialog close). */
    var _oauthMessageHandler = null;

    function cleanupOauthListener() {
        if (_oauthMessageHandler) {
            window.removeEventListener('message', _oauthMessageHandler);
            _oauthMessageHandler = null;
        }
    }

    // ── Multi-instance helpers ────────────────────────────────────

    /** Get active (enabled) instances from a catalog item. */
    function activeInstances(item) {
        return (item.instances || []).filter(function(i) { return i.enabled; });
    }

    /** Auto-generate next instance name (e.g. "gmail-2"). */
    function nextInstanceName(recipeId, instances) {
        for (var n = 2; n <= 99; n++) {
            var candidate = recipeId + '-' + n;
            if (!instances.some(function(i) { return i.name === candidate; })) return candidate;
        }
        return recipeId + '-' + Date.now();
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
            var active = activeInstances(item);
            var statusBadge;
            if (active.length === 0) {
                statusBadge = '<span class="conn-status-badge conn-status-not-connected">Not connected</span>';
            } else if (active.length === 1) {
                statusBadge = '<span class="conn-status-badge conn-status-connected">Connected</span>';
            } else {
                statusBadge = '<span class="conn-status-badge conn-status-connected">' + active.length + ' connected</span>';
            }
            var authLabel = (item.auth_mode === 'oauth' || item.auth_mode === 'mcp_oauth') ? 'OAuth' : 'API Key';
            var authBadge = '<span class="badge badge-neutral">' + escapeHtml(authLabel) + '</span>';
            var toolCount = active.length > 0 && item.connection_status && item.connection_status.tool_count ? ' \u00b7 ' + item.connection_status.tool_count + ' tools' : '';

            html += '<div class="conn-card' + (active.length > 0 ? ' conn-card--connected' : '') + '" data-recipe-id="' + escapeHtml(item.id) + '">' +
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
                    '<button class="btn btn-sm ' + (active.length > 0 ? 'btn-secondary' : 'btn-primary') + ' conn-action-btn" data-recipe-id="' + escapeHtml(item.id) + '">' +
                        (active.length > 0 ? 'Manage' : 'Connect') +
                    '</button>' +
                '</div>' +
            '</div>';
        }
        elGrid.textContent = '';
        elGrid.insertAdjacentHTML('beforeend', html);
    }

    // ── Dialog router ────────────────────────────────────────────────

    function openConnectDialog(recipeId) {
        cleanupOauthListener();
        var recipe = state.recipes.find(function(r) { return r.id === recipeId; });
        if (!recipe) return;

        var active = activeInstances(recipe);
        if (active.length > 0) {
            openManageDialog(recipe, recipe.instances || []);
        } else {
            openConnectForm(recipe, recipe.id, false);
        }
    }

    // ── Manage dialog (instances list) ───────────────────────────────

    function openManageDialog(recipe, instances) {
        var modalOverlay = document.getElementById('mcp-modal-overlay');
        var modalTitle = document.getElementById('mcp-modal-title');
        var modalSubtitle = document.getElementById('mcp-modal-subtitle');
        var modalMeta = document.getElementById('mcp-modal-meta');
        var modalContent = document.getElementById('mcp-modal-content');
        var modalFooter = document.getElementById('mcp-modal-footer');
        if (!modalOverlay || !modalContent) return;

        modalTitle.textContent = 'Manage ' + recipe.display_name;
        modalSubtitle.textContent = recipe.subtitle;
        if (modalMeta) {
            modalMeta.textContent = '';
            modalMeta.insertAdjacentHTML('beforeend',
                '<span class="badge badge-neutral">' + escapeHtml(recipe.category) + '</span> ' +
                '<span class="badge badge-neutral">' + escapeHtml(recipe.auth_mode === 'oauth' ? 'OAuth' : 'API Key') + '</span>'
            );
        }

        // Build instances list
        var html = '<div class="conn-instances-list">';
        for (var i = 0; i < instances.length; i++) {
            var inst = instances[i];
            html += '<div class="conn-instance-row" data-name="' + escapeHtml(inst.name) + '">' +
                '<span class="conn-instance-name">' + escapeHtml(inst.name) + '</span>' +
                '<span class="conn-instance-tools">' + inst.tool_count + ' tools</span>' +
                '<button class="btn btn-sm btn-secondary conn-instance-test" data-name="' + escapeHtml(inst.name) + '">Test</button>' +
                '<button class="btn btn-sm btn-danger conn-instance-disconnect" data-name="' + escapeHtml(inst.name) + '">Disconnect</button>' +
            '</div>';
        }
        html += '</div>';

        modalContent.textContent = '';
        modalContent.insertAdjacentHTML('beforeend', html);

        modalFooter.textContent = '';
        modalFooter.insertAdjacentHTML('beforeend',
            '<button class="btn btn-primary" id="conn-add-account-btn">Add Account</button>'
        );

        modalOverlay.classList.add('active');

        // Bind instance test buttons
        modalContent.querySelectorAll('.conn-instance-test').forEach(function(btn) {
            btn.addEventListener('click', async function() {
                btn.disabled = true;
                btn.textContent = '...';
                var res = await api('/api/v1/connections/' + encodeURIComponent(btn.dataset.name) + '/test', { method: 'POST' });
                btn.disabled = false;
                btn.textContent = 'Test';
                if (res.ok && res.body && res.body.connected) {
                    showToast(btn.dataset.name + ': OK \u2014 ' + res.body.tool_count + ' tools', 'success');
                    // Update tool count display in the instance row
                    var row = btn.closest('.conn-instance-row');
                    if (row) {
                        var toolsSpan = row.querySelector('.conn-instance-tools');
                        if (toolsSpan) toolsSpan.textContent = res.body.tool_count + ' tools';
                    }
                } else {
                    showToast(btn.dataset.name + ': ' + ((res.body && res.body.error) || 'Test failed'), 'error');
                }
            });
        });

        // Bind disconnect buttons
        modalContent.querySelectorAll('.conn-instance-disconnect').forEach(function(btn) {
            btn.addEventListener('click', async function() {
                btn.disabled = true;
                btn.textContent = '...';
                var res = await api('/api/v1/connections/' + encodeURIComponent(btn.dataset.name), { method: 'DELETE' });
                if (res.ok) {
                    showToast('Disconnected ' + btn.dataset.name, 'success');
                    // Reload and re-open manage dialog
                    await loadCatalog();
                    var updated = state.recipes.find(function(r) { return r.id === recipe.id; });
                    if (updated && activeInstances(updated).length > 0) {
                        openManageDialog(updated, updated.instances || []);
                    } else {
                        modalOverlay.classList.remove('active');
                    }
                } else {
                    btn.disabled = false;
                    btn.textContent = 'Disconnect';
                    showToast('Failed to disconnect', 'error');
                }
            });
        });

        // Bind "Add Account"
        var addBtn = document.getElementById('conn-add-account-btn');
        if (addBtn) {
            addBtn.addEventListener('click', function() {
                var name = nextInstanceName(recipe.id, instances);
                openConnectForm(recipe, name, true);
            });
        }
    }

    // ── Connect form ─────────────────────────────────────────────────

    function openConnectForm(recipe, instanceName, showNameField) {
        cleanupOauthListener();
        var oauthConfig = oauthConfigForRecipe(recipe);
        var mcpOauthConfig = mcpOauthConfigForRecipe(recipe);

        // Build fields HTML
        var fieldsHtml = '';

        // Instance name field (only for multi-account)
        if (showNameField) {
            fieldsHtml += '<div class="form-group">' +
                '<label for="conn-instance-name">Account Name *</label>' +
                '<input id="conn-instance-name" class="input" type="text" value="' + escapeHtml(instanceName) + '" placeholder="e.g. ' + escapeHtml(recipe.id) + '-work">' +
                '<div class="form-hint">Unique name for this account</div>' +
            '</div>';
        }

        // Show redirect URI hint for standard OAuth recipes (not mcp_oauth — those are automatic)
        if (oauthConfig) {
            var callbackUrl = window.location.origin + '/mcp/oauth/' + oauthConfig.provider + '/callback';
            fieldsHtml += '<div class="form-group">' +
                '<label>Redirect URI</label>' +
                '<input class="input" type="text" value="' + escapeHtml(callbackUrl) + '" readonly style="opacity:0.8;cursor:text" onclick="this.select()">' +
                '<div class="form-hint">Add this URL as Authorized redirect URI in your OAuth app settings</div>' +
            '</div>';
        }

        // MCP OAuth 2.1: no user fields — just a Connect button + hidden token field
        if (mcpOauthConfig) {
            fieldsHtml += '<input type="hidden" id="conn-field-' + escapeHtml(mcpOauthConfig.tokenField) + '" data-field-id="' + escapeHtml(mcpOauthConfig.tokenField) + '">' +
                '<div class="form-group" style="text-align:center;padding:var(--sp-24) 0">' +
                    '<p style="margin-bottom:var(--sp-16);color:var(--text-secondary)">Click below to authorize access via Notion\'s OAuth.</p>' +
                    '<button type="button" class="btn btn-primary" id="conn-mcp-oauth-btn">' +
                        '\uD83D\uDD10 Connect to ' + escapeHtml(mcpOauthConfig.providerLabel) + '</button>' +
                    '<div class="form-hint" id="conn-mcp-oauth-status" style="margin-top:var(--sp-8)"></div>' +
                '</div>';
        }

        for (var i = 0; i < recipe.fields.length; i++) {
            var f = recipe.fields[i];

            // For MCP OAuth 2.1: skip all fields (the hidden token is already rendered above)
            if (mcpOauthConfig && f.id === mcpOauthConfig.tokenField) continue;

            // For standard OAuth: replace token field with Authorize button
            var oauthTokenField = oauthConfig ? (oauthConfig.tokenField || 'refresh_token') : null;
            if (oauthConfig && f.id === oauthTokenField) {
                var providerLabel = oauthConfig.providerLabel || 'Google';
                fieldsHtml += '<input type="hidden" id="conn-field-' + escapeHtml(f.id) + '" data-field-id="' + escapeHtml(f.id) + '">' +
                    '<div class="form-group">' +
                        '<label>Authorization</label>' +
                        '<button type="button" class="btn btn-secondary" id="conn-oauth-btn">' +
                            '\uD83D\uDD10 Authorize with ' + escapeHtml(providerLabel) + '</button>' +
                        '<div class="form-hint" id="conn-oauth-status"></div>' +
                    '</div>';
                continue;
            }

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

        modalTitle.textContent = 'Connect ' + recipe.display_name;
        if (showNameField) modalTitle.textContent += ' (' + instanceName + ')';
        modalSubtitle.textContent = recipe.subtitle;

        if (modalMeta) {
            modalMeta.textContent = '';
            modalMeta.insertAdjacentHTML('beforeend',
                '<span class="badge badge-neutral">' + escapeHtml(recipe.category) + '</span> ' +
                '<span class="badge badge-neutral">' + escapeHtml(recipe.auth_mode === 'oauth' ? 'OAuth' : 'API Key') + '</span>'
            );
        }

        modalContent.textContent = '';
        modalContent.insertAdjacentHTML('beforeend',
            '<form id="conn-form" class="form">' + fieldsHtml + '</form>'
        );

        modalFooter.textContent = '';
        modalFooter.insertAdjacentHTML('beforeend',
            '<button class="btn btn-primary" id="conn-submit-btn">Connect</button>'
        );

        modalOverlay.classList.add('active');

        // Bind submit
        var submitBtn = document.getElementById('conn-submit-btn');
        if (submitBtn) {
            submitBtn.addEventListener('click', async function() {
                // Resolve instance name
                var resolvedName = instanceName;
                if (showNameField) {
                    var nameInput = document.getElementById('conn-instance-name');
                    resolvedName = nameInput ? nameInput.value.trim() : instanceName;
                    if (!resolvedName) {
                        showToast('Enter an account name', 'error');
                        return;
                    }
                }

                var fields = {};
                var form = document.getElementById('conn-form');
                if (form) {
                    form.querySelectorAll('input[data-field-id]').forEach(function(input) {
                        if (input.value.trim()) fields[input.dataset.fieldId] = input.value.trim();
                    });
                }

                // Validate required
                var missing = recipe.fields.filter(function(f) { return f.required && !fields[f.id]; });
                if (missing.length > 0) {
                    showToast('Missing required: ' + missing.map(function(f) { return f.label; }).join(', '), 'error');
                    return;
                }

                submitBtn.disabled = true;
                submitBtn.textContent = 'Connecting...';

                var res = await api('/api/v1/connections/recipes/' + encodeURIComponent(recipe.id) + '/connect', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ fields: fields, skip_test: false, instance_name: resolvedName }),
                });

                submitBtn.disabled = false;
                submitBtn.textContent = 'Connect';

                if (res.ok && res.body && res.body.ok) {
                    showSuccessScreen(recipe, res.body);
                } else {
                    var msg = (res.body && res.body.message) || 'Connection failed';
                    showToast(msg, 'error');
                }
            });
        }

        // ── OAuth flow (Gmail, Google Calendar) ──────────────────────
        bindOauthFlow(oauthConfig, recipe);
        // ── MCP OAuth 2.1 flow (Notion) ─────────────────────────────
        bindMcpOauthFlow(mcpOauthConfig, recipe, instanceName, showNameField);
    }

    // ── OAuth flow binding ───────────────────────────────────────────

    function bindOauthFlow(oauthConfig, recipe) {
        var oauthBtn = document.getElementById('conn-oauth-btn');
        if (!oauthBtn || !oauthConfig) return;

        var oauthStatusEl = document.getElementById('conn-oauth-status');

        function setOauthStatus(msg) {
            if (oauthStatusEl) oauthStatusEl.textContent = msg;
        }

        // Exchange auth code for token (auto, no manual step)
        var tokenField = oauthConfig.tokenField || 'refresh_token';
        var tokenKey = oauthConfig.tokenKey || 'refresh_token';

        async function exchangeOauthCode(code) {
            setOauthStatus('Authorization received, obtaining token...');
            var clientId = (document.getElementById('conn-field-client_id') || {}).value || '';
            var clientSecret = (document.getElementById('conn-field-client_secret') || {}).value || '';
            var redirectUri = window.location.origin + '/mcp/oauth/' + oauthConfig.provider + '/callback';

            var res = await api('/api/v1/mcp/oauth/' + oauthConfig.provider + '/exchange', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    service: oauthConfig.service,
                    code: code,
                    client_id: clientId,
                    client_secret: clientSecret,
                    redirect_uri: redirectUri,
                }),
            });

            var tokenValue = res.body && res.body[tokenKey];
            if (res.ok && tokenValue) {
                var hidden = document.getElementById('conn-field-' + tokenField);
                if (hidden) hidden.value = tokenValue;
                // Auto-fill instance name from Google email (e.g. "gmail-fabio")
                var nameInput = document.getElementById('conn-instance-name');
                if (nameInput && res.body.email) {
                    var local = res.body.email.split('@')[0] || '';
                    if (local) nameInput.value = recipe.id + '-' + local.toLowerCase().replace(/[^a-z0-9-]/g, '-');
                }
                setOauthStatus('\u2713 Authorization complete' + (res.body.email ? ' (' + res.body.email + ')' : ''));
                oauthBtn.disabled = true;
                oauthBtn.textContent = '\u2713 Authorized';
            } else {
                var errMsg = (res.body && (res.body.message || res.body.error_description)) || 'Token exchange failed';
                setOauthStatus('Error: ' + errMsg);
                showToast(errMsg, 'error');
            }
        }

        // Listen for callback postMessage from popup
        _oauthMessageHandler = function(event) {
            if (event.origin !== window.location.origin) return;
            var data = event.data || {};
            if (data.type !== 'homun-mcp-oauth-code') return;
            if (data.provider !== oauthConfig.provider) return;
            if (data.error) {
                setOauthStatus('Error: ' + (data.error_description || data.error));
                showToast(data.error_description || data.error, 'error');
                return;
            }
            exchangeOauthCode(data.code);
        };
        window.addEventListener('message', _oauthMessageHandler);

        // Start OAuth flow on button click
        oauthBtn.addEventListener('click', async function() {
            var clientId = (document.getElementById('conn-field-client_id') || {}).value || '';
            if (!clientId.trim()) {
                showToast('Enter your Client ID first', 'error');
                return;
            }
            var redirectUri = window.location.origin + '/mcp/oauth/' + oauthConfig.provider + '/callback';

            oauthBtn.disabled = true;
            setOauthStatus('Opening ' + (oauthConfig.providerLabel || 'OAuth') + ' authorization...');

            var res = await api('/api/v1/mcp/oauth/' + oauthConfig.provider + '/start', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    service: oauthConfig.service,
                    client_id: clientId.trim(),
                    redirect_uri: redirectUri,
                }),
            });

            if (res.ok && res.body && res.body.auth_url) {
                window.open(res.body.auth_url, '_blank', 'popup,width=720,height=840');
                setOauthStatus('Waiting for authorization in popup...');
                oauthBtn.disabled = false;
            } else {
                var errMsg = (res.body && res.body.message) || 'Failed to start OAuth';
                setOauthStatus('Error: ' + errMsg);
                showToast(errMsg, 'error');
                oauthBtn.disabled = false;
            }
        });
    }

    // ── MCP OAuth 2.1 flow (PKCE + Dynamic Client Registration) ────

    function bindMcpOauthFlow(mcpOauthConfig, recipe, instanceName, showNameField) {
        var btn = document.getElementById('conn-mcp-oauth-btn');
        if (!btn || !mcpOauthConfig) return;

        var statusEl = document.getElementById('conn-mcp-oauth-status');
        function setStatus(msg) { if (statusEl) statusEl.textContent = msg; }

        // Store PKCE verifier + client_id in closure
        var _codeVerifier = null;
        var _clientId = null;
        var tokenField = mcpOauthConfig.tokenField || 'token';
        var tokenKey = mcpOauthConfig.tokenKey || 'access_token';
        var redirectUri = window.location.origin + '/mcp/oauth/' + mcpOauthConfig.provider + '/callback';

        // Exchange auth code for tokens (PKCE)
        async function exchangeMcpOauthCode(code) {
            setStatus('Authorization received, exchanging token...');
            var res = await api('/api/v1/mcp/oauth/' + mcpOauthConfig.provider + '/exchange', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    code: code,
                    code_verifier: _codeVerifier,
                    client_id: _clientId,
                    redirect_uri: redirectUri,
                    instance_name: instanceName || recipe.id,
                }),
            });

            var tokenValue = res.body && res.body[tokenKey];
            if (res.ok && tokenValue) {
                var hidden = document.getElementById('conn-field-' + tokenField);
                if (hidden) hidden.value = tokenValue;
                setStatus('\u2713 Authorization complete');
                btn.disabled = true;
                btn.textContent = '\u2713 Connected';
                // Auto-submit the form
                autoSubmitConnection(recipe, instanceName, showNameField);
            } else {
                var errMsg = (res.body && (res.body.message || res.body.error)) || 'Token exchange failed';
                setStatus('Error: ' + errMsg);
                showToast(errMsg, 'error');
                btn.disabled = false;
            }
        }

        // Listen for callback postMessage
        _oauthMessageHandler = function(event) {
            if (event.origin !== window.location.origin) return;
            var data = event.data || {};
            if (data.type !== 'homun-mcp-oauth-code') return;
            if (data.provider !== mcpOauthConfig.provider) return;
            if (data.error) {
                setStatus('Error: ' + (data.error_description || data.error));
                showToast(data.error_description || data.error, 'error');
                return;
            }
            exchangeMcpOauthCode(data.code);
        };
        window.addEventListener('message', _oauthMessageHandler);

        // Start flow on click
        btn.addEventListener('click', async function() {
            btn.disabled = true;
            setStatus('Registering client...');

            var res = await api('/api/v1/mcp/oauth/' + mcpOauthConfig.provider + '/start', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ redirect_uri: redirectUri }),
            });

            if (res.ok && res.body && res.body.auth_url) {
                _codeVerifier = res.body.code_verifier;
                _clientId = res.body.client_id;
                window.open(res.body.auth_url, '_blank', 'popup,width=720,height=840');
                setStatus('Waiting for authorization...');
                btn.disabled = false;
            } else {
                var errMsg = (res.body && res.body.error) || 'Failed to start MCP OAuth';
                setStatus('Error: ' + errMsg);
                showToast(errMsg, 'error');
                btn.disabled = false;
            }
        });
    }

    /** Auto-submit the connection form after MCP OAuth completes. */
    function autoSubmitConnection(recipe, instanceName, showNameField) {
        var resolvedName = instanceName;
        if (showNameField) {
            var nameInput = document.getElementById('conn-instance-name');
            resolvedName = nameInput ? nameInput.value.trim() : instanceName;
        }

        var fields = {};
        var form = document.getElementById('conn-form');
        if (form) {
            form.querySelectorAll('input[data-field-id]').forEach(function(input) {
                if (input.value.trim()) fields[input.dataset.fieldId] = input.value.trim();
            });
        }

        var submitBtn = document.getElementById('conn-submit-btn');
        if (submitBtn) {
            submitBtn.disabled = true;
            submitBtn.textContent = 'Connecting...';
        }

        api('/api/v1/connections/recipes/' + encodeURIComponent(recipe.id) + '/connect', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ fields: fields, skip_test: false, instance_name: resolvedName }),
        }).then(function(res) {
            if (submitBtn) { submitBtn.disabled = false; submitBtn.textContent = 'Connect'; }
            if (res.ok && res.body && res.body.ok) {
                showSuccessScreen(recipe, res.body);
            } else {
                var msg = (res.body && res.body.message) || 'Connection failed';
                showToast(msg, 'error');
            }
        });
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
                cleanupOauthListener();
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

    // ── Modal close cleanup (shared overlay is closed by mcp.js) ────

    var modalOverlayEl = document.getElementById('mcp-modal-overlay');
    if (modalOverlayEl) {
        modalOverlayEl.addEventListener('click', function(e) {
            if (e.target === modalOverlayEl) cleanupOauthListener();
        });
    }
    document.addEventListener('keydown', function(e) {
        if (e.key === 'Escape') cleanupOauthListener();
    });

    // ── Init ─────────────────────────────────────────────────────────

    window._connectionsLoad = loadCatalog;
    loadCatalog();
})();
