// Homun — Shared Embedding Model Loader
// Fetches embedding-capable providers and their models from the API.
// Populates provider and model <select> elements dynamically.
// Used by: setup.js (Settings page Embeddings section).

window.EmbeddingLoader = {

    _cache: null,

    /**
     * Fetch embedding providers and their available models.
     * Calls GET /api/v1/providers/embedding-models.
     *
     * @param {Object} [opts]
     * @param {boolean} [opts.fresh] - Bypass cache and re-fetch
     * @returns {Promise<Object>} API response with providers, current_provider, current_model
     */
    async fetchModels(opts) {
        if (this._cache && !(opts && opts.fresh)) return this._cache;
        try {
            var res = await fetch('/api/v1/providers/embedding-models');
            this._cache = await res.json();
        } catch (_) {
            this._cache = { ok: false, providers: [], current_provider: '', current_model: '' };
        }
        return this._cache;
    },

    /**
     * Populate a <select> with configured embedding providers.
     * Only shows providers where configured=true.
     *
     * @param {HTMLSelectElement} selectEl
     * @param {Array} providers - From API response
     * @param {string} currentProvider - Currently selected provider name
     */
    populateProviderSelect: function(selectEl, providers, currentProvider) {
        selectEl.textContent = '';

        var configured = providers.filter(function(p) { return p.configured; });

        if (configured.length === 0) {
            var none = document.createElement('option');
            none.value = '';
            none.textContent = 'No embedding providers configured';
            selectEl.appendChild(none);
            return;
        }

        configured.forEach(function(p) {
            var opt = document.createElement('option');
            opt.value = p.name;
            opt.textContent = p.display_name;
            if (p.name === currentProvider || (!currentProvider && p.name === 'ollama')) {
                opt.selected = true;
            }
            selectEl.appendChild(opt);
        });
    },

    /**
     * Populate a <select> with models for a given provider.
     * Adds "(Provider default)" as first option and "Custom..." as last.
     *
     * @param {HTMLSelectElement} selectEl
     * @param {Array} models - Model list from provider info
     * @param {string} currentModel - Currently selected model id
     * @param {string} defaultModel - Provider's default model name
     */
    populateModelSelect: function(selectEl, models, currentModel, defaultModel) {
        selectEl.textContent = '';

        // Default option
        var defOpt = document.createElement('option');
        defOpt.value = '';
        defOpt.textContent = defaultModel
            ? '(Default: ' + defaultModel + ')'
            : '(Provider default)';
        if (!currentModel) defOpt.selected = true;
        selectEl.appendChild(defOpt);

        // Model options
        (models || []).forEach(function(m) {
            var opt = document.createElement('option');
            opt.value = m.id;
            opt.textContent = m.label || m.id;
            if (m.id === currentModel) opt.selected = true;
            selectEl.appendChild(opt);
        });

        // Custom option
        var customOpt = document.createElement('option');
        customOpt.value = '__custom__';
        customOpt.textContent = 'Custom model...';
        // If current model is set but not in the list, select custom
        if (currentModel && !models.some(function(m) { return m.id === currentModel; })) {
            customOpt.selected = true;
        }
        selectEl.appendChild(customOpt);
    },

    /**
     * Find a provider info object by name in the providers array.
     *
     * @param {Array} providers
     * @param {string} name
     * @returns {Object|null}
     */
    findProvider: function(providers, name) {
        return providers.find(function(p) { return p.name === name; }) || null;
    },

    /** Clear cached data (e.g. after config changes). */
    clearCache: function() {
        this._cache = null;
    },
};
