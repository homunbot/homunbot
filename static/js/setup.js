// Homun — Settings: unified provider accordion, model selection, agent config

// Global error handler for debugging
window.onerror = function(msg, url, line, col, error) {
    console.error('[Global Error]', msg, 'at', url, ':', line, ':', col, error);
    return false;
};

// ═══ Utilities ═══

function providerDisplayName(name) {
    var map = {
        anthropic: 'Anthropic', openai: 'OpenAI', openrouter: 'OpenRouter',
        gemini: 'Gemini', deepseek: 'DeepSeek', groq: 'Groq',
        ollama: 'Ollama (local)', ollama_cloud: 'Ollama Cloud',
        mistral: 'Mistral', xai: 'xAI', together: 'Together',
        fireworks: 'Fireworks', perplexity: 'Perplexity', cohere: 'Cohere',
        venice: 'Venice', aihubmix: 'AiHubMix', vercel: 'Vercel',
        cloudflare: 'Cloudflare', copilot: 'Copilot', bedrock: 'Bedrock',
        moonshot: 'Moonshot', zhipu: 'Zhipu', dashscope: 'DashScope',
        minimax: 'MiniMax', vllm: 'vLLM', custom: 'Custom',
    };
    return map[name] || name;
}

/** Strip provider prefix from model string for display */
function stripPrefix(model) {
    if (!model) return '';
    var idx = model.indexOf('/');
    return idx >= 0 ? model.substring(idx + 1) : model;
}

function modelFromQuery() {
    try {
        return new URLSearchParams(window.location.search).get('model') || '';
    } catch (_) {
        return '';
    }
}

/** Patch a single config key via API */
async function patchConfig(key, value) {
    var resp = await fetch('/api/v1/config', {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ key: key, value: value }),
    });
    if (!resp.ok) throw new Error('Failed to save ' + key);
    return resp.json();
}

function setFieldValidation(inputEl, ok, message) {
    if (!inputEl) return;
    var hint = inputEl.parentElement ? inputEl.parentElement.querySelector('.form-hint') : null;
    if (ok) {
        inputEl.classList.remove('input-invalid');
        if (hint && message) {
            hint.classList.remove('validation-error');
            hint.classList.add('validation-ok');
            hint.textContent = message;
        } else if (hint) {
            hint.classList.remove('validation-error');
            hint.classList.remove('validation-ok');
        }
        return;
    }

    inputEl.classList.add('input-invalid');
    if (hint && message) {
        hint.classList.remove('validation-ok');
        hint.classList.add('validation-error');
        hint.textContent = message;
    }
}

function validateNumberField(inputEl, min, max) {
    if (!inputEl) return true;
    var raw = String(inputEl.value || '').trim();
    if (!raw) {
        setFieldValidation(inputEl, false, 'Required field');
        return false;
    }
    var value = Number(raw);
    if (!Number.isFinite(value)) {
        setFieldValidation(inputEl, false, 'Must be a number');
        return false;
    }
    if (value < min || value > max) {
        setFieldValidation(inputEl, false, 'Expected range: ' + min + '-' + max);
        return false;
    }
    setFieldValidation(inputEl, true);
    return true;
}

function validateUrlField(inputEl, allowEmpty) {
    if (!inputEl) return true;
    var raw = String(inputEl.value || '').trim();
    if (!raw) {
        if (allowEmpty) {
            setFieldValidation(inputEl, true);
            return true;
        }
        setFieldValidation(inputEl, false, 'URL is required');
        return false;
    }
    try {
        var parsed = new URL(raw);
        var ok = parsed.protocol === 'http:' || parsed.protocol === 'https:';
        if (!ok) {
            setFieldValidation(inputEl, false, 'URL must start with http:// or https://');
            return false;
        }
        setFieldValidation(inputEl, true);
        return true;
    } catch (_) {
        setFieldValidation(inputEl, false, 'Invalid URL format');
        return false;
    }
}


// ═══ Active Model Banner ═══

var activeBanner = document.getElementById('active-model-banner');
var noModelBanner = document.getElementById('no-model-banner');
var activeModelName = document.getElementById('active-model-name');
var activeModelProvider = document.getElementById('active-model-provider');

function updateActiveBanner(model) {
    if (!model) {
        if (activeBanner) activeBanner.style.display = 'none';
        if (noModelBanner) noModelBanner.style.display = '';
        return;
    }
    if (activeBanner) activeBanner.style.display = '';
    if (noModelBanner) noModelBanner.style.display = 'none';
    if (activeModelName) activeModelName.textContent = stripPrefix(model);

    // Infer provider from prefix
    var prefix = model.indexOf('/') >= 0 ? model.substring(0, model.indexOf('/')) : '';
    if (activeModelProvider) activeModelProvider.textContent = 'via ' + providerDisplayName(prefix);

    // Update active badges and card styles
    document.querySelectorAll('.provider-card').forEach(function(card) {
        var prov = card.dataset.provider;
        var badge = card.querySelector('.provider-active-badge');
        var status = card.querySelector('.provider-card-status');
        var isActive = model.startsWith(prov + '/');

        if (isActive) {
            card.classList.add('provider-card--active');
            if (!badge) {
                badge = document.createElement('span');
                badge.className = 'provider-active-badge';
                badge.textContent = 'Active';
                var right = card.querySelector('.provider-card-right');
                if (right) right.insertBefore(badge, right.firstChild);
            }
            if (status) { status.className = 'provider-card-status active'; }
        } else {
            card.classList.remove('provider-card--active');
            if (badge) badge.remove();
            if (status) {
                status.className = 'provider-card-status' +
                    (card.dataset.configured === 'true' ? ' configured' : '');
            }
        }
    });

    // Update radio buttons
    document.querySelectorAll('input[name="active-model"]').forEach(function(radio) {
        var badge = radio.closest('.model-radio').querySelector('.model-radio-badge');
        if (radio.value === model) {
            radio.checked = true;
            if (badge) { badge.textContent = 'Active'; badge.className = 'model-radio-badge active'; }
        } else {
            radio.checked = false;
            if (badge) { badge.textContent = ''; badge.className = 'model-radio-badge'; }
        }
    });

    if (typeof refreshSetupWizard === 'function') refreshSetupWizard();
}

var lastProviderTestOk = false;

async function runProviderConnectionTest(card, opts) {
    opts = opts || {};
    if (!card) return null;

    var provider = card.dataset.provider;
    var resultEl = card.querySelector('.provider-test-result');
    var btn = card.querySelector('.provider-test-connection');
    var customInput = card.querySelector('.provider-custom-model');
    var selected = card.querySelector('input[name="active-model"]:checked');
    var model = selected ? selected.value : '';
    if (!model && customInput && customInput.value.trim()) {
        model = customInput.value.trim();
    }
    if (model && !model.startsWith(provider + '/')) {
        model = provider + '/' + model;
    }

    var payload = {
        name: provider,
        model: model || undefined,
    };
    var keyInput = card.querySelector('.provider-api-key');
    if (keyInput && keyInput.value.trim()) {
        payload.api_key = keyInput.value.trim();
    }
    var baseInput = card.querySelector('.provider-api-base');
    if (baseInput && baseInput.value.trim()) {
        payload.api_base = baseInput.value.trim();
    }

    if (btn) {
        btn.disabled = true;
        btn.textContent = 'Testing…';
    }
    if (resultEl) {
        resultEl.textContent = 'Testing connection…';
        resultEl.className = 'form-hint provider-test-result';
    }

    try {
        var resp = await fetch('/api/v1/providers/test', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(payload),
        });
        var data = await resp.json();
        lastProviderTestOk = !!data.ok;
        if (resultEl) {
            resultEl.textContent = (data.ok ? '✓ ' : '✗ ') + (data.message || '');
            resultEl.className = 'form-hint provider-test-result ' + (data.ok ? 'pairing-status success' : 'pairing-status error');
        }
        if (!opts.silent) {
            showToast(data.ok ? 'Provider connection OK' : 'Provider test failed', data.ok ? 'success' : 'error');
        }
        return data;
    } catch (err) {
        lastProviderTestOk = false;
        if (resultEl) {
            resultEl.textContent = '✗ Connection test failed';
            resultEl.className = 'form-hint provider-test-result pairing-status error';
        }
        if (!opts.silent) showToast('Provider test failed', 'error');
        return null;
    } finally {
        if (btn) {
            btn.disabled = false;
            btn.textContent = 'Test Connection';
        }
    }
}

function getActiveProviderCard() {
    var activeCard = document.querySelector('.provider-card.provider-card--active');
    if (activeCard) return activeCard;

    var checkedModel = document.querySelector('input[name="active-model"]:checked');
    var model = checkedModel ? checkedModel.value : '';
    if (!model) return null;
    var slash = model.indexOf('/');
    if (slash <= 0) return null;
    var provider = model.substring(0, slash);
    return document.querySelector('.provider-card[data-provider="' + provider + '"]');
}


// ═══ Provider Cards Grid ═══

var providerGrid = document.getElementById('provider-grid');

if (providerGrid) {
    // --- Expand/collapse card ---
    providerGrid.addEventListener('click', function(e) {
        var header = e.target.closest('.provider-card-header');
        if (!header) return;
        var card = header.closest('.provider-card');
        var body = card.querySelector('.provider-card-body');
        if (!body) return;

        var isExpanded = !body.hidden;
        body.hidden = isExpanded;
        header.setAttribute('aria-expanded', String(!isExpanded));
        var chevron = header.querySelector('.provider-chevron');
        if (chevron) chevron.classList.toggle('expanded', !isExpanded);

        // Load models when expanding for the first time
        if (!isExpanded && !card.dataset.modelsLoaded) {
            loadProviderModels(card);
        }
    });

    // --- Save API key ---
    providerGrid.addEventListener('click', function(e) {
        var btn = e.target.closest('.provider-save-key');
        if (!btn) return;
        var card = btn.closest('.provider-card');
        var provider = card.dataset.provider;
        var keyInput = card.querySelector('.provider-api-key');
        var apiKey = keyInput ? keyInput.value.trim() : '';
        if (!apiKey) { showToast('Enter an API key', 'error'); return; }

        var payload = { name: provider, api_key: apiKey };
        btn.textContent = 'Saving\u2026';
        btn.disabled = true;

        fetch('/api/v1/providers/configure', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(payload),
        }).then(function(r) { return r.json(); }).then(function(data) {
            if (data.ok) {
                card.dataset.configured = 'true';
                var status = card.querySelector('.provider-card-status');
                if (status && !status.classList.contains('active')) status.className = 'provider-card-status configured';
                keyInput.value = '';
                keyInput.placeholder = 'Configured \u2014 enter new key to replace';
                showToast('API key saved!', 'success');
                _allModelsCache = null;
                loadProviderModels(card);
                loadVisionDropdown();
                populateFallbackDropdown();
                refreshSetupWizard();
            } else {
                showToast(data.message || 'Failed to save', 'error');
            }
            btn.textContent = 'Save Key';
            btn.disabled = false;
        }).catch(function() {
            showToast('Failed to save API key', 'error');
            btn.textContent = 'Save Key';
            btn.disabled = false;
        });
    });

    // --- Save Base URL ---
    providerGrid.addEventListener('click', function(e) {
        var btn = e.target.closest('.provider-save-url');
        if (!btn) return;
        var card = btn.closest('.provider-card');
        var provider = card.dataset.provider;
        var urlInput = card.querySelector('.provider-api-base');
        var apiBase = urlInput ? urlInput.value.trim() : '';
        if (!apiBase) { showToast('Enter a base URL', 'error'); return; }
        if (!validateUrlField(urlInput, false)) { return; }

        var payload = { name: provider, api_base: apiBase };
        btn.textContent = 'Saving\u2026';
        btn.disabled = true;

        fetch('/api/v1/providers/configure', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(payload),
        }).then(function(r) { return r.json(); }).then(function(data) {
            if (data.ok) {
                card.dataset.configured = 'true';
                var status = card.querySelector('.provider-card-status');
                if (status && !status.classList.contains('active')) status.className = 'provider-card-status configured';
                showToast('URL saved!', 'success');
                _allModelsCache = null;
                loadProviderModels(card);
                loadVisionDropdown();
                populateFallbackDropdown();
                refreshSetupWizard();
            } else {
                showToast(data.message || 'Failed to save', 'error');
            }
            btn.textContent = 'Save URL';
            btn.disabled = false;
        }).catch(function() {
            showToast('Failed to save URL', 'error');
            btn.textContent = 'Save URL';
            btn.disabled = false;
        });
    });

    // --- Model radio selection ---
    providerGrid.addEventListener('change', function(e) {
        if (e.target.name !== 'active-model') return;
        var model = e.target.value;
        patchConfig('agent.model', model).then(function() {
            _allModelsCache = null;
            updateActiveBanner(model);
            showToast('Model changed to ' + stripPrefix(model), 'success');
        }).catch(function() {
            showToast('Failed to change model', 'error');
        });
    });

    // --- Custom model ---
    providerGrid.addEventListener('click', function(e) {
        var btn = e.target.closest('.provider-use-custom');
        if (!btn) return;
        var card = btn.closest('.provider-card');
        var provider = card.dataset.provider;
        var input = card.querySelector('.provider-custom-model');
        var modelName = input ? input.value.trim() : '';
        if (!modelName) return;
        if (/\s/.test(modelName)) {
            input.classList.add('input-invalid');
            showToast('Model name cannot contain spaces', 'error');
            return;
        }
        input.classList.remove('input-invalid');

        if (!modelName.startsWith(provider + '/')) {
            modelName = provider + '/' + modelName;
        }

        patchConfig('agent.model', modelName).then(function() {
            _allModelsCache = null;
            updateActiveBanner(modelName);
            input.value = '';
            showToast('Model set to ' + stripPrefix(modelName), 'success');
            addModelRadio(card, modelName, true);
        }).catch(function() {
            showToast('Failed to set model', 'error');
        });
    });

    // --- Provider connection test ---
    providerGrid.addEventListener('click', function(e) {
        var btn = e.target.closest('.provider-test-connection');
        if (!btn) return;
        var card = btn.closest('.provider-card');
        runProviderConnectionTest(card).then(function() {
            refreshSetupWizard();
        });
    });

    // --- Realtime provider field validation ---
    providerGrid.addEventListener('input', function(e) {
        if (e.target.classList.contains('provider-api-base')) {
            validateUrlField(e.target, true);
            return;
        }
        if (e.target.classList.contains('provider-custom-model')) {
            var val = e.target.value.trim();
            if (!val || !/\s/.test(val)) {
                e.target.classList.remove('input-invalid');
            } else {
                e.target.classList.add('input-invalid');
            }
        }
    });

    // --- Deactivate provider ---
    providerGrid.addEventListener('click', function(e) {
        var btn = e.target.closest('.provider-deactivate');
        if (!btn) return;
        var card = btn.closest('.provider-card');
        var provider = card.dataset.provider;
        var displayName = card.querySelector('.provider-card-name').textContent;

        if (!confirm('Remove credentials for ' + displayName + '?')) return;

        fetch('/api/v1/providers/deactivate', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ name: provider }),
        }).then(function(r) { return r.json(); }).then(function(data) {
            if (data.ok) {
                showToast('Provider removed', 'success');
                window.location.reload();
            } else {
                showToast(data.message || 'Failed to remove', 'error');
            }
        }).catch(function() {
            showToast('Failed to remove provider', 'error');
        });
    });

    // --- Load models for expanded cards on init ---
    providerGrid.querySelectorAll('.provider-card-body:not([hidden])').forEach(function(body) {
        var card = body.closest('.provider-card');
        if (card) loadProviderModels(card);
    });

    providerGrid.querySelectorAll('.provider-api-base').forEach(function(input) {
        if (input.value && input.value.trim()) validateUrlField(input, true);
    });
}

