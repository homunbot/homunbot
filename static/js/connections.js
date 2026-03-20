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
        google: '<svg viewBox="0 0 24 24" fill="currentColor"><path d="M12.48 10.92v3.28h7.84c-.24 1.84-.853 3.187-1.787 4.133-1.147 1.147-2.933 2.4-6.053 2.4-4.827 0-8.6-3.893-8.6-8.72s3.773-8.72 8.6-8.72c2.6 0 4.507 1.027 5.907 2.347l2.307-2.307C18.747 1.44 16.133 0 12.48 0 5.867 0 .307 5.387.307 12s5.56 12 12.173 12c3.573 0 6.267-1.173 8.373-3.36 2.16-2.16 2.84-5.213 2.84-7.667 0-.76-.053-1.467-.173-2.053H12.48z"/></svg>',
        'google-maps': '<svg viewBox="0 0 24 24" fill="currentColor"><path d="M12 2C8.13 2 5 5.13 5 9c0 5.25 7 13 7 13s7-7.75 7-13c0-3.87-3.13-7-7-7zm0 9.5c-1.38 0-2.5-1.12-2.5-2.5s1.12-2.5 2.5-2.5 2.5 1.12 2.5 2.5-1.12 2.5-2.5 2.5z"/></svg>',
        gitlab: '<svg viewBox="0 0 24 24" fill="currentColor"><path d="M23.955 13.587l-1.342-4.135-2.664-8.189a.455.455 0 0 0-.867 0L16.418 9.45H7.582L4.918 1.263a.455.455 0 0 0-.867 0L1.386 9.452.044 13.587a.924.924 0 0 0 .331 1.023L12 23.054l11.625-8.443a.92.92 0 0 0 .33-1.024"/></svg>',
        linear: '<svg viewBox="0 0 24 24" fill="currentColor"><path d="M3.357 16.643a10.2 10.2 0 0 1-.899-1.604l6.468-6.469a.5.5 0 0 1 .707 0l1.797 1.797a.5.5 0 0 0 .707 0l5.283-5.283a10.2 10.2 0 0 1 1.604.899L11.37 13.54a.5.5 0 0 1-.707 0l-1.797-1.797a.5.5 0 0 0-.707 0l-4.802 4.9zm-1.4-3.974A9.959 9.959 0 0 1 2 12C2 6.477 6.477 2 12 2c.91 0 1.795.122 2.634.35l-4.04 4.04a.5.5 0 0 0 0 .706l1.797 1.798a.5.5 0 0 1 0 .707L7.109 14.88a.5.5 0 0 1-.707 0l-1.797-1.797a.5.5 0 0 0-.707 0l-1.94 1.587zM12 22C6.477 22 2 17.523 2 12c0-.177.005-.353.014-.528l3.668-3.668a.5.5 0 0 1 .707 0l1.797 1.797a.5.5 0 0 0 .707 0L14.18 4.31c.23.077.456.163.678.257l-6.47 6.47a.5.5 0 0 1-.706 0L5.884 9.24a.5.5 0 0 0-.707 0L2.483 11.93C2.926 17.49 7.438 22 12 22z"/></svg>',
        jira: '<svg viewBox="0 0 24 24" fill="currentColor"><path d="M11.571 11.513H0a5.218 5.218 0 0 0 5.232 5.215h2.13v2.057A5.215 5.215 0 0 0 12.575 24V12.518a1.005 1.005 0 0 0-1.005-1.005zm5.723-5.756H5.736a5.215 5.215 0 0 0 5.215 5.214h2.129v2.058a5.218 5.218 0 0 0 5.215 5.214V6.758a1.001 1.001 0 0 0-1.001-1.001zM23 .262H11.443a5.215 5.215 0 0 0 5.214 5.215h2.129v2.057A5.215 5.215 0 0 0 24 12.749V1.263A1.001 1.001 0 0 0 23 .262z"/></svg>',
        reddit: '<svg viewBox="0 0 24 24" fill="currentColor"><path d="M12 0A12 12 0 0 0 0 12a12 12 0 0 0 12 12 12 12 0 0 0 12-12A12 12 0 0 0 12 0zm5.01 4.744c.688 0 1.25.561 1.25 1.249a1.25 1.25 0 0 1-2.498.056l-2.597-.547-.8 3.747c1.824.07 3.48.632 4.674 1.488.308-.309.73-.491 1.207-.491.968 0 1.754.786 1.754 1.754 0 .716-.435 1.333-1.052 1.598.047.28.07.564.07.852 0 3.073-3.407 5.568-7.608 5.568S3.8 17.525 3.8 14.452c0-.292.025-.581.073-.864a1.749 1.749 0 0 1-1.023-1.588c0-.968.786-1.754 1.754-1.754.463 0 .898.196 1.207.49 1.207-.883 2.878-1.43 4.744-1.487l.885-4.182a.342.342 0 0 1 .14-.197.35.35 0 0 1 .238-.042l2.906.617a1.214 1.214 0 0 1 1.108-.701zM9.25 12C8.561 12 8 12.562 8 13.25c0 .687.561 1.248 1.25 1.248.687 0 1.248-.561 1.248-1.249 0-.688-.561-1.249-1.249-1.249zm5.5 0c-.687 0-1.248.561-1.248 1.25 0 .687.561 1.248 1.249 1.248.688 0 1.249-.561 1.249-1.249 0-.687-.562-1.249-1.25-1.249zm-5.466 3.99a.327.327 0 0 0-.231.094.33.33 0 0 0 0 .463c.842.842 2.484.913 2.961.913.477 0 2.105-.056 2.961-.913a.361.361 0 0 0 0-.463.327.327 0 0 0-.464 0c-.547.533-1.684.73-2.512.73-.828 0-1.979-.196-2.512-.73a.326.326 0 0 0-.232-.095z"/></svg>',
        brave: '<svg viewBox="0 0 24 24" fill="currentColor"><path d="M12 0L3.6 4.8v9.6L12 24l8.4-9.6V4.8L12 0zm5.4 13.2L12 19.8l-5.4-6.6V6.6L12 3l5.4 3.6v6.6z"/></svg>',
        stripe: '<svg viewBox="0 0 24 24" fill="currentColor"><path d="M13.976 9.15c-2.172-.806-3.356-1.426-3.356-2.409 0-.831.683-1.305 1.901-1.305 2.227 0 4.515.858 6.09 1.631l.89-5.494C18.252.975 15.697 0 12.165 0 9.667 0 7.589.654 6.104 1.872 4.56 3.147 3.757 4.992 3.757 7.218c0 4.039 2.467 5.76 6.476 7.219 2.585.92 3.445 1.574 3.445 2.583 0 .98-.84 1.545-2.354 1.545-1.875 0-4.965-.921-6.99-2.109l-.9 5.555C5.175 22.99 8.385 24 11.714 24c2.641 0 4.843-.624 6.328-1.813 1.664-1.305 2.525-3.236 2.525-5.732 0-4.128-2.524-5.851-6.591-7.305z"/></svg>',
        spotify: '<svg viewBox="0 0 24 24" fill="currentColor"><path d="M12 0C5.4 0 0 5.4 0 12s5.4 12 12 12 12-5.4 12-12S18.66 0 12 0zm5.521 17.34c-.24.359-.66.48-1.021.24-2.82-1.74-6.36-2.101-10.561-1.141-.418.122-.779-.179-.899-.539-.12-.421.18-.78.54-.9 4.56-1.021 8.52-.6 11.64 1.32.42.18.479.659.301 1.02zm1.44-3.3c-.301.42-.841.6-1.262.3-3.239-1.98-8.159-2.58-11.939-1.38-.479.12-1.02-.12-1.14-.6-.12-.48.12-1.021.6-1.141C9.6 9.9 15 10.561 18.72 12.84c.361.181.54.78.241 1.2zm.12-3.36C15.24 8.4 8.82 8.16 5.16 9.301c-.6.179-1.2-.181-1.38-.721-.18-.601.18-1.2.72-1.381 4.26-1.26 11.28-1.02 15.721 1.621.539.3.719 1.02.419 1.56-.299.421-1.02.599-1.559.3z"/></svg>',
        twitter: '<svg viewBox="0 0 24 24" fill="currentColor"><path d="M18.244 2.25h3.308l-7.227 8.26 8.502 11.24H16.17l-5.214-6.817L4.99 21.75H1.68l7.73-8.835L1.254 2.25H8.08l4.713 6.231zm-1.161 17.52h1.833L7.084 4.126H5.117z"/></svg>',
        sentry: '<svg viewBox="0 0 24 24" fill="currentColor"><path d="M13.91 2.505c-.873-1.448-2.972-1.448-3.844 0L6.572 8.17a8.66 8.66 0 0 1 4.857 5.593h-1.963a6.773 6.773 0 0 0-3.844-4.453L3.28 13.31a4.882 4.882 0 0 1 2.878 3.193H3.036a.538.538 0 0 1-.491-.323.57.57 0 0 1 .06-.59L4.527 12.6a2.99 2.99 0 0 0-1.82-.623l-1.9 3.15c-.873 1.449.181 3.29 1.86 3.29h4.41a6.88 6.88 0 0 0-3.502-5.467l1.04-1.724a8.858 8.858 0 0 1 4.498 7.191h3.9a10.63 10.63 0 0 0-5.902-9.063l1.525-2.532c4.29 2.118 7.084 6.39 7.34 11.195h1.963c-.262-5.617-3.576-10.535-8.628-12.89l1.04-1.725c5.95 2.884 9.836 8.665 10.168 15.015h1.963C21.07 10.87 16.752 4.912 10.598 2.026z"/></svg>',
        todoist: '<svg viewBox="0 0 24 24" fill="currentColor"><path d="M21 0H3C1.35 0 0 1.35 0 3v18c0 1.65 1.35 3 3 3h18c1.65 0 3-1.35 3-3V3c0-1.65-1.35-3-3-3zM5.1 17.1l1.4-1.4c.2-.2.5-.2.7 0l1.8 1.8 5.8-5.8c.2-.2.5-.2.7 0l1.4 1.4c.2.2.2.5 0 .7l-7.2 7.2c-.2.2-.5.2-.7 0L5.1 17.8c-.2-.2-.2-.5 0-.7zm0-5l1.4-1.4c.2-.2.5-.2.7 0l1.8 1.8 5.8-5.8c.2-.2.5-.2.7 0l1.4 1.4c.2.2.2.5 0 .7l-7.2 7.2c-.2.2-.5.2-.7 0L5.1 12.8c-.2-.2-.2-.5 0-.7z"/></svg>',
        'home-assistant': '<svg viewBox="0 0 24 24" fill="currentColor"><path d="M12 2L3 9v12h6v-6h6v6h6V9l-9-7zm0 2.5L19 11v8h-2v-6H7v6H5v-8l7-6.5z"/></svg>',
        wordpress: '<svg viewBox="0 0 24 24" fill="currentColor"><path d="M12.158 12.786l-2.698 7.84c.806.236 1.657.365 2.54.365 1.047 0 2.051-.18 2.986-.51-.024-.039-.046-.08-.065-.124l-2.762-7.57zM3.009 12c0 3.56 2.07 6.634 5.068 8.093L3.788 8.341A8.943 8.943 0 0 0 3.009 12zm17.159-1.067c0-1.112-.4-1.882-.742-2.48-.456-.742-.884-1.37-.884-2.112 0-.828.627-1.6 1.513-1.6.04 0 .078.005.117.008A8.959 8.959 0 0 0 12 3.009c-3.233 0-6.077 1.656-7.749 4.17.218.006.423.01.6.01.97 0 2.478-.118 2.478-.118.5-.03.56.706.059.766 0 0-.504.059-1.065.089l3.388 10.08 2.036-6.107-1.45-3.973c-.5-.03-0.974-.089-.974-.089-.5-.03-.442-.795.059-.766 0 0 1.537.118 2.45.118.97 0 2.478-.118 2.478-.118.5-.03.56.706.06.766 0 0-.505.059-1.066.089l3.363 10.003.928-3.1c.401-1.289.707-2.213.707-3.012zM20.991 12c0 2.738-1.023 5.213-2.703 7.103A8.947 8.947 0 0 0 20.991 12zM12 22.5C6.201 22.5 1.5 17.799 1.5 12S6.201 1.5 12 1.5 22.5 6.201 22.5 12 17.799 22.5 12 22.5zm0-22C5.649.5.5 5.649.5 12S5.649 23.5 12 23.5 23.5 18.351 23.5 12 18.351.5 12 .5z"/></svg>',
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
            var deprecatedBadge = item.deprecated_notice ? '<span class="badge badge-warning" title="' + escapeHtml(item.deprecated_notice) + '">Deprecated</span>' : '';

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
                    '<div class="conn-card-badges">' + statusBadge + authBadge + deprecatedBadge + toolCount + '</div>' +
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
        var modal = document.getElementById('mcp-modal');

        if (!modalOverlay || !modalContent) return;

        modalTitle.textContent = 'Connect ' + recipe.display_name;
        if (showNameField) modalTitle.textContent += ' (' + instanceName + ')';
        modalSubtitle.textContent = recipe.subtitle;

        if (modalMeta) {
            modalMeta.textContent = '';
            modalMeta.insertAdjacentHTML('beforeend',
                '<span class="badge badge-neutral">' + escapeHtml(recipe.category) + '</span> ' +
                '<span class="badge badge-neutral">' + escapeHtml(recipe.auth_mode === 'oauth' || recipe.auth_mode === 'mcp_oauth' ? 'OAuth' : 'API Key') + '</span>'
            );
        }

        var hasGuide = recipe.setup_guide && recipe.setup_guide.trim();

        // Toggle split-pane mode
        if (modal) modal.classList.toggle('skill-modal--split', !!hasGuide);

        var modalBody = modalContent.parentElement; // .skill-modal-body

        if (hasGuide) {
            // Render guide markdown
            var guideHtml = '';
            if (typeof marked !== 'undefined') {
                guideHtml = marked.parse(recipe.setup_guide);
            } else {
                guideHtml = '<pre>' + escapeHtml(recipe.setup_guide) + '</pre>';
            }

            // Split-pane: fields left, guide right
            modalBody.style.padding = '0';
            modalContent.textContent = '';
            modalContent.style.display = 'none';
            modalBody.insertAdjacentHTML('beforeend',
                '<div class="skill-modal-split-body" id="conn-split-body">' +
                    '<div class="skill-modal-split-left">' +
                        '<form id="conn-form" class="form">' + fieldsHtml + '</form>' +
                    '</div>' +
                    '<div class="skill-modal-split-right">' +
                        '<div class="conn-guide-content">' + guideHtml + '</div>' +
                    '</div>' +
                '</div>'
            );
        } else {
            // Standard single-column
            modalContent.style.display = '';
            modalContent.textContent = '';
            modalContent.insertAdjacentHTML('beforeend',
                '<form id="conn-form" class="form">' + fieldsHtml + '</form>'
            );
        }

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
