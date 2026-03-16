// Homun — Shared MCP Loader
// Fetches MCP servers and discovers tools via on-demand connection.
// Used by: automations.js, chat.js, mcp.js — single source of truth for MCP data.

window.McpLoader = {

    _serversCache: null,
    _toolsCache: {},  // keyed by server name

    /**
     * Fetch configured MCP servers (with caching).
     * @param {Object} [opts]
     * @param {boolean} [opts.fresh] - Bypass cache
     * @returns {Promise<Array>} Array of server objects {name, enabled, ...}
     */
    async fetchServers(opts) {
        if (this._serversCache && !(opts && opts.fresh)) return this._serversCache;
        try {
            var res = await fetch('/api/v1/mcp/servers');
            this._serversCache = await res.json();
        } catch (_) {
            this._serversCache = [];
        }
        return this._serversCache;
    },

    /**
     * Discover tools for a specific MCP server (on-demand connection).
     * Connects to the server, lists tools with parameters schema, then disconnects.
     * Results are cached per server name.
     *
     * @param {string} serverName - MCP server name
     * @param {Object} [opts]
     * @param {boolean} [opts.fresh] - Bypass cache and reconnect
     * @returns {Promise<{ok: boolean, tools: Array, error?: string}>}
     */
    async discoverTools(serverName, opts) {
        if (!serverName) return { ok: false, tools: [], error: 'No server name' };
        if (this._toolsCache[serverName] && !(opts && opts.fresh)) {
            return this._toolsCache[serverName];
        }
        try {
            var res = await fetch(
                '/api/v1/mcp/servers/' + encodeURIComponent(serverName) + '/tools'
            );
            var data = await res.json();
            this._toolsCache[serverName] = data;
            return data;
        } catch (e) {
            return { ok: false, tools: [], error: String(e) };
        }
    },

    /**
     * Get enabled server names (convenience).
     * @returns {Promise<string[]>}
     */
    async enabledServerNames() {
        var servers = await this.fetchServers();
        if (!Array.isArray(servers)) return [];
        return servers
            .filter(function(s) { return s.enabled !== false; })
            .map(function(s) { return s.name; });
    },

    /**
     * Populate a <select> with enabled MCP servers.
     *
     * @param {HTMLSelectElement} selectEl
     * @param {string} currentServer - Currently selected server name
     * @param {string} [defaultText] - Placeholder text
     */
    async populateServerSelect(selectEl, currentServer, defaultText) {
        selectEl.textContent = '';
        var loading = document.createElement('option');
        loading.value = '';
        loading.textContent = 'Loading servers...';
        loading.disabled = true;
        selectEl.appendChild(loading);

        var servers = await this.fetchServers();
        selectEl.textContent = '';

        var defOpt = document.createElement('option');
        defOpt.value = '';
        defOpt.textContent = defaultText || '-- Select MCP server --';
        if (!currentServer) defOpt.selected = true;
        selectEl.appendChild(defOpt);

        var list = Array.isArray(servers) ? servers : [];
        list.filter(function(s) { return s.enabled !== false; })
            .forEach(function(s) {
                var opt = document.createElement('option');
                opt.value = s.name;
                opt.textContent = s.name;
                if (currentServer === s.name) opt.selected = true;
                selectEl.appendChild(opt);
            });

        if (list.filter(function(s) { return s.enabled !== false; }).length === 0) {
            defOpt.textContent = 'No MCP servers configured';
        }
    },

    /**
     * Populate a <select> with tools from a discovered tool list.
     *
     * @param {HTMLSelectElement} selectEl
     * @param {Array} tools - Array of {name, description, parameters}
     * @param {string} currentTool - Currently selected tool name
     * @param {string} [defaultText] - Placeholder text
     */
    populateToolSelect: function(selectEl, tools, currentTool, defaultText) {
        selectEl.textContent = '';
        var defOpt = document.createElement('option');
        defOpt.value = '';
        defOpt.textContent = defaultText || '-- Select tool --';
        if (!currentTool) defOpt.selected = true;
        selectEl.appendChild(defOpt);

        if (!tools || tools.length === 0) {
            defOpt.textContent = 'No tools found';
            return;
        }

        tools.forEach(function(t) {
            var opt = document.createElement('option');
            opt.value = t.name;
            opt.textContent = t.name +
                (t.description ? ' \u2014 ' + t.description.substring(0, 40) : '');
            if (currentTool === t.name) opt.selected = true;
            selectEl.appendChild(opt);
        });
    },

    /** Clear all caches (e.g. after server config changes). */
    clearCache: function() {
        this._serversCache = null;
        this._toolsCache = {};
    },
};