// ═══ Provider Catalog Modal ═══

(function() {
    var catalogModal = document.getElementById('provider-catalog-modal');
    var addBtn = document.getElementById('btn-add-provider');
    var searchInput = document.getElementById('catalog-search');

    if (!catalogModal || !addBtn) return;

    function openCatalog() { catalogModal.classList.add('open'); if (searchInput) searchInput.focus(); }
    function closeCatalog() { catalogModal.classList.remove('open'); }

    addBtn.addEventListener('click', openCatalog);
    catalogModal.querySelector('.modal-backdrop').addEventListener('click', closeCatalog);
    catalogModal.querySelector('.catalog-modal-close').addEventListener('click', closeCatalog);

    // Search filter
    if (searchInput) {
        searchInput.addEventListener('input', function() {
            var q = searchInput.value.toLowerCase();
            catalogModal.querySelectorAll('.catalog-card').forEach(function(card) {
                var name = card.querySelector('.catalog-card-name').textContent.toLowerCase();
                var desc = card.querySelector('.catalog-card-desc').textContent.toLowerCase();
                card.classList.toggle('hidden', q && name.indexOf(q) === -1 && desc.indexOf(q) === -1);
            });
        });
    }

    // Configure button — add provider card to the grid
    catalogModal.addEventListener('click', function(e) {
        var btn = e.target.closest('.catalog-configure-btn');
        if (!btn) return;
        var catalogCard = btn.closest('.catalog-card');
        var provider = catalogCard.dataset.provider;
        var hasKey = catalogCard.dataset.hasKey === 'true';
        var hasUrl = catalogCard.dataset.hasUrl === 'true';
        var displayName = catalogCard.querySelector('.catalog-card-name').textContent;
        var desc = catalogCard.querySelector('.catalog-card-desc').textContent;

        // Build credential fields
        var keyField = hasKey ?
            '<div class="form-group"><label>API Key</label><div class="credential-row"><input type="password" class="input provider-api-key" placeholder="Enter API key..."><button type="button" class="btn btn-secondary btn--sm provider-save-key">Save Key</button></div><div class="form-hint">Stored encrypted locally.</div></div>' : '';
        var urlField = hasUrl ?
            '<div class="form-group"><label>Base URL</label><div class="credential-row"><input type="text" class="input provider-api-base" placeholder="http://localhost:11434/v1"><button type="button" class="btn btn-secondary btn--sm provider-save-url">Save URL</button></div><div class="form-hint">API endpoint URL.</div></div>' : '';

        // Create the provider card HTML
        var cardHtml = '<div class="provider-card" data-provider="' + provider + '" data-configured="false" data-has-key="' + hasKey + '" data-has-url="' + hasUrl + '" data-active-model="">' +
            '<div class="provider-card-header" role="button" tabindex="0" aria-expanded="true">' +
                '<div class="provider-card-left"><span class="provider-card-status">&bull;</span><div class="provider-card-info"><span class="provider-card-name">' + displayName + '</span><span class="provider-card-desc">' + desc + '</span></div></div>' +
                '<div class="provider-card-right"><span class="provider-chevron expanded">&#9662;</span></div>' +
            '</div>' +
            '<div class="provider-card-body">' +
                '<div class="provider-credentials">' + keyField + urlField + '</div>' +
                '<div class="provider-card-tools"><button type="button" class="btn btn-secondary btn--sm provider-test-connection">Test Connection</button><span class="form-hint provider-test-result"></span></div>' +
                '<div class="provider-models" data-provider="' + provider + '"><label class="provider-models-label">Models</label><div class="provider-model-list"><div class="form-hint">Configure credentials to see models.</div></div><div class="custom-model-row"><input type="text" class="input input--inline provider-custom-model" placeholder="Custom model name\u2026"><button type="button" class="btn btn-secondary btn--sm provider-use-custom">Use</button></div><div class="form-hint">Enter a model name. Provider prefix is added automatically.</div></div>' +
                '<button type="button" class="btn btn-ghost btn--sm provider-deactivate" style="margin-top:8px;color:var(--text-muted);">Remove credentials</button>' +
            '</div></div>';

        // Add to grid and remove from catalog
        var grid = document.getElementById('provider-grid');
        if (grid) grid.insertAdjacentHTML('beforeend', cardHtml);
        catalogCard.remove();
        closeCatalog();

        // Focus the new card's first input
        var newCard = grid.querySelector('.provider-card[data-provider="' + provider + '"]');
        if (newCard) {
            var firstInput = newCard.querySelector('.provider-api-key, .provider-api-base');
            if (firstInput) firstInput.focus();
        }
        refreshSetupWizard();
    });
})();


// ═══ Setup Wizard ═══

var wizardEls = {
    section: document.getElementById('setup-wizard-section'),
    root: document.getElementById('setup-wizard'),
    status: document.getElementById('wizard-status'),
    nextBtn: document.getElementById('wizard-next-step'),
    testBtn: document.getElementById('wizard-test-active-provider'),
    hideBtn: document.getElementById('wizard-hide'),
    stepProvider: document.getElementById('wizard-step-provider'),
    stepModel: document.getElementById('wizard-step-model'),
    stepTest: document.getElementById('wizard-step-test'),
    stepChat: document.getElementById('wizard-step-chat'),
};

// ─── Wizard Checkpoint (survives reload) ───
var WIZARD_CHECKPOINT_KEY = 'homun-wizard-checkpoint';

function saveWizardCheckpoint(step) {
    try { localStorage.setItem(WIZARD_CHECKPOINT_KEY, JSON.stringify({ step: step, ts: Date.now() })); } catch(_) {}
}

function loadWizardCheckpoint() {
    try {
        var raw = localStorage.getItem(WIZARD_CHECKPOINT_KEY);
        if (!raw) return null;
        var data = JSON.parse(raw);
        // Expire after 24h
        if (Date.now() - data.ts > 86400000) {
            localStorage.removeItem(WIZARD_CHECKPOINT_KEY);
            return null;
        }
        return data.step;
    } catch(_) { return null; }
}

function clearWizardCheckpoint() {
    try { localStorage.removeItem(WIZARD_CHECKPOINT_KEY); } catch(_) {}
}

function setWizardStepState(stepEl, state) {
    if (!stepEl) return;
    stepEl.classList.remove('is-active');
    stepEl.classList.remove('is-done');
    if (state) stepEl.classList.add(state);
}

function getConfiguredProviders() {
    return Array.from(document.querySelectorAll('.provider-card')).filter(function(card) {
        return card.dataset.configured === 'true';
    });
}

function hasActiveModelConfigured() {
    if (!noModelBanner) return false;
    return window.getComputedStyle(noModelBanner).display === 'none';
}

function refreshSetupWizard() {
    if (!wizardEls.root) return;

    var providers = getConfiguredProviders();
    var hasProvider = providers.length > 0;
    var hasModel = hasActiveModelConfigured();
    var hasTest = !!lastProviderTestOk;
    // Restore test checkpoint from localStorage (survives reload)
    if (!hasTest && loadWizardCheckpoint() === 'chat') hasTest = true;
    var hasChat = loadWizardCheckpoint() === 'done';

    // Step 1: Provider
    if (hasProvider) setWizardStepState(wizardEls.stepProvider, 'is-done');
    else setWizardStepState(wizardEls.stepProvider, 'is-active');

    // Step 2: Model
    if (!hasProvider) setWizardStepState(wizardEls.stepModel, null);
    else if (hasModel) setWizardStepState(wizardEls.stepModel, 'is-done');
    else setWizardStepState(wizardEls.stepModel, 'is-active');

    // Step 3: Test
    if (!hasProvider || !hasModel) setWizardStepState(wizardEls.stepTest, null);
    else if (hasTest) setWizardStepState(wizardEls.stepTest, 'is-done');
    else setWizardStepState(wizardEls.stepTest, 'is-active');

    // Step 4: First message
    if (!hasTest) setWizardStepState(wizardEls.stepChat, null);
    else if (hasChat) setWizardStepState(wizardEls.stepChat, 'is-done');
    else setWizardStepState(wizardEls.stepChat, 'is-active');

    // Checkpoint: save highest completed step
    if (hasProvider && hasModel && hasTest && !hasChat) saveWizardCheckpoint('chat');
    if (hasChat) saveWizardCheckpoint('done');

    // Status text
    if (wizardEls.status) {
        wizardEls.status.classList.remove('validation-ok');
        if (!hasProvider) {
            wizardEls.status.textContent = 'Next: configure at least one provider.';
        } else if (!hasModel) {
            wizardEls.status.textContent = 'Next: choose an active model.';
        } else if (!hasTest) {
            wizardEls.status.textContent = 'Next: run provider connection test.';
        } else if (!hasChat) {
            wizardEls.status.textContent = 'Next: send your first message to verify the full pipeline.';
        } else {
            wizardEls.status.textContent = 'Setup complete! Homun is ready.';
            wizardEls.status.classList.add('validation-ok');
        }
    }

    // Button state
    if (wizardEls.nextBtn) {
        if (hasChat) {
            wizardEls.nextBtn.textContent = 'Setup complete';
            wizardEls.nextBtn.disabled = true;
        } else if (hasTest) {
            wizardEls.nextBtn.textContent = 'Open chat';
        } else {
            wizardEls.nextBtn.textContent = 'Next step';
            wizardEls.nextBtn.disabled = false;
        }
    }
}

async function runWizardNextStep() {
    var providers = getConfiguredProviders();
    var hasProvider = providers.length > 0;
    var hasModel = hasActiveModelConfigured();
    var hasTest = !!lastProviderTestOk || loadWizardCheckpoint() === 'chat';

    if (!hasProvider) {
        if (providerGrid) providerGrid.scrollIntoView({ behavior: 'smooth', block: 'start' });
        var addBtn = document.getElementById('btn-add-provider');
        if (addBtn) addBtn.click();
        return;
    }

    if (!hasModel) {
        var firstConfigured = providers[0];
        if (!firstConfigured) return;
        var body = firstConfigured.querySelector('.provider-card-body');
        var header = firstConfigured.querySelector('.provider-card-header');
        if (body && body.hidden && header) header.click();
        firstConfigured.scrollIntoView({ behavior: 'smooth', block: 'center' });
        var radio = firstConfigured.querySelector('input[name="active-model"]');
        var customInput = firstConfigured.querySelector('.provider-custom-model');
        if (radio) radio.focus();
        else if (customInput) customInput.focus();
        return;
    }

    if (!hasTest) {
        var activeCard = getActiveProviderCard() || providers[0];
        if (activeCard) {
            var body = activeCard.querySelector('.provider-card-body');
            var header = activeCard.querySelector('.provider-card-header');
            if (body && body.hidden && header) header.click();
            activeCard.scrollIntoView({ behavior: 'smooth', block: 'center' });
            await runProviderConnectionTest(activeCard);
            refreshSetupWizard();
        }
        return;
    }

    // Step 4: Open chat page for first message
    saveWizardCheckpoint('chat');
    window.location.href = '/chat';
}

(function initSetupWizard() {
    if (!wizardEls.root) return;

    var hidden = localStorage.getItem('homun-setup-wizard-hidden') === '1';
    if (hidden && wizardEls.section) wizardEls.section.style.display = 'none';

    if (wizardEls.nextBtn) {
        wizardEls.nextBtn.addEventListener('click', runWizardNextStep);
    }
    if (wizardEls.testBtn) {
        wizardEls.testBtn.addEventListener('click', async function() {
            var active = getActiveProviderCard();
            if (!active) {
                showToast('No active provider selected', 'error');
                return;
            }
            await runProviderConnectionTest(active);
            refreshSetupWizard();
        });
    }
    if (wizardEls.hideBtn) {
        wizardEls.hideBtn.addEventListener('click', function() {
            localStorage.setItem('homun-setup-wizard-hidden', '1');
            if (wizardEls.section) wizardEls.section.style.display = 'none';
        });
    }

    refreshSetupWizard();
    detectOllamaLocal();
})();

// ─── Ollama Local Detection ───

async function detectOllamaLocal() {
    var banner = document.getElementById('ollama-local-banner');
    if (!banner) return;

    // Only show if no provider configured yet
    var providers = getConfiguredProviders();
    if (providers.length > 0) return;

    try {
        var resp = await fetch('/api/v1/providers/ollama/models');
        var data = await resp.json();
        if (!data.ok || !data.models) return;

        banner.style.display = 'block';
        var select = document.getElementById('ollama-model-select');
        if (!select) return;

        while (select.firstChild) select.removeChild(select.firstChild);

        if (data.models.length === 0) {
            var defaultModels = ['llama3.2:3b', 'gemma3:4b', 'phi4-mini:3.8b', 'qwen3:4b'];
            defaultModels.forEach(function(m) {
                var opt = document.createElement('option');
                opt.value = m;
                opt.textContent = m + ' (will download)';
                opt.dataset.needsPull = 'true';
                select.appendChild(opt);
            });
        } else {
            data.models.forEach(function(m) {
                var opt = document.createElement('option');
                opt.value = m.name;
                opt.textContent = m.name + ' (' + m.size + ')';
                select.appendChild(opt);
            });
        }
    } catch (_) {
        // Ollama not running
    }
}

document.addEventListener('click', async function(e) {
    if (e.target.id !== 'ollama-quick-setup') return;
    var select = document.getElementById('ollama-model-select');
    var status = document.getElementById('ollama-banner-status');
    var btn = e.target;
    if (!select || !select.value) return;

    var model = select.value;
    btn.disabled = true;
    btn.textContent = 'Setting up\u2026';

    var needsPull = select.selectedOptions[0] && select.selectedOptions[0].dataset.needsPull === 'true';
    if (needsPull) {
        if (status) status.textContent = 'Downloading ' + model + '\u2026 this may take a few minutes.';
        try {
            var pullResp = await fetch('/api/v1/providers/ollama/pull', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ model: model }),
            });
            var pullData = await pullResp.json();
            if (!pullData.ok) {
                if (status) status.textContent = 'Pull failed: ' + (pullData.message || 'unknown error');
                btn.disabled = false;
                btn.textContent = 'Use this model';
                return;
            }
        } catch (err) {
            if (status) status.textContent = 'Pull failed: ' + err.message;
            btn.disabled = false;
            btn.textContent = 'Use this model';
            return;
        }
    }

    if (status) status.textContent = 'Configuring Ollama as active provider\u2026';
    try {
        var activateResp = await fetch('/api/v1/providers/activate', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ name: 'ollama', model: 'ollama/' + model }),
        });
        var activateData = await activateResp.json();
        if (activateData.ok) {
            if (status) status.textContent = 'Ollama configured! Model: ' + model;
            status.classList.add('validation-ok');
            btn.textContent = 'Done';
            setTimeout(function() { window.location.reload(); }, 1000);
        } else {
            if (status) status.textContent = 'Activation failed: ' + (activateData.error || 'unknown');
            btn.disabled = false;
            btn.textContent = 'Use this model';
        }
    } catch (err) {
        if (status) status.textContent = 'Error: ' + err.message;
        btn.disabled = false;
        btn.textContent = 'Use this model';
    }
});

