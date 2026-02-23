// Homun — Settings: agent form, provider toggles, model dropdown

// ═══ Agent Form ═══

const agentForm = document.getElementById('agent-form');

if (agentForm) {
    agentForm.addEventListener('submit', async (e) => {
        e.preventDefault();
        const btn = agentForm.querySelector('button[type="submit"]');
        const originalText = btn.textContent;
        btn.textContent = 'Saving…';
        btn.disabled = true;

        // Sync model value from dropdown/custom into hidden input
        syncModelValue();

        const form = new FormData(agentForm);
        const patches = [
            { key: 'agent.model', value: form.get('model') },
            { key: 'agent.max_tokens', value: form.get('max_tokens') },
            { key: 'agent.temperature', value: form.get('temperature') },
            { key: 'agent.max_iterations', value: form.get('max_iterations') },
        ];

        try {
            for (const patch of patches) {
                if (patch.value) {
                    await fetch('/api/v1/config', {
                        method: 'PATCH',
                        headers: { 'Content-Type': 'application/json' },
                        body: JSON.stringify(patch),
                    });
                }
            }
            btn.textContent = 'Saved ✓';
            setTimeout(() => {
                btn.textContent = originalText;
                btn.disabled = false;
            }, 2000);
        } catch (err) {
            btn.textContent = 'Error!';
            setTimeout(() => {
                btn.textContent = originalText;
                btn.disabled = false;
            }, 2000);
        }
    });
}


// ═══ Model Dropdown ═══

const modelSelect = document.getElementById('model-select');
const modelCustom = document.getElementById('model-custom');
const modelValue = document.getElementById('model-value');
const modelWrap = document.getElementById('model-select-wrap');

/** Helper: remove all children from a DOM node */
function clearChildren(el) {
    while (el.firstChild) el.removeChild(el.firstChild);
}

/** Helper: create an <option> element */
function makeOption(value, text, selected) {
    const opt = document.createElement('option');
    opt.value = value;
    opt.textContent = text;
    if (selected) opt.selected = true;
    return opt;
}

/** Sync the actual form value from whichever input is active */
function syncModelValue() {
    if (!modelValue) return;
    if (modelWrap && modelWrap.classList.contains('show-custom')) {
        modelValue.value = modelCustom ? modelCustom.value : '';
    } else {
        modelValue.value = modelSelect ? modelSelect.value : '';
    }
}

/** Populate the model <select> from /api/v1/providers/models */
async function loadModelDropdown() {
    if (!modelSelect) return;

    try {
        const resp = await fetch('/api/v1/providers/models');
        const data = await resp.json();

        clearChildren(modelSelect);

        // Group cloud models by provider
        const groups = {};
        if (data.ok && data.models.length > 0) {
            data.models.forEach(m => {
                const key = m.provider;
                if (!groups[key]) groups[key] = [];
                groups[key].push(m);
            });
        }

        // If Ollama is configured, fetch live models
        if (data.ollama_configured) {
            try {
                const ollamaResp = await fetch('/api/v1/providers/ollama/models');
                const ollamaData = await ollamaResp.json();
                if (ollamaData.ok && ollamaData.models.length > 0) {
                    groups['ollama'] = ollamaData.models.map(m => ({
                        provider: 'ollama',
                        model: 'ollama/' + m.name,
                        label: 'Ollama (local) / ' + m.name + ' (' + m.size + ')',
                    }));
                }
            } catch (_) { /* Ollama might not be running */ }
        }

        // If Ollama Cloud is configured, fetch live models
        if (data.ollama_cloud_configured) {
            try {
                const cloudResp = await fetch('/api/v1/providers/ollama-cloud/models');
                const cloudData = await cloudResp.json();
                if (cloudData.ok && cloudData.models.length > 0) {
                    groups['ollama_cloud'] = cloudData.models.map(m => ({
                        provider: 'ollama_cloud',
                        model: 'ollama_cloud/' + m.id,
                        label: 'Ollama Cloud / ' + m.id,
                    }));
                }
            } catch (_) { /* Ollama Cloud might not be reachable */ }
        }

        // If no models from any source, show placeholder
        if (Object.keys(groups).length === 0) {
            modelSelect.appendChild(makeOption('', 'No models available \u2014 configure a provider first'));
            return;
        }

        // Track whether current model was found in the list
        let currentFound = false;

        // Build optgroups
        for (const [provider, models] of Object.entries(groups)) {
            const optgroup = document.createElement('optgroup');
            optgroup.label = providerDisplayName(provider);
            models.forEach(m => {
                const isSelected = m.model === data.current;
                if (isSelected) currentFound = true;
                optgroup.appendChild(makeOption(m.model, m.label, isSelected));
            });
            modelSelect.appendChild(optgroup);
        }

        // Add "Custom model…" option at the end
        modelSelect.appendChild(makeOption('__custom__', '\u270F Custom model…'));

        // If current model is not in the list, switch to custom mode
        if (data.current && !currentFound) {
            modelWrap.classList.add('show-custom');
            modelCustom.value = data.current;
            modelValue.value = data.current;
        } else {
            modelValue.value = modelSelect.value;
        }

    } catch (err) {
        clearChildren(modelSelect);
        modelSelect.appendChild(makeOption('', 'Error loading models'));
    }
}

