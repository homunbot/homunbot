// Homun — Shared Model Loader
// Fetches LLM models from all configured providers and populates <select> elements.
// Used by: chat.js, automations.js, setup.js — single source of truth for model loading.

window.ModelLoader = {

    PROVIDER_NAMES: {
        anthropic: 'Anthropic',
        openai: 'OpenAI',
        gemini: 'Google Gemini',
        openrouter: 'OpenRouter',
        deepseek: 'DeepSeek',
        groq: 'Groq',
        mistral: 'Mistral',
        xai: 'xAI',
        together: 'Together',
        ollama: 'Ollama (local)',
        ollama_cloud: 'Ollama Cloud',
        fireworks: 'Fireworks',
        perplexity: 'Perplexity',
        cohere: 'Cohere',
        venice: 'Venice',
        aihubmix: 'AiHubMix',
        vllm: 'vLLM',
        custom: 'Custom',
    },

    // Cache to avoid redundant API calls within a session
    _cache: null,

    /**
     * Fetch all available LLM models grouped by provider.
     * Calls /api/v1/providers/models for static models,
     * plus Ollama local/cloud endpoints for live model lists.
     *
     * @param {Object} [opts]
     * @param {boolean} [opts.fresh] - Bypass cache and re-fetch
     * @returns {Promise<{groups: Object, raw: Object}>}
     *   groups: { providerKey: [{value, label}] }
     *   raw: original /providers/models response
     */
    async fetchGrouped(opts) {
        if (this._cache && !(opts && opts.fresh)) return this._cache;

        var res = await fetch('/api/v1/providers/models');
        var data = await res.json();

        // Group static models by provider
        var groups = {};
        (data.models || []).forEach(function(m) {
            var key = m.provider;
            if (!groups[key]) groups[key] = [];
            groups[key].push({ value: m.model, label: m.label || m.model });
        });

        // Fetch live Ollama local models
        if (data.ollama_configured) {
            try {
                var olResp = await fetch('/api/v1/providers/ollama/models');
                var olData = await olResp.json();
                if (olData.ok && Array.isArray(olData.models) && olData.models.length > 0) {
                    groups['ollama'] = olData.models.map(function(m) {
                        return {
                            value: 'ollama/' + m.name,
                            label: m.name + (m.size ? ' (' + m.size + ')' : ''),
                        };
                    });
                }
            } catch (_) { /* Ollama might not be running */ }
        }

        // Fetch live Ollama Cloud models
        if (data.ollama_cloud_configured) {
            try {
                var ocResp = await fetch('/api/v1/providers/ollama-cloud/models');
                var ocData = await ocResp.json();
                if (ocData.ok && Array.isArray(ocData.models) && ocData.models.length > 0) {
                    groups['ollama_cloud'] = ocData.models.map(function(m) {
                        return {
                            value: 'ollama_cloud/' + m.id,
                            label: m.id,
                        };
                    });
                }
            } catch (_) { /* Ollama Cloud might not be reachable */ }
        }

        this._cache = { groups: groups, raw: data };
        return this._cache;
    },

    /**
     * Populate a <select> element with optgroups from model groups.
     *
     * @param {HTMLSelectElement} selectEl - The select to populate
     * @param {Object} groups - { providerKey: [{value, label}] }
     * @param {string} currentModel - Currently selected model value
     * @param {string} [defaultText] - Text for the default empty option
     */
    populateSelect: function(selectEl, groups, currentModel, defaultText) {
        selectEl.textContent = '';

        var defOpt = document.createElement('option');
        defOpt.value = '';
        defOpt.textContent = defaultText || '-- Default model --';
        if (!currentModel) defOpt.selected = true;
        selectEl.appendChild(defOpt);

        var providerNames = this.PROVIDER_NAMES;
        for (var provider in groups) {
            if (!groups.hasOwnProperty(provider)) continue;
            var models = groups[provider];
            var optgroup = document.createElement('optgroup');
            optgroup.label = providerNames[provider] || provider;
            models.forEach(function(m) {
                var opt = document.createElement('option');
                opt.value = m.value;
                opt.textContent = m.label;
                if (currentModel === m.value) opt.selected = true;
                optgroup.appendChild(opt);
            });
            selectEl.appendChild(optgroup);
        }

        if (Object.keys(groups).length === 0) {
            defOpt.textContent = 'No models configured';
        }
    },

    /** Clear cached models (e.g. after provider config changes). */
    clearCache: function() {
        this._cache = null;
    },
};