// ═══ Load Models for a Provider Accordion Item ═══

var _allModelsCache = null;

async function hydrateModelCapabilities(data) {
    if (!data || !Array.isArray(data.models)) return data;

    var existing = data.model_capabilities || {};
    var merged = {};
    Object.keys(existing).forEach(function(modelId) {
        merged[modelId] = existing[modelId];
    });

    var modelIds = data.models.map(function(m) { return m.model; });
    if (data.current) modelIds.push(data.current);
    if (data.vision_model) modelIds.push(data.vision_model);
    if (data.model_overrides) {
        Object.keys(data.model_overrides).forEach(function(modelId) {
            modelIds.push(modelId);
        });
    }
    if (data.hidden_models) {
        Object.keys(data.hidden_models).forEach(function(provider) {
            data.hidden_models[provider].forEach(function(modelId) {
                modelIds.push(modelId);
            });
        });
    }

    var missing = Array.from(new Set(modelIds)).filter(function(modelId) {
        return modelId && !merged[modelId];
    });

    if (missing.length > 0) {
        try {
            var resp = await fetch('/api/v1/providers/model-capabilities', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ models: missing }),
            });
            var payload = await resp.json();
            if (payload && payload.ok && payload.model_capabilities) {
                Object.keys(payload.model_capabilities).forEach(function(modelId) {
                    merged[modelId] = payload.model_capabilities[modelId];
                });
            }
        } catch (_) {}
    }

    data.model_capabilities = merged;
    return data;
}

async function fetchAllModels() {
    if (_allModelsCache) return _allModelsCache;
    try {
        var resp = await fetch('/api/v1/providers/models');
        var data = await resp.json();

        // Fetch live Ollama models and merge into data.models
        if (data.ollama_configured) {
            try {
                var ollamaResp = await fetch('/api/v1/providers/ollama/models');
                var ollamaData = await ollamaResp.json();
                if (ollamaData.ok && ollamaData.models) {
                    ollamaData.models.forEach(function(m) {
                        data.models.push({
                            provider: 'ollama',
                            model: 'ollama/' + m.name,
                            label: m.name + ' (' + m.size + ')',
                        });
                    });
                }
            } catch (_) {}
        }

        // Fetch live Ollama Cloud models and merge into data.models
        if (data.ollama_cloud_configured) {
            try {
                var cloudResp = await fetch('/api/v1/providers/ollama-cloud/models');
                var cloudData = await cloudResp.json();
                if (cloudData.ok && cloudData.models) {
                    cloudData.models.forEach(function(m) {
                        data.models.push({
                            provider: 'ollama_cloud',
                            model: 'ollama_cloud/' + m.id,
                            label: m.id,
                        });
                    });
                }
            } catch (_) {}
        }

        await hydrateModelCapabilities(data);
        _allModelsCache = data;
        return data;
    } catch (err) {
        return { ok: false, models: [], current: '', vision_model: '' };
    }
}

/**
 * Return a copy of data with hidden models merged back into the models array.
 * Hidden models are excluded from provider card radio buttons but should still
 * be available in vision/fallback dropdowns.
 */
function allModelsIncludingHidden(data) {
    if (!data.hidden_models) return data;
    var extra = [];
    Object.keys(data.hidden_models).forEach(function(prov) {
        data.hidden_models[prov].forEach(function(modelId) {
            var exists = data.models.some(function(m) { return m.model === modelId; });
            if (!exists) {
                var label = modelId.replace(prov + '/', '');
                extra.push({ provider: prov, model: modelId, label: label });
            }
        });
    });
    if (extra.length > 0) {
        var merged = {};
        for (var k in data) { if (data.hasOwnProperty(k)) merged[k] = data[k]; }
        merged.models = data.models.concat(extra);
        return merged;
    }
    return data;
}

/**
 * Reusable model selector — populates a <select> with models grouped by provider.
 *
 * @param {HTMLSelectElement} selectEl   Target <select>
 * @param {Object} data                 Result of fetchAllModels()
 * @param {Object} opts                 Configuration
 * @param {string}  opts.placeholder    Text for the first empty option
 * @param {Array}   opts.specialOptions [{value, label}] inserted before groups
 * @param {boolean} opts.includeCustom  Append "Custom model…" option
 * @param {string}  opts.selectedValue  Pre-select this value
 */
function buildModelOptions(selectEl, data, opts) {
    if (!selectEl) return;
    opts = opts || {};
    while (selectEl.firstChild) selectEl.removeChild(selectEl.firstChild);

    // Placeholder
    if (opts.placeholder) {
        var ph = document.createElement('option');
        ph.value = '';
        ph.textContent = opts.placeholder;
        selectEl.appendChild(ph);
    }

    // Special options (e.g., "(Same as chat model)")
    if (opts.specialOptions) {
        opts.specialOptions.forEach(function(so) {
            var opt = document.createElement('option');
            opt.value = so.value;
            opt.textContent = so.label;
            if (opts.selectedValue != null && so.value === opts.selectedValue) opt.selected = true;
            selectEl.appendChild(opt);
        });
    }

    // Group models by provider
    var groups = {};
    if (data.ok && data.models) {
        data.models.forEach(function(m) {
            if (!groups[m.provider]) groups[m.provider] = [];
            groups[m.provider].push(m);
        });
    }

    // Build optgroups
    var found = false;
    Object.keys(groups).forEach(function(prov) {
        var optgroup = document.createElement('optgroup');
        optgroup.label = providerDisplayName(prov);
        groups[prov].forEach(function(m) {
            var opt = document.createElement('option');
            opt.value = m.model;
            opt.textContent = m.label;
            if (opts.selectedValue && m.model === opts.selectedValue) {
                opt.selected = true;
                found = true;
            }
            optgroup.appendChild(opt);
        });
        selectEl.appendChild(optgroup);
    });

    // Custom model option
    if (opts.includeCustom) {
        var customGroup = document.createElement('optgroup');
        customGroup.label = 'Custom';
        var customOpt = document.createElement('option');
        customOpt.value = '__custom__';
        customOpt.textContent = '\u270f Custom model\u2026';
        customGroup.appendChild(customOpt);
        selectEl.appendChild(customGroup);
    }

    // Orphaned selected value — insert as "(current)" after special options
    if (opts.selectedValue && !found && opts.selectedValue !== '') {
        var cur = document.createElement('option');
        cur.value = opts.selectedValue;
        cur.textContent = opts.selectedValue + ' (current)';
        cur.selected = true;
        // Insert after placeholder + special options
        var insertPoint = selectEl.firstChild;
        if (insertPoint) insertPoint = insertPoint.nextSibling;
        if (opts.specialOptions) {
            for (var i = 0; i < opts.specialOptions.length && insertPoint; i++) {
                insertPoint = insertPoint.nextSibling;
            }
        }
        if (insertPoint) {
            selectEl.insertBefore(cur, insertPoint);
        } else {
            selectEl.appendChild(cur);
        }
    }
}

async function loadProviderModels(item) {
    var provider = item.dataset.provider;
    var modelList = item.querySelector('.provider-model-list');
    if (!modelList) return;

    item.dataset.modelsLoaded = 'true';

    var data = await fetchAllModels();
    var currentModel = data.current || '';
    var models = [];

    // Get static models for this provider
    if (data.ok && data.models) {
        data.models.forEach(function(m) {
            if (m.provider === provider) {
                models.push({ value: m.model, label: m.label });
            }
        });
    }

    // Fetch live models for Ollama/Ollama Cloud
    if (provider === 'ollama' && data.ollama_configured) {
        try {
            var ollamaResp = await fetch('/api/v1/providers/ollama/models');
            var ollamaData = await ollamaResp.json();
            if (ollamaData.ok && ollamaData.models && ollamaData.models.length > 0) {
                models = ollamaData.models.map(function(m) {
                    return { value: 'ollama/' + m.name, label: m.name + ' (' + m.size + ')' };
                });
            }
        } catch (_) {}
    }
    if (provider === 'ollama_cloud' && data.ollama_cloud_configured) {
        try {
            var cloudResp = await fetch('/api/v1/providers/ollama-cloud/models');
            var cloudData = await cloudResp.json();
            if (cloudData.ok && cloudData.models && cloudData.models.length > 0) {
                models = cloudData.models.map(function(m) {
                    return { value: 'ollama_cloud/' + m.id, label: m.id };
                });
            }
        } catch (_) {}
    }

    // If current model belongs to this provider but isn't in the list, add it
    if (currentModel.startsWith(provider + '/')) {
        var found = models.some(function(m) { return m.value === currentModel; });
        if (!found) {
            models.unshift({ value: currentModel, label: stripPrefix(currentModel) + ' (current)' });
        }
    }

    // Render model radio buttons
    while (modelList.firstChild) modelList.removeChild(modelList.firstChild);

    if (models.length === 0 && item.dataset.configured !== 'true') {
        var hint = document.createElement('div');
        hint.className = 'form-hint';
        hint.textContent = 'Configure this provider to see available models.';
        modelList.appendChild(hint);
        return;
    }

    if (models.length === 0) {
        var hint = document.createElement('div');
        hint.className = 'form-hint';
        hint.textContent = 'No predefined models. Use the custom field below.';
        modelList.appendChild(hint);
        return;
    }

    var provHidden = (data.hidden_models && data.hidden_models[provider]) || [];
    var overrides = data.model_overrides || {};

    models.forEach(function(m) {
        addModelRadioElement(modelList, m.value, m.label, currentModel, provider, overrides);
    });

    // "Show N hidden" toggle if there are hidden models
    if (provHidden.length > 0) {
        var toggleBtn = document.createElement('button');
        toggleBtn.type = 'button';
        toggleBtn.className = 'model-show-hidden';
        toggleBtn.textContent = 'Show ' + provHidden.length + ' hidden';
        toggleBtn.addEventListener('click', function() {
            toggleHiddenModels(modelList, provider, provHidden, currentModel, toggleBtn, overrides);
        });
        modelList.appendChild(toggleBtn);
    }
}

function addModelRadioElement(container, value, label, currentModel, provider, overrides) {
    var wrapper = document.createElement('div');
    wrapper.className = 'model-radio-wrapper';

    var lbl = document.createElement('label');
    lbl.className = 'model-radio';

    var radio = document.createElement('input');
    radio.type = 'radio';
    radio.name = 'active-model';
    radio.value = value;
    if (value === currentModel) radio.checked = true;
    lbl.appendChild(radio);

    var nameSpan = document.createElement('span');
    nameSpan.className = 'model-radio-name';
    nameSpan.textContent = label || stripPrefix(value);
    lbl.appendChild(nameSpan);

    var badge = document.createElement('span');
    badge.className = 'model-radio-badge' + (value === currentModel ? ' active' : '');
    badge.textContent = value === currentModel ? 'Active' : '';
    lbl.appendChild(badge);

    // Gear button (per-model overrides)
    var gearBtn = document.createElement('button');
    gearBtn.type = 'button';
    gearBtn.className = 'model-radio-gear';
    if (overrides && overrides[value]) gearBtn.classList.add('has-overrides');
    gearBtn.textContent = '\u2699';
    gearBtn.title = 'Model settings';
    gearBtn.addEventListener('click', function(e) {
        e.preventDefault();
        e.stopPropagation();
        toggleModelOverrides(wrapper, value, overrides);
    });
    lbl.appendChild(gearBtn);

    // Hide button (only for non-active models)
    if (value !== currentModel && provider) {
        var hideBtn = document.createElement('button');
        hideBtn.type = 'button';
        hideBtn.className = 'model-radio-hide';
        hideBtn.textContent = '\u00D7';
        hideBtn.title = 'Hide model';
        hideBtn.addEventListener('click', function(e) {
            e.preventDefault();
            e.stopPropagation();
            hideModel(provider, value, wrapper);
        });
        lbl.appendChild(hideBtn);
    }

    wrapper.appendChild(lbl);
    container.appendChild(wrapper);
}

/** Add a custom model as a new radio in the provider's list */
function addModelRadio(item, model, setActive) {
    var modelList = item.querySelector('.provider-model-list');
    if (!modelList) return;
    // Check if already exists
    var exists = modelList.querySelector('input[value="' + CSS.escape(model) + '"]');
    if (exists) return;
    var currentModel = setActive ? model : '';
    var provider = item.dataset.provider || '';
    addModelRadioElement(modelList, model, stripPrefix(model), currentModel, provider, {});
}

async function focusModelSettingsFromQuery() {
    var modelId = modelFromQuery();
    if (!modelId || !providerGrid) return;

    var slash = modelId.indexOf('/');
    if (slash < 0) return;
    var provider = modelId.substring(0, slash);
    var card = providerGrid.querySelector('.provider-card[data-provider="' + CSS.escape(provider) + '"]');
    if (!card) return;

    var body = card.querySelector('.provider-card-body');
    var header = card.querySelector('.provider-card-header');
    var chevron = header ? header.querySelector('.provider-chevron') : null;
    if (body && body.hidden) {
        body.hidden = false;
        if (header) header.setAttribute('aria-expanded', 'true');
        if (chevron) chevron.classList.add('expanded');
    }

    if (!card.dataset.modelsLoaded) {
        await loadProviderModels(card);
    }

    var wrapper = card.querySelector('input[name="active-model"][value="' + CSS.escape(modelId) + '"]');
    if (!wrapper) {
        addModelRadio(card, modelId, false);
        wrapper = card.querySelector('input[name="active-model"][value="' + CSS.escape(modelId) + '"]');
    }
    var radioWrapper = wrapper ? wrapper.closest('.model-radio-wrapper') : null;
    if (!radioWrapper) return;

    var data = await fetchAllModels();
    var overrides = data.model_overrides || {};
    if (!radioWrapper.querySelector('.model-overrides-form')) {
        toggleModelOverrides(radioWrapper, modelId, overrides);
    }

    card.scrollIntoView({ behavior: 'smooth', block: 'start' });
    window.setTimeout(function() {
        radioWrapper.scrollIntoView({ behavior: 'smooth', block: 'center' });
    }, 120);
}


// ═══ Model Hiding ═══