/** Handle select change — switch to custom input if needed */
if (modelSelect) {
    modelSelect.addEventListener('change', () => {
        if (modelSelect.value === '__custom__') {
            modelWrap.classList.add('show-custom');
            modelCustom.focus();
        } else {
            modelWrap.classList.remove('show-custom');
            syncModelValue();
        }
    });
}

if (modelCustom) {
    modelCustom.addEventListener('input', syncModelValue);
}

function providerDisplayName(name) {
    const map = {
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

// Initial load
loadModelDropdown();


// ═══ Provider Toggle Cards ═══

const providerCards = document.querySelectorAll('.provider-card:not(#channel-grid .provider-card)');
const modal = document.getElementById('provider-modal');

if (modal && providerCards.length > 0) {
    const modalBackdrop = modal.querySelector('.modal-backdrop');
    const modalClose = modal.querySelector('.modal-close');
    const modalCancel = modal.querySelector('.modal-cancel');
    const providerForm = document.getElementById('provider-config-form');
    const apiKeyGroup = document.getElementById('api-key-group');
    const apiBaseGroup = document.getElementById('api-base-group');
    const ollamaModelsGroup = document.getElementById('ollama-models-group');
    const ollamaModelSelect = document.getElementById('ollama-model-select');
    const ollamaLoading = document.getElementById('ollama-models-loading');
    const ollamaError = document.getElementById('ollama-models-error');
    const refreshOllamaBtn = document.getElementById('refresh-ollama-models');
    const btnActivate = document.getElementById('btn-activate');

    let currentProvider = null;

    // --- Click handlers for each card ---
    providerCards.forEach(card => {
        const toggle = card.querySelector('.toggle-input');
        const toggleLabel = card.querySelector('.toggle-label');
        const setDefaultLink = card.querySelector('.provider-set-default');

        // Click anywhere on card (except toggle / set-default) → open config modal
        card.style.cursor = 'pointer';
        card.addEventListener('click', (e) => {
            if (e.target === toggle || e.target === toggleLabel) return;
            if (e.target === setDefaultLink) return;
            openProviderModal(card);
        });

        // "Set default" link — quick-activate without opening modal
        if (setDefaultLink) {
            setDefaultLink.addEventListener('click', (e) => {
                e.preventDefault();
                const providerName = card.dataset.provider;
                // For providers that show models (Ollama, Ollama Cloud), need to pick a model → open modal instead
                if (card.dataset.showsModels === 'true') {
                    openProviderModal(card);
                    return;
                }
                setProviderAsDefault(providerName);
            });
        }

        // Toggle switch logic
        if (toggle) {
            toggle.addEventListener('change', (e) => {
                if (toggle.checked) {
                    // Turning ON → if not configured, open modal to configure
                    if (card.dataset.configured !== 'true') {
                        e.preventDefault();
                        toggle.checked = false;
                        openProviderModal(card);
                    }
                } else {
                    // Turning OFF → confirm then deactivate
                    const name = card.dataset.provider;
                    const displayName = card.dataset.display;
                    if (confirm('Deactivate ' + displayName + '? This will remove its stored credentials.')) {
                        deactivateProvider(name, card);
                    } else {
                        toggle.checked = true;
                    }
                }
            });
        }
    });

    // --- Deactivate provider ---
    async function deactivateProvider(name, card) {
        try {
            const resp = await fetch('/api/v1/providers/deactivate', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ name }),
            });
            const data = await resp.json();
            if (data.ok) {
                card.classList.remove('is-configured', 'is-default');
                card.dataset.configured = 'false';
                const badge = card.querySelector('.provider-default-badge');
                if (badge) badge.textContent = '';
                loadModelDropdown();
            } else {
                const toggle = card.querySelector('.toggle-input');
                if (toggle) toggle.checked = true;
                alert(data.message || 'Failed to deactivate');
            }
        } catch (err) {
            const toggle = card.querySelector('.toggle-input');
            if (toggle) toggle.checked = true;
            alert('Failed to deactivate provider');
        }
    }

    // --- Set provider as default (quick, no modal) ---
    async function setProviderAsDefault(name) {
        try {
            const resp = await fetch('/api/v1/providers/activate', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ name: name, model: null }),
            });
            const data = await resp.json();
            if (data.ok) {
                window.location.reload();
            } else {
                alert(data.message || 'Failed to set as default');
            }
        } catch (err) {
            alert('Failed to set as default');
        }
    }

    // --- Close modal handlers ---
    [modalBackdrop, modalClose, modalCancel].forEach(el => {
        if (el) el.addEventListener('click', closeModal);
    });

    document.addEventListener('keydown', (e) => {
        if (e.key === 'Escape' && modal.classList.contains('open')) {
            closeModal();
        }
    });

    function openProviderModal(card) {
        currentProvider = card.dataset.provider;
        const displayName = card.dataset.display;
        const description = card.dataset.description;
        const hasKey = card.dataset.hasKey === 'true';
        const hasUrl = card.dataset.hasUrl === 'true';
        const isOllama = card.dataset.isOllama === 'true';
        const showsModels = card.dataset.showsModels === 'true';
        const apiKeyMask = card.dataset.apiKeyMask || '';
        const apiBase = card.dataset.apiBase || '';

        document.getElementById('modal-provider-name').textContent = displayName;
        document.getElementById('modal-provider-desc').textContent = description;
        document.getElementById('modal-provider-id').value = currentProvider;

        apiKeyGroup.style.display = hasKey ? 'block' : 'none';
        document.getElementById('api-key').value = '';
        document.getElementById('api-key').placeholder = apiKeyMask ? 'Current: ' + apiKeyMask : 'sk-...';

        apiBaseGroup.style.display = hasUrl ? 'block' : 'none';
        document.getElementById('api-base').value = apiBase;

        const baseHint = document.getElementById('api-base-hint');
        if (isOllama) {
            baseHint.textContent = 'Ollama server URL (default: http://localhost:11434/v1)';
            document.getElementById('api-base').placeholder = 'http://localhost:11434/v1';
        } else if (currentProvider === 'ollama_cloud') {
            baseHint.textContent = 'Ollama Cloud API endpoint (default: https://api.ollama.ai/v1)';
            document.getElementById('api-base').placeholder = 'https://api.ollama.ai/v1';
        } else if (currentProvider === 'vllm' || currentProvider === 'custom') {
            baseHint.textContent = 'API endpoint URL (required)';
        } else {
            baseHint.textContent = 'Custom API endpoint (optional)';
        }

        ollamaModelsGroup.style.display = showsModels ? 'block' : 'none';
        if (showsModels) {
            loadOllamaModels(currentProvider);
        }

        const noConfigNeeded = isOllama || currentProvider === 'vllm' || currentProvider === 'custom';
        const isDefault = card.classList.contains('is-default');
        btnActivate.style.display = (isDefault && !noConfigNeeded) ? 'none' : 'block';
        btnActivate.dataset.provider = currentProvider;

        modal.classList.add('open');
        document.body.style.overflow = 'hidden';
    }

    function closeModal() {
        modal.classList.remove('open');
        document.body.style.overflow = '';
        currentProvider = null;
        if (ollamaModelSelect) {
            clearChildren(ollamaModelSelect);
            ollamaModelSelect.appendChild(makeOption('', 'Select a model...'));
            ollamaModelSelect.style.display = 'none';
        }
        if (ollamaLoading) ollamaLoading.style.display = 'block';
        if (ollamaError) ollamaError.style.display = 'none';
    }

    // --- Load Ollama models ---
    async function loadOllamaModels(provider) {
        ollamaLoading.style.display = 'block';
        ollamaLoading.textContent = 'Loading models...';
        ollamaError.style.display = 'none';
        ollamaModelSelect.style.display = 'none';

        // Choose endpoint based on provider
        const endpoint = provider === 'ollama_cloud'
            ? '/api/v1/providers/ollama-cloud/models'
            : '/api/v1/providers/ollama/models';

        try {
            const resp = await fetch(endpoint);
            const data = await resp.json();

            if (data.ok && data.models.length > 0) {
                clearChildren(ollamaModelSelect);
                ollamaModelSelect.appendChild(makeOption('', 'Select a model...'));

                if (provider === 'ollama_cloud') {
                    // Ollama Cloud response format
                    data.models.forEach(model => {
                        ollamaModelSelect.appendChild(
                            makeOption(model.id, model.id + ' (' + model.owned_by + ')')
                        );
                    });
                } else {
                    // Local Ollama response format
                    data.models.forEach(model => {
                        ollamaModelSelect.appendChild(
                            makeOption(model.name, model.name + ' (' + model.size + ')')
                        );
                    });
                }
                ollamaModelSelect.style.display = 'block';
                ollamaLoading.style.display = 'none';
            } else {
                ollamaLoading.style.display = 'none';
                const errorMsg = provider === 'ollama_cloud'
                    ? (data.error || 'No models found. Check your API key.')
                    : (data.error || 'No models found. Run `ollama pull llama3` to download a model.');
                ollamaError.textContent = errorMsg;
                ollamaError.style.display = 'block';
            }
        } catch (err) {
            ollamaLoading.style.display = 'none';
            ollamaError.textContent = provider === 'ollama_cloud'
                ? 'Failed to load models. Check your API key and connection.'
                : 'Failed to load models. Is Ollama running?';
            ollamaError.style.display = 'block';
        }
    }

    if (refreshOllamaBtn) {
        refreshOllamaBtn.addEventListener('click', () => {
            if (currentProvider) {
                loadOllamaModels(currentProvider);
            }
        });
    }

    // --- Save provider configuration ---
    if (providerForm) {
        providerForm.addEventListener('submit', async (e) => {
            e.preventDefault();
            const formData = new FormData(providerForm);
            const btn = providerForm.querySelector('button[type="submit"]');
            const originalText = btn.textContent;
            btn.textContent = 'Saving…';
            btn.disabled = true;

            const providerName = formData.get('provider');
            const apiKey = formData.get('api_key');
            const apiBaseVal = formData.get('api_base');

            const payload = { name: providerName };

            if (apiKeyGroup.style.display !== 'none') {
                payload.api_key = apiKey ? apiKey.trim() : '';
            }
            if (apiBaseGroup.style.display !== 'none') {
                payload.api_base = apiBaseVal ? apiBaseVal.trim() : '';
            }

            try {
                const resp = await fetch('/api/v1/providers/configure', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify(payload),
                });
                const data = await resp.json();

                if (data.ok) {
                    btn.textContent = 'Saved ✓';
                    btnActivate.style.display = 'block';

                    const card = document.querySelector('.provider-card[data-provider="' + providerName + '"]');
                    if (card) {
                        card.classList.add('is-configured');
                        card.dataset.configured = 'true';
                        const toggle = card.querySelector('.toggle-input');
                        if (toggle) toggle.checked = true;
                        if (payload.api_key) card.dataset.apiKeyMask = '\u2022\u2022\u2022\u2022\u2022\u2022\u2022\u2022';
                        if (payload.api_base) card.dataset.apiBase = payload.api_base;
                    }

                    loadModelDropdown();
                } else {
                    btn.textContent = 'Error';
                }
            } catch (err) {
                btn.textContent = 'Error';
            }

            setTimeout(() => {
                btn.textContent = originalText;
                btn.disabled = false;
            }, 2000);
        });
    }

    // --- Activate provider (make default) ---
    if (btnActivate) {
        btnActivate.addEventListener('click', async () => {
            const provider = btnActivate.dataset.provider;
            let selectedModel = null;

            if (provider === 'ollama') {
                selectedModel = ollamaModelSelect.value;
                if (!selectedModel) {
                    alert('Please select a model first.');
                    return;
                }
            }

            btnActivate.textContent = 'Activating…';
            btnActivate.disabled = true;

            try {
                const resp = await fetch('/api/v1/providers/activate', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ name: provider, model: selectedModel }),
                });
                const data = await resp.json();

                if (data.ok) {
                    closeModal();
                    window.location.reload();
                } else {
                    alert(data.message || 'Failed to activate provider');
                    btnActivate.textContent = 'Activate Provider';
                    btnActivate.disabled = false;
                }
            } catch (err) {
                alert('Failed to activate provider');
                btnActivate.textContent = 'Activate Provider';
                btnActivate.disabled = false;
            }
        });
    }
}


