/**
 * agents.js — Multi-agent management page.
 *
 * Depends on model-loader.js (loadModels).
 */
(function () {
    'use strict';

    var grid = document.getElementById('agent-grid');
    var emptyState = document.getElementById('agents-empty');
    var countBadge = document.getElementById('agent-count');
    var addBtn = document.getElementById('add-agent-btn');
    var modal = document.getElementById('agent-modal');
    var modalTitle = document.getElementById('modal-title');
    var form = document.getElementById('agent-form');
    var idInput = document.getElementById('af-id');
    var modelSelect = document.getElementById('af-model');
    var classifierSelect = document.getElementById('classifier-model');
    var saveRoutingBtn = document.getElementById('save-routing-btn');

    var state = { agents: [], editingId: null };

    // ── Load agents ──────────────────────────────────────────────────

    function loadAgents() {
        fetch('/api/v1/agents')
            .then(function (r) { return r.json(); })
            .then(function (data) {
                state.agents = data;
                render();
            })
            .catch(function (e) { console.error('Failed to load agents:', e); });
    }

    function render() {
        countBadge.textContent = state.agents.length;
        if (state.agents.length <= 1) {
            emptyState.style.display = '';
            clearCards();
        } else {
            emptyState.style.display = 'none';
            clearCards();
            state.agents.forEach(function (a) { grid.appendChild(buildCard(a)); });
        }
    }

    function clearCards() {
        var cards = grid.querySelectorAll('.provider-card');
        for (var i = 0; i < cards.length; i++) grid.removeChild(cards[i]);
    }

    function buildCard(a) {
        var card = document.createElement('div');
        card.className = 'provider-card' + (a.is_default ? ' is-active' : '');

        var header = document.createElement('div');
        header.className = 'provider-card-header';
        var title = document.createElement('h3');
        title.className = 'provider-card-title';
        title.textContent = a.id;
        header.appendChild(title);
        if (a.is_default) {
            var badge = document.createElement('span');
            badge.className = 'badge badge-success';
            badge.textContent = 'default';
            header.appendChild(badge);
        }
        if (a.is_implicit) {
            var implBadge = document.createElement('span');
            implBadge.className = 'badge badge-muted';
            implBadge.textContent = 'implicit';
            header.appendChild(implBadge);
        }

        var body = document.createElement('div');
        body.className = 'provider-card-body';
        appendStat(body, 'Model', a.model || 'Inherited');
        appendStat(body, 'Tools', a.tools.length ? a.tools.join(', ') : 'All tools');
        var instrPreview = a.instructions
            ? a.instructions.substring(0, 100) + (a.instructions.length > 100 ? '...' : '')
            : 'No instructions';
        appendStat(body, 'Instructions', instrPreview);

        var actions = document.createElement('div');
        actions.className = 'provider-card-actions';
        var editBtn = document.createElement('button');
        editBtn.className = 'btn btn-sm';
        editBtn.textContent = 'Edit';
        editBtn.setAttribute('data-edit', a.id);
        actions.appendChild(editBtn);
        if (!a.is_default) {
            var delBtn = document.createElement('button');
            delBtn.className = 'btn btn-sm btn-danger';
            delBtn.textContent = 'Delete';
            delBtn.setAttribute('data-delete', a.id);
            actions.appendChild(delBtn);
        }

        card.appendChild(header);
        card.appendChild(body);
        card.appendChild(actions);
        return card;
    }

    function appendStat(parent, label, value) {
        var lbl = document.createElement('div');
        lbl.className = 'stat-label';
        if (parent.children.length > 0) lbl.style.marginTop = '8px';
        lbl.textContent = label;
        var val = document.createElement('div');
        val.className = 'stat-value';
        val.style.fontSize = 'var(--text-sm)';
        val.textContent = value;
        parent.appendChild(lbl);
        parent.appendChild(val);
    }

    // ── Modal ────────────────────────────────────────────────────────

    function openModal(agentId) {
        state.editingId = agentId || null;
        modalTitle.textContent = agentId ? 'Edit Agent' : 'New Agent';
        idInput.disabled = !!agentId;
        form.reset();

        if (agentId) {
            var a = state.agents.find(function (x) { return x.id === agentId; });
            if (a) {
                idInput.value = a.id;
                modelSelect.value = a.model || '';
                document.getElementById('af-instructions').value = a.instructions || '';
                document.getElementById('af-tools').value = (a.tools || []).join(', ');
                document.getElementById('af-concurrency').value = a.max_concurrency || 0;
            }
        }
        modal.style.display = '';
    }

    function closeModal() {
        modal.style.display = 'none';
        state.editingId = null;
    }

    function saveAgent(e) {
        e.preventDefault();
        var id = state.editingId || idInput.value.trim().toLowerCase();
        if (!id) return;

        var toolsRaw = document.getElementById('af-tools').value.trim();
        var tools = toolsRaw ? toolsRaw.split(',').map(function (t) { return t.trim(); }).filter(Boolean) : [];

        var payload = {
            model: modelSelect.value || '',
            instructions: document.getElementById('af-instructions').value,
            tools: tools,
            skills: [],
            max_concurrency: parseInt(document.getElementById('af-concurrency').value) || 0,
        };

        var method, url;
        if (state.editingId) {
            method = 'PUT';
            url = '/api/v1/agents/' + encodeURIComponent(id);
        } else {
            method = 'POST';
            url = '/api/v1/agents';
            payload.id = id;
        }

        fetch(url, {
            method: method,
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(payload),
        })
            .then(function (r) { return r.json(); })
            .then(function (res) {
                if (res.ok) {
                    closeModal();
                    loadAgents();
                } else {
                    alert(res.message || 'Failed to save');
                }
            })
            .catch(function (e) { console.error('Save failed:', e); });
    }

    function deleteAgent(id) {
        if (!confirm('Delete agent "' + id + '"? This cannot be undone.')) return;
        fetch('/api/v1/agents/' + encodeURIComponent(id), { method: 'DELETE' })
            .then(function (r) { return r.json(); })
            .then(function (res) { if (res.ok) loadAgents(); })
            .catch(function (e) { console.error('Delete failed:', e); });
    }

    // ── Routing ──────────────────────────────────────────────────────

    function loadRouting() {
        fetch('/api/v1/agents/routing')
            .then(function (r) { return r.json(); })
            .then(function (data) {
                classifierSelect.value = data.classifier_model || '';
            })
            .catch(function (e) { console.error('Failed to load routing:', e); });
    }

    function saveRouting() {
        fetch('/api/v1/agents/routing', {
            method: 'PUT',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ classifier_model: classifierSelect.value || '' }),
        })
            .then(function (r) { return r.json(); })
            .then(function (res) {
                if (res.ok) saveRoutingBtn.textContent = 'Saved!';
                setTimeout(function () { saveRoutingBtn.textContent = 'Save'; }, 1500);
            })
            .catch(function (e) { console.error('Save routing failed:', e); });
    }

    // ── Event listeners ──────────────────────────────────────────────

    addBtn.addEventListener('click', function () { openModal(null); });
    document.getElementById('modal-close').addEventListener('click', closeModal);
    document.getElementById('modal-cancel').addEventListener('click', closeModal);
    form.addEventListener('submit', saveAgent);
    saveRoutingBtn.addEventListener('click', saveRouting);

    grid.addEventListener('click', function (e) {
        var editBtn = e.target.closest('[data-edit]');
        if (editBtn) return openModal(editBtn.dataset.edit);
        var delBtn = e.target.closest('[data-delete]');
        if (delBtn) return deleteAgent(delBtn.dataset.delete);
    });

    modal.addEventListener('click', function (e) {
        if (e.target === modal) closeModal();
    });

    // ── Init ─────────────────────────────────────────────────────────

    loadAgents();
    loadRouting();

    // Populate model dropdowns via shared ModelLoader
    if (window.ModelLoader) {
        window.ModelLoader.fetchGrouped().then(function (result) {
            window.ModelLoader.populateSelect(modelSelect, result.groups, '', 'Inherit from global');
            window.ModelLoader.populateSelect(classifierSelect, result.groups, '', 'Disabled (config-only routing)');
        }).catch(function (e) {
            console.warn('Failed to load models for agent dropdowns:', e);
        });
    }
})();