function hideModel(provider, modelId, wrapperEl) {
    var data = _allModelsCache || {};
    var hidden = (data.hidden_models && data.hidden_models[provider]) || [];
    if (hidden.indexOf(modelId) === -1) hidden.push(modelId);

    patchConfig('providers.' + provider + '.hidden_models', hidden).then(function() {
        wrapperEl.remove();
        _allModelsCache = null;
        loadVisionDropdown();
        populateFallbackDropdown();
        showToast('Model hidden', 'success');
        // Reload provider model list to update "Show N hidden" link
        var item = document.querySelector('.provider-card[data-provider="' + provider + '"]');
        if (item) {
            item.dataset.modelsLoaded = '';
            loadProviderModels(item);
        }
    }).catch(function() {
        showToast('Failed to hide model', 'error');
    });
}

function toggleHiddenModels(modelList, provider, hiddenIds, currentModel, toggleBtn, overrides) {
    var existing = modelList.querySelector('.hidden-models-section');
    if (existing) {
        existing.remove();
        toggleBtn.textContent = 'Show ' + hiddenIds.length + ' hidden';
        return;
    }

    var section = document.createElement('div');
    section.className = 'hidden-models-section';

    hiddenIds.forEach(function(id) {
        var row = document.createElement('div');
        row.className = 'hidden-model-row';

        var name = document.createElement('span');
        name.className = 'hidden-model-name';
        name.textContent = stripPrefix(id);
        row.appendChild(name);

        var restoreBtn = document.createElement('button');
        restoreBtn.type = 'button';
        restoreBtn.className = 'btn btn-xs';
        restoreBtn.textContent = 'Restore';
        restoreBtn.addEventListener('click', function() {
            restoreModel(provider, id);
        });
        row.appendChild(restoreBtn);

        section.appendChild(row);
    });

    modelList.insertBefore(section, toggleBtn);
    toggleBtn.textContent = 'Hide list';
}

function restoreModel(provider, modelId) {
    var data = _allModelsCache || {};
    var hidden = (data.hidden_models && data.hidden_models[provider]) || [];
    hidden = hidden.filter(function(id) { return id !== modelId; });

    patchConfig('providers.' + provider + '.hidden_models', hidden).then(function() {
        _allModelsCache = null;
        showToast('Model restored', 'success');
        var item = document.querySelector('.provider-card[data-provider="' + provider + '"]');
        if (item) {
            item.dataset.modelsLoaded = '';
            loadProviderModels(item);
        }
        loadVisionDropdown();
        populateFallbackDropdown();
    }).catch(function() {
        showToast('Failed to restore model', 'error');
    });
}


// ═══ Per-Model Settings ═══

function getEffectiveModelCapabilities(modelId, data) {
    var caps = (data && data.model_capabilities && data.model_capabilities[modelId]) || {};
    return {
        multimodal: !!caps.multimodal,
        image_input: !!caps.image_input,
        tool_calls: caps.tool_calls !== false,
        thinking: !!caps.thinking,
    };
}

function toggleModelOverrides(wrapper, modelId, overrides) {
    var existing = wrapper.querySelector('.model-overrides-form');
    if (existing) {
        existing.remove();
        return;
    }

    var current = (overrides && overrides[modelId]) || {};
    var capabilityDefaults = getEffectiveModelCapabilities(modelId, _allModelsCache || {});

    var form = document.createElement('div');
    form.className = 'model-overrides-form';

    // Temperature row
    var tempLabel = document.createElement('label');
    tempLabel.className = 'override-label';
    tempLabel.textContent = 'Temperature';
    form.appendChild(tempLabel);

    var tempInput = document.createElement('input');
    tempInput.type = 'number';
    tempInput.className = 'input override-input';
    tempInput.step = '0.1';
    tempInput.min = '0';
    tempInput.max = '2';
    tempInput.placeholder = 'global';
    if (current.temperature != null) tempInput.value = current.temperature;
    form.appendChild(tempInput);

    // Max tokens row
    var tokLabel = document.createElement('label');
    tokLabel.className = 'override-label';
    tokLabel.textContent = 'Max tokens';
    form.appendChild(tokLabel);

    var tokInput = document.createElement('input');
    tokInput.type = 'number';
    tokInput.className = 'input override-input';
    tokInput.step = '1';
    tokInput.min = '1';
    tokInput.placeholder = 'global';
    if (current.max_tokens != null) tokInput.value = current.max_tokens;
    form.appendChild(tokInput);

    var capabilityHint = document.createElement('div');
    capabilityHint.className = 'form-hint';
    capabilityHint.textContent = 'Capabilities are prefilled from known model defaults. Adjust them for custom or BYOK models.';
    form.appendChild(capabilityHint);

    var capabilityFields = [
        {
            key: 'multimodal',
            label: 'Multimodal',
            help: 'Treat this model as non-text capable.',
            checked: current.multimodal != null ? current.multimodal : capabilityDefaults.multimodal,
        },
        {
            key: 'image_input',
            label: 'Image input',
            help: 'Use this model directly for image attachments.',
            checked: current.image_input != null ? current.image_input : capabilityDefaults.image_input,
        },
        {
            key: 'tool_calls',
            label: 'Native tool calls',
            help: 'Disable this if the model needs XML tool dispatch instead of native function calling.',
            checked: current.tool_calls != null ? current.tool_calls : capabilityDefaults.tool_calls,
        },
        {
            key: 'thinking',
            label: 'Thinking',
            help: 'Enable reasoning/thinking mode. Auto-detected for DeepSeek R1, QwQ, cloud models.',
            checked: current.thinking != null ? current.thinking : capabilityDefaults.thinking,
        },
    ];

    var capabilityInputs = {};
    capabilityFields.forEach(function(field) {
        var row = document.createElement('label');
        row.className = 'checkbox-row';

        var checkbox = document.createElement('input');
        checkbox.type = 'checkbox';
        checkbox.checked = !!field.checked;
        row.appendChild(checkbox);

        var text = document.createElement('span');
        text.textContent = field.label;
        row.appendChild(text);
        form.appendChild(row);

        var help = document.createElement('div');
        help.className = 'form-hint';
        help.textContent = field.help;
        form.appendChild(help);

        capabilityInputs[field.key] = checkbox;
    });

    if (capabilityInputs.multimodal && capabilityInputs.image_input) {
        capabilityInputs.multimodal.addEventListener('change', function() {
            if (capabilityInputs.multimodal.checked) {
                capabilityInputs.image_input.checked = true;
            }
        });
    }

    // Buttons row
    var btnRow = document.createElement('div');
    btnRow.className = 'override-buttons';

    var saveBtn = document.createElement('button');
    saveBtn.type = 'button';
    saveBtn.className = 'btn btn-xs btn-primary';
    saveBtn.textContent = 'Save';
    saveBtn.addEventListener('click', function() {
        saveModelOverrides(modelId, tempInput.value, tokInput.value, capabilityInputs, capabilityDefaults, form);
    });
    btnRow.appendChild(saveBtn);

    var clearBtn = document.createElement('button');
    clearBtn.type = 'button';
    clearBtn.className = 'btn btn-xs';
    clearBtn.textContent = 'Clear';
    clearBtn.addEventListener('click', function() {
        clearModelOverrides(modelId, form);
    });
    btnRow.appendChild(clearBtn);

    form.appendChild(btnRow);
    wrapper.appendChild(form);
}

function saveModelOverrides(modelId, tempVal, tokensVal, capabilityInputs, capabilityDefaults, formEl) {
    var data = _allModelsCache || {};
    var allOverrides = Object.assign({}, data.model_overrides || {});

    var entry = {};
    if (tempVal !== '') entry.temperature = parseFloat(tempVal);
    if (tokensVal !== '') entry.max_tokens = parseInt(tokensVal, 10);
    if (capabilityInputs.multimodal && capabilityInputs.multimodal.checked && capabilityInputs.image_input) {
        capabilityInputs.image_input.checked = true;
    }
    ['multimodal', 'image_input', 'tool_calls', 'thinking'].forEach(function(key) {
        if (!capabilityInputs[key]) return;
        if (capabilityInputs[key].checked !== !!capabilityDefaults[key]) {
            entry[key] = capabilityInputs[key].checked;
        }
    });

    if (Object.keys(entry).length === 0) {
        delete allOverrides[modelId];
    } else {
        allOverrides[modelId] = entry;
    }

    var wrapper = formEl.closest('.model-radio-wrapper');
    patchConfig('agent.model_overrides', allOverrides).then(function() {
        _allModelsCache = null;
        formEl.remove();
        showToast('Model settings saved', 'success');
        if (wrapper) {
            var gear = wrapper.querySelector('.model-radio-gear');
            if (gear) {
                if (Object.keys(entry).length > 0) {
                    gear.classList.add('has-overrides');
                } else {
                    gear.classList.remove('has-overrides');
                }
            }
        }
    }).catch(function() {
        showToast('Failed to save settings', 'error');
    });
}

function clearModelOverrides(modelId, formEl) {
    var data = _allModelsCache || {};
    var allOverrides = Object.assign({}, data.model_overrides || {});
    delete allOverrides[modelId];

    var wrapper = formEl.closest('.model-radio-wrapper');
    patchConfig('agent.model_overrides', allOverrides).then(function() {
        _allModelsCache = null;
        formEl.remove();
        showToast('Model settings cleared', 'success');
        if (wrapper) {
            var gear = wrapper.querySelector('.model-radio-gear');
            if (gear) gear.classList.remove('has-overrides');
        }
    }).catch(function() {
        showToast('Failed to clear settings', 'error');
    });
}


// ═══ Advanced Agent Settings ═══

var agentForm = document.getElementById('agent-form');
var visionModelSelect = document.getElementById('vision-model-select');
var visionModelValue = document.getElementById('vision-model-value');

if (agentForm) {
    var maxTokensInput = agentForm.querySelector('input[name="max_tokens"]');
    var temperatureInput = agentForm.querySelector('input[name="temperature"]');
    var maxIterationsInput = agentForm.querySelector('input[name="max_iterations"]');
    var xmlDelayInput = agentForm.querySelector('input[name="xml_fallback_delay_ms"]');

    function validateAgentForm() {
        var ok = true;
        ok = validateNumberField(maxTokensInput, 1, 400000) && ok;
        ok = validateNumberField(temperatureInput, 0, 2) && ok;
        ok = validateNumberField(maxIterationsInput, 1, 50) && ok;
        ok = validateNumberField(xmlDelayInput, 0, 60000) && ok;
        return ok;
    }

    [maxTokensInput, temperatureInput, maxIterationsInput, xmlDelayInput].forEach(function(input) {
        if (!input) return;
        input.addEventListener('input', validateAgentForm);
        input.addEventListener('blur', validateAgentForm);
    });

    agentForm.addEventListener('submit', async function(e) {
        e.preventDefault();
        if (!validateAgentForm()) {
            showToast('Fix validation errors before saving', 'error');
            return;
        }
        var btn = agentForm.querySelector('button[type="submit"]');
        var originalText = btn.textContent;
        btn.textContent = 'Saving\u2026';
        btn.disabled = true;

        // Sync vision model
        if (visionModelSelect && visionModelValue) {
            visionModelValue.value = visionModelSelect.value;
        }

        var form = new FormData(agentForm);
        var patches = [
            { key: 'agent.vision_model', value: form.get('vision_model') || '' },
            { key: 'agent.max_tokens', value: form.get('max_tokens') },
            { key: 'agent.temperature', value: form.get('temperature') },
            { key: 'agent.max_iterations', value: form.get('max_iterations') },
            { key: 'agent.xml_fallback_delay_ms', value: form.get('xml_fallback_delay_ms') },
            { key: 'agent.fallback_models', value: fallbackModels },
        ];

        try {
            for (var i = 0; i < patches.length; i++) {
                await fetch('/api/v1/config', {
                    method: 'PATCH',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify(patches[i]),
                });
            }
            btn.textContent = 'Saved!';
            setTimeout(function() { btn.textContent = originalText; btn.disabled = false; }, 2000);
        } catch (err) {
            btn.textContent = 'Error!';
            setTimeout(function() { btn.textContent = originalText; btn.disabled = false; }, 2000);
        }
    });
}

// Load vision model dropdown (uses reusable buildModelOptions)
async function loadVisionDropdown() {
    if (!visionModelSelect) return;
    var data = await fetchAllModels();
    var fullData = allModelsIncludingHidden(data);
    buildModelOptions(visionModelSelect, fullData, {
        specialOptions: [{ value: '', label: '(Same as chat model)' }],
        includeCustom: true,
        selectedValue: data.vision_model || '',
    });
}

loadVisionDropdown();


// ═══ Fallback Models ═══

var fallbackList = document.getElementById('fallback-models-list');
var fallbackSelect = document.getElementById('fallback-model-select');
var btnAddFallback = document.getElementById('btn-add-fallback');
var fallbackModels = [];

function renderFallbackTags() {
    if (!fallbackList) return;
    while (fallbackList.firstChild) fallbackList.removeChild(fallbackList.firstChild);
    if (fallbackModels.length === 0) {
        var empty = document.createElement('span');
        empty.className = 'tag-list-empty';
        empty.textContent = 'No fallback models configured. The agent will retry the primary model only.';
        fallbackList.appendChild(empty);
        return;
    }
    fallbackModels.forEach(function(model, idx) {
        var tag = document.createElement('span');
        tag.className = 'tag';
        var label = document.createElement('span');
        label.className = 'tag-label';
        label.textContent = (idx + 1) + '. ' + model;
        tag.appendChild(label);
        var removeBtn = document.createElement('button');
        removeBtn.type = 'button';
        removeBtn.className = 'tag-remove';
        removeBtn.textContent = '\u00d7';
        removeBtn.addEventListener('click', function() { fallbackModels.splice(idx, 1); renderFallbackTags(); });
        tag.appendChild(removeBtn);
        fallbackList.appendChild(tag);
    });
}

// Populate fallback dropdown (uses reusable buildModelOptions)
async function populateFallbackDropdown() {
    if (!fallbackSelect) return;
    var data = await fetchAllModels();
    var fullData = allModelsIncludingHidden(data);
    buildModelOptions(fallbackSelect, fullData, {
        placeholder: 'Add fallback model\u2026',
        includeCustom: true,
    });
}

if (btnAddFallback) {
    btnAddFallback.addEventListener('click', function() {
        if (!fallbackSelect) return;
        var val = fallbackSelect.value;
        if (!val) return;
        if (val === '__custom__') {
            var custom = prompt('Enter model name (e.g. openai/gpt-4o):');
            if (custom && custom.trim()) val = custom.trim(); else return;
        }
        if (fallbackModels.indexOf(val) !== -1) return;
        fallbackModels.push(val);
        renderFallbackTags();
        fallbackSelect.value = '';
    });
}

if (fallbackList) {
    try {
        var initial = JSON.parse(fallbackList.getAttribute('data-models') || '[]');
        if (Array.isArray(initial)) fallbackModels = initial;
    } catch (_) {}
    renderFallbackTags();
}

populateFallbackDropdown();


// ═══ Channel Configuration Cards ═══

// Helper: remove all children from an element
function clearChildren(el) {
    while (el && el.firstChild) {
        el.removeChild(el.firstChild);
    }
}

const channelCards = document.querySelectorAll('.channel-card');
const chModal = document.getElementById('channel-modal');