// ═══ Channel Configuration Cards ═══

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
        whatsapp: [
            'Enter your phone number (international format without +)',
            'Click "Start Pairing" \u2014 you will receive an 8-digit code',
            'Open WhatsApp \u2192 Linked Devices \u2192 Link a Device',
            'Select "Link with phone number" and enter the code',
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
        chTokenGroup.style.display = hasToken ? 'block' : 'none';
        chPhoneGroup.style.display = currentChannel === 'whatsapp' ? 'block' : 'none';
        chAllowGroup.style.display = (currentChannel !== 'web') ? 'block' : 'none';
        chDiscordGroup.style.display = currentChannel === 'discord' ? 'block' : 'none';
        chWebHostGroup.style.display = isWeb ? 'block' : 'none';
        chWebPortGroup.style.display = isWeb ? 'block' : 'none';
        chWaPairing.style.display = 'none';
        btnWaPair.style.display = currentChannel === 'whatsapp' ? 'inline-flex' : 'none';
        btnTestCh.style.display = isWeb ? 'none' : 'inline-flex';
        btnChSave.style.display = isWeb ? 'none' : 'inline-flex';

        // Set hints
        if (currentChannel === 'telegram') {
            document.getElementById('ch-token-hint').textContent = 'Bot token from @BotFather. Stored encrypted locally.';
            document.getElementById('ch-allow-from-hint').textContent = 'Telegram User IDs (numeric). Get yours from @userinfobot.';
        } else if (currentChannel === 'discord') {
            document.getElementById('ch-token-hint').textContent = 'Bot token from Discord Developer Portal. Stored encrypted.';
            document.getElementById('ch-allow-from-hint').textContent = 'Discord User IDs (numeric). Enable Developer Mode to copy.';
        } else if (currentChannel === 'whatsapp') {
            document.getElementById('ch-allow-from-hint').textContent = 'Phone numbers of allowed senders (e.g. 393331234567).';
        }

        // Clear fields first (defaults from card data attributes)
        document.getElementById('ch-token').value = '';
        document.getElementById('ch-token').placeholder = 'Paste token here...';
        document.getElementById('ch-phone').value = card.dataset.phone || '';
        document.getElementById('ch-allow-from').value = card.dataset.allowFrom || '';
        document.getElementById('ch-discord-channel').value = card.dataset.discordChannel || '';
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
                // Web host/port
                if (data.host) {
                    document.getElementById('ch-web-host').value = data.host;
                }
                if (data.port) {
                    document.getElementById('ch-web-port').value = data.port;
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
