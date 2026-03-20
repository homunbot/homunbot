// Homun MCP page: catalog + guided setup + server management

(function() {
    async function api(url, opts) {
        var resp = await fetch(url, opts || {});
        var body = null;
        try {
            body = await resp.json();
        } catch (_) {
            body = null;
        }
        return { ok: resp.ok, status: resp.status, body: body };
    }

    function escapeHtml(text) {
        return String(text || '')
            .replaceAll('&', '&amp;')
            .replaceAll('<', '&lt;')
            .replaceAll('>', '&gt;')
            .replaceAll('"', '&quot;')
            .replaceAll("'", '&#39;');
    }

    function parseArgs(input) {
        var raw = String(input || '').trim();
        if (!raw) return [];
        var parts = raw.match(/(?:[^\s"]+|"[^"]*")+/g) || [];
        return parts.map(function(part) {
            var t = part.trim();
            if (t.startsWith('"') && t.endsWith('"')) return t.slice(1, -1);
            return t;
        });
    }

    function parseEnvLines(text) {
        var env = {};
        String(text || '').split('\n').forEach(function(line) {
            var raw = line.trim();
            if (!raw) return;
            var idx = raw.indexOf('=');
            if (idx <= 0) return;
            var key = raw.slice(0, idx).trim();
            var value = raw.slice(idx + 1).trim();
            if (key) env[key] = value;
        });
        return env;
    }

    var debounceTimer = null;
    function debounce(fn, ms) {
        return function() {
            var args = arguments;
            clearTimeout(debounceTimer);
            debounceTimer = setTimeout(function() {
                fn.apply(null, args);
            }, ms);
        };
    }

    function slugifyServerName(text) {
        var out = String(text || '')
            .toLowerCase()
            .replace(/[^a-z0-9]+/g, '-')
            .replace(/^-+|-+$/g, '')
            .slice(0, 40);
        return out || 'mcp-server';
    }

    var state = {
        catalogAll: [],
        catalog: [],
        servers: [],
        sandboxStatus: null,
        selectedCategory: 'All',
        activeQuery: '',
        showAlternatives: false,
        installPanelExpanded: false,
        oauthItem: null,
        oauthSavedVaultKeys: [],
    };

    var elCatalog = document.getElementById('mcp-catalog-grid');
    var elServers = document.getElementById('mcp-servers-list');
    var elServerCount = document.getElementById('mcp-server-count');
    var elConfiguredCount = document.getElementById('mcp-configured-count');
    var elConfiguredSection = document.getElementById('mcp-configured-section');
    var elCatalogCount = document.getElementById('mcp-catalog-count');
    var elSuggestInput = document.getElementById('mcp-suggest-input');
    var elSuggestStatus = document.getElementById('mcp-suggest-status');
    var elSearchSpinner = document.getElementById('mcp-search-spinner');
    var elCategoryChips = document.getElementById('mcp-category-chips');
    var elManualForm = document.getElementById('mcp-manual-form');
    var elInstallSection = document.getElementById('mcp-install-section');
    var elInstallPanel = document.getElementById('mcp-install-panel');
    var elInstallPanelHome = document.getElementById('mcp-install-panel-home');
    var elToggleInstallBtn = document.getElementById('mcp-toggle-install-btn');
    var elInstallHint = document.getElementById('mcp-install-hint');
    var elInstallAssistant = document.getElementById('mcp-install-assistant');
    var elOauthHelper = document.getElementById('mcp-oauth-helper');
    var elModalOverlay = document.getElementById('mcp-modal-overlay');
    var elModalTitle = document.getElementById('mcp-modal-title');
    var elModalSubtitle = document.getElementById('mcp-modal-subtitle');
    var elModalMeta = document.getElementById('mcp-modal-meta');
    var elModalContent = document.getElementById('mcp-modal-content');
    var elModalFooter = document.getElementById('mcp-modal-footer');
    var elModalClose = document.getElementById('mcp-modal-close');
    var elTransport = document.getElementById('mcp-transport');
    var elStdioGroup = document.getElementById('mcp-stdio-group');
    var elHttpGroup = document.getElementById('mcp-http-group');
    var elSandboxBadge = document.getElementById('mcp-sandbox-runtime-badge');
    var elSandboxText = document.getElementById('mcp-sandbox-runtime-text');
    var elRefreshSandboxStatusBtn = document.getElementById('mcp-refresh-sandbox-status-btn');
    var elUiLanguage = document.getElementById('mcp-ui-language');
    var installGuideReqSeq = 0;

    function sourceLabel(src) {
        if (src === 'official-registry') return 'Official Registry';
        if (src === 'mcpmarket') return 'MCPMarket';
        if (src === 'curated' || src === 'preset') return 'Curated';
        if (!src) return 'Source';
        return src;
    }

    function sourceBadgeClass(src) {
        if (src === 'official-registry') return 'skill-source-badge--clawhub';
        if (src === 'mcpmarket') return 'skill-source-badge--openskills';
        return 'skill-source-badge--github';
    }

    function getUiLanguage() {
        var configured = (elUiLanguage && elUiLanguage.value) || localStorage.getItem('homun-language') || 'system';
        if (configured === 'system') {
            return ((navigator.language || 'en').split('-')[0] || 'en').toLowerCase();
        }
        return String(configured || 'en').toLowerCase();
    }

    function closeModal() {
        if (!elModalOverlay) return;
        restoreInstallPanelHome();
        elModalOverlay.classList.remove('active');
        // Clean up split-pane layout (set by connections.js)
        var modal = document.getElementById('mcp-modal');
        if (modal) modal.classList.remove('skill-modal--split');
        var splitBody = document.getElementById('conn-split-body');
        if (splitBody) splitBody.remove();
        var modalContent = document.getElementById('mcp-modal-content');
        if (modalContent) modalContent.style.display = '';
        var modalBody = modalContent ? modalContent.parentElement : null;
        if (modalBody) modalBody.style.padding = '';
    }

    function syncInstallPanelVisibility() {
        if (!elInstallPanelHome) return;
        elInstallPanelHome.style.display = state.installPanelExpanded ? '' : 'none';
        if (elToggleInstallBtn) {
            elToggleInstallBtn.textContent = state.installPanelExpanded
                ? 'Hide manual installer'
                : 'Open manual installer';
            elToggleInstallBtn.setAttribute('aria-expanded', state.installPanelExpanded ? 'true' : 'false');
        }
    }

    function restoreInstallPanelHome() {
        if (!elInstallPanel || !elInstallPanelHome) return;
        if (elInstallPanel.parentElement !== elInstallPanelHome) {
            elInstallPanelHome.appendChild(elInstallPanel);
        }
        syncInstallPanelVisibility();
    }

    function attachInstallPanelToModal() {
        if (!elInstallPanel || !elModalContent) return;
        var dock = document.getElementById('mcp-modal-install-dock');
        if (!dock) return;
        dock.appendChild(elInstallPanel);
    }

    function buildModalDetailsHtml(item, installMode) {
        var args = (item.args || []).map(escapeHtml).join(' ');
        var envRows = (item.env || []).map(function(e) {
            var badges = '';
            if (e.required) badges += ' <span class="badge badge-warning">required</span>';
            if (e.secret) badges += ' <span class="badge badge-neutral">secret</span>';
            return '' +
                '<tr>' +
                    '<td><code>' + escapeHtml(e.key || '') + '</code></td>' +
                    '<td>' + escapeHtml(e.description || 'No description') + badges + '</td>' +
                '</tr>';
        }).join('');
        var detailLine = '';
        if ((item.transport || (item.url ? 'http' : 'stdio')) === 'http' && item.url) {
            detailLine = '<p><strong>Endpoint:</strong> <code>' + escapeHtml(item.url) + '</code></p>';
        } else if (item.command) {
            detailLine = '<p><strong>Command:</strong> <code>' + escapeHtml(item.command + ' ' + args) + '</code></p>';
        } else {
            detailLine = '<p><strong>Runtime:</strong> manual setup required</p>';
        }
        return '' +
            (item.description ? '<p>' + escapeHtml(item.description) + '</p>' : '') +
            detailLine +
            (item.docs_url
                ? '<p><strong>Documentation:</strong> <a href="' + escapeHtml(item.docs_url) + '" target="_blank" rel="noopener noreferrer">' + escapeHtml(item.docs_url) + '</a></p>'
                : '') +
            '<h3>Environment Variables</h3>' +
            (envRows
                ? '<table><thead><tr><th>Key</th><th>How to fill</th></tr></thead><tbody>' + envRows + '</tbody></table>'
                : '<p>No env vars required.</p>') +
            (installMode
                ? '<h3>Installer</h3><div id="mcp-modal-install-dock"></div>'
                : '');
    }

    function openModalForItem(item, idx, installMode) {
        if (!elModalOverlay || !item) return;
        restoreInstallPanelHome();
        if (elModalTitle) elModalTitle.textContent = item.display_name || item.id || 'MCP Server';
        if (elModalSubtitle) {
            var subtitle = item.description || '';
            if (!subtitle && item.package_name) subtitle = item.package_name;
            elModalSubtitle.textContent = subtitle;
        }
        if (elModalMeta) {
            var transport = item.transport || (item.url ? 'http' : 'stdio');
            var source = item.source || '';
            var html = '' +
                '<span class="skill-source-badge ' + sourceBadgeClass(source) + '">' + escapeHtml(sourceLabel(source)) + '</span>' +
                '<span class="skill-modal-meta-item"><code>' + escapeHtml(item.id || '') + '</code></span>' +
                '<span class="skill-modal-meta-item"><code>' + escapeHtml(transport) + '</code></span>';
            if (item.popularity_rank) {
                html += '<span class="skill-modal-meta-item">#' + escapeHtml(item.popularity_rank) + ' top100</span>';
            }
            elModalMeta.innerHTML = html;
        }
        if (elModalContent) {
            elModalContent.innerHTML = buildModalDetailsHtml(item, !!installMode);
        }
        if (installMode) {
            attachInstallPanelToModal();
        }
        if (elModalFooter) {
            var actions = '';
            if (item.kind === 'preset') {
                actions += '<button type="button" class="btn btn-sm btn-primary mcp-modal-connect-btn" data-index="' + idx + '">Connect</button>';
            } else if (item.install_supported) {
                if (installMode) {
                    actions += '<button type="button" class="btn btn-sm btn-secondary mcp-modal-quick-btn" data-index="' + idx + '">Quick Add</button>';
                } else {
                    actions += '<button type="button" class="btn btn-sm btn-primary mcp-modal-install-btn" data-index="' + idx + '">Install (guided)</button>';
                    actions += '<button type="button" class="btn btn-sm btn-secondary mcp-modal-quick-btn" data-index="' + idx + '">Quick Add</button>';
                }
            }
            if (item.docs_url) {
                actions += '<a class="btn btn-sm btn-secondary" href="' + escapeHtml(item.docs_url) + '" target="_blank" rel="noopener noreferrer">Documentation</a>';
            }
            actions += '<button type="button" class="btn btn-sm btn-secondary mcp-modal-copy-btn" data-index="' + idx + '">Copy ID</button>';
            elModalFooter.innerHTML = actions;
        }
        elModalOverlay.classList.add('active');
    }

    function updateTransportUI() {
        var transport = elTransport ? elTransport.value : 'stdio';
        if (elStdioGroup) elStdioGroup.style.display = transport === 'stdio' ? '' : 'none';
        if (elHttpGroup) elHttpGroup.style.display = transport === 'http' ? '' : 'none';
    }

    function assistantSourceLabel(source) {
        if (source === 'llm+docs') return 'AI + docs';
        if (source === 'llm') return 'AI guidance';
        if (source === 'docs') return 'Docs guidance';
        return 'Fallback guidance';
    }

    function renderInstallAssistantLoading(item, targetEl) {
        var target = targetEl || elInstallAssistant;
        if (!target) return;
        target.style.display = '';
        target.className = 'mcp-install-assistant loading';
        target.innerHTML = '' +
            '<div class="mcp-assistant-loading-row">' +
                '<span class="mcp-assistant-spinner" aria-hidden="true"></span>' +
                '<div>' +
                    '<div class="mcp-assistant-title">Install Assistant</div>' +
                    '<div class="mcp-assistant-summary">Analyzing docs and required variables for "' + escapeHtml(item.display_name || item.id) + '"...</div>' +
                '</div>' +
            '</div>';
    }

    function renderInstallAssistantError(message, targetEl) {
        var target = targetEl || elInstallAssistant;
        if (!target) return;
        target.style.display = '';
        target.className = 'mcp-install-assistant fallback';
        target.innerHTML = '' +
            '<div class="mcp-assistant-title">Install Assistant</div>' +
            '<div class="mcp-assistant-summary">' + escapeHtml(message || 'Unable to generate install guidance.') + '</div>';
    }

    function renderInstallAssistant(guide, item, targetEl) {
        var target = targetEl || elInstallAssistant;
        if (!target) return;
        var source = assistantSourceLabel(guide.source);
        var steps = (guide.steps || []).map(function(step) {
            return '<li>' + escapeHtml(step) + '</li>';
        }).join('');
        var envRows = (guide.env_help || []).map(function(e) {
            var retrievalSteps = (e.retrieval_steps || []).map(function(step) {
                return '<li>' + escapeHtml(step) + '</li>';
            }).join('');
            return '' +
                '<div class="mcp-assistant-env-item">' +
                    '<div class="mcp-assistant-env-key">' + escapeHtml(e.key) + '</div>' +
                    '<div class="mcp-assistant-env-line"><strong>Why:</strong> ' + escapeHtml(e.why || '') + '</div>' +
                    '<div class="mcp-assistant-env-line"><strong>Where:</strong> ' + escapeHtml(e.where_to_get || '') + '</div>' +
                    '<div class="mcp-assistant-env-line"><strong>Format:</strong> <code>' + escapeHtml(e.format_hint || '') + '</code></div>' +
                    '<div class="mcp-assistant-env-line"><strong>Vault:</strong> <code>' + escapeHtml(e.vault_hint || '') + '</code></div>' +
                    (retrievalSteps ? '<ul class="mcp-assistant-env-steps">' + retrievalSteps + '</ul>' : '') +
                '</div>';
        }).join('');
        var notes = (guide.notes || []).map(function(note) {
            return '<li>' + escapeHtml(note) + '</li>';
        }).join('');
        var docs = guide.documentation || null;
        var docsHtml = '';
        if (docs && (docs.summary || (docs.highlights || []).length)) {
            var highlights = (docs.highlights || []).map(function(line) {
                return '<li>' + escapeHtml(line) + '</li>';
            }).join('');
            docsHtml = '' +
                '<div class="mcp-assistant-docs">' +
                    '<div class="mcp-assistant-title">Documentation Reading</div>' +
                    (docs.summary ? '<div class="mcp-assistant-summary">' + escapeHtml(docs.summary) + '</div>' : '') +
                    (highlights ? '<ul class="mcp-assistant-notes">' + highlights + '</ul>' : '') +
                '</div>';
        }

        target.style.display = '';
        target.className = 'mcp-install-assistant';
        target.innerHTML = '' +
            '<div class="mcp-assistant-head">' +
                '<div class="mcp-assistant-title">Install Assistant</div>' +
                '<span class="badge badge-neutral">' + escapeHtml(source) + '</span>' +
            '</div>' +
            '<div class="mcp-assistant-summary">' + escapeHtml(guide.summary || ('Guidance for ' + (item.display_name || item.id))) + '</div>' +
            docsHtml +
            (steps ? '<ol class="mcp-assistant-steps">' + steps + '</ol>' : '') +
            (envRows ? '<div class="mcp-assistant-env-list">' + envRows + '</div>' : '') +
            (notes ? '<ul class="mcp-assistant-notes">' + notes + '</ul>' : '') +
            (guide.error ? '<div class="mcp-assistant-error">Assistant fallback reason: ' + escapeHtml(guide.error) + '</div>' : '');
    }

    async function requestInstallAssistant(item, targetEl) {
        var requestSeq = ++installGuideReqSeq;
        renderInstallAssistantLoading(item, targetEl);
        var payload = {
            id: item.id,
            display_name: item.display_name,
            description: item.description,
            docs_url: item.docs_url,
            transport: item.transport || (item.url ? 'http' : 'stdio'),
            command: item.command,
            args: item.args || [],
            env: item.env || [],
            language: getUiLanguage(),
        };

        var startedAt = Date.now();
        var res = await api('/api/v1/mcp/install-guide', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(payload),
        });
        var elapsed = Date.now() - startedAt;
        if (elapsed < 550) {
            await new Promise(function(resolve) { setTimeout(resolve, 550 - elapsed); });
        }
        if (requestSeq !== installGuideReqSeq) return;

        if (!res.ok || !res.body) {
            renderInstallAssistantError('Install guide unavailable. Continue with template and documentation.', targetEl);
            return;
        }
        renderInstallAssistant(res.body, item, targetEl);
    }

    function oauthConfigForItem(item) {
        if (!item || !item.id) return null;
        var id = String(item.id || '').toLowerCase();
        if (id === 'gmail' || id === 'google-calendar') {
            return {
                provider: 'google',
                service: id,
                title: 'Google OAuth Helper',
                consentLabel: 'Open Google Consent',
                startEndpoint: '/api/v1/mcp/oauth/google/start',
                exchangeEndpoint: '/api/v1/mcp/oauth/google/exchange',
                redirectPath: '/mcp/oauth/google/callback',
                statusCopy: 'Use your Google OAuth client, authorize the consent link, then exchange the code to fill <code>GOOGLE_REFRESH_TOKEN</code> automatically.',
                helperCopy: 'Generate the Google consent URL, finish authorization in a new tab, then exchange the returned code to populate the MCP env template.',
                redirectHint: 'Register this exact redirect URI in Google Cloud. Homun will catch the callback page and send the code back to this setup window.',
                saveVaultCopy: 'Store client credentials and refresh token in Vault, then write <code>vault://...</code> refs into env',
                plannedVaultKeys: function(service) {
                    return [
                        oauthVaultKey(service, 'client_id'),
                        oauthVaultKey(service, 'client_secret'),
                        oauthVaultKey(service, 'refresh_token')
                    ];
                },
                prefill: function(envMap, refs) {
                    if (refs.clientId && !refs.clientId.value.trim() && envMap.GOOGLE_CLIENT_ID && envMap.GOOGLE_CLIENT_ID.indexOf('vault://') !== 0) {
                        refs.clientId.value = envMap.GOOGLE_CLIENT_ID;
                    }
                    if (refs.clientSecret && !refs.clientSecret.value.trim() && envMap.GOOGLE_CLIENT_SECRET && envMap.GOOGLE_CLIENT_SECRET.indexOf('vault://') !== 0) {
                        refs.clientSecret.value = envMap.GOOGLE_CLIENT_SECRET;
                    }
                },
                buildExchangePayload: function(values) {
                    return {
                        service: id,
                        code: values.authCode,
                        client_id: values.clientId,
                        client_secret: values.clientSecret,
                        redirect_uri: values.redirectUri
                    };
                },
                applyExchangeResult: async function(values, body, saveToVault) {
                    var refreshToken = String(body.refresh_token || '').trim();
                    var envUpdates = {};
                    var savedVaultKeys = [];
                    if (saveToVault) {
                        var clientIdKey = oauthVaultKey(id, 'client_id');
                        var clientSecretKey = oauthVaultKey(id, 'client_secret');
                        await saveVaultSecret(clientIdKey, values.clientId);
                        await saveVaultSecret(clientSecretKey, values.clientSecret);
                        savedVaultKeys.push(clientIdKey, clientSecretKey);
                        envUpdates.GOOGLE_CLIENT_ID = 'vault://' + clientIdKey;
                        envUpdates.GOOGLE_CLIENT_SECRET = 'vault://' + clientSecretKey;
                        if (refreshToken) {
                            var refreshTokenKey = oauthVaultKey(id, 'refresh_token');
                            await saveVaultSecret(refreshTokenKey, refreshToken);
                            savedVaultKeys.push(refreshTokenKey);
                            envUpdates.GOOGLE_REFRESH_TOKEN = 'vault://' + refreshTokenKey;
                        }
                    } else {
                        envUpdates.GOOGLE_CLIENT_ID = values.clientId;
                        envUpdates.GOOGLE_CLIENT_SECRET = values.clientSecret;
                        if (refreshToken) envUpdates.GOOGLE_REFRESH_TOKEN = refreshToken;
                    }

                    mergeManualEnvValues(envUpdates);
                    return {
                        ok: !!refreshToken,
                        savedVaultKeys: savedVaultKeys,
                        successMessage: '<strong>OAuth complete.</strong> Refresh token captured and written into the MCP env template.' +
                            (savedVaultKeys.length ? (' Saved in Vault as ' + savedVaultKeys.map(function(key) { return '<code>' + escapeHtml(key) + '</code>'; }).join(', ') + '.') : ''),
                        errorMessage: '<strong>Exchange succeeded, but no refresh token was returned.</strong> Retry consent after forcing a fresh Google approval. In Google Cloud, keep this redirect URI registered, request offline access, and if needed revoke the app grant before retrying.',
                        toastSuccess: 'Google OAuth token captured',
                        toastError: 'Google OAuth succeeded without refresh token'
                    };
                }
            };
        }
        if (id === 'github') {
            return {
                provider: 'github',
                service: id,
                title: 'GitHub OAuth Helper',
                consentLabel: 'Open GitHub Consent',
                startEndpoint: '/api/v1/mcp/oauth/github/start',
                exchangeEndpoint: '/api/v1/mcp/oauth/github/exchange',
                redirectPath: '/mcp/oauth/github/callback',
                statusCopy: 'Use your GitHub OAuth app, approve the consent page, then exchange the code to fill <code>GITHUB_PERSONAL_ACCESS_TOKEN</code> automatically.',
                helperCopy: 'Generate the GitHub consent URL, finish authorization in a new tab, then exchange the returned code to populate the MCP env template.',
                redirectHint: 'Register this exact callback URL in your GitHub OAuth App settings. Homun will capture the callback page and send the code back to this setup window.',
                saveVaultCopy: 'Store the exchanged GitHub access token in Vault, then write <code>vault://...</code> into env',
                plannedVaultKeys: function() {
                    return [oauthVaultKey('github', 'token')];
                },
                prefill: function() {},
                buildExchangePayload: function(values) {
                    return {
                        service: id,
                        code: values.authCode,
                        client_id: values.clientId,
                        client_secret: values.clientSecret,
                        redirect_uri: values.redirectUri
                    };
                },
                applyExchangeResult: async function(values, body, saveToVault) {
                    var accessToken = String(body.access_token || '').trim();
                    var envUpdates = {};
                    var savedVaultKeys = [];
                    if (saveToVault) {
                        var tokenKey = oauthVaultKey('github', 'token');
                        await saveVaultSecret(tokenKey, accessToken);
                        savedVaultKeys.push(tokenKey);
                        envUpdates.GITHUB_PERSONAL_ACCESS_TOKEN = 'vault://' + tokenKey;
                    } else {
                        envUpdates.GITHUB_PERSONAL_ACCESS_TOKEN = accessToken;
                    }
                    mergeManualEnvValues(envUpdates);
                    return {
                        ok: !!accessToken,
                        savedVaultKeys: savedVaultKeys,
                        successMessage: '<strong>OAuth complete.</strong> GitHub access token captured and written into the MCP env template.' +
                            (savedVaultKeys.length ? (' Saved in Vault as ' + savedVaultKeys.map(function(key) { return '<code>' + escapeHtml(key) + '</code>'; }).join(', ') + '.') : ''),
                        errorMessage: '<strong>Exchange succeeded, but GitHub did not return an access token.</strong> Verify your GitHub OAuth App client, callback URL, and requested scopes before retrying.',
                        toastSuccess: 'GitHub OAuth token captured',
                        toastError: 'GitHub OAuth did not return a token'
                    };
                }
            };
        }
        return null;
    }

    function shouldUseGuidedSetup(item) {
        if (!item) return false;
        if (oauthConfigForItem(item)) return true;
        return Array.isArray(item.env) && item.env.length > 0;
    }

    function oauthVaultKey(service, suffix) {
        return ('mcp_' + String(service || 'oauth').replace(/[^a-z0-9]+/g, '_') + '_' + suffix)
            .replace(/_+/g, '_')
            .replace(/^_+|_+$/g, '');
    }

    function oauthRedirectUri(config) {
        return window.location.origin + String((config && config.redirectPath) || '/mcp/oauth/google/callback');
    }

    function plannedOauthVaultKeys(config) {
        if (!config || typeof config.plannedVaultKeys !== 'function') return [];
        return config.plannedVaultKeys(config.service);
    }

    function hideOauthHelper() {
        state.oauthItem = null;
        state.oauthSavedVaultKeys = [];
        if (!elOauthHelper) return;
        elOauthHelper.style.display = 'none';
        elOauthHelper.innerHTML = '';
    }

    function setOauthHelperStatus(message, type) {
        var statusEl = document.getElementById('mcp-oauth-status');
        if (!statusEl) return;
        statusEl.className = 'mcp-oauth-helper-status' + (type ? (' ' + type) : '');
        statusEl.innerHTML = message || '';
    }

    function oauthErrorDetail(res, fallback) {
        if (res && res.body) {
            if (res.body.error) return String(res.body.error);
            if (res.body.message) return String(res.body.message);
        }
        return fallback || 'Unknown OAuth error.';
    }

    function getManualEnvMap() {
        var envInput = document.getElementById('mcp-env');
        return parseEnvLines(envInput ? envInput.value : '');
    }

    function setManualEnvMap(envMap) {
        var envInput = document.getElementById('mcp-env');
        if (!envInput) return;
        var ordered = [];
        var seen = {};
        if (state.oauthItem && Array.isArray(state.oauthItem.env)) {
            state.oauthItem.env.forEach(function(spec) {
                if (!spec || !spec.key) return;
                var key = String(spec.key);
                if (Object.prototype.hasOwnProperty.call(envMap, key)) {
                    ordered.push(key);
                    seen[key] = true;
                }
            });
        }
        Object.keys(envMap).sort().forEach(function(key) {
            if (!seen[key]) ordered.push(key);
        });
        envInput.value = ordered.map(function(key) {
            return key + '=' + String(envMap[key] || '');
        }).join('\n');
    }

    function mergeManualEnvValues(updates) {
        var envMap = getManualEnvMap();
        Object.keys(updates || {}).forEach(function(key) {
            envMap[key] = updates[key];
        });
        setManualEnvMap(envMap);
    }

    function renderOauthVaultSummary(config) {
        var target = document.getElementById('mcp-oauth-vault-keys');
        if (!target) return;
        var keys = state.oauthSavedVaultKeys.length
            ? state.oauthSavedVaultKeys.slice()
            : plannedOauthVaultKeys(config);
        target.innerHTML = keys.map(function(key) {
            return '<code>' + escapeHtml(key) + '</code>';
        }).join(' ');
    }

    function prefillOauthHelperFromEnv(config) {
        var envMap = getManualEnvMap();
        var clientIdEl = document.getElementById('mcp-oauth-client-id');
        var clientSecretEl = document.getElementById('mcp-oauth-client-secret');
        var redirectEl = document.getElementById('mcp-oauth-redirect-uri');
        if (config && typeof config.prefill === 'function') {
            config.prefill(envMap, {
                clientId: clientIdEl,
                clientSecret: clientSecretEl,
                redirectUri: redirectEl
            });
        }
        if (redirectEl && !redirectEl.value.trim()) redirectEl.value = oauthRedirectUri(config);
        var saveVaultEl = document.getElementById('mcp-oauth-save-vault');
        if (saveVaultEl) saveVaultEl.checked = true;
        renderOauthVaultSummary(config);
        setOauthHelperStatus(config ? config.statusCopy : '', '');
    }

    function renderOauthHelper(item) {
        if (!elOauthHelper) return;
        var config = oauthConfigForItem(item);
        if (!config) {
            hideOauthHelper();
            return;
        }

        state.oauthItem = item;
        elOauthHelper.style.display = '';
        elOauthHelper.innerHTML = '' +
            '<div class="mcp-oauth-helper-header">' +
                '<div class="mcp-assistant-title">' + escapeHtml(config.title) + '</div>' +
                '<span class="badge badge-info">Beta</span>' +
            '</div>' +
            '<p class="mcp-oauth-helper-copy">' + escapeHtml(config.helperCopy) + '</p>' +
            '<div class="mcp-oauth-helper-grid">' +
                '<div class="form-group">' +
                    '<label for="mcp-oauth-client-id">Client ID</label>' +
                    '<input id="mcp-oauth-client-id" class="input" type="text" autocomplete="off">' +
                '</div>' +
                '<div class="form-group">' +
                    '<label for="mcp-oauth-client-secret">Client Secret</label>' +
                    '<input id="mcp-oauth-client-secret" class="input" type="password" autocomplete="off">' +
                '</div>' +
                '<div class="form-group form-group--full">' +
                    '<label for="mcp-oauth-redirect-uri">Redirect URI</label>' +
                    '<input id="mcp-oauth-redirect-uri" class="input" type="text" autocomplete="off">' +
                    '<div class="form-hint">' + escapeHtml(config.redirectHint) + '</div>' +
                '</div>' +
                '<div class="form-group form-group--full">' +
                    '<label for="mcp-oauth-auth-code">Authorization Code</label>' +
                    '<textarea id="mcp-oauth-auth-code" class="input" rows="3" placeholder="Filled automatically after callback, or paste the code manually."></textarea>' +
                '</div>' +
            '</div>' +
            '<label class="checkbox-label">' +
                '<input type="checkbox" id="mcp-oauth-save-vault">' +
                '<span>' + config.saveVaultCopy + '</span>' +
            '</label>' +
            '<div class="mcp-oauth-helper-status"><strong>Vault keys</strong> <span id="mcp-oauth-vault-keys"></span></div>' +
            '<div class="form-actions">' +
                '<button type="button" class="btn btn-secondary btn-sm" id="mcp-oauth-start-btn">' + escapeHtml(config.consentLabel) + '</button>' +
                '<button type="button" class="btn btn-primary btn-sm" id="mcp-oauth-exchange-btn">Exchange Code</button>' +
                '<button type="button" class="btn btn-secondary btn-sm" id="mcp-oauth-retry-btn">Retry Consent</button>' +
                '<a class="btn btn-secondary btn-sm" href="/vault" target="_blank" rel="noopener noreferrer">Open Vault</a>' +
            '</div>' +
            '<div class="mcp-oauth-helper-status" id="mcp-oauth-status"></div>';

        prefillOauthHelperFromEnv(config);
    }

    async function saveVaultSecret(key, value) {
        var res = await api('/api/v1/vault', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ key: key, value: value })
        });
        if (!res.ok || !res.body || !res.body.ok) {
            throw new Error((res.body && res.body.message) || 'Failed to save secret to Vault');
        }
    }

    async function startOauthFlow() {
        if (!state.oauthItem) return;
        var config = oauthConfigForItem(state.oauthItem);
        if (!config) return;
        var clientId = String((document.getElementById('mcp-oauth-client-id') || {}).value || '').trim();
        var redirectUri = String((document.getElementById('mcp-oauth-redirect-uri') || {}).value || '').trim();
        if (!clientId || !redirectUri) {
            setOauthHelperStatus('<strong>Missing fields.</strong> Client ID and redirect URI are required before starting OAuth.', 'error');
            return;
        }

        setOauthHelperStatus('Generating ' + escapeHtml(config.provider === 'github' ? 'GitHub' : 'Google') + ' consent URL...', '');
        var res = await api(config.startEndpoint, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                service: config.service,
                client_id: clientId,
                redirect_uri: redirectUri
            })
        });
        if (!res.ok || !res.body || !res.body.auth_url) {
            setOauthHelperStatus(
                '<strong>Unable to start OAuth.</strong> ' +
                escapeHtml(oauthErrorDetail(res, 'Check your client ID and redirect URI.')),
                'error'
            );
            return;
        }

        window.open(res.body.auth_url, '_blank', 'popup,width=720,height=840');
        setOauthHelperStatus(
            (config.provider === 'github' ? 'GitHub' : 'Google') + ' consent opened in a new tab. After approving access, Homun will capture the callback and prefill the authorization code here.',
            ''
        );
    }

    async function exchangeOauthCode() {
        if (!state.oauthItem) return;
        var config = oauthConfigForItem(state.oauthItem);
        if (!config) return;
        var clientId = String((document.getElementById('mcp-oauth-client-id') || {}).value || '').trim();
        var clientSecret = String((document.getElementById('mcp-oauth-client-secret') || {}).value || '').trim();
        var redirectUri = String((document.getElementById('mcp-oauth-redirect-uri') || {}).value || '').trim();
        var authCode = String((document.getElementById('mcp-oauth-auth-code') || {}).value || '').trim();
        var saveToVault = !!((document.getElementById('mcp-oauth-save-vault') || {}).checked);

        if (!clientId || !clientSecret || !redirectUri || !authCode) {
            setOauthHelperStatus('<strong>Missing fields.</strong> Fill client ID, client secret, redirect URI, and authorization code first.', 'error');
            return;
        }

        setOauthHelperStatus('Exchanging authorization code with ' + (config.provider === 'github' ? 'GitHub' : 'Google') + '...', '');
        var res = await api(config.exchangeEndpoint, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(config.buildExchangePayload({
                clientId: clientId,
                clientSecret: clientSecret,
                redirectUri: redirectUri,
                authCode: authCode
            }))
        });
        if (!res.ok || !res.body || !res.body.ok) {
            setOauthHelperStatus(
                '<strong>Token exchange failed.</strong> ' +
                escapeHtml(oauthErrorDetail(res, 'Verify redirect URI, code, and OAuth app settings.')),
                'error'
            );
            return;
        }

        var outcome = await config.applyExchangeResult({
            clientId: clientId,
            clientSecret: clientSecret,
            redirectUri: redirectUri,
            authCode: authCode
        }, res.body, saveToVault);

        state.oauthSavedVaultKeys = outcome.savedVaultKeys || [];
        renderOauthVaultSummary(config);
        if (outcome.ok) {
            setOauthHelperStatus(outcome.successMessage, 'success');
            showToast(outcome.toastSuccess, 'success');
        } else {
            setOauthHelperStatus(outcome.errorMessage, 'error');
            showToast(outcome.toastError, 'warning');
        }
    }

    function renderSandboxStatus(status) {
        if (!elSandboxBadge || !elSandboxText) return;
        if (!status) {
            elSandboxBadge.textContent = 'unknown';
            elSandboxBadge.classList.remove('badge-success', 'badge-warning', 'badge-error');
            elSandboxBadge.classList.add('badge-neutral');
            elSandboxText.textContent = 'Unable to determine execution sandbox status.';
            return;
        }

        var badgeLabel = status.enabled ? ('resolved: ' + status.resolved_backend) : 'disabled';
        elSandboxBadge.textContent = badgeLabel;
        elSandboxBadge.classList.remove('badge-success', 'badge-warning', 'badge-error', 'badge-neutral');
        if (!status.enabled) {
            elSandboxBadge.classList.add('badge-neutral');
        } else if (!status.valid) {
            elSandboxBadge.classList.add('badge-error');
        } else if (status.fallback_to_native) {
            elSandboxBadge.classList.add('badge-warning');
        } else {
            elSandboxBadge.classList.add('badge-success');
        }

        var availabilityText = status.availability_summary
            || ('Docker: ' + (status.docker_available ? 'available' : 'unavailable') + '.');
        elSandboxText.textContent = ((status.message || 'Sandbox status updated.') + ' ' + availabilityText).trim();
    }

    async function loadSandboxStatus() {
        var res = await api('/api/v1/security/sandbox/status');
        if (!res.ok || !res.body) {
            state.sandboxStatus = null;
            renderSandboxStatus(null);
            return;
        }
        state.sandboxStatus = res.body;
        renderSandboxStatus(state.sandboxStatus);
    }

    function inferCategory(item) {
        if (item.popularity_rank && item.popularity_rank <= 100) return 'Top Ranked';
        var text = (
            (item.display_name || '') + ' ' +
            (item.description || '') + ' ' +
            (item.id || '') + ' ' +
            ((item.keywords || []).join(' '))
        ).toLowerCase();

        if (/(github|git|code|cursor|xcode|dev|agent|workflow|task|automation|n8n)/.test(text)) return 'Developer';
        if (/(browser|chrome|playwright|web|http|fetch|crawl|scrap)/.test(text)) return 'Web & Browser';
        if (/(database|sql|postgres|mysql|mongo|redis|vector|storage|memory|notion)/.test(text)) return 'Data & Storage';
        if (/(email|gmail|calendar|slack|jira|figma|whatsapp|meeting|productivity)/.test(text)) return 'Productivity';
        if (/(aws|cloud|docker|kubernetes|deploy|devops|server|runtime)/.test(text)) return 'Cloud & DevOps';
        if (/(security|test|audit|scan|sandbox)/.test(text)) return 'Security & Testing';
        return 'Other';
    }

    function categoryOrder(category) {
        var order = {
            'Top Ranked': 0,
            'Developer': 1,
            'Web & Browser': 2,
            'Data & Storage': 3,
            'Productivity': 4,
            'Cloud & DevOps': 5,
            'Security & Testing': 6,
            'Other': 7,
        };
        return Object.prototype.hasOwnProperty.call(order, category) ? order[category] : 999;
    }

    function renderCategoryChips(counts) {
        if (!elCategoryChips) return;
        var categories = Object.keys(counts).sort(function(a, b) {
            return categoryOrder(a) - categoryOrder(b);
        });
        var total = categories.reduce(function(acc, key) { return acc + counts[key]; }, 0);
        var html = '' +
            '<button type="button" class="mcp-chip ' + (state.selectedCategory === 'All' ? 'active' : '') + '" data-category="All">' +
                'All <span>' + escapeHtml(total) + '</span>' +
            '</button>';
        categories.forEach(function(category) {
            html += '' +
                '<button type="button" class="mcp-chip ' + (state.selectedCategory === category ? 'active' : '') + '" data-category="' + escapeHtml(category) + '">' +
                    escapeHtml(category) + ' <span>' + escapeHtml(counts[category]) + '</span>' +
                '</button>';
        });
        elCategoryChips.innerHTML = html;
    }

    function renderCatalogCard(item, idx, mode) {
        mode = mode || 'default';
        var envCount = (item.env || []).length;
        var transport = item.transport || (item.url ? 'http' : 'stdio');
        var source = item.source || '';
        var sourceBadge = '<span class="skill-source-badge ' + sourceBadgeClass(source) + '">' + escapeHtml(sourceLabel(source)) + '</span>';
        var primaryLabel = item.kind === 'preset' ? 'Connect' : 'Install';
        var quickAddBtn = (item.kind !== 'preset' && item.install_supported)
            ? '<button type="button" class="btn btn-sm btn-secondary mcp-quickadd-btn" data-index="' + idx + '">Quick Add</button>'
            : '';
        var primaryDisabled = (item.kind !== 'preset' && !item.install_supported) ? ' disabled' : '';
        var footerMeta = '<span class="skill-path">' + escapeHtml(transport) + ' · ' + envCount + ' env</span>';
        if (item.popularity_rank) {
            footerMeta = '<span class="skill-path">#' + escapeHtml(item.popularity_rank) + ' top100 · ' + escapeHtml(transport) + '</span>';
        }
        var decisionTags = state.activeQuery && item.decision_tags && item.decision_tags.length
            ? '<div class="mcp-card-tags">' + item.decision_tags.map(function(tag) {
                return '<span class="badge badge-neutral">' + escapeHtml(tag) + '</span>';
            }).join('') + '</div>'
            : '';
        var decisionNotes = '';
        if (mode === 'alternative' && (item.why_choose || item.tradeoff)) {
            decisionNotes = '' +
                '<div class="mcp-card-rationale mcp-card-rationale--compact">' +
                    (item.why_choose ? '<div><strong>Why choose this</strong> ' + escapeHtml(item.why_choose) + '</div>' : '') +
                    (item.tradeoff ? '<div><strong>Tradeoff</strong> ' + escapeHtml(item.tradeoff) + '</div>' : '') +
                '</div>';
        }
        var recommendationBadge = item.recommended
            ? '<span class="mcp-card-flag">Recommended</span>'
            : '';
        return '' +
            '<div class="skill-card mcp-catalog-card' +
                (state.activeQuery ? ' mcp-catalog-card--decision' : '') +
                (mode === 'featured' ? ' mcp-catalog-card--featured' : '') +
                (mode === 'alternative' ? ' mcp-catalog-card--alternative' : '') +
                '" data-index="' + idx + '">' +
                '<div class="skill-card-header">' +
                    '<div class="skill-name">' + escapeHtml(item.display_name || item.id) + recommendationBadge + '</div>' +
                    sourceBadge +
                '</div>' +
                '<div class="skill-desc">' + escapeHtml(item.description || 'No description available.') + '</div>' +
                ((mode === 'featured' || mode === 'alternative') ? decisionTags : '') +
                '<div class="skill-meta mcp-card-meta">' +
                    '<span class="skill-stat"><code>' + escapeHtml(item.id || '') + '</code></span>' +
                '</div>' +
                decisionNotes +
                '<div class="skill-card-footer">' +
                    footerMeta +
                    '<div class="mcp-card-actions">' +
                        '<button type="button" class="btn btn-sm btn-primary mcp-connect-btn" data-index="' + idx + '"' + primaryDisabled + '>' + primaryLabel + '</button>' +
                        quickAddBtn +
                    '</div>' +
                '</div>' +
            '</div>';
    }

    function renderSearchDecisionView(items) {
        if (!items.length) {
            return '<div class="empty-state"><p>No MCP services found.</p></div>';
        }

        var recommendedIndex = items.findIndex(function(item) { return !!item.recommended; });
        if (recommendedIndex < 0) recommendedIndex = 0;
        var recommended = items[recommendedIndex];
        var alternatives = items.filter(function(_, idx) { return idx !== recommendedIndex; });
        var recommendationMeta = [];
        recommendationMeta.push('<span class="badge badge-info">Recommended choice</span>');
        if (recommended.transport) {
            recommendationMeta.push('<span class="badge badge-neutral">' + escapeHtml(recommended.transport) + '</span>');
        }
        if (recommended.env && recommended.env.length) {
            recommendationMeta.push('<span class="badge badge-neutral">' + escapeHtml(recommended.env.length) + ' env</span>');
        }
        if (recommended.popularity_rank) {
            recommendationMeta.push('<span class="badge badge-success">#' + escapeHtml(recommended.popularity_rank) + ' top100</span>');
        }
        if (recommended.setup_effort) {
            recommendationMeta.push('<span class="badge badge-neutral">' + escapeHtml(recommended.setup_effort) + ' setup</span>');
        }
        if (recommended.auth_profile) {
            recommendationMeta.push('<span class="badge badge-neutral">' + escapeHtml(recommended.auth_profile) + '</span>');
        }
        var preflightHtml = (recommended.preflight_checks || []).map(function(step) {
            return '<li>' + escapeHtml(step) + '</li>';
        }).join('');

        var html = '' +
            '<section class="mcp-decision-shell">' +
                '<div class="mcp-decision-header">' +
                    '<div class="mcp-recommendation-label">Best match for "' + escapeHtml(state.activeQuery) + '"</div>' +
                    '<div class="mcp-recommendation-meta">' + recommendationMeta.join(' ') + '</div>' +
                '</div>' +
                '<div class="mcp-decision-lead">' + escapeHtml(recommended.recommended_reason || 'Best overall match for this service and easiest starting point.') + '</div>' +
                '<div class="mcp-decision-card-wrap">' +
                    renderCatalogCard(recommended, recommendedIndex, 'featured') +
                '</div>' +
                (preflightHtml
                    ? '<div class="mcp-preflight">' +
                        '<div class="mcp-preflight-title">Before you start</div>' +
                        '<ul class="mcp-preflight-list">' + preflightHtml + '</ul>' +
                      '</div>'
                    : '') +
                (alternatives.length
                    ? '<div class="mcp-recommendation-actions">' +
                        '<button type="button" class="btn btn-secondary btn-sm mcp-toggle-alternatives-btn" aria-expanded="' + (state.showAlternatives ? 'true' : 'false') + '">' +
                            (state.showAlternatives ? 'Hide alternatives' : ('Show alternatives (' + escapeHtml(alternatives.length) + ')')) +
                        '</button>' +
                      '</div>'
                    : '') +
            '</section>';

        if (alternatives.length && state.showAlternatives) {
            html += '' +
                '<section class="mcp-alternatives">' +
                    '<div class="mcp-alternatives-header">Alternative options</div>' +
                    '<div class="skill-list mcp-skill-list mcp-alternatives-grid">' +
                        alternatives.map(function(item, idx) {
                            var actualIndex = idx >= recommendedIndex ? idx + 1 : idx;
                            return renderCatalogCard(item, actualIndex, 'alternative');
                        }).join('') +
                    '</div>' +
                '</section>';
        }

        return html;
    }

    function renderCatalog(items) {
        state.catalogAll = items || [];
        if (!elCatalog) return;
        elCatalog.className = state.activeQuery
            ? 'mcp-search-results-panel'
            : 'skill-list mcp-skill-list';
        if (!state.catalogAll.length) {
            if (elCatalogCount) elCatalogCount.textContent = '0 results';
            if (elCategoryChips) elCategoryChips.innerHTML = '';
            elCatalog.innerHTML = '<div class="empty-state"><p>No MCP services found.</p></div>';
            return;
        }

        if (state.activeQuery) {
            state.catalog = state.catalogAll.slice();
            if (elCategoryChips) elCategoryChips.innerHTML = '';
            elCatalog.innerHTML = renderSearchDecisionView(state.catalog);
            if (elCatalogCount) {
                var altCount = Math.max(state.catalog.length - 1, 0);
                elCatalogCount.textContent = '1 recommended' + (altCount ? (' · ' + altCount + ' alternatives') : '');
            }
            return;
        }

        var counts = {};
        state.catalogAll.forEach(function(item) {
            item._ui_category = inferCategory(item);
            counts[item._ui_category] = (counts[item._ui_category] || 0) + 1;
        });
        if (state.selectedCategory !== 'All' && !counts[state.selectedCategory]) {
            state.selectedCategory = 'All';
        }
        renderCategoryChips(counts);

        state.catalog = state.catalogAll
            .filter(function(item) {
                return state.selectedCategory === 'All' || item._ui_category === state.selectedCategory;
            })
            .sort(function(a, b) {
                var aRank = a.popularity_rank || 99999;
                var bRank = b.popularity_rank || 99999;
                return aRank - bRank || String(a.display_name || '').localeCompare(String(b.display_name || ''));
            });
        var html = state.catalog.map(function(item, idx) {
            return renderCatalogCard(item, idx);
        }).join('');

        elCatalog.innerHTML = html;
        if (elCatalogCount) {
            var suffix = state.selectedCategory === 'All' ? '' : (' in ' + state.selectedCategory);
            elCatalogCount.textContent = state.catalog.length + ' results' + suffix;
        }
    }

    function renderServers(items) {
        state.servers = items || [];
        if (elServerCount) elServerCount.textContent = state.servers.length + ' configured';
        if (elConfiguredCount) elConfiguredCount.textContent = state.servers.length + ' configured';
        if (elConfiguredSection) {
            elConfiguredSection.style.display = state.servers.length ? '' : 'none';
        }
        if (!elServers) return;
        if (!state.servers.length) {
            elServers.innerHTML = '<div class="empty-state"><p>No MCP servers configured.</p></div>';
            return;
        }

        elServers.innerHTML = state.servers.map(function(server) {
            var env = (server.env || []).map(function(e) {
                return '<span class="badge badge-neutral">' + escapeHtml(e.key) + ': ' + escapeHtml(e.value_preview || '(empty)') + '</span>';
            }).join(' ');
            var capabilityBadges = (server.capabilities || []).map(function(cap) {
                return '<span class="badge badge-neutral">' + escapeHtml(cap) + '</span>';
            }).join(' ');
            var detail = server.transport === 'stdio'
                ? ((server.command || '?') + ' ' + (server.args || []).join(' '))
                : (server.url || '?');
            var statusBadge = server.enabled
                ? '<span class="badge badge-success">Enabled</span>'
                : '<span class="badge badge-warning">Disabled</span>';

            return '' +
                '<div class="skill-card mcp-server-card">' +
                    '<div class="skill-card-header">' +
                        '<div class="skill-name">' + escapeHtml(server.name) + '</div>' +
                        statusBadge +
                    '</div>' +
                    '<div class="skill-desc">' + escapeHtml(detail) + '</div>' +
                    '<div class="mcp-card-tags">' +
                        '<span class="badge badge-neutral">' + escapeHtml(server.transport) + '</span>' +
                        '<span class="badge badge-neutral">' + escapeHtml((server.env || []).length) + ' env</span>' +
                        capabilityBadges +
                    '</div>' +
                    '<div class="mcp-server-env">' + (env || '<span class="form-hint">No env vars.</span>') + '</div>' +
                    '<div class="skill-card-footer">' +
                        '<span class="skill-path">' + escapeHtml(server.enabled ? 'active' : 'disabled') + '</span>' +
                        '<div class="mcp-card-actions">' +
                            '<button type="button" class="btn btn-secondary btn-sm mcp-test-btn" data-name="' + escapeHtml(server.name) + '">Test</button>' +
                            '<button type="button" class="btn btn-secondary btn-sm mcp-toggle-btn" data-name="' + escapeHtml(server.name) + '" data-enabled="' + (server.enabled ? '1' : '0') + '">' + (server.enabled ? 'Disable' : 'Enable') + '</button>' +
                            '<button type="button" class="btn btn-ghost btn-sm mcp-remove-btn" data-name="' + escapeHtml(server.name) + '">Remove</button>' +
                        '</div>' +
                    '</div>' +
                '</div>';
        }).join('');
    }

    async function loadCatalog(query) {
        state.activeQuery = (query || '').trim();
        state.showAlternatives = false;
        var url = '/api/v1/mcp/catalog';
        if (state.activeQuery) {
            url = '/api/v1/mcp/search?q=' + encodeURIComponent(state.activeQuery) + '&limit=24';
        }
        if (elSearchSpinner) elSearchSpinner.style.display = 'block';
        var res;
        try {
            res = await api(url);
        } finally {
            if (elSearchSpinner) elSearchSpinner.style.display = 'none';
        }
        if (!res.ok || !Array.isArray(res.body)) {
            showToast('Failed to load MCP catalog', 'error');
            return;
        }
        renderCatalog(res.body);
    }

    async function loadServers() {
        var servers = await McpLoader.fetchServers({ fresh: true });
        if (!Array.isArray(servers)) {
            showErrorState('mcp-servers-list', 'Could not load MCP servers.', loadServers);
            return;
        }
        clearErrorState('mcp-servers-list');
        renderServers(servers);
    }

    async function refreshAll(query) {
        await Promise.all([loadCatalog(query), loadServers(), loadSandboxStatus()]);
    }

    async function connectPreset(serviceId, forceOverwrite) {
        var preset = state.catalog.find(function(item) { return item.id === serviceId; });
        if (!preset) {
            showToast('Preset not found', 'error');
            return;
        }

        var env = {};
        var envSchema = preset.env || [];
        for (var i = 0; i < envSchema.length; i++) {
            var spec = envSchema[i];
            var hint = spec.secret ? ' (secret, can use vault://...)' : '';
            var value = window.prompt('Value for ' + spec.key + hint + (spec.required ? ' [required]' : ' [optional]'), '');
            if (value === null) return; // user cancelled
            if (!value.trim()) {
                if (spec.required) {
                    showToast('Missing required value: ' + spec.key, 'error');
                    return;
                }
                continue;
            }
            env[spec.key] = value.trim();
        }

        var payload = {
            service: serviceId,
            env: env,
            overwrite: !!forceOverwrite,
        };
        var res = await api('/api/v1/mcp/setup', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(payload),
        });

        if (res.status === 409 && !forceOverwrite) {
            if (window.confirm('A server with this name already exists. Overwrite it?')) {
                await connectPreset(serviceId, true);
            }
            return;
        }

        if (!res.ok || !res.body) {
            showToast('MCP setup failed', 'error');
            return;
        }

        var info = res.body;
        if (info.missing_required_env && info.missing_required_env.length) {
            showToast('Setup saved, missing env: ' + info.missing_required_env.join(', '), 'warning');
        } else if (info.tested && info.connected === false) {
            showToast('Server configured, but test failed', 'warning');
        } else {
            showToast('MCP server configured', 'success');
        }
        await loadServers();
    }

    function prefillManualFromCatalog(item) {
        var baseName = item.package_name || item.id || item.display_name || 'mcp-server';
        var suggested = slugifyServerName(baseName.split('/').pop());
        var transport = item.transport || (item.url ? 'http' : 'stdio');

        var nameInput = document.getElementById('mcp-name');
        var cmdInput = document.getElementById('mcp-command');
        var argsInput = document.getElementById('mcp-args');
        var urlInput = document.getElementById('mcp-url');
        var envInput = document.getElementById('mcp-env');

        if (nameInput && !nameInput.value.trim()) nameInput.value = suggested;
        if (elTransport) elTransport.value = transport === 'http' ? 'http' : 'stdio';
        if (cmdInput) cmdInput.value = transport === 'http' ? '' : (item.command || '');
        if (argsInput) argsInput.value = transport === 'http' ? '' : (item.args || []).join(' ');
        if (urlInput) urlInput.value = transport === 'http' ? (item.url || '') : '';

        if (envInput) {
            var envLines = (item.env || []).map(function(spec) {
                if (spec.secret) {
                    var keyName = 'mcp.' + suggested.replace(/-/g, '.') + '.' + String(spec.key || '').toLowerCase();
                    return spec.key + '=vault://' + keyName;
                }
                return spec.key + '=';
            });
            envInput.value = envLines.join('\n');
        }

        updateTransportUI();
        if (elInstallHint) {
            elInstallHint.textContent =
                'Template loaded for "' + (item.display_name || item.id) +
                '". Complete env values and click Save Server.';
        }
        renderOauthHelper(item);
    }

    function openInstallWizard(item, idx) {
        prefillManualFromCatalog(item);
        openModalForItem(item, idx, true);
        requestInstallAssistant(item, elInstallAssistant);
        showToast('Installer template prepared. Review the guide in the modal.', 'info');
    }

    async function addFromCatalog(item, forceOverwrite) {
        var baseName = item.package_name || item.id || item.display_name || 'mcp-server';
        var suggested = slugifyServerName(baseName.split('/').pop());
        var serverName = window.prompt('Server name for "' + (item.display_name || item.id) + '"', suggested);
        if (serverName === null) return;
        serverName = slugifyServerName(serverName);
        if (!serverName) {
            showToast('Invalid server name', 'error');
            return;
        }

        var env = {};
        var specs = item.env || [];
        for (var i = 0; i < specs.length; i++) {
            var spec = specs[i];
            var hint = spec.secret ? ' (secret, can use vault://...)' : '';
            var value = window.prompt(
                'Value for ' + spec.key + hint + (spec.required ? ' [required]' : ' [optional]'),
                ''
            );
            if (value === null) return;
            if (!value.trim()) {
                if (spec.required) {
                    showToast('Missing required value: ' + spec.key, 'error');
                    return;
                }
                continue;
            }
            env[spec.key] = value.trim();
        }

        var extraEnvText = window.prompt(
            'Optional additional env vars (KEY=VALUE, one per line).',
            ''
        );
        if (extraEnvText === null) return;
        var extra = parseEnvLines(extraEnvText);
        Object.keys(extra).forEach(function(k) { env[k] = extra[k]; });

        var transport = item.transport || (item.url ? 'http' : 'stdio');
        if (transport === 'http' && !item.url) {
            showToast('Missing remote URL for this MCP server', 'error');
            return;
        }
        if (transport !== 'http' && !item.command) {
            showToast('This catalog item requires manual runtime setup', 'warning');
            if (item.docs_url) window.open(item.docs_url, '_blank', 'noopener,noreferrer');
            return;
        }

        var payload = {
            name: serverName,
            transport: transport,
            command: transport === 'http' ? null : (item.command || 'npx'),
            args: transport === 'http' ? [] : (item.args || []),
            url: transport === 'http' ? item.url : null,
            env: env,
            enabled: true,
            overwrite: !!forceOverwrite,
        };

        var res = await api('/api/v1/mcp/servers', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(payload),
        });

        if (res.status === 409 && !forceOverwrite) {
            if (window.confirm('Server already exists. Overwrite it?')) {
                await addFromCatalog(item, true);
            }
            return;
        }

        if (!res.ok) {
            showToast('Failed to add MCP server', 'error');
            return;
        }

        showToast('MCP server added', 'success');
        await loadServers();
        await testServer(serverName);
    }

    async function toggleServer(name, enabled) {
        var res = await api('/api/v1/mcp/servers/' + encodeURIComponent(name) + '/toggle', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ enabled: enabled }),
        });
        if (!res.ok) {
            showToast('Failed to toggle server', 'error');
            return;
        }
        showToast('Server updated', 'success');
        await loadServers();
    }

    async function testServer(name, opts) {
        opts = opts || {};
        var res = await api('/api/v1/mcp/servers/' + encodeURIComponent(name) + '/test', {
            method: 'POST',
        });
        if (!res.ok || !res.body) {
            if (!opts.silent) showToast('MCP test failed', 'error');
            return null;
        }
        if (!opts.silent) {
            showToast(res.body.message || 'Test completed', res.body.connected ? 'success' : 'warning');
        }
        return res.body;
    }

    async function removeServer(name) {
        if (!window.confirm('Remove MCP server "' + name + '"?')) return;
        var res = await api('/api/v1/mcp/servers/' + encodeURIComponent(name), {
            method: 'DELETE',
        });
        if (!res.ok) {
            showToast('Failed to remove server', 'error');
            return;
        }
        showToast('Server removed', 'success');
        await loadServers();
    }

    function bindCatalogActions() {
        if (!elCatalog) return;
        elCatalog.addEventListener('click', function(e) {
            var card = e.target.closest('.mcp-catalog-card');
            if (card && !e.target.closest('button') && !e.target.closest('a')) {
                var cardIdx = Number(card.dataset.index);
                var cardItem = state.catalog[cardIdx];
                if (cardItem) openModalForItem(cardItem, cardIdx);
                return;
            }
            var connectBtn = e.target.closest('.mcp-connect-btn');
            if (connectBtn) {
                var idx = Number(connectBtn.dataset.index);
                var item = state.catalog[idx];
                if (!item) return;
                if (item.kind === 'preset') {
                    if (shouldUseGuidedSetup(item)) {
                        openInstallWizard(item, idx);
                        return;
                    }
                    connectPreset(item.id);
                    return;
                }
                if (!item.install_supported) {
                    if (item.docs_url) {
                        window.open(item.docs_url, '_blank', 'noopener,noreferrer');
                    } else {
                        showToast('This item requires manual setup', 'warning');
                    }
                    return;
                }
                openInstallWizard(item, idx);
                return;
            }
            var alternativesToggleBtn = e.target.closest('.mcp-toggle-alternatives-btn');
            if (alternativesToggleBtn) {
                state.showAlternatives = !state.showAlternatives;
                renderCatalog(state.catalogAll);
                return;
            }
            var quickAddBtn = e.target.closest('.mcp-quickadd-btn');
            if (quickAddBtn) {
                var quickItem = state.catalog[Number(quickAddBtn.dataset.index)];
                if (!quickItem) return;
                addFromCatalog(quickItem);
                return;
            }
        });
    }

    function bindOauthHelper() {
        if (elOauthHelper) {
            elOauthHelper.addEventListener('click', function(e) {
                if (e.target.closest('#mcp-oauth-start-btn')) {
                    startOauthFlow();
                    return;
                }
                if (e.target.closest('#mcp-oauth-retry-btn')) {
                    startOauthFlow();
                    return;
                }
                if (e.target.closest('#mcp-oauth-exchange-btn')) {
                    exchangeOauthCode();
                }
            });
        }

        window.addEventListener('message', function(event) {
            if (event.origin !== window.location.origin) return;
            var data = event.data || {};
            if (data.type !== 'homun-mcp-oauth-code') return;
            var config = oauthConfigForItem(state.oauthItem);
            if (!config) return;
            if (data.provider && data.provider !== config.provider) return;
            if (data.error) {
                setOauthHelperStatus(
                    '<strong>OAuth callback failed.</strong> ' + escapeHtml(data.error_description || data.error),
                    'error'
                );
                return;
            }
            var codeEl = document.getElementById('mcp-oauth-auth-code');
            if (codeEl && data.code) {
                codeEl.value = data.code;
                setOauthHelperStatus(
                    '<strong>Authorization code received.</strong> You can now exchange it for a refresh token.',
                    'success'
                );
            }
        });
    }

    function bindModalActions() {
        if (elModalClose) {
            elModalClose.addEventListener('click', closeModal);
        }
        if (elModalOverlay) {
            elModalOverlay.addEventListener('click', function(e) {
                if (e.target === elModalOverlay) closeModal();
            });
        }
        document.addEventListener('keydown', function(e) {
            if (e.key === 'Escape') closeModal();
        });
        if (!elModalFooter) return;
        elModalFooter.addEventListener('click', function(e) {
            var connectBtn = e.target.closest('.mcp-modal-connect-btn');
            if (connectBtn) {
                var idx = Number(connectBtn.dataset.index);
                var item = state.catalog[idx];
                if (!item) return;
                if (shouldUseGuidedSetup(item)) {
                    openInstallWizard(item, idx);
                    return;
                }
                closeModal();
                connectPreset(item.id);
                return;
            }
            var installBtn = e.target.closest('.mcp-modal-install-btn');
            if (installBtn) {
                var installItem = state.catalog[Number(installBtn.dataset.index)];
                if (!installItem) return;
                openInstallWizard(installItem, Number(installBtn.dataset.index));
                return;
            }
            var quickBtn = e.target.closest('.mcp-modal-quick-btn');
            if (quickBtn) {
                var quickItem = state.catalog[Number(quickBtn.dataset.index)];
                if (!quickItem) return;
                closeModal();
                addFromCatalog(quickItem);
                return;
            }
            var copyBtn = e.target.closest('.mcp-modal-copy-btn');
            if (copyBtn) {
                var copyItem = state.catalog[Number(copyBtn.dataset.index)];
                var id = copyItem ? (copyItem.id || '') : '';
                navigator.clipboard.writeText(id).then(function() {
                    showToast('Server ID copied', 'info');
                }).catch(function() {
                    showToast('Unable to copy server ID', 'warning');
                });
            }
        });
    }

    function bindServerActions() {
        if (!elServers) return;
        elServers.addEventListener('click', function(e) {
            var testBtn = e.target.closest('.mcp-test-btn');
            if (testBtn) {
                testServer(testBtn.dataset.name);
                return;
            }
            var toggleBtn = e.target.closest('.mcp-toggle-btn');
            if (toggleBtn) {
                var name = toggleBtn.dataset.name;
                var enabled = toggleBtn.dataset.enabled !== '1';
                toggleServer(name, enabled);
                return;
            }
            var removeBtn = e.target.closest('.mcp-remove-btn');
            if (removeBtn) {
                removeServer(removeBtn.dataset.name);
            }
        });
    }

    function bindSearchControls() {
        var debouncedCatalogSearch = debounce(function(q) {
            if (elSuggestStatus) {
                elSuggestStatus.textContent = q
                    ? ('Searching MCP sources for: "' + q + '"')
                    : 'Showing full catalog.';
            }
            loadCatalog(q);
        }, 300);

        if (elSuggestInput) {
            elSuggestInput.addEventListener('input', function() {
                var q = elSuggestInput.value.trim();
                if (!q) {
                    debouncedCatalogSearch('');
                    return;
                }
                if (q.length < 2) {
                    return;
                }
                debouncedCatalogSearch(q);
            });
            elSuggestInput.addEventListener('keydown', function(e) {
                if (e.key === 'Enter') {
                    e.preventDefault();
                    var q = elSuggestInput.value.trim();
                    if (!q) {
                        if (elSuggestStatus) elSuggestStatus.textContent = 'Showing full catalog.';
                        loadCatalog('');
                        return;
                    }
                    if (elSuggestStatus) {
                        elSuggestStatus.textContent = 'Searching MCP sources for: "' + q + '"';
                    }
                    loadCatalog(q);
                }
            });
        }
        if (elRefreshSandboxStatusBtn) {
            elRefreshSandboxStatusBtn.addEventListener('click', function() {
                loadSandboxStatus().then(function() {
                    showToast('Sandbox status refreshed', 'success');
                });
            });
        }
        if (elToggleInstallBtn) {
            elToggleInstallBtn.addEventListener('click', function() {
                state.installPanelExpanded = !state.installPanelExpanded;
                syncInstallPanelVisibility();
            });
        }
    }

    function bindCategoryChips() {
        if (!elCategoryChips) return;
        elCategoryChips.addEventListener('click', function(e) {
            var chip = e.target.closest('.mcp-chip');
            if (!chip) return;
            state.selectedCategory = chip.dataset.category || 'All';
            renderCatalog(state.catalogAll);
        });
    }

    function bindManualForm() {
        if (!elManualForm) return;
        if (elTransport) {
            elTransport.addEventListener('change', updateTransportUI);
            updateTransportUI();
        }

        elManualForm.addEventListener('submit', async function(e) {
            e.preventDefault();
            var fd = new FormData(elManualForm);
            var name = String(fd.get('name') || '').trim();
            var transport = String(fd.get('transport') || 'stdio').trim();
            var command = String(fd.get('command') || '').trim();
            var args = parseArgs(String(fd.get('args') || ''));
            var url = String(fd.get('url') || '').trim();
            var env = parseEnvLines(String(fd.get('env') || ''));
            var capabilities = String(fd.get('capabilities') || '')
                .split(',')
                .map(function(item) { return item.trim().toLowerCase(); })
                .filter(Boolean);

            if (!name) {
                showToast('Server name is required', 'error');
                return;
            }
            if (transport === 'stdio' && !command) {
                showToast('Command is required for stdio transport', 'error');
                return;
            }
            if (transport === 'http' && !url) {
                showToast('URL is required for http transport', 'error');
                return;
            }

            var payload = {
                name: name,
                transport: transport,
                command: transport === 'stdio' ? command : null,
                args: transport === 'stdio' ? args : [],
                url: transport === 'http' ? url : null,
                env: env,
                capabilities: capabilities,
                enabled: true,
                overwrite: true,
            };
            var res = await api('/api/v1/mcp/servers', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(payload),
            });
            if (!res.ok) {
                showToast('Failed to save MCP server', 'error');
                return;
            }
            var savedVaultKeys = state.oauthSavedVaultKeys.slice();
            elManualForm.reset();
            if (elTransport) elTransport.value = 'stdio';
            updateTransportUI();
            hideOauthHelper();
            await loadServers();
            var testResult = await testServer(name, { silent: true });
            if (testResult && testResult.connected) {
                showToast(
                    'MCP server saved and test passed' + (savedVaultKeys.length ? (' · Vault: ' + savedVaultKeys.join(', ')) : ''),
                    'success'
                );
            } else if (testResult) {
                showToast(
                    'MCP server saved, but test failed' + (savedVaultKeys.length ? (' · Vault: ' + savedVaultKeys.join(', ')) : ''),
                    'warning'
                );
            } else {
                showToast(
                    'MCP server saved' + (savedVaultKeys.length ? (' · Vault: ' + savedVaultKeys.join(', ')) : ''),
                    'success'
                );
            }
            if (elModalOverlay && elModalOverlay.classList.contains('active')) {
                closeModal();
            }
        });
    }

    bindCatalogActions();
    bindServerActions();
    bindSearchControls();
    bindCategoryChips();
    bindManualForm();
    bindModalActions();
    bindOauthHelper();
    syncInstallPanelVisibility();

    // ── View toggle: Connect Services ↔ Advanced MCP ─────────────
    var elConnView = document.getElementById('connections-view');
    var elAdvView = document.getElementById('mcp-advanced-view');
    var elViewToggle = document.getElementById('conn-view-toggle');

    function switchView(mode) {
        localStorage.setItem('mcp-view-mode', mode);
        if (elConnView) elConnView.style.display = mode === 'connections' ? '' : 'none';
        if (elAdvView) elAdvView.style.display = mode === 'advanced' ? '' : 'none';
        if (elViewToggle) {
            elViewToggle.querySelectorAll('.conn-view-tab').forEach(function(tab) {
                tab.classList.toggle('active', tab.dataset.view === mode);
            });
        }
        // Lazy-load advanced MCP data only when switching to it
        if (mode === 'advanced' && state.servers.length === 0) {
            refreshAll('');
        }
    }

    if (elViewToggle) {
        elViewToggle.addEventListener('click', function(e) {
            var tab = e.target.closest('.conn-view-tab');
            if (!tab) return;
            switchView(tab.dataset.view);
        });
    }

    // Initialize with saved preference or default to "connections"
    var savedView = localStorage.getItem('mcp-view-mode') || 'connections';
    switchView(savedView);

    // Always load advanced data if starting in advanced mode; otherwise lazy
    if (savedView === 'advanced') {
        refreshAll('');
    }
})();