if (chModal && channelCards.length > 0) {
    const chBackdrop = chModal.querySelector('.modal-backdrop');
    const chCloseBtn = chModal.querySelector('.ch-modal-close');
    const chCancelBtn = chModal.querySelector('.ch-modal-cancel');
    const chForm = document.getElementById('channel-config-form');
    const chSubtitle = document.getElementById('channel-subtitle');
    const chTokenGroup = document.getElementById('ch-token-group');
    const chPhoneGroup = document.getElementById('ch-phone-group');
    const chAllowGroup = document.getElementById('ch-allow-from-group');
    const chDiscordGroup = document.getElementById('ch-discord-channel-group');
    const chSlackGroup = document.getElementById('ch-slack-channel-group');
    const chEmailServersGroup = document.getElementById('ch-email-servers-group');
    const chEmailCredsGroup = document.getElementById('ch-email-credentials-group');
    const chEmailBehaviorGroup = document.getElementById('ch-email-behavior-group');
    const chEmailNotifyGroup = document.getElementById('ch-email-notify-group');
    const chEmailTriggerGroup = document.getElementById('ch-email-trigger-group');
    const chWebHostGroup = document.getElementById('ch-web-host-group');
    const chWebPortGroup = document.getElementById('ch-web-port-group');
    const chWaPairing = document.getElementById('ch-wa-pairing');
    const chNotifyHint = document.getElementById('ch-notify-hint');
    const btnWaPair = document.getElementById('btn-wa-pair');
    const btnTestCh = document.getElementById('btn-test-channel');
    const btnChSave = document.getElementById('btn-ch-save');
    const chTestResult = document.getElementById('ch-test-result');

    let currentChannel = null;

    // --- Email mode field visibility logic ---
    function updateEmailModeFields() {
        var modeSelect = document.getElementById('ch-email-mode');
        var modeHint = document.getElementById('ch-email-mode-hint');
        if (!modeSelect) return;
        var mode = modeSelect.value;
        // Notify group: visible for assisted & automatic
        if (chEmailNotifyGroup) {
            chEmailNotifyGroup.style.display = (mode === 'assisted' || mode === 'automatic') ? 'block' : 'none';
        }
        // Trigger word: only for on_demand
        if (chEmailTriggerGroup) {
            chEmailTriggerGroup.style.display = mode === 'on_demand' ? 'block' : 'none';
            // Auto-fetch/generate trigger word when switching to on_demand
            if (mode === 'on_demand') {
                var twInput = document.getElementById('ch-email-trigger-word');
                if (twInput && !twInput.value.trim()) {
                    fetch('/api/v1/channels/email/trigger-word', {
                        method: 'POST',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify({ account: 'default' })
                    })
                    .then(function(r) { return r.json(); })
                    .then(function(d) {
                        if (d.trigger_word && !twInput.value.trim()) {
                            twInput.value = d.trigger_word;
                        }
                    })
                    .catch(function() {});
                }
            }
        }
        // Update hint text
        if (modeHint) {
            if (mode === 'assisted') {
                modeHint.textContent = 'Generates summary and draft, sends to notification channel for approval.';
            } else if (mode === 'automatic') {
                modeHint.textContent = 'Responds directly to emails. Escalates to notification channel if unsure.';
            } else if (mode === 'on_demand') {
                modeHint.textContent = 'Only processes emails containing the trigger word. Others are ignored.';
            }
        }
    }

    // Attach mode change listener
    var emailModeSelect = document.getElementById('ch-email-mode');
    if (emailModeSelect) {
        emailModeSelect.addEventListener('change', updateEmailModeFields);
    }

    // Chat channel behavior: show/hide notify fields based on response mode
    var chResponseMode = document.getElementById('ch-response-mode');
    if (chResponseMode) {
        chResponseMode.addEventListener('change', function () {
            var mode = chResponseMode.value;
            var showNotify = mode === 'assisted';
            var notifyCh = document.getElementById('ch-notify-channel-group');
            var notifyCid = document.getElementById('ch-notify-chatid-group');
            if (notifyCh) notifyCh.style.display = showNotify ? 'block' : 'none';
            if (notifyCid) notifyCid.style.display = showNotify ? 'block' : 'none';
        });
    }

    // --- Channel subtitles (service description) ---
    const SUBTITLES = {
        telegram: 'Telegram Bot API \u2014 create a bot with @BotFather, paste the token, add your User ID (from @userinfobot).',
        discord: 'Discord Bot gateway \u2014 create app at discord.com/developers, enable Message Content intent.',
        slack: 'Slack Web API \u2014 create app at api.slack.com/apps, add bot scopes (chat:write, channels:history).',
        whatsapp: 'Native WhatsApp Web \u2014 enter phone number, click Start Pairing, link device in WhatsApp settings.',
        email: 'IMAP/SMTP \u2014 for Gmail enable 2FA and create an App Password.',
        web: 'Built-in browser chat interface. Always enabled.',
    };

    function updateSubtitle(channelName) {
        if (chSubtitle) chSubtitle.textContent = SUBTITLES[channelName] || '';
    }

    // --- Auto-populate Notify Chat ID from channel config ---
    var notifyChannelSelect = document.getElementById('ch-email-notify-channel');
    var notifyChatIdInput = document.getElementById('ch-email-notify-chat-id');
    if (notifyChannelSelect) {
        notifyChannelSelect.addEventListener('change', function() {
            var ch = notifyChannelSelect.value;
            if (chNotifyHint) chNotifyHint.textContent = '';
            if (!ch) return;
            fetch('/api/v1/channels/' + ch)
                .then(function(r) { return r.ok ? r.json() : null; })
                .then(function(data) {
                    if (!data) return;
                    var id = '';
                    if (ch === 'discord') id = data.default_channel_id || (data.allow_from || [])[0] || '';
                    else if (ch === 'slack') id = data.channel_id || (data.allow_from || [])[0] || '';
                    else id = (data.allow_from || [])[0] || '';
                    if (id && notifyChatIdInput && !notifyChatIdInput.value.trim()) {
                        notifyChatIdInput.value = id;
                    }
                    if (id && chNotifyHint) chNotifyHint.textContent = 'Suggested: ' + id;
                })
                .catch(function() {});
        });
    }

    // --- Card click handlers ---
    channelCards.forEach(function(card) {
        var toggle = card.querySelector('.toggle-input');
        var toggleLabel = card.querySelector('.toggle-label');
        var isWeb = card.dataset.isWeb === 'true';

        card.style.cursor = 'pointer';
        card.addEventListener('click', function(e) {
            if (e.target === toggle || e.target === toggleLabel) return;
            openChannelModal(card);
        });

        if (toggle && !isWeb) {
            toggle.addEventListener('change', function() {
                if (toggle.checked) {
                    if (card.dataset.configured !== 'true') {
                        toggle.checked = false;
                        openChannelModal(card);
                    }
                } else {
                    var displayName = card.dataset.display;
                    if (confirm('Deactivate ' + displayName + '? This will remove stored credentials.')) {
                        deactivateChannel(card.dataset.channel, card);
                    } else {
                        toggle.checked = true;
                    }
                }
            });
        }
    });

    // --- Open channel modal ---
    function openChannelModal(card) {
        currentChannel = card.dataset.channel;
        var display = card.dataset.display;
        var isWeb = card.dataset.isWeb === 'true';
        var hasToken = card.dataset.hasToken === 'true';

        document.getElementById('modal-channel-name').textContent = display;
        document.getElementById('modal-channel-id').value = currentChannel;

        // Subtitle
        updateSubtitle(currentChannel);

        // Show/hide form groups based on channel type
        var isEmail = currentChannel === 'email';
        if (chTokenGroup) chTokenGroup.style.display = hasToken ? 'block' : 'none';
        if (chPhoneGroup) chPhoneGroup.style.display = currentChannel === 'whatsapp' ? 'block' : 'none';
        if (chAllowGroup) chAllowGroup.style.display = (currentChannel !== 'web' && !isEmail) ? 'block' : 'none';
        if (chDiscordGroup) chDiscordGroup.style.display = currentChannel === 'discord' ? 'block' : 'none';
        if (chSlackGroup) chSlackGroup.style.display = currentChannel === 'slack' ? 'block' : 'none';
        // Email groups (servers, credentials, behavior)
        if (chEmailServersGroup) chEmailServersGroup.style.display = isEmail ? 'block' : 'none';
        if (chEmailCredsGroup) chEmailCredsGroup.style.display = isEmail ? 'block' : 'none';
        if (chEmailBehaviorGroup) chEmailBehaviorGroup.style.display = isEmail ? 'block' : 'none';
        if (chWebHostGroup) chWebHostGroup.style.display = isWeb ? 'block' : 'none';
        if (chWebPortGroup) chWebPortGroup.style.display = isWeb ? 'block' : 'none';
        if (chWaPairing) chWaPairing.style.display = 'none';
        // Chat channel behavior group (response mode + notify)
        var isChatChannel = ['telegram', 'whatsapp', 'discord', 'slack'].indexOf(currentChannel) >= 0;
        var chBehaviorGroup = document.getElementById('ch-behavior-group');
        if (chBehaviorGroup) chBehaviorGroup.style.display = isChatChannel ? 'block' : 'none';
        // Email notify/trigger shown by updateEmailModeFields
        if (chEmailNotifyGroup) chEmailNotifyGroup.style.display = 'none';
        if (chEmailTriggerGroup) chEmailTriggerGroup.style.display = 'none';
        if (btnWaPair) btnWaPair.style.display = currentChannel === 'whatsapp' ? 'inline-flex' : 'none';
        if (btnTestCh) btnTestCh.style.display = isWeb ? 'none' : 'inline-flex';
        if (btnChSave) btnChSave.style.display = isWeb ? 'none' : 'inline-flex';
        if (chNotifyHint) chNotifyHint.textContent = '';

        // Set hints
        if (currentChannel === 'telegram') {
            document.getElementById('ch-token-hint').textContent = 'Bot token from @BotFather. Stored encrypted locally.';
            document.getElementById('ch-allow-from-hint').textContent = 'Telegram User IDs (numeric). Get yours from @userinfobot.';
        } else if (currentChannel === 'discord') {
            document.getElementById('ch-token-hint').textContent = 'Bot token from Discord Developer Portal. Stored encrypted.';
            document.getElementById('ch-allow-from-hint').textContent = 'Discord User IDs (numeric). Enable Developer Mode to copy.';
        } else if (currentChannel === 'slack') {
            document.getElementById('ch-token-hint').textContent = 'Bot User OAuth Token (xoxb-...) from Slack App. Stored encrypted.';
            document.getElementById('ch-allow-from-hint').textContent = 'Slack User IDs (U...). Use "*" to allow everyone.';
        } else if (currentChannel === 'whatsapp') {
            document.getElementById('ch-allow-from-hint').textContent = 'Phone numbers of allowed senders (e.g. 393331234567).';
        } else if (currentChannel === 'email') {
            document.getElementById('ch-allow-from-hint').textContent = 'Email addresses or domains (e.g. user@example.com, @company.com). Use "*" to allow everyone.';
        }

        // Clear fields first (defaults from card data attributes)
        document.getElementById('ch-token').value = '';
        document.getElementById('ch-token').placeholder = 'Paste token here...';
        document.getElementById('ch-phone').value = card.dataset.phone || '';
        document.getElementById('ch-allow-from').value = card.dataset.allowFrom || '';
        document.getElementById('ch-discord-channel').value = card.dataset.discordChannel || '';
        document.getElementById('ch-slack-channel').value = card.dataset.slackChannel || '';
        document.getElementById('ch-web-host').value = card.dataset.webHost || '';
        document.getElementById('ch-web-port').value = card.dataset.webPort || '';

        // Email fields from data attributes
        var emailImapHost = document.getElementById('ch-email-imap-host');
        var emailImapPort = document.getElementById('ch-email-imap-port');
        var emailSmtpHost = document.getElementById('ch-email-smtp-host');
        var emailSmtpPort = document.getElementById('ch-email-smtp-port');
        var emailUsername = document.getElementById('ch-email-username');
        var emailFrom = document.getElementById('ch-email-from');
        var emailPassword = document.getElementById('ch-email-password');
        if (emailImapHost) emailImapHost.value = card.dataset.emailImapHost || '';
        if (emailImapPort) emailImapPort.value = card.dataset.emailImapPort || '';
        if (emailSmtpHost) emailSmtpHost.value = card.dataset.emailSmtpHost || '';
        if (emailSmtpPort) emailSmtpPort.value = card.dataset.emailSmtpPort || '';
        if (emailUsername) emailUsername.value = card.dataset.emailUsername || '';
        if (emailFrom) emailFrom.value = card.dataset.emailFrom || '';
        if (emailPassword) { emailPassword.value = ''; emailPassword.placeholder = 'App password (stored encrypted)'; }

        // Email mode/notify/trigger from data attributes
        if (currentChannel === 'email') {
            var modeEl = document.getElementById('ch-email-mode');
            if (modeEl) modeEl.value = card.dataset.emailMode || 'assisted';
            var notifyChEl = document.getElementById('ch-email-notify-channel');
            if (notifyChEl) notifyChEl.value = card.dataset.emailNotifyChannel || '';
            var notifyChatEl = document.getElementById('ch-email-notify-chat-id');
            if (notifyChatEl) notifyChatEl.value = card.dataset.emailNotifyChatId || '';
            var triggerEl = document.getElementById('ch-email-trigger-word');
            if (triggerEl) triggerEl.value = card.dataset.emailTriggerWord || '';
            updateEmailModeFields();
        }

        // Persona & tone from card data attributes
        var personaEl = document.getElementById('ch-persona');
        if (personaEl) personaEl.value = card.dataset.persona || 'bot';
        var toneEl = document.getElementById('ch-tone');
        if (toneEl) toneEl.value = card.dataset.toneOfVoice || '';

        // Reset test result
        chTestResult.textContent = '';
        chTestResult.className = 'form-hint';

        chModal.classList.add('open');
        document.body.style.overflow = 'hidden';

        // Fetch live data from API (resolves encrypted tokens)
        fetch('/api/v1/channels/' + currentChannel)
            .then(function(r) { return r.ok ? r.json() : null; })
            .then(function(data) {
                if (!data) return;
                // Token placeholder (masked)
                if (data.has_token && data.token_masked) {
                    document.getElementById('ch-token').placeholder = data.token_masked;
                }
                // Allow-from list
                if (data.allow_from && data.allow_from.length > 0) {
                    document.getElementById('ch-allow-from').value = data.allow_from.join(', ');
                }
                // WhatsApp phone
                if (data.phone_number) {
                    document.getElementById('ch-phone').value = data.phone_number;
                }
                // Discord default channel
                if (data.default_channel_id) {
                    document.getElementById('ch-discord-channel').value = data.default_channel_id;
                }
                // Slack channel
                if (data.channel_id) {
                    document.getElementById('ch-slack-channel').value = data.channel_id;
                }
                // Web host/port
                if (data.host) {
                    document.getElementById('ch-web-host').value = data.host;
                }
                if (data.port) {
                    document.getElementById('ch-web-port').value = data.port;
                }
                // Email fields
                if (data.imap_host) {
                    var imapHost = document.getElementById('ch-email-imap-host');
                    if (imapHost) imapHost.value = data.imap_host;
                }
                if (data.imap_port) {
                    var imapPort = document.getElementById('ch-email-imap-port');
                    if (imapPort) imapPort.value = data.imap_port;
                }
                if (data.smtp_host) {
                    var smtpHost = document.getElementById('ch-email-smtp-host');
                    if (smtpHost) smtpHost.value = data.smtp_host;
                }
                if (data.smtp_port) {
                    var smtpPort = document.getElementById('ch-email-smtp-port');
                    if (smtpPort) smtpPort.value = data.smtp_port;
                }
                if (data.username) {
                    var emailUser = document.getElementById('ch-email-username');
                    if (emailUser) emailUser.value = data.username;
                }
                if (data.has_password) {
                    var emailPass = document.getElementById('ch-email-password');
                    if (emailPass) emailPass.placeholder = '•••••••• (stored encrypted)';
                }
                if (data.from_address) {
                    var emailFrom = document.getElementById('ch-email-from');
                    if (emailFrom) emailFrom.value = data.from_address;
                }
                // Email mode/notify/trigger from API
                if (currentChannel === 'email') {
                    var modeEl = document.getElementById('ch-email-mode');
                    if (modeEl && data.email_mode) modeEl.value = data.email_mode;
                    var notifyChEl = document.getElementById('ch-email-notify-channel');
                    if (notifyChEl && data.email_notify_channel !== undefined) notifyChEl.value = data.email_notify_channel;
                    var notifyChatEl = document.getElementById('ch-email-notify-chat-id');
                    if (notifyChatEl && data.email_notify_chat_id !== undefined) notifyChatEl.value = data.email_notify_chat_id;
                    var triggerEl = document.getElementById('ch-email-trigger-word');
                    if (triggerEl && data.email_trigger_word !== undefined) triggerEl.value = data.email_trigger_word;
                    updateEmailModeFields();
                    // Auto-suggest chat ID if notify channel is set but chat ID is empty
                    if (notifyChEl && notifyChEl.value && notifyChatEl && !notifyChatEl.value.trim()) {
                        notifyChannelSelect.dispatchEvent(new Event('change'));
                    }
                }
                // Persona & tone from API
                var pEl = document.getElementById('ch-persona');
                if (pEl && data.persona) pEl.value = data.persona;
                var toEl = document.getElementById('ch-tone');
                if (toEl && data.tone_of_voice !== undefined) toEl.value = data.tone_of_voice;
            })
            .catch(function() { /* silently use card data fallback */ });
    }

    // --- Close modal ---
    function closeChannelModal() {
        chModal.classList.remove('open');
        document.body.style.overflow = '';
        currentChannel = null;
    }

    [chBackdrop, chCloseBtn, chCancelBtn].forEach(function(el) {
        if (el) el.addEventListener('click', closeChannelModal);
    });
    document.addEventListener('keydown', function(e) {
        if (e.key === 'Escape' && chModal.classList.contains('open')) {
            closeChannelModal();
        }
    });

    // --- Deactivate channel ---
    async function deactivateChannel(name, card) {
        try {
            var resp = await fetch('/api/v1/channels/deactivate', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ name: name }),
            });
            var data = await resp.json();
            if (data.ok) {
                card.classList.remove('is-configured', 'is-active');
                card.dataset.configured = 'false';
                card.dataset.enabled = 'false';
                var badge = card.querySelector('.provider-default-badge');
                if (badge) badge.style.display = 'none';
            } else {
                var toggle = card.querySelector('.toggle-input');
                if (toggle) toggle.checked = true;
                alert(data.message || 'Failed to deactivate');
            }
        } catch (err) {
            var toggle = card.querySelector('.toggle-input');
            if (toggle) toggle.checked = true;
            alert('Failed to deactivate channel');
        }
    }

    // --- Save channel config ---
    if (chForm) {
        chForm.addEventListener('submit', async function(e) {
            e.preventDefault();
            var btn = btnChSave;
            var originalText = btn.textContent;
            btn.textContent = 'Saving\u2026';
            btn.disabled = true;

            var payload = { name: currentChannel };

            // Collect visible fields
            if (chTokenGroup.style.display !== 'none') {
                var tokenVal = document.getElementById('ch-token').value.trim();
                if (tokenVal) payload.token = tokenVal;
            }
            if (chPhoneGroup.style.display !== 'none') {
                payload.phone_number = document.getElementById('ch-phone').value.trim();
            }
            if (chAllowGroup.style.display !== 'none') {
                var raw = document.getElementById('ch-allow-from').value.trim();
                if (raw) {
                    payload.allow_from = raw.split(',').map(function(s) { return s.trim(); }).filter(Boolean);
                }
            }
            if (chDiscordGroup.style.display !== 'none') {
                payload.default_channel_id = document.getElementById('ch-discord-channel').value.trim();
            }
            if (chSlackGroup.style.display !== 'none') {
                payload.default_channel_id = document.getElementById('ch-slack-channel').value.trim();
            }
            // Email fields (grouped under servers, credentials, behavior wrappers)
            if (chEmailServersGroup && chEmailServersGroup.style.display !== 'none') {
                var imapHost = document.getElementById('ch-email-imap-host');
                var imapPort = document.getElementById('ch-email-imap-port');
                if (imapHost) payload.imap_host = imapHost.value.trim();
                if (imapPort && imapPort.value) payload.imap_port = parseInt(imapPort.value, 10);
                var smtpHost = document.getElementById('ch-email-smtp-host');
                var smtpPort = document.getElementById('ch-email-smtp-port');
                if (smtpHost) payload.smtp_host = smtpHost.value.trim();
                if (smtpPort && smtpPort.value) payload.smtp_port = parseInt(smtpPort.value, 10);
            }
            if (chEmailCredsGroup && chEmailCredsGroup.style.display !== 'none') {
                var emailUser = document.getElementById('ch-email-username');
                var emailPass = document.getElementById('ch-email-password');
                var emailFrom = document.getElementById('ch-email-from');
                if (emailUser) payload.username = emailUser.value.trim();
                if (emailPass) payload.password = emailPass.value;
                if (emailFrom) payload.from_address = emailFrom.value.trim();
            }
            // Chat channel behavior (response mode + notify)
            var chBehaviorGroup = document.getElementById('ch-behavior-group');
            if (chBehaviorGroup && chBehaviorGroup.style.display !== 'none') {
                var rmEl = document.getElementById('ch-response-mode');
                if (rmEl && rmEl.value) payload.response_mode = rmEl.value;
                var ncEl = document.getElementById('ch-notify-channel');
                if (ncEl && ncEl.value) payload.notify_channel = ncEl.value;
                var ncidEl = document.getElementById('ch-notify-chatid');
                if (ncidEl && ncidEl.value) payload.notify_chat_id = ncidEl.value;
                // Persona & tone
                var personaEl = document.getElementById('ch-persona');
                if (personaEl) payload.persona = personaEl.value;
                var toneEl = document.getElementById('ch-tone');
                if (toneEl) payload.tone_of_voice = toneEl.value.trim();
            }
            // Email mode/notify/trigger
            if (chEmailBehaviorGroup && chEmailBehaviorGroup.style.display !== 'none') {
                var modeEl = document.getElementById('ch-email-mode');
                if (modeEl) payload.email_mode = modeEl.value;
                var notifyChEl = document.getElementById('ch-email-notify-channel');
                if (notifyChEl && notifyChEl.value) payload.email_notify_channel = notifyChEl.value;
                var notifyChatEl = document.getElementById('ch-email-notify-chat-id');
                if (notifyChatEl && notifyChatEl.value) payload.email_notify_chat_id = notifyChatEl.value;
                var triggerEl = document.getElementById('ch-email-trigger-word');
                if (triggerEl && triggerEl.value) payload.email_trigger_word = triggerEl.value;
            }
            if (chWebHostGroup.style.display !== 'none') {
                payload.host = document.getElementById('ch-web-host').value.trim();
            }
            if (chWebPortGroup.style.display !== 'none') {
                var portVal = document.getElementById('ch-web-port').value.trim();
                if (portVal) payload.port = parseInt(portVal, 10);
            }

            try {
                var resp = await fetch('/api/v1/channels/configure', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify(payload),
                });
                var data = await resp.json();
                if (data.ok) {
                    btn.textContent = 'Saved \u2713';
                    // Update the card state
                    var card = document.querySelector('.channel-card[data-channel="' + currentChannel + '"]');
                    if (card) {
                        card.classList.add('is-configured', 'is-active');
                        card.dataset.configured = 'true';
                        card.dataset.enabled = 'true';
                        var toggle = card.querySelector('.toggle-input');
                        if (toggle) toggle.checked = true;
                        var badge = card.querySelector('.provider-default-badge');
                        if (badge) badge.style.display = 'inline-flex';
                    }
                    setTimeout(function() {
                        closeChannelModal();
                        btn.textContent = originalText;
                        btn.disabled = false;
                    }, 1000);
                } else {
                    btn.textContent = 'Error';
                    setTimeout(function() {
                        btn.textContent = originalText;
                        btn.disabled = false;
                    }, 2000);
                }
            } catch (err) {
                btn.textContent = 'Error';
                setTimeout(function() {
                    btn.textContent = originalText;
                    btn.disabled = false;
                }, 2000);
            }
        });
    }

    // --- Test channel connection ---
    if (btnTestCh) {
        btnTestCh.addEventListener('click', async function() {
            btnTestCh.textContent = 'Testing\u2026';
            btnTestCh.disabled = true;

            var payload = { name: currentChannel };
            // Pass the current token value (if user just typed it)
            var tokenVal = document.getElementById('ch-token').value.trim();
            if (tokenVal) payload.token = tokenVal;

            try {
                var resp = await fetch('/api/v1/channels/test', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify(payload),
                });
                var data = await resp.json();
                chTestResult.textContent = (data.ok ? '\u2713 ' : '\u2717 ') + data.message;
                chTestResult.className = 'form-hint ' + (data.ok ? 'pairing-status success' : 'pairing-status error');
            } catch (err) {
                chTestResult.textContent = '\u2717 Connection failed';
                chTestResult.className = 'form-hint pairing-status error';
            }

            btnTestCh.textContent = 'Test Connection';
            btnTestCh.disabled = false;
        });
    }

    // --- WhatsApp Pairing (WebSocket) ---
    if (btnWaPair) {
        var pairingWs = null;

        btnWaPair.addEventListener('click', function() {
            var phone = document.getElementById('ch-phone').value.trim();
            if (!phone) {
                alert('Enter a phone number first.');
                return;
            }

            // Close existing connection
            if (pairingWs) {
                pairingWs.close();
                pairingWs = null;
            }

            var statusEl = document.getElementById('ch-wa-pairing-status');
            var codeEl = document.getElementById('ch-wa-pairing-code');

            chWaPairing.style.display = 'block';
            statusEl.textContent = 'Connecting\u2026';
            statusEl.className = 'pairing-status';
            codeEl.style.display = 'none';
            btnWaPair.disabled = true;
            btnWaPair.textContent = 'Pairing\u2026';

            var proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
            var ws = new WebSocket(proto + '//' + location.host + '/api/v1/channels/whatsapp/pair');
            pairingWs = ws;

            ws.onopen = function() {
                ws.send(JSON.stringify({ phone: phone }));
            };

            ws.onmessage = function(evt) {
                var msg = JSON.parse(evt.data);
                switch (msg.type) {
                    case 'pairing_code':
                        statusEl.textContent = 'Enter this code on your phone:';
                        statusEl.className = 'pairing-status';
                        codeEl.textContent = msg.code;
                        codeEl.style.display = 'block';
                        break;
                    case 'paired':
                        statusEl.textContent = 'Paired successfully!';
                        statusEl.className = 'pairing-status success';
                        codeEl.style.display = 'none';
                        break;
                    case 'connected':
                        statusEl.textContent = 'Connected! WhatsApp is ready.';
                        statusEl.className = 'pairing-status success';
                        // Update card state
                        var card = document.querySelector('.channel-card[data-channel="whatsapp"]');
                        if (card) {
                            card.classList.add('is-configured', 'is-active');
                            card.dataset.configured = 'true';
                            card.dataset.enabled = 'true';
                            var toggle = card.querySelector('.toggle-input');
                            if (toggle) toggle.checked = true;
                            var badge = card.querySelector('.provider-default-badge');
                            if (badge) badge.style.display = 'inline-flex';
                        }
                        btnWaPair.textContent = 'Start Pairing';
                        btnWaPair.disabled = false;
                        break;
                    case 'error':
                        statusEl.textContent = msg.message || 'Pairing failed';
                        statusEl.className = 'pairing-status error';
                        codeEl.style.display = 'none';
                        btnWaPair.textContent = 'Retry Pairing';
                        btnWaPair.disabled = false;
                        break;
                }
            };

            ws.onerror = function() {
                statusEl.textContent = 'Connection error. Is the gateway running?';
                statusEl.className = 'pairing-status error';
                btnWaPair.textContent = 'Retry Pairing';
                btnWaPair.disabled = false;
            };

            ws.onclose = function() {
                if (pairingWs === ws) pairingWs = null;
                if (btnWaPair.disabled) {
                    btnWaPair.textContent = 'Start Pairing';
                    btnWaPair.disabled = false;
                }
            };
        });
    }
}


