// Homun — Settings: unified provider accordion, model selection, agent config

// Global error handler for debugging
window.onerror = function(msg, url, line, col, error) {
    console.error('[Global Error]', msg, 'at', url, ':', line, ':', col, error);
    return false;
};

// ═══ Utilities ═══

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
        setTimeout(function() { toast.remove(); }, 300);
    }, 4000);
}

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

        if (!modelName.startsWith(provider + '/')) {
            modelName = provider + '/' + modelName;
        }

        patchConfig('agent.model', modelName).then(function() {
            updateActiveBanner(modelName);
            input.value = '';
            showToast('Model set to ' + stripPrefix(modelName), 'success');
            addModelRadio(card, modelName, true);
        }).catch(function() {
            showToast('Failed to set model', 'error');
        });
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
    });
})();


// ═══ Load Models for a Provider Accordion Item ═══

var _allModelsCache = null;

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

function toggleModelOverrides(wrapper, modelId, overrides) {
    var existing = wrapper.querySelector('.model-overrides-form');
    if (existing) {
        existing.remove();
        return;
    }

    var current = (overrides && overrides[modelId]) || {};

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

    // Buttons row
    var btnRow = document.createElement('div');
    btnRow.className = 'override-buttons';

    var saveBtn = document.createElement('button');
    saveBtn.type = 'button';
    saveBtn.className = 'btn btn-xs btn-primary';
    saveBtn.textContent = 'Save';
    saveBtn.addEventListener('click', function() {
        saveModelOverrides(modelId, tempInput.value, tokInput.value, form);
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

function saveModelOverrides(modelId, tempVal, tokensVal, formEl) {
    var data = _allModelsCache || {};
    var allOverrides = Object.assign({}, data.model_overrides || {});

    var entry = {};
    if (tempVal !== '') entry.temperature = parseFloat(tempVal);
    if (tokensVal !== '') entry.max_tokens = parseInt(tokensVal, 10);

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
    agentForm.addEventListener('submit', async function(e) {
        e.preventDefault();
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
    const chGuide = document.getElementById('channel-guide');
    const chTokenGroup = document.getElementById('ch-token-group');
    const chPhoneGroup = document.getElementById('ch-phone-group');
    const chAllowGroup = document.getElementById('ch-allow-from-group');
    const chDiscordGroup = document.getElementById('ch-discord-channel-group');
    const chSlackGroup = document.getElementById('ch-slack-channel-group');
    const chEmailImapGroup = document.getElementById('ch-email-imap-group');
    const chEmailSmtpGroup = document.getElementById('ch-email-smtp-group');
    const chEmailCredsGroup = document.getElementById('ch-email-credentials-group');
    const chWebHostGroup = document.getElementById('ch-web-host-group');
    const chWebPortGroup = document.getElementById('ch-web-port-group');
    const chWaPairing = document.getElementById('ch-wa-pairing');
    const btnWaPair = document.getElementById('btn-wa-pair');
    const btnTestCh = document.getElementById('btn-test-channel');
    const btnChSave = document.getElementById('btn-ch-save');
    const chTestResult = document.getElementById('ch-test-result');

    let currentChannel = null;

    // --- Channel guides (plain text, built with safe DOM) ---
    const GUIDES = {
        telegram: [
            'Open Telegram and search for @BotFather',
            'Send /newbot and follow the instructions',
            'Copy the bot token and paste it below',
            'To find your User ID, search @userinfobot on Telegram',
        ],
        discord: [
            'Go to discord.com/developers/applications',
            'Create a new Application \u2192 Bot \u2192 Reset Token',
            'Copy the token and paste it below',
            'Enable Privileged Gateway Intents (Message Content)',
            'Generate invite URL (OAuth2 \u2192 bot scope \u2192 Send Messages)',
        ],
        slack: [
            'Go to api.slack.com/apps and create a new app',
            'Go to OAuth & Permissions \u2192 Bot Token Scopes',
            'Add scopes: chat:write, channels:history, groups:history',
            'Install app to workspace and copy Bot User OAuth Token',
            'Channel ID is optional \u2014 leave empty for auto-discovery',
        ],
        whatsapp: [
            'Enter your phone number (international format without +)',
            'Click "Start Pairing" \u2014 you will receive an 8-digit code',
            'Open WhatsApp \u2192 Linked Devices \u2192 Link a Device',
            'Select "Link with phone number" and enter the code',
        ],
        email: [
            'Enter IMAP server (e.g., imap.gmail.com)',
            'Enter SMTP server (e.g., smtp.gmail.com)',
            'For Gmail: enable 2FA and create an App Password',
            'Enter your email address as username',
            'Paste the App Password (not your regular password)',
            'Add allowed senders (emails or domains like @company.com)',
        ],
        web: [],
    };

    function buildGuide(channelName) {
        clearChildren(chGuide);
        const steps = GUIDES[channelName] || [];
        if (steps.length === 0) {
            chGuide.style.display = 'none';
            return;
        }
        chGuide.style.display = 'block';
        var title = document.createElement('strong');
        title.textContent = 'Setup guide:';
        chGuide.appendChild(title);

        var ol = document.createElement('ol');
        steps.forEach(function(step) {
            var li = document.createElement('li');
            li.textContent = step;
            ol.appendChild(li);
        });
        chGuide.appendChild(ol);
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

        // Build guide
        buildGuide(currentChannel);

        // Show/hide form groups based on channel type
        if (chTokenGroup) chTokenGroup.style.display = hasToken ? 'block' : 'none';
        if (chPhoneGroup) chPhoneGroup.style.display = currentChannel === 'whatsapp' ? 'block' : 'none';
        if (chAllowGroup) chAllowGroup.style.display = (currentChannel !== 'web') ? 'block' : 'none';
        if (chDiscordGroup) chDiscordGroup.style.display = currentChannel === 'discord' ? 'block' : 'none';
        if (chSlackGroup) chSlackGroup.style.display = currentChannel === 'slack' ? 'block' : 'none';
        // Email fields
        if (chEmailImapGroup) chEmailImapGroup.style.display = currentChannel === 'email' ? 'block' : 'none';
        if (chEmailSmtpGroup) chEmailSmtpGroup.style.display = currentChannel === 'email' ? 'block' : 'none';
        if (chEmailCredsGroup) chEmailCredsGroup.style.display = currentChannel === 'email' ? 'block' : 'none';
        if (chWebHostGroup) chWebHostGroup.style.display = isWeb ? 'block' : 'none';
        if (chWebPortGroup) chWebPortGroup.style.display = isWeb ? 'block' : 'none';
        if (chWaPairing) chWaPairing.style.display = 'none';
        if (btnWaPair) btnWaPair.style.display = currentChannel === 'whatsapp' ? 'inline-flex' : 'none';
        if (btnTestCh) btnTestCh.style.display = isWeb ? 'none' : 'inline-flex';
        if (btnChSave) btnChSave.style.display = isWeb ? 'none' : 'inline-flex';

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
            // Email fields
            if (chEmailImapGroup && chEmailImapGroup.style.display !== 'none') {
                var imapHost = document.getElementById('ch-email-imap-host');
                var imapPort = document.getElementById('ch-email-imap-port');
                if (imapHost) payload.imap_host = imapHost.value.trim();
                if (imapPort && imapPort.value) payload.imap_port = parseInt(imapPort.value, 10);
                var imapFolder = document.getElementById('ch-email-imap-folder');
                if (imapFolder && imapFolder.value) payload.imap_folder = imapFolder.value.trim();
            }
            if (chEmailSmtpGroup && chEmailSmtpGroup.style.display !== 'none') {
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

// ─── Browser Form ─────────────────────────────────────────────────

(function() {
    console.log('[Browser] Initializing browser form handler...');
    var browserForm = document.getElementById('browser-form');
    var btnTestBrowser = document.getElementById('btn-test-browser');
    var browserResult = document.getElementById('browser-result');
    var headlessToggle = document.getElementById('browser-headless');

    if (browserForm) {
        browserForm.addEventListener('submit', async function(e) {
            e.preventDefault();
            e.stopPropagation();

            var btn = browserForm.querySelector('button[type="submit"]');
            var originalText = btn.textContent;
            btn.textContent = 'Saving…';
            btn.disabled = true;
            browserResult.textContent = '';
            browserResult.className = 'form-hint';

            var headless = headlessToggle ? headlessToggle.checked : true;
            var executablePath = document.getElementById('browser-executable');
            var actionTimeout = document.getElementById('browser-action-timeout');
            var navTimeout = document.getElementById('browser-nav-timeout');
            var snapshotLimit = document.getElementById('browser-snapshot-limit');

            var patches = [
                { key: 'browser.headless', value: String(headless) },
                { key: 'browser.executable_path', value: executablePath ? executablePath.value : '' },
                { key: 'browser.action_timeout_secs', value: actionTimeout ? (actionTimeout.value || '10') : '10' },
                { key: 'browser.navigation_timeout_secs', value: navTimeout ? (navTimeout.value || '30') : '30' },
                { key: 'browser.snapshot_limit', value: snapshotLimit ? (snapshotLimit.value || '50') : '50' },
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
    console.log('[Browser] Form handler initialized');
})();

// ═══ Appearance Form ═══

(function() {
    var appearanceForm = document.getElementById('appearance-form');
    var themeSelect = document.getElementById('theme-select');

    if (appearanceForm) {
        appearanceForm.addEventListener('submit', async function(e) {
            e.preventDefault();
            var btn = appearanceForm.querySelector('button[type="submit"]');
            var originalText = btn.textContent;
            btn.textContent = 'Saving…';
            btn.disabled = true;

            var theme = themeSelect ? themeSelect.value : 'system';

            try {
                var resp = await fetch('/api/v1/config', {
                    method: 'PATCH',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ key: 'ui.theme', value: theme }),
                });

                if (resp.ok) {
                    // Apply theme immediately
                    applyTheme(theme);
                    btn.textContent = 'Saved!';
                    setTimeout(function() {
                        btn.textContent = originalText;
                        btn.disabled = false;
                    }, 1500);
                } else {
                    throw new Error('Failed to save theme');
                }
            } catch (err) {
                console.error('[Appearance] Save error:', err);
                btn.textContent = 'Error!';
                setTimeout(function() {
                    btn.textContent = originalText;
                    btn.disabled = false;
                }, 1500);
            }
        });
    }

    // Apply theme on page load
    function applyTheme(theme) {
        // Save to localStorage for persistence across pages
        localStorage.setItem('homun-theme', theme);

        // Remove any existing theme classes
        document.documentElement.classList.remove('dark');

        if (theme === 'system') {
            var prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
            if (prefersDark) {
                document.documentElement.classList.add('dark');
            }
        } else if (theme === 'dark') {
            document.documentElement.classList.add('dark');
        }
    }

    // Load and apply saved theme on startup
    if (themeSelect) {
        applyTheme(themeSelect.value);

        // Listen for system theme changes when in 'system' mode
        window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', function(e) {
            if (themeSelect.value === 'system') {
                applyTheme('system');
            }
        });
    }

    console.log('[Appearance] Form handler initialized');
})();

console.log('[Setup] Script loaded completely');
