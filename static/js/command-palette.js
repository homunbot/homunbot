// Homun — Command Palette (Cmd/Ctrl+K)
// Global keyboard shortcut system + fuzzy-filtered action launcher.

(function() {
    'use strict';

    // ── Actions registry ────────────────────────────────────────────
    var actions = [
        { id: 'nav-dashboard',    label: 'Go to Dashboard',      keys: '', icon: '📊', fn: function() { go('/dashboard'); } },
        { id: 'nav-chat',         label: 'Go to Chat',           keys: '', icon: '💬', fn: function() { go('/chat'); } },
        { id: 'nav-automations',  label: 'Go to Automations',    keys: '', icon: '⚡', fn: function() { go('/automations'); } },
        { id: 'nav-workflows',    label: 'Go to Workflows',      keys: '', icon: '🔀', fn: function() { go('/workflows'); } },
        { id: 'nav-skills',       label: 'Go to Skills',         keys: '', icon: '🧩', fn: function() { go('/skills'); } },
        { id: 'nav-knowledge',    label: 'Go to Knowledge Base', keys: '', icon: '📚', fn: function() { go('/knowledge'); } },
        { id: 'nav-memory',       label: 'Go to Memory',         keys: '', icon: '🧠', fn: function() { go('/memory'); } },
        { id: 'nav-mcp',          label: 'Go to MCP Servers',    keys: '', icon: '🔌', fn: function() { go('/mcp'); } },
        { id: 'nav-vault',        label: 'Go to Vault',          keys: '', icon: '🔒', fn: function() { go('/vault'); } },
        { id: 'nav-logs',         label: 'Go to Logs',           keys: '', icon: '📋', fn: function() { go('/logs'); } },
        { id: 'nav-settings',     label: 'Go to Settings',       keys: '', icon: '⚙️', fn: function() { go('/setup'); } },
        { id: 'nav-account',      label: 'Go to Account',        keys: '', icon: '👤', fn: function() { go('/account'); } },
        { id: 'nav-browser',      label: 'Go to Browser',        keys: '', icon: '🌐', fn: function() { go('/browser'); } },
        { id: 'nav-business',     label: 'Go to Business',       keys: '', icon: '💼', fn: function() { go('/business'); } },
        { id: 'toggle-theme',     label: 'Toggle Dark/Light Mode', keys: '', icon: '🌓',
            fn: function() {
                var html = document.documentElement;
                var current = html.getAttribute('data-theme') || 'light';
                var next = current === 'dark' ? 'light' : 'dark';
                html.setAttribute('data-theme', next);
                localStorage.setItem('homun.theme', next);
            }
        },
    ];

    function go(path) { window.location.href = path; }

    // ── Palette DOM ─────────────────────────────────────────────────
    var overlay = null;
    var input = null;
    var list = null;
    var selectedIdx = 0;
    var filtered = [];

    function createPalette() {
        overlay = document.createElement('div');
        overlay.className = 'cmd-palette-overlay';
        overlay.hidden = true;
        overlay.addEventListener('click', function(e) {
            if (e.target === overlay) closePalette();
        });

        var dialog = document.createElement('div');
        dialog.className = 'cmd-palette';

        input = document.createElement('input');
        input.className = 'cmd-palette-input';
        input.type = 'text';
        input.placeholder = 'Type a command\u2026';
        input.setAttribute('autocomplete', 'off');
        input.addEventListener('input', onInput);
        input.addEventListener('keydown', onKeydown);

        list = document.createElement('ul');
        list.className = 'cmd-palette-list';

        dialog.appendChild(input);
        dialog.appendChild(list);
        overlay.appendChild(dialog);
        document.body.appendChild(overlay);
    }

    function openPalette() {
        if (!overlay) createPalette();
        overlay.hidden = false;
        input.value = '';
        selectedIdx = 0;
        renderList('');
        requestAnimationFrame(function() { input.focus(); });
    }

    function closePalette() {
        if (overlay) overlay.hidden = true;
    }

    function isOpen() { return overlay && !overlay.hidden; }

    // ── Filtering + rendering ───────────────────────────────────────

    function fuzzyMatch(query, text) {
        var q = query.toLowerCase();
        var t = text.toLowerCase();
        if (!q) return true;
        var qi = 0;
        for (var ti = 0; ti < t.length && qi < q.length; ti++) {
            if (t[ti] === q[qi]) qi++;
        }
        return qi === q.length;
    }

    function renderList(query) {
        filtered = actions.filter(function(a) { return fuzzyMatch(query, a.label); });
        selectedIdx = Math.min(selectedIdx, Math.max(filtered.length - 1, 0));

        list.textContent = '';
        if (filtered.length === 0) {
            var empty = document.createElement('li');
            empty.className = 'cmd-palette-empty';
            empty.textContent = 'No matching commands';
            list.appendChild(empty);
            return;
        }

        filtered.forEach(function(action, i) {
            var li = document.createElement('li');
            li.className = 'cmd-palette-item' + (i === selectedIdx ? ' is-selected' : '');

            var icon = document.createElement('span');
            icon.className = 'cmd-palette-icon';
            icon.textContent = action.icon || '';

            var label = document.createElement('span');
            label.className = 'cmd-palette-label';
            label.textContent = action.label;

            li.appendChild(icon);
            li.appendChild(label);

            if (action.keys) {
                var kbd = document.createElement('kbd');
                kbd.className = 'cmd-palette-kbd';
                kbd.textContent = action.keys;
                li.appendChild(kbd);
            }

            li.addEventListener('click', function() { executeAction(action); });
            li.addEventListener('mouseenter', function() {
                selectedIdx = i;
                updateSelection();
            });

            list.appendChild(li);
        });
    }

    function updateSelection() {
        var items = list.querySelectorAll('.cmd-palette-item');
        items.forEach(function(el, i) {
            el.classList.toggle('is-selected', i === selectedIdx);
        });
        // Scroll selected into view
        var sel = list.querySelector('.is-selected');
        if (sel) sel.scrollIntoView({ block: 'nearest' });
    }

    function executeAction(action) {
        closePalette();
        if (typeof action.fn === 'function') action.fn();
    }

    // ── Input handlers ──────────────────────────────────────────────

    function onInput() {
        selectedIdx = 0;
        renderList(input.value);
    }

    function onKeydown(e) {
        if (e.key === 'Escape') {
            e.preventDefault();
            closePalette();
        } else if (e.key === 'ArrowDown') {
            e.preventDefault();
            selectedIdx = Math.min(selectedIdx + 1, filtered.length - 1);
            updateSelection();
        } else if (e.key === 'ArrowUp') {
            e.preventDefault();
            selectedIdx = Math.max(selectedIdx - 1, 0);
            updateSelection();
        } else if (e.key === 'Enter') {
            e.preventDefault();
            if (filtered[selectedIdx]) executeAction(filtered[selectedIdx]);
        }
    }

    // ── Global keybinding ───────────────────────────────────────────

    document.addEventListener('keydown', function(e) {
        // Cmd/Ctrl+K → open palette
        if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
            e.preventDefault();
            if (isOpen()) {
                closePalette();
            } else {
                openPalette();
            }
            return;
        }

        // Escape → close palette if open
        if (e.key === 'Escape' && isOpen()) {
            e.preventDefault();
            closePalette();
        }
    });

    // Expose for other scripts to register custom actions
    window.homunCommandPalette = {
        register: function(action) { actions.push(action); },
        open: openPalette,
        close: closePalette,
    };
})();