// ═══ Memory Configuration Form ═══

const memoryForm = document.getElementById('memory-form');
const btnRunCleanup = document.getElementById('btn-run-cleanup');
const memoryResult = document.getElementById('memory-result');

if (memoryForm) {
    memoryForm.addEventListener('submit', async function(e) {
        e.preventDefault();
        var btn = memoryForm.querySelector('button[type="submit"]');
        var originalText = btn.textContent;
        btn.textContent = 'Saving…';
        btn.disabled = true;
        memoryResult.textContent = '';
        memoryResult.className = 'form-hint';

        var form = new FormData(memoryForm);
        var autoCleanup = form.get('auto_cleanup') === 'on';

        // Convert to strings since ConfigPatch expects String type
        var patches = [
            { key: 'memory.conversation_retention_days', value: String(form.get('conversation_retention_days')) },
            { key: 'memory.history_retention_days', value: String(form.get('history_retention_days')) },
            { key: 'memory.daily_archive_months', value: String(form.get('daily_archive_months')) },
            { key: 'memory.auto_cleanup', value: String(autoCleanup) },
        ];

        try {
            for (var i = 0; i < patches.length; i++) {
                var patch = patches[i];
                var resp = await fetch('/api/v1/config', {
                    method: 'PATCH',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify(patch),
                });
                if (!resp.ok) {
                    console.error('Config patch failed:', patch.key, resp.status);
                    throw new Error('Failed to save ' + patch.key);
                }
            }
            memoryResult.textContent = '✓ Settings saved to config.toml';
            memoryResult.className = 'form-hint pairing-status success';
            btn.textContent = 'Saved ✓';
            setTimeout(function() {
                btn.textContent = originalText;
                btn.disabled = false;
            }, 2000);
        } catch (err) {
            memoryResult.textContent = '✗ ' + (err.message || 'Failed to save settings');
            memoryResult.className = 'form-hint pairing-status error';
            btn.textContent = 'Error!';
            setTimeout(function() {
                btn.textContent = originalText;
                btn.disabled = false;
            }, 2000);
        }
    });
}

