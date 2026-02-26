// Homun — Settings: agent form, provider toggles, model dropdown

// Global error handler for debugging
window.onerror = function(msg, url, line, col, error) {
    console.error('[Global Error]', msg, 'at', url, ':', line, ':', col, error);
    return false;
};

console.log('[Setup] Script loading...');

// ═══ Agent Form ═══

const agentForm = document.getElementById('agent-form');

if (agentForm) {
    agentForm.addEventListener('submit', async (e) => {
        e.preventDefault();
        const btn = agentForm.querySelector('button[type="submit"]');
        const originalText = btn.textContent;
        btn.textContent = 'Saving…';
        btn.disabled = true;

        // Sync model values from dropdown/custom into hidden inputs
        syncModelValue();
        syncVisionModelValue();

        const form = new FormData(agentForm);
        const patches = [
            { key: 'agent.model', value: form.get('model') },
            { key: 'agent.vision_model', value: form.get('vision_model') || '' },
            { key: 'agent.max_tokens', value: form.get('max_tokens') },
            { key: 'agent.temperature', value: form.get('temperature') },
            { key: 'agent.max_iterations', value: form.get('max_iterations') },
        ];

        try {
            for (const patch of patches) {
                if (patch.value !== undefined) {
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


// ═══ Model Dropdown (Native Select with Optgroups) ═══

const modelSelect = document.getElementById('model-select');
const modelValue = document.getElementById('model-value');
const visionModelSelect = document.getElementById('vision-model-select');
const visionModelValue = document.getElementById('vision-model-value');

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

/** Populate a model dropdown using native select with optgroups */
function populateModelDropdown(selectEl, valueEl, currentModel, groups) {
    // Clear existing options
    while (selectEl.firstChild) {
        selectEl.removeChild(selectEl.firstChild);
    }

    // Add "Same as chat model" option for vision dropdown
    if (selectEl.id === 'vision-model-select') {
        var sameOption = document.createElement('option');
        sameOption.value = '';
        sameOption.textContent = '(Same as chat model)';
        if (!currentModel || currentModel === '') {
            sameOption.selected = true;
        }
        selectEl.appendChild(sameOption);
    }

    var foundCurrent = false;

    // Build native select options with optgroups using safe DOM methods
    Object.keys(groups).forEach(function(provider) {
        var optgroup = document.createElement('optgroup');
        optgroup.label = providerDisplayName(provider);

        groups[provider].forEach(function(m) {
            var option = document.createElement('option');
            option.value = m.value;
            option.textContent = m.label;
            if (m.value === currentModel) {
                option.selected = true;
                foundCurrent = true;
            }
            optgroup.appendChild(option);
        });

        selectEl.appendChild(optgroup);
    });

    // Add custom option group
    var customGroup = document.createElement('optgroup');
    customGroup.label = 'Custom';
    var customOption = document.createElement('option');
    customOption.value = '__custom__';
    customOption.textContent = '✏ Custom model…';
    customGroup.appendChild(customOption);
    selectEl.appendChild(customGroup);

    // If current model wasn't found in options, add it at the top (after the "same" option for vision)
    if (currentModel && !foundCurrent) {
        var currentOpt = document.createElement('option');
        currentOpt.value = currentModel;
        currentOpt.textContent = currentModel + ' (current)';
        currentOpt.selected = true;
        if (selectEl.id === 'vision-model-select' && selectEl.firstChild) {
            selectEl.insertBefore(currentOpt, selectEl.firstChild.nextSibling);
        } else {
            selectEl.insertBefore(currentOpt, selectEl.firstChild);
        }
    }

    // Update hidden value with current model
    if (valueEl && currentModel) {
        valueEl.value = currentModel;
    }
}

/** Populate both model dropdowns using native select with optgroups */
async function loadModelDropdown() {
    if (!modelSelect) return;

    try {
        var resp = await fetch('/api/v1/providers/models');
        var data = await resp.json();

        var currentModel = data.current || '';
        var currentVisionModel = data.vision_model || '';

        // Group models by provider
        var groups = {};

        // Add static cloud models
        if (data.ok && data.models.length > 0) {
            data.models.forEach(function(m) {
                if (!groups[m.provider]) groups[m.provider] = [];
                groups[m.provider].push({ value: m.model, label: m.label });
            });
        }

        // If Ollama is configured, fetch live models
        if (data.ollama_configured) {
            try {
                var ollamaResp = await fetch('/api/v1/providers/ollama/models');
                var ollamaData = await ollamaResp.json();
                if (ollamaData.ok && ollamaData.models.length > 0) {
                    groups['ollama'] = ollamaData.models.map(function(m) {
                        return { value: 'ollama/' + m.name, label: m.name + ' (' + m.size + ')' };
                    });
                }
            } catch (_) { /* Ollama might not be running */ }
        }

        // If Ollama Cloud is configured, fetch live models
        if (data.ollama_cloud_configured) {
            try {
                var cloudResp = await fetch('/api/v1/providers/ollama-cloud/models');
                var cloudData = await cloudResp.json();
                if (cloudData.ok && cloudData.models.length > 0) {
                    groups['ollama_cloud'] = cloudData.models.map(function(m) {
                        return { value: 'ollama_cloud/' + m.id, label: m.id };
                    });
                }
            } catch (_) { /* Ollama Cloud might not be reachable */ }
        }

        // Populate chat model dropdown
        populateModelDropdown(modelSelect, modelValue, currentModel, groups);

        // Populate vision model dropdown
        if (visionModelSelect) {
            populateModelDropdown(visionModelSelect, visionModelValue, currentVisionModel, groups);
        }

    } catch (err) {
        console.error('Failed to load models:', err);
        // Clear and show error using safe DOM methods
        while (modelSelect.firstChild) {
            modelSelect.removeChild(modelSelect.firstChild);
        }
        var errorOpt = document.createElement('option');
        errorOpt.value = '';
        errorOpt.textContent = 'Error loading models';
        modelSelect.appendChild(errorOpt);

        if (visionModelSelect) {
            while (visionModelSelect.firstChild) {
                visionModelSelect.removeChild(visionModelSelect.firstChild);
            }
            var visionErrorOpt = document.createElement('option');
            visionErrorOpt.value = '';
            visionErrorOpt.textContent = 'Error loading models';
            visionModelSelect.appendChild(visionErrorOpt);
        }
    }
}

// Handle selection change for chat model
if (modelSelect) {
    modelSelect.addEventListener('change', function() {
        if (modelSelect.value === '__custom__') {
            var customModel = prompt('Enter custom model (e.g., ollama/my-model:latest):');
            if (customModel && customModel.trim()) {
                modelValue.value = customModel.trim();
            } else {
                // Reset selection
                loadModelDropdown();
                return;
            }
        } else if (modelSelect.value) {
            modelValue.value = modelSelect.value;
        }
    });
}

// Handle selection change for vision model
if (visionModelSelect) {
    visionModelSelect.addEventListener('change', function() {
        if (visionModelSelect.value === '__custom__') {
            var customModel = prompt('Enter custom vision model (e.g., ollama/llava:latest):');
            if (customModel && customModel.trim()) {
                visionModelValue.value = customModel.trim();
            } else {
                // Reset selection
                loadModelDropdown();
                return;
            }
        } else {
            // Empty string means "same as chat model"
            visionModelValue.value = visionModelSelect.value;
        }
    });
}

// Sync model value from select to hidden input
function syncModelValue() {
    if (!modelValue || !modelSelect) return;
    if (modelSelect.value && modelSelect.value !== '__custom__') {
        modelValue.value = modelSelect.value;
    }
}

// Sync vision model value from select to hidden input
function syncVisionModelValue() {
    if (!visionModelValue || !visionModelSelect) return;
    if (visionModelSelect.value !== '__custom__') {
        visionModelValue.value = visionModelSelect.value;
    }
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

    let currentProvider = null;

    // --- Click handlers for each card ---
    providerCards.forEach(card => {
        const toggle = card.querySelector('.toggle-input');
        const toggleLabel = card.querySelector('.toggle-label');

        // Click anywhere on card (except toggle) → open config modal
        card.style.cursor = 'pointer';
        card.addEventListener('click', (e) => {
            if (e.target === toggle || e.target === toggleLabel) return;
            openProviderModal(card);
        });

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
                // Update card UI
                card.classList.remove('is-configured');
                card.dataset.configured = 'false';
                card.dataset.apiKeyMask = '';
                card.dataset.apiBase = '';

                // Remove active badge if present
                const badge = card.querySelector('.provider-active-badge');
                if (badge) badge.remove();

                // Ensure toggle is unchecked
                const toggle = card.querySelector('.toggle-input');
                if (toggle) toggle.checked = false;

                showToast('Provider deactivated', 'success');
                loadModelDropdown();
            } else {
                const toggle = card.querySelector('.toggle-input');
                if (toggle) toggle.checked = true;
                showToast(data.message || 'Failed to deactivate', 'error');
            }
        } catch (err) {
            const toggle = card.querySelector('.toggle-input');
            if (toggle) toggle.checked = true;
            showToast('Failed to deactivate provider', 'error');
        }
    }

    // Simple toast notification
    function showToast(message, type) {
        // Remove existing toast
        const existing = document.querySelector('.toast-notification');
        if (existing) existing.remove();

        const toast = document.createElement('div');
        toast.className = 'toast-notification toast-' + (type || 'info');
        toast.textContent = message;
        document.body.appendChild(toast);

        // Trigger animation
        setTimeout(() => toast.classList.add('show'), 10);

        // Auto-remove after 4 seconds
        setTimeout(() => {
            toast.classList.remove('show');
            setTimeout(() => toast.remove(), 300);
        }, 4000);
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
            baseHint.textContent = 'Ollama Cloud API endpoint (default: https://ollama.com)';
            document.getElementById('api-base').placeholder = 'https://ollama.com';
        } else if (currentProvider === 'vllm' || currentProvider === 'custom') {
            baseHint.textContent = 'API endpoint URL (required)';
        } else {
            baseHint.textContent = 'Custom API endpoint (optional)';
        }

        modal.classList.add('open');
        document.body.style.overflow = 'hidden';
    }

    function closeModal() {
        modal.classList.remove('open');
        document.body.style.overflow = '';
        currentProvider = null;
    }

    // --- Save provider configuration ---
    if (providerForm) {
        providerForm.addEventListener('submit', async (e) => {
            e.preventDefault();
            const formData = new FormData(providerForm);
            const btn = providerForm.querySelector('button[type="submit"]');
            const originalText = btn.textContent;

            const providerName = formData.get('provider');
            const apiKey = (formData.get('api_key') || '').trim();
            const apiBaseVal = (formData.get('api_base') || '').trim();

            // Validation: check required fields
            const needsApiKey = apiKeyGroup.style.display !== 'none';
            const needsBaseUrl = apiBaseGroup.style.display !== 'none' &&
                (providerName === 'vllm' || providerName === 'custom');

            if (needsApiKey && !apiKey) {
                showToast('API key is required', 'error');
                return;
            }
            if (needsBaseUrl && !apiBaseVal) {
                showToast('Base URL is required for this provider', 'error');
                return;
            }

            btn.textContent = 'Saving…';
            btn.disabled = true;

            const payload = { name: providerName };

            if (needsApiKey) {
                payload.api_key = apiKey;
            }
            if (apiBaseGroup.style.display !== 'none') {
                payload.api_base = apiBaseVal;
            }

            try {
                const resp = await fetch('/api/v1/providers/configure', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify(payload),
                });
                const data = await resp.json();

                if (data.ok) {
                    // Update card UI
                    const card = document.querySelector('.provider-card[data-provider="' + providerName + '"]');
                    if (card) {
                        card.classList.add('is-configured');
                        card.dataset.configured = 'true';
                        const toggle = card.querySelector('.toggle-input');
                        if (toggle) toggle.checked = true;
                        if (payload.api_key) card.dataset.apiKeyMask = '••••••••';
                        if (payload.api_base) card.dataset.apiBase = payload.api_base;
                    }

                    // Close modal, show toast, reload models
                    closeModal();
                    showToast('Provider configured!', 'success');
                    loadModelDropdown();
                } else {
                    showToast(data.message || 'Failed to save configuration', 'error');
                    btn.textContent = originalText;
                    btn.disabled = false;
                }
            } catch (err) {
                showToast('Failed to save configuration', 'error');
                btn.textContent = originalText;
                btn.disabled = false;
            }
        });
    }
}


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
    var enabledToggle = document.getElementById('browser-enabled');
    var headlessToggle = document.getElementById('browser-headless');

    console.log('[Browser] Form:', browserForm, 'Enabled:', enabledToggle, 'Headless:', headlessToggle);

    if (browserForm) {
        browserForm.addEventListener('submit', async function(e) {
            console.log('[Browser] Form submit triggered');
            e.preventDefault();
            e.stopPropagation();

            var btn = browserForm.querySelector('button[type="submit"]');
            var originalText = btn.textContent;
            btn.textContent = 'Saving…';
            btn.disabled = true;
            browserResult.textContent = '';
            browserResult.className = 'form-hint';

            // Get values from inputs (toggles are outside form)
            var enabled = enabledToggle ? enabledToggle.checked : false;
            var headless = headlessToggle ? headlessToggle.checked : true;
            var browserType = document.getElementById('browser-type');
            var actionTimeout = document.getElementById('browser-action-timeout');

            console.log('[Browser] Saving:', {
                enabled: enabled,
                headless: headless,
                browserType: browserType ? browserType.value : 'chromium',
                actionTimeout: actionTimeout ? actionTimeout.value : '10'
            });

            var patches = [
                { key: 'browser.enabled', value: String(enabled) },
                { key: 'browser.headless', value: String(headless) },
                { key: 'browser.browser_type', value: browserType ? (browserType.value || 'chromium') : 'chromium' },
                { key: 'browser.action_timeout_secs', value: actionTimeout ? (actionTimeout.value || '10') : '10' },
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