if (btnRunCleanup) {
    btnRunCleanup.addEventListener('click', async function() {
        btnRunCleanup.textContent = 'Running…';
        btnRunCleanup.disabled = true;
        memoryResult.textContent = '';
        memoryResult.className = 'form-hint';

        try {
            var resp = await fetch('/api/v1/memory/cleanup', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({}),
            });
            var data = await resp.json();

            if (data.ok) {
                memoryResult.textContent = '✓ ' + data.message;
                memoryResult.className = 'form-hint pairing-status success';
            } else {
                memoryResult.textContent = '✗ Cleanup failed';
                memoryResult.className = 'form-hint pairing-status error';
            }
        } catch (err) {
            memoryResult.textContent = '✗ Request failed';
            memoryResult.className = 'form-hint pairing-status error';
        }

        btnRunCleanup.textContent = 'Run Cleanup Now';
        btnRunCleanup.disabled = false;
    });
}

// ═══ Embeddings Configuration Form ═══

(function() {
    var embForm = document.getElementById('embeddings-form');
    var embResult = document.getElementById('embeddings-result');
    var providerSelect = document.getElementById('embedding-provider-select');
    var modelSelect = document.getElementById('embedding-model-select');
    var customRow = document.getElementById('embedding-custom-model-row');
    var customInput = document.getElementById('embedding-custom-model');
    var customHint = document.getElementById('embedding-custom-hint');
    var apiBaseInput = document.getElementById('embedding-api-base');

    if (!embForm || !window.EmbeddingLoader) return;

    var _embData = null; // Cached API response

    // ── Initialize: fetch providers and populate selects ──
    async function initEmbeddings() {
        _embData = await EmbeddingLoader.fetchModels();
        if (!_embData.ok) return;

        EmbeddingLoader.populateProviderSelect(
            providerSelect, _embData.providers, _embData.current_provider
        );
        updateModelSelect();
    }

    // ── Update model select when provider changes ──
    function updateModelSelect() {
        if (!_embData) return;
        var provName = providerSelect.value;
        var prov = EmbeddingLoader.findProvider(_embData.providers, provName);
        if (!prov) return;

        EmbeddingLoader.populateModelSelect(
            modelSelect, prov.models, _embData.current_model, prov.default_model
        );

        // Update API base placeholder
        if (apiBaseInput) apiBaseInput.placeholder = prov.default_api_base || '(provider default)';

        // Show/hide custom row
        handleModelChange();
    }

    // ── Handle model select change ──
    function handleModelChange() {
        var isCustom = modelSelect.value === '__custom__';
        customRow.style.display = isCustom ? '' : 'none';

        if (isCustom && _embData && _embData.current_model) {
            var provName = providerSelect.value;
            var prov = EmbeddingLoader.findProvider(_embData.providers, provName);
            var inList = prov && prov.models.some(function(m) { return m.id === _embData.current_model; });
            if (!inList) customInput.value = _embData.current_model;
        }

        // Check if selected model needs pull
        var selectedOpt = modelSelect.selectedOptions[0];
        var needsPull = selectedOpt && selectedOpt.dataset.needsPull === 'true';
        var isOllamaProvider = providerSelect.value === 'ollama';
        if (customHint) {
            if (needsPull) {
                customHint.textContent = 'This model will be downloaded automatically when you save.';
            } else if (isCustom && isOllamaProvider) {
                customHint.textContent = 'Model will be pulled automatically if not already downloaded.';
            } else if (isCustom) {
                customHint.textContent = '';
            } else {
                customHint.textContent = '';
            }
            customHint.style.display = (needsPull || isCustom) ? '' : 'none';
        }
    }

    /** Check if selected Ollama model needs pulling. */
    function selectedModelNeedsPull() {
        var selectedOpt = modelSelect.selectedOptions[0];
        return selectedOpt && selectedOpt.dataset.needsPull === 'true';
    }

    providerSelect.addEventListener('change', updateModelSelect);
    modelSelect.addEventListener('change', handleModelChange);

    // ── Form submit with auto-pull ──
    embForm.addEventListener('submit', async function(e) {
        e.preventDefault();
        var btn = embForm.querySelector('button[type="submit"]');
        var originalText = btn.textContent;
        btn.disabled = true;
        embResult.textContent = '';
        embResult.className = 'form-hint';

        // Resolve model: from select or custom input
        var modelValue = modelSelect.value === '__custom__'
            ? (customInput.value || '').trim()
            : (modelSelect.value || '');

        // Auto-pull if the selected Ollama model isn't downloaded yet
        // For custom models, always attempt pull (idempotent — instant if already present)
        var isOllama = providerSelect.value === 'ollama';
        var isCustomModel = modelSelect.value === '__custom__';
        var needsPull = selectedModelNeedsPull() || isCustomModel;
        if (isOllama && needsPull && modelValue) {
            btn.textContent = 'Pulling model\u2026';
            embResult.textContent = 'Downloading ' + modelValue + ' from Ollama\u2026 this may take a minute.';
            embResult.className = 'form-hint';

            var pullResult = await EmbeddingLoader.pullModel(modelValue);
            if (!pullResult.ok) {
                embResult.textContent = '\u2717 Pull failed: ' + pullResult.message;
                embResult.className = 'form-hint pairing-status error';
                btn.textContent = originalText;
                btn.disabled = false;
                return;
            }
            embResult.textContent = '\u2713 Model downloaded. Saving config\u2026';
            embResult.className = 'form-hint pairing-status success';
        }

        btn.textContent = 'Saving\u2026';
        var form = new FormData(embForm);
        var patches = [
            { key: 'memory.embedding_provider', value: providerSelect.value || 'ollama' },
            { key: 'memory.embedding_model', value: modelValue },
            { key: 'memory.embedding_api_base', value: (form.get('embedding_api_base') || '').trim() },
            { key: 'memory.embedding_api_key', value: (form.get('embedding_api_key') || '').trim() },
            { key: 'memory.embedding_dimensions', value: String(form.get('embedding_dimensions') || '384') },
        ];

        try {
            for (var i = 0; i < patches.length; i++) {
                var resp = await fetch('/api/v1/config', {
                    method: 'PATCH',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify(patches[i]),
                });
                if (!resp.ok) throw new Error('Failed to save ' + patches[i].key);
            }
            EmbeddingLoader.clearCache();
            embResult.textContent = '\u2713 Embeddings config saved. Restart gateway to apply.';
            embResult.className = 'form-hint pairing-status success';
            btn.textContent = 'Saved \u2713';
            // Refresh model list to reflect pulled state
            _embData = await EmbeddingLoader.fetchModels({ fresh: true });
            updateModelSelect();
            setTimeout(function() { btn.textContent = originalText; btn.disabled = false; }, 2000);
        } catch (err) {
            embResult.textContent = '\u2717 ' + (err.message || 'Failed to save settings');
            embResult.className = 'form-hint pairing-status error';
            btn.textContent = 'Error!';
            setTimeout(function() { btn.textContent = originalText; btn.disabled = false; }, 2000);
        }
    });

    initEmbeddings();
})();

// ═══ Embedding Index Mismatch Detection ═══

(function() {
    var warningEl = document.getElementById('embedding-index-warning');
    var unknownEl = document.getElementById('embedding-index-unknown');
    var okEl = document.getElementById('embedding-index-ok');
    var reindexRow = document.getElementById('embedding-reindex-row');
    var reindexBtn = document.getElementById('btn-reindex');
    var reindexResult = document.getElementById('reindex-result');

    if (!warningEl || !reindexBtn) return;

    /** Fetch index status and update UI. */
    async function checkStatus() {
        try {
            var resp = await fetch('/api/v1/embeddings/status');
            if (!resp.ok) return;
            var data = await resp.json();
            if (!data.ok) return;

            var hasChunks = (data.memory_chunks_in_db > 0 || data.rag_chunks_in_db > 0);

            if (data.mismatch && hasChunks) {
                // Config differs from what built the index
                warningEl.style.display = '';
                unknownEl.style.display = 'none';
                okEl.style.display = 'none';
                reindexRow.style.display = '';
            } else if (!data.memory_index && !data.rag_index && hasChunks) {
                // No meta files but chunks exist — unknown state (pre-feature index)
                warningEl.style.display = 'none';
                unknownEl.style.display = '';
                okEl.style.display = 'none';
                reindexRow.style.display = '';
            } else if (hasChunks) {
                // All good — config matches stored meta
                warningEl.style.display = 'none';
                unknownEl.style.display = 'none';
                okEl.style.display = '';
                reindexRow.style.display = 'none';
            } else {
                // No chunks at all — hide everything
                warningEl.style.display = 'none';
                unknownEl.style.display = 'none';
                okEl.style.display = 'none';
                reindexRow.style.display = 'none';
            }
        } catch (e) {
            console.warn('[Embeddings] Failed to check index status:', e);
        }
    }

    /** Handle rebuild button click. */
    reindexBtn.addEventListener('click', async function() {
        reindexBtn.disabled = true;
        reindexBtn.textContent = 'Rebuilding\u2026';
        reindexResult.textContent = 'Re-embedding all chunks with current model. This may take several minutes.';
        reindexResult.className = 'form-hint';

        try {
            var resp = await fetch('/api/v1/embeddings/reindex', { method: 'POST' });
            var data = await resp.json();
            if (data.ok) {
                reindexResult.textContent = '\u2713 ' + data.message;
                reindexResult.className = 'form-hint pairing-status success';
                // Update status display
                warningEl.style.display = 'none';
                unknownEl.style.display = 'none';
                okEl.style.display = '';
                reindexRow.style.display = 'none';
            } else {
                reindexResult.textContent = '\u2717 ' + (data.message || 'Rebuild failed');
                reindexResult.className = 'form-hint pairing-status error';
            }
        } catch (e) {
            reindexResult.textContent = '\u2717 Request failed: ' + e.message;
            reindexResult.className = 'form-hint pairing-status error';
        }

        reindexBtn.textContent = 'Rebuild Vector Indices';
        reindexBtn.disabled = false;
    });

    // Check status on page load
    checkStatus();

    // Re-check after embeddings config save (MutationObserver on result div)
    var embResult = document.getElementById('embeddings-result');
    if (embResult) {
        var observer = new MutationObserver(function() {
            if (embResult.textContent.indexOf('\u2713') >= 0) {
                setTimeout(checkStatus, 500);
            }
        });
        observer.observe(embResult, { childList: true, characterData: true, subtree: true });
    }
})();

// ─── Browser Form ─────────────────────────────────────────────────

(function() {
    console.log('[Browser] Initializing browser form handler...');
    var browserForm = document.getElementById('browser-form');
    var btnTestBrowser = document.getElementById('btn-test-browser');
    var browserResult = document.getElementById('browser-result');
    var browserEnabledToggle = document.getElementById('browser-enabled');
    var headlessToggle = document.getElementById('browser-headless');
    var actionTimeout = document.getElementById('browser-action-timeout');
    var navTimeout = document.getElementById('browser-nav-timeout');
    var snapshotLimit = document.getElementById('browser-snapshot-limit');
    var browserVisionSelect = document.getElementById('browser-vision-model');
    var browserVisionValue = document.getElementById('browser-vision-value');

    // Load vision model dropdown (reuses fetchAllModels + buildModelOptions)
    (async function loadBrowserVisionDropdown() {
        if (!browserVisionSelect) return;
        try {
            var data = await fetchAllModels();
            var fullData = allModelsIncludingHidden(data);
            buildModelOptions(browserVisionSelect, fullData, {
                specialOptions: [{ value: '', label: '(Same as chat model)' }],
                includeCustom: true,
                selectedValue: browserVisionValue ? browserVisionValue.value : (data.vision_model || ''),
            });
            browserVisionSelect.addEventListener('change', function() {
                if (browserVisionValue) browserVisionValue.value = browserVisionSelect.value;
            });
        } catch (err) {
            console.warn('[Browser] Failed to load vision models:', err);
        }
    })();

    function validateBrowserForm() {
        var ok = true;
        ok = validateNumberField(actionTimeout, 5, 300) && ok;
        ok = validateNumberField(navTimeout, 5, 300) && ok;
        ok = validateNumberField(snapshotLimit, 10, 500) && ok;
        return ok;
    }

    [actionTimeout, navTimeout, snapshotLimit].forEach(function(input) {
        if (!input) return;
        input.addEventListener('input', validateBrowserForm);
        input.addEventListener('blur', validateBrowserForm);
    });

    if (browserForm) {
        browserForm.addEventListener('submit', async function(e) {
            e.preventDefault();
            e.stopPropagation();
            if (!validateBrowserForm()) {
                browserResult.textContent = '✗ Fix validation errors before saving.';
                browserResult.className = 'form-hint pairing-status error';
                showToast('Fix validation errors before saving', 'error');
                return;
            }

            var btn = browserForm.querySelector('button[type="submit"]');
            var originalText = btn.textContent;
            btn.textContent = 'Saving…';
            btn.disabled = true;
            browserResult.textContent = '';
            browserResult.className = 'form-hint';

            var headless = headlessToggle ? headlessToggle.checked : true;
            var browserEnabled = browserEnabledToggle ? browserEnabledToggle.checked : true;
            var executablePath = document.getElementById('browser-executable');

            var patches = [
                { key: 'browser.enabled', value: String(browserEnabled) },
                { key: 'browser.headless', value: String(headless) },
                { key: 'browser.executable_path', value: executablePath ? executablePath.value : '' },
                { key: 'browser.action_timeout_secs', value: actionTimeout ? (actionTimeout.value || '10') : '10' },
                { key: 'browser.navigation_timeout_secs', value: navTimeout ? (navTimeout.value || '30') : '30' },
                { key: 'browser.snapshot_limit', value: snapshotLimit ? (snapshotLimit.value || '50') : '50' },
                { key: 'agent.vision_model', value: browserVisionSelect ? browserVisionSelect.value : '' },
            ];

            try {
                for (var i = 0; i < patches.length; i++) {
                    var patch = patches[i];
                    console.log('[Browser] Patching:', patch.key, '=', patch.value);
                    var resp = await fetch('/api/v1/config', {
                        method: 'PATCH',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify({ key: patch.key, value: patch.value }),
                    });
                    if (!resp.ok) {
                        var errData = await resp.json();
                        throw new Error(errData.error || errData.message || 'Failed to save ' + patch.key);
                    }
                }

                browserResult.textContent = '✓ Browser settings saved. Restart gateway to apply.';
                browserResult.className = 'form-hint pairing-status success';
                btn.textContent = 'Saved!';
                setTimeout(function() {
                    btn.textContent = originalText;
                    btn.disabled = false;
                }, 2000);
            } catch (err) {
                console.error('[Browser] Save error:', err);
                browserResult.textContent = '✗ ' + (err.message || 'Failed to save settings');
                browserResult.className = 'form-hint pairing-status error';
                btn.textContent = 'Error!';
                setTimeout(function() {
                    btn.textContent = originalText;
                    btn.disabled = false;
                }, 2000);
            }
        });
    }

    if (btnTestBrowser) {
        btnTestBrowser.addEventListener('click', async function(e) {
            console.log('[Browser] Test button clicked');
            e.preventDefault();
            e.stopPropagation();
            btnTestBrowser.textContent = 'Testing…';
            btnTestBrowser.disabled = true;
            browserResult.textContent = '';
            browserResult.className = 'form-hint';

            try {
                var resp = await fetch('/api/v1/browser/test', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                });
                var data = await resp.json();

                if (data.success) {
                    browserResult.textContent = '✓ ' + data.message;
                    browserResult.className = 'form-hint pairing-status success';
                } else {
                    browserResult.textContent = '✗ ' + (data.message || 'Browser test failed');
                    browserResult.className = 'form-hint pairing-status error';
                }
            } catch (err) {
                console.error('[Browser] Test error:', err);
                browserResult.textContent = '✗ Request failed: ' + err.message;
                browserResult.className = 'form-hint pairing-status error';
            }

            btnTestBrowser.textContent = 'Test Browser';
            btnTestBrowser.disabled = false;
        });
    }
    validateBrowserForm();
    console.log('[Browser] Form handler initialized');
})();

// ═══ Browser Profiles ═══

(function() {
    var container = document.getElementById('profiles-list');
    if (!container) return;

    var editingKey = null; // null = creating, string = editing

    function formatBytes(bytes) {
        if (bytes < 1024) return bytes + ' B';
        if (bytes < 1048576) return (bytes / 1024).toFixed(1) + ' KB';
        return (bytes / 1048576).toFixed(1) + ' MB';
    }

    function makeBtn(text, cls, onClick) {
        var b = document.createElement('button');
        b.className = 'btn ' + (cls || 'btn-secondary');
        b.style.cssText = 'font-size:11px;padding:3px 8px;';
        b.textContent = text;
        b.addEventListener('click', onClick);
        return b;
    }

    function buildProfileCard(p) {
        var card = document.createElement('div');
        card.className = 'provider-card';
        card.style.marginBottom = '8px';

        var row = document.createElement('div');
        row.style.cssText = 'display:flex;justify-content:space-between;align-items:center;';

        // Left: info
        var info = document.createElement('div');
        var title = document.createElement('strong');
        title.textContent = p.display_name;
        info.appendChild(title);
        if (p.is_default) {
            var badge = document.createElement('span');
            badge.className = 'status-badge success';
            badge.style.cssText = 'font-size:10px;margin-left:6px;';
            badge.textContent = 'default';
            info.appendChild(badge);
        }
        if (p.description) {
            var desc = document.createElement('div');
            desc.style.cssText = 'font-size:12px;color:var(--t3);margin-top:2px;';
            desc.textContent = p.description;
            info.appendChild(desc);
        }
        var meta = document.createElement('div');
        meta.style.cssText = 'font-size:11px;color:var(--t3);margin-top:2px;display:flex;gap:8px;align-items:center;';
        var keySpan = document.createElement('span');
        keySpan.style.opacity = '0.6';
        keySpan.textContent = p.name;
        meta.appendChild(keySpan);
        if (p.exists) {
            var sizeSpan = document.createElement('span');
            sizeSpan.className = 'pairing-status success';
            sizeSpan.style.fontSize = '11px';
            sizeSpan.textContent = formatBytes(p.size_bytes);
            meta.appendChild(sizeSpan);
        } else {
            var notCreated = document.createElement('span');
            notCreated.className = 'pairing-status';
            notCreated.style.fontSize = '11px';
            notCreated.textContent = 'Not created yet';
            meta.appendChild(notCreated);
        }
        if (p.wrong_owner_count > 0) {
            var warn = document.createElement('span');
            warn.className = 'pairing-status error';
            warn.style.fontSize = '11px';
            warn.textContent = p.wrong_owner_count + ' wrong owner';
            meta.appendChild(warn);
        }
        info.appendChild(meta);

        // Right: buttons
        var btns = document.createElement('div');
        btns.style.cssText = 'display:flex;gap:4px;flex-shrink:0;';
        if (!p.is_default) btns.appendChild(makeBtn('Set Default', 'btn-secondary', function() { setDefault(p.name); }));
        btns.appendChild(makeBtn('Edit', 'btn-secondary', function() { openEditModal(p); }));
        if (p.wrong_owner_count > 0) btns.appendChild(makeBtn('Fix', 'btn-secondary', function() { fixProfile(p.name); }));
        if (!p.is_default) {
            var del = makeBtn('Delete', 'btn-secondary', function() { deleteProfile(p.name); });
            del.style.color = 'var(--danger)';
            btns.appendChild(del);
        }
        if (p.exists) {
            var clean = makeBtn('Clean Data', 'btn-secondary', function() { cleanProfile(p.name); });
            clean.style.color = 'var(--danger)';
            btns.appendChild(clean);
        }

        row.appendChild(info);
        row.appendChild(btns);
        card.appendChild(row);
        return card;
    }

    // ─── Modal ───

    function getOrCreateModal() {
        var modal = document.getElementById('profile-modal');
        if (modal) return modal;
        modal = document.createElement('div');
        modal.id = 'profile-modal';
        modal.className = 'modal';

        var content = document.createElement('div');
        content.className = 'modal-content';
        content.style.maxWidth = '480px';

        var h3 = document.createElement('h3');
        h3.id = 'profile-modal-title';
        h3.textContent = 'Add Profile';
        content.appendChild(h3);

        var fields = [
            { id: 'pm-key', label: 'Profile Key (kebab-case)', type: 'text', placeholder: 'e.g. social-media' },
            { id: 'pm-name', label: 'Display Name', type: 'text', placeholder: 'e.g. Social Media' },
            { id: 'pm-description', label: 'Description', type: 'text', placeholder: 'What this profile is for' },
            { id: 'pm-browser-type', label: 'Browser Type', type: 'select', options: [
                ['', 'Default (inherit global)'], ['chromium', 'Chromium'], ['firefox', 'Firefox'], ['webkit', 'WebKit']
            ]},
            { id: 'pm-headless', label: 'Headless', type: 'select', options: [
                ['', 'Default (inherit global)'], ['true', 'Yes'], ['false', 'No']
            ]},
            { id: 'pm-proxy', label: 'Proxy (optional)', type: 'text', placeholder: 'http://proxy:8080' },
            { id: 'pm-user-agent', label: 'User Agent (optional)', type: 'text', placeholder: 'Custom user agent string' },
        ];

        fields.forEach(function(f) {
            var group = document.createElement('div');
            group.className = 'form-group';
            var label = document.createElement('label');
            label.textContent = f.label;
            group.appendChild(label);
            if (f.type === 'select') {
                var sel = document.createElement('select');
                sel.id = f.id;
                f.options.forEach(function(opt) {
                    var o = document.createElement('option');
                    o.value = opt[0];
                    o.textContent = opt[1];
                    sel.appendChild(o);
                });
                group.appendChild(sel);
            } else {
                var inp = document.createElement('input');
                inp.type = f.type;
                inp.id = f.id;
                inp.placeholder = f.placeholder || '';
                group.appendChild(inp);
            }
            content.appendChild(group);
        });

        var btnRow = document.createElement('div');
        btnRow.style.cssText = 'display:flex;gap:8px;justify-content:flex-end;margin-top:16px;';
        var cancelBtn = document.createElement('button');
        cancelBtn.className = 'btn btn-secondary';
        cancelBtn.textContent = 'Cancel';
        cancelBtn.addEventListener('click', closeModal);
        btnRow.appendChild(cancelBtn);
        var saveBtn = document.createElement('button');
        saveBtn.className = 'btn';
        saveBtn.textContent = 'Save';
        saveBtn.addEventListener('click', saveProfile);
        btnRow.appendChild(saveBtn);
        content.appendChild(btnRow);

        modal.appendChild(content);
        document.body.appendChild(modal);
        modal.addEventListener('click', function(e) { if (e.target === modal) closeModal(); });
        return modal;
    }

    function openAddModal() {
        editingKey = null;
        var modal = getOrCreateModal();
        modal.querySelector('#profile-modal-title').textContent = 'Add Profile';
        modal.querySelector('#pm-key').value = '';
        modal.querySelector('#pm-key').disabled = false;
        modal.querySelector('#pm-name').value = '';
        modal.querySelector('#pm-description').value = '';
        modal.querySelector('#pm-browser-type').value = '';
        modal.querySelector('#pm-headless').value = '';
        modal.querySelector('#pm-proxy').value = '';
        modal.querySelector('#pm-user-agent').value = '';
        modal.style.display = 'flex';
    }

    function openEditModal(p) {
        editingKey = p.name;
        var modal = getOrCreateModal();
        modal.querySelector('#profile-modal-title').textContent = 'Edit Profile';
        modal.querySelector('#pm-key').value = p.name;
        modal.querySelector('#pm-key').disabled = true;
        modal.querySelector('#pm-name').value = p.display_name || '';
        modal.querySelector('#pm-description').value = p.description || '';
        modal.querySelector('#pm-browser-type').value = '';
        modal.querySelector('#pm-headless').value = '';
        modal.querySelector('#pm-proxy').value = '';
        modal.querySelector('#pm-user-agent').value = '';
        modal.style.display = 'flex';
    }

    function closeModal() {
        var modal = document.getElementById('profile-modal');
        if (modal) modal.style.display = 'none';
        editingKey = null;
    }

    async function saveProfile() {
        var key = document.getElementById('pm-key').value.trim();
        var name = document.getElementById('pm-name').value.trim();
        if (!key || !name) { showToast('Key and name are required', 'error'); return; }

        var body = { name: name };
        var desc = document.getElementById('pm-description').value.trim();
        var bt = document.getElementById('pm-browser-type').value;
        var hl = document.getElementById('pm-headless').value;
        var proxy = document.getElementById('pm-proxy').value.trim();
        var ua = document.getElementById('pm-user-agent').value.trim();
        if (desc) body.description = desc;
        if (bt) body.browser_type = bt;
        if (hl) body.headless = hl === 'true';
        if (proxy) body.proxy = proxy;
        if (ua) body.user_agent = ua;

        try {
            var resp;
            if (editingKey) {
                resp = await fetch('/api/v1/browser/profiles/' + encodeURIComponent(editingKey), {
                    method: 'PUT',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify(body),
                });
            } else {
                body.key = key;
                resp = await fetch('/api/v1/browser/profiles', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify(body),
                });
            }
            var data = await resp.json();
            showToast(data.message, data.success ? 'success' : 'error');
            if (data.success) { closeModal(); loadProfiles(); }
        } catch (err) {
            showToast('Failed: ' + err.message, 'error');
        }
    }

    // ─── Actions ───

    async function loadProfiles() {
        try {
            var resp = await fetch('/api/v1/browser/profiles');
            if (!resp.ok) throw new Error('Failed to load profiles');
            var profiles = await resp.json();
            container.textContent = '';

            var addBtn = document.createElement('button');
            addBtn.className = 'btn';
            addBtn.style.cssText = 'margin-bottom:12px;font-size:13px;';
            addBtn.textContent = '+ Add Profile';
            addBtn.addEventListener('click', openAddModal);
            container.appendChild(addBtn);

            if (profiles.length === 0) {
                var empty = document.createElement('div');
                empty.style.cssText = 'color:var(--t3);padding:16px 0;';
                empty.textContent = 'No profiles configured.';
                container.appendChild(empty);
                return;
            }
            profiles.forEach(function(p) { container.appendChild(buildProfileCard(p)); });
        } catch (err) {
            container.textContent = err.message;
        }
    }

    async function setDefault(name) {
        try {
            var resp = await fetch('/api/v1/browser/profiles/' + encodeURIComponent(name) + '/set-default', { method: 'POST' });
            var data = await resp.json();
            showToast(data.message, data.success ? 'success' : 'error');
            loadProfiles();
        } catch (err) { showToast('Failed: ' + err.message, 'error'); }
    }

    async function deleteProfile(name) {
        if (!confirm('Delete profile "' + name + '" and all its data?')) return;
        try {
            var resp = await fetch('/api/v1/browser/profiles/' + encodeURIComponent(name) + '/delete', { method: 'POST' });
            var data = await resp.json();
            showToast(data.message, data.success ? 'success' : 'error');
            loadProfiles();
        } catch (err) { showToast('Failed: ' + err.message, 'error'); }
    }

    async function fixProfile(name) {
        if (!confirm('Fix file ownership for profile "' + name + '"?')) return;
        try {
            var resp = await fetch('/api/v1/browser/profiles/' + encodeURIComponent(name) + '/fix-permissions', { method: 'POST' });
            var data = await resp.json();
            showToast(data.message, data.success ? 'success' : 'error');
            loadProfiles();
        } catch (err) { showToast('Failed: ' + err.message, 'error'); }
    }

    async function cleanProfile(name) {
        if (!confirm('Delete ALL data for profile "' + name + '"? Cookies, sessions, and cache will be lost.')) return;
        try {
            var resp = await fetch('/api/v1/browser/profiles/' + encodeURIComponent(name), { method: 'DELETE' });
            var data = await resp.json();
            showToast(data.message, data.success ? 'success' : 'error');
            loadProfiles();
        } catch (err) { showToast('Failed: ' + err.message, 'error'); }
    }

    loadProfiles();
})();

// ═══ Web Search Form ═══

(function() {
    var searchForm = document.getElementById('web-search-form');
    if (!searchForm) return;

    searchForm.addEventListener('submit', async function(e) {
        e.preventDefault();
        var btn = searchForm.querySelector('button[type="submit"]');
        var originalText = btn.textContent;
        btn.textContent = 'Saving…';
        btn.disabled = true;

        var provider = document.getElementById('search-provider').value;
        var apiKey = document.getElementById('search-api-key').value;
        var maxResults = document.getElementById('search-max-results').value;

        try {
            var responses = await Promise.all([
                fetch('/api/v1/config', {
                    method: 'PATCH',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ key: 'tools.web_search.provider', value: provider }),
                }),
                fetch('/api/v1/config', {
                    method: 'PATCH',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ key: 'tools.web_search.api_key', value: apiKey }),
                }),
                fetch('/api/v1/config', {
                    method: 'PATCH',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ key: 'tools.web_search.max_results', value: parseInt(maxResults, 10) }),
                }),
            ]);

            if (responses.every(function(resp) { return resp.ok; })) {
                btn.textContent = 'Saved!';
            } else {
                throw new Error('Failed to save');
            }
        } catch (err) {
            console.error('[WebSearch] Save error:', err);
            btn.textContent = 'Error!';
        }

        setTimeout(function() {
            btn.textContent = originalText;
            btn.disabled = false;
        }, 1500);
    });

    console.log('[WebSearch] Form handler initialized');
})();

console.log('[Setup] Script loaded completely');
focusModelSettingsFromQuery();
