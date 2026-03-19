// Homun — Onboarding v2: 4-phase first-run with theme picker + LLM chat
// Uses standard WebUI layout (page_html), accent-utils.js for theme/accent.
// All innerHTML is from trusted i18n strings / internal state, never user input.

(function () {
    'use strict';

    var PHASES = ['welcome', 'provider', 'channels', 'meet'];
    var currentPhase = 0;
    var strings = {};
    var state = {
        completed: [],
        providerConfigured: null,
        modelSelected: '',
        ollamaDetected: false,
        ollamaModels: [],
        chatWs: null,
        chatConvId: null,
        streamingEl: null,
        streamBuf: '',
    };

    // ═══ i18n ═══

    function t(key, vars) {
        var s = strings[key] || key;
        if (vars) Object.keys(vars).forEach(function (k) { s = s.replace('{' + k + '}', vars[k]); });
        return s;
    }
    function esc(str) { var d = document.createElement('div'); d.textContent = str; return d.innerHTML; }

    async function loadStrings() {
        var lang = localStorage.getItem('homun-language') || 'system';
        if (lang === 'system') lang = (navigator.language || 'en').split('-')[0];
        if (lang !== 'en' && lang !== 'it') lang = 'en';
        try { var r = await fetch('/static/i18n/' + lang + '.json'); if (r.ok) strings = await r.json(); } catch (_) {}
    }

    function setHTML(el, html) { el.innerHTML = html; }

    // ═══ API helpers ═══

    async function patchConfig(key, value) {
        await fetch('/api/v1/config', { method: 'PATCH', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ key: key, value: value }) });
    }
    async function testProvider(name) {
        var r = await fetch('/api/v1/providers/' + encodeURIComponent(name) + '/test', { method: 'POST' });
        return r.ok;
    }
    async function fetchModels() {
        var models = [];
        try {
            var r = await fetch('/api/v1/providers/models');
            if (r.ok) {
                var d = await r.json();
                models = (d.models || []).map(function (m) { return typeof m === 'string' ? m : (m.id || m.name || ''); }).filter(Boolean);
                if (d.current && models.indexOf(d.current) < 0) models.unshift(d.current);
            }
        } catch (_) {}
        if (state.ollamaDetected && state.ollamaModels.length) {
            state.ollamaModels.forEach(function (m) { var f = 'ollama/' + m; if (models.indexOf(f) < 0) models.push(f); });
        }
        return models;
    }
    async function fetchOnboardingStatus() {
        try { var r = await fetch('/api/v1/onboarding/status'); if (r.ok) return await r.json(); } catch (_) {}
        return null;
    }
    async function completeOnboarding() {
        await fetch('/api/v1/onboarding/complete', { method: 'POST' });
    }
    async function detectOllama() {
        try {
            var r = await fetch('/api/v1/providers/ollama/models');
            if (r.ok) {
                var d = await r.json();
                state.ollamaModels = (d.models || []).map(function (m) { return typeof m === 'string' ? m : (m.name || ''); }).filter(Boolean);
                state.ollamaDetected = true;
                return;
            }
        } catch (_) {}
        state.ollamaDetected = false;
        state.ollamaModels = [];
    }

    // ═══ Provider info ═══

    var PROVIDERS = {
        anthropic: { name: 'Anthropic', color: '#D97706' },
        openai:    { name: 'OpenAI', color: '#10A37F' },
        gemini:    { name: 'Google Gemini', color: '#4285F4' },
        openrouter:{ name: 'OpenRouter', color: '#6366F1' },
        deepseek:  { name: 'DeepSeek', color: '#0EA5E9' },
        groq:      { name: 'Groq', color: '#F97316' },
        mistral:   { name: 'Mistral', color: '#FF7000' },
        xai:       { name: 'xAI', color: '#111111' },
    };

    var ACCENT_PRESETS = [
        { id: '', color: '#3B82F6', label: 'Blue' },
        { id: 'moss', color: '#B85C38', label: 'Moss' },
        { id: 'terracotta', color: '#C96D47', label: 'Terra' },
        { id: 'plum', color: '#8B5CF6', label: 'Plum' },
        { id: 'stone', color: '#78716C', label: 'Stone' },
    ];

    // ═══ Stepper ═══

    function renderStepper() {
        var el = document.getElementById('ob-stepper');
        if (!el) return;
        var html = '';
        PHASES.forEach(function (phase, i) {
            var done = state.completed.indexOf(phase) >= 0;
            var active = i === currentPhase;
            var cls = 'ob-step' + (active ? ' is-active' : '') + (done ? ' is-done' : '');
            var num = done ? '✓' : String(i + 1);
            html += '<div class="' + cls + '"><span class="ob-step-num">' + num + '</span>' +
                '<span class="ob-step-label">' + esc(t('step.' + phase)) + '</span></div>';
            if (i < PHASES.length - 1) html += '<div class="ob-step-line"></div>';
        });
        setHTML(el, html);
    }

    function updateNav() {
        var back = document.getElementById('ob-back');
        var next = document.getElementById('ob-next');
        if (!back || !next) return;
        back.style.visibility = currentPhase === 0 ? 'hidden' : 'visible';
        back.textContent = t('nav.back');
        next.textContent = currentPhase === PHASES.length - 1 ? t('nav.finish') :
            currentPhase === 0 ? t('welcome.cta') : t('nav.next');
    }

    // ═══ Phase 1: Welcome + Personalize ═══

    function renderWelcome() {
        var tz = Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC';
        var curTheme = localStorage.getItem('homun-theme') || 'system';
        var curAccent = localStorage.getItem('homun-accent') || '';

        function themeBtn(val, label) {
            return '<button class="ob-theme-btn' + (curTheme === val ? ' is-active' : '') + '" data-theme="' + val + '">' + esc(label) + '</button>';
        }

        var dots = ACCENT_PRESETS.map(function (p) {
            var active = curAccent === p.id || (!curAccent && p.id === '');
            return '<div class="ob-accent-dot' + (active ? ' is-active' : '') + '" data-accent="' + p.id + '" style="background:' + p.color + '" title="' + esc(p.label) + '"></div>';
        }).join('');

        return '<div class="ob-phase">' +
            '<div class="ob-hero"><div class="logo-icon"></div>' +
                '<h1>' + esc(t('welcome.title')) + '</h1><p>' + esc(t('welcome.subtitle')) + '</p></div>' +
            '<div class="ob-card">' +
                '<div class="ob-field"><label>' + esc(t('welcome.name.label')) + '</label>' +
                    '<input type="text" id="ob-name" class="input" placeholder="' + esc(t('welcome.name.placeholder')) + '"></div>' +
                '<div class="ob-field-row">' +
                    '<div class="ob-field"><label>' + esc(t('welcome.language.label')) + '</label>' +
                        '<select id="ob-lang" class="input"><option value="en">English</option><option value="it">Italiano</option></select></div>' +
                    '<div class="ob-field"><label>' + esc(t('welcome.timezone.label')) + '</label>' +
                        '<input type="text" id="ob-tz" class="input" value="' + esc(tz) + '"></div>' +
                '</div></div>' +
            '<div class="ob-card"><div class="ob-section-label">' + esc(t('welcome.theme')) + '</div>' +
                '<div class="ob-theme-row">' + themeBtn('system', t('welcome.theme.system')) +
                    themeBtn('light', t('welcome.theme.light')) + themeBtn('dark', t('welcome.theme.dark')) + '</div></div>' +
            '<div class="ob-card"><div class="ob-section-label">' + esc(t('welcome.accent')) + '</div>' +
                '<div class="ob-accent-row">' + dots +
                    '<input type="color" id="ob-accent-custom" class="ob-accent-dot" style="padding:0;width:28px;height:28px;border-radius:50%;cursor:pointer" title="Custom"></div></div>' +
        '</div>';
    }

    // ═══ Phase 2: Provider + Model ═══

    function renderProvider() {
        var cards = Object.keys(PROVIDERS).map(function (key) {
            var p = PROVIDERS[key], ok = state.providerConfigured === key;
            return '<button class="ob-provider-card' + (ok ? ' is-configured' : '') + '" data-provider="' + key + '">' +
                '<span class="ob-provider-dot" style="background:' + p.color + '"></span>' +
                '<span style="flex:1;text-align:left">' + esc(p.name) + '</span>' +
                (ok ? '<span class="ob-badge-ok">✓</span>' : '') + '</button>';
        }).join('');

        var ollama = '';
        if (state.ollamaDetected) {
            ollama = '<div class="ob-ollama is-detected"><div class="ob-ollama-header"><span class="ob-status-dot is-ok"></span>' +
                '<strong>' + esc(t('provider.ollama.detected')) + '</strong>' +
                '<span class="ob-muted">' + esc(t('provider.ollama.models', { n: state.ollamaModels.length })) + '</span></div>' +
                '<div class="ob-ollama-models">' + state.ollamaModels.slice(0, 6).map(function (m) {
                    return '<button class="ob-model-pill" data-ollama-model="' + esc(m) + '">' + esc(m) + '</button>';
                }).join('') + '</div></div>';
        } else {
            ollama = '<div class="ob-ollama is-missing"><span class="ob-status-dot is-off"></span>' +
                '<strong>' + esc(t('provider.ollama.not_detected')) + '</strong>' +
                '<span class="ob-muted">' + esc(t('provider.ollama.install_hint')) + '</span></div>';
        }

        return '<div class="ob-phase">' +
            '<div class="ob-hero"><h1>' + esc(t('provider.title')) + '</h1><p>' + esc(t('provider.subtitle')) + '</p></div>' +
            '<div class="ob-card"><div class="ob-section-label">' + esc(t('provider.cloud')) + '</div>' +
                '<div class="ob-provider-grid">' + cards + '</div>' +
                '<div id="ob-provider-form" style="display:none"></div></div>' +
            '<div class="ob-card"><div class="ob-section-label">' + esc(t('provider.local')) + '</div>' + ollama + '</div>' +
            '<div class="ob-card" id="ob-model-section" style="display:none">' +
                '<div class="ob-section-label">' + esc(t('provider.choose_model')) + '</div>' +
                '<div id="ob-model-list"><div class="ob-loading">Loading…</div></div></div></div>';
    }

    function renderProviderForm(key) {
        var p = PROVIDERS[key] || { name: key };
        return '<div class="ob-provider-detail ob-phase"><div class="ob-detail-header"><strong>' + esc(p.name) + '</strong>' +
            '<button class="btn btn-ghost btn-sm" id="ob-pf-close">&times;</button></div>' +
            '<div class="ob-field"><label>' + esc(t('provider.api_key')) + '</label>' +
                '<input type="password" id="ob-apikey" class="input" placeholder="' + esc(t('provider.api_key.placeholder')) + '" autocomplete="off"></div>' +
            '<div class="ob-field-actions"><button class="btn btn-primary btn-sm" id="ob-pf-test">' + esc(t('provider.test')) + '</button>' +
                '<span class="ob-test-status" id="ob-pf-status"></span></div></div>';
    }

    async function showModelSection() {
        var section = document.getElementById('ob-model-section');
        if (!section) return;
        section.style.display = 'block';
        var models = await fetchModels();
        var list = document.getElementById('ob-model-list');
        if (!list) return;
        if (!models.length) { setHTML(list, '<div class="ob-loading">No models found</div>'); return; }

        var priority = ['claude-sonnet-4-5', 'claude-sonnet-4', 'gpt-4o', 'claude-opus-4', 'gemini-2.0-flash'];
        var rec = [], rest = [];
        models.forEach(function (m) {
            if (priority.some(function (p) { return m.indexOf(p) >= 0; }) && rec.length < 3) rec.push(m);
            else rest.push(m);
        });
        if (!state.modelSelected) state.modelSelected = rec[0] || rest[0] || models[0];

        function item(name) {
            var sel = name === state.modelSelected;
            var disp = name.indexOf('/') >= 0 ? name.split('/').slice(1).join('/') : name;
            return '<label class="ob-model-item' + (sel ? ' is-selected' : '') + '">' +
                '<input type="radio" name="ob-model" value="' + esc(name) + '"' + (sel ? ' checked' : '') + '>' +
                '<span class="ob-model-radio"></span><span class="ob-model-name">' + esc(disp) + '</span></label>';
        }
        var html = '';
        if (rec.length) html += '<div class="ob-section-label">' + esc(t('model.recommended')) + '</div>' + rec.map(item).join('');
        if (rest.length) html += '<details class="ob-model-more"><summary>' + esc(t('model.all')) + ' (' + rest.length + ')</summary>' + rest.map(item).join('') + '</details>';
        setHTML(list, html);
        list.addEventListener('change', function (e) {
            if (e.target.name === 'ob-model') {
                state.modelSelected = e.target.value;
                list.querySelectorAll('.ob-model-item').forEach(function (el) { el.classList.toggle('is-selected', el.querySelector('input').checked); });
            }
        });
    }

    // ═══ Phase 3: Channels ═══

    function renderChannels() {
        var chs = [
            { key: 'telegram', icon: '✈', name: 'Telegram' }, { key: 'discord', icon: '🎮', name: 'Discord' },
            { key: 'whatsapp', icon: '📱', name: 'WhatsApp' }, { key: 'slack', icon: '💬', name: 'Slack' },
            { key: 'email', icon: '✉', name: 'Email' }, { key: 'web', icon: '🌐', name: 'Web UI', always: true },
        ];
        var cards = chs.map(function (ch) {
            if (ch.always) return '<div class="ob-channel-card is-always"><span class="ob-channel-icon">' + ch.icon + '</span>' +
                '<span class="ob-channel-name">' + esc(ch.name) + '</span><span class="ob-badge-ok">' + esc(t('channels.web_always')) + '</span></div>';
            return '<button class="ob-channel-card" data-channel="' + ch.key + '"><span class="ob-channel-icon">' + ch.icon + '</span>' +
                '<span class="ob-channel-name">' + esc(ch.name) + '</span><span class="ob-channel-action">' + esc(t('channels.setup')) + '</span></button>';
        }).join('');
        return '<div class="ob-phase"><div class="ob-hero"><h1>' + esc(t('channels.title')) + '</h1><p>' + esc(t('channels.subtitle')) + '</p></div>' +
            '<div class="ob-channel-grid">' + cards + '</div><div id="ob-ch-form" style="display:none"></div>' +
            '<p class="ob-hint">' + esc(t('channels.later_hint')) + '</p></div>';
    }

    function renderChannelForm(key) {
        var hint = t('channels.' + key + '.hint');
        var fields = '';
        if (key === 'telegram' || key === 'discord' || key === 'slack') {
            fields = '<div class="ob-field"><label>' + esc(t('channels.token')) + '</label>' +
                '<input type="password" id="ob-ch-token" class="input" placeholder="' + esc(t('channels.token.placeholder')) + '"></div>';
        } else if (key === 'email') {
            fields = '<div class="ob-field-row"><div class="ob-field"><label>IMAP Host</label><input type="text" id="ob-imap" class="input" placeholder="imap.gmail.com"></div>' +
                '<div class="ob-field"><label>SMTP Host</label><input type="text" id="ob-smtp" class="input" placeholder="smtp.gmail.com"></div></div>' +
                '<div class="ob-field"><label>Email</label><input type="email" id="ob-email-user" class="input"></div>' +
                '<div class="ob-field"><label>App Password</label><input type="password" id="ob-email-pass" class="input"></div>';
        }
        return '<div class="ob-channel-detail ob-phase"><div class="ob-detail-header"><strong>' + esc(key.charAt(0).toUpperCase() + key.slice(1)) + '</strong>' +
            '<button class="btn btn-ghost btn-sm" id="ob-ch-close">&times;</button></div>' +
            '<p class="ob-hint" style="text-align:left">' + esc(hint) + '</p>' + fields +
            '<div class="ob-field-actions"><button class="btn btn-primary btn-sm" id="ob-ch-save">' + esc(t('channels.test_save')) + '</button>' +
            '<span class="ob-test-status" id="ob-ch-status"></span></div></div>';
    }

    // ═══ Phase 4: Meet Homun (chat) ═══

    function renderMeet() {
        return '<div class="ob-phase">' +
            '<div class="ob-hero"><h1>' + esc(t('meet.title')) + '</h1><p>' + esc(t('meet.subtitle')) + '</p></div>' +
            '<div class="ob-chat" id="ob-chat">' +
                '<div class="ob-chat-messages" id="ob-chat-msgs"><div class="ob-chat-connecting">' + esc(t('meet.connecting')) + '</div></div>' +
                '<form class="ob-chat-input" id="ob-chat-form">' +
                    '<textarea id="ob-chat-text" class="input" placeholder="' + esc(t('meet.placeholder')) + '" rows="1"></textarea>' +
                    '<button type="submit" class="btn btn-primary btn-sm">↑</button></form></div></div>';
    }

    function initChat() {
        state.chatConvId = 'onboarding-' + Date.now();
        var proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
        var ws = new WebSocket(proto + '//' + location.host + '/ws/chat?conversation_id=' + state.chatConvId);
        state.chatWs = ws;
        var msgs = document.getElementById('ob-chat-msgs');
        if (!msgs) return;

        ws.onopen = function () {
            setHTML(msgs, '');
            addChatMsg('assistant', t('meet.greeting'));
        };
        ws.onmessage = function (e) {
            try {
                var data = JSON.parse(e.data);
                if (data.type === 'stream') {
                    if (!state.streamingEl) { state.streamingEl = addChatMsg('assistant', ''); state.streamBuf = ''; }
                    state.streamBuf += data.delta;
                    renderMd(state.streamingEl, state.streamBuf);
                } else if (data.type === 'response') {
                    if (state.streamingEl) {
                        renderMd(state.streamingEl, data.content);
                        state.streamingEl.parentElement.classList.remove('streaming');
                        state.streamingEl = null;
                        state.streamBuf = '';
                    }
                }
            } catch (_) {}
        };
        ws.onerror = function () { if (msgs) setHTML(msgs, '<div class="ob-chat-connecting">Connection error</div>'); };

        var form = document.getElementById('ob-chat-form');
        var input = document.getElementById('ob-chat-text');
        if (form && input) {
            form.addEventListener('submit', function (e) {
                e.preventDefault();
                var text = input.value.trim();
                if (!text || !state.chatWs || state.chatWs.readyState !== 1) return;
                addChatMsg('user', text);
                state.chatWs.send(JSON.stringify({ content: text, attachments: [], mcp_servers: [] }));
                input.value = '';
                input.style.height = 'auto';
            });
            input.addEventListener('input', function () { this.style.height = 'auto'; this.style.height = Math.min(this.scrollHeight, 120) + 'px'; });
            input.addEventListener('keydown', function (e) { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); form.dispatchEvent(new Event('submit')); } });
        }
    }

    function addChatMsg(role, content) {
        var msgs = document.getElementById('ob-chat-msgs');
        if (!msgs) return null;
        var div = document.createElement('div');
        div.className = 'chat-msg ' + role;
        var body = document.createElement('div');
        body.className = 'chat-msg-body';
        if (role === 'user') body.textContent = content;
        else { renderMd(body, content); if (!content) div.classList.add('streaming'); }
        div.appendChild(body);
        msgs.appendChild(div);
        msgs.scrollTop = msgs.scrollHeight;
        return body;
    }

    function renderMd(el, text) {
        if (typeof marked !== 'undefined' && typeof DOMPurify !== 'undefined') {
            el.innerHTML = DOMPurify.sanitize(marked.parse(text || ''));
        } else {
            el.textContent = text || '';
        }
    }

    // ═══ Phase rendering ═══

    var RENDERERS = [renderWelcome, renderProvider, renderChannels, renderMeet];

    function renderPhase() {
        var area = document.getElementById('ob-phase-area');
        if (!area) return;
        setHTML(area, RENDERERS[currentPhase]());
        renderStepper();
        updateNav();
        bindEvents();
        if (currentPhase === 3) initChat();
    }

    // ═══ Event binding ═══

    function bindEvents() {
        if (currentPhase === 0) {
            fetchOnboardingStatus().then(function (s) {
                if (!s) return;
                var el = document.getElementById('ob-name');
                if (el && s.user_name) el.value = s.user_name;
                var langEl = document.getElementById('ob-lang');
                if (langEl && s.language && s.language !== 'system') langEl.value = s.language;
                if (s.timezone) { var tz = document.getElementById('ob-tz'); if (tz) tz.value = s.timezone; }
            });
            var langSel = document.getElementById('ob-lang');
            if (langSel) langSel.addEventListener('change', async function () {
                localStorage.setItem('homun-language', langSel.value);
                await loadStrings();
                renderPhase();
            });
            document.querySelectorAll('.ob-theme-btn').forEach(function (btn) {
                btn.addEventListener('click', function () {
                    document.querySelectorAll('.ob-theme-btn').forEach(function (b) { b.classList.remove('is-active'); });
                    btn.classList.add('is-active');
                    window.HomunAccent.applyTheme(btn.dataset.theme);
                });
            });
            document.querySelectorAll('.ob-accent-dot[data-accent]').forEach(function (dot) {
                dot.addEventListener('click', function () {
                    document.querySelectorAll('.ob-accent-dot').forEach(function (d) { d.classList.remove('is-active'); });
                    dot.classList.add('is-active');
                    window.HomunAccent.applyAccent(dot.dataset.accent);
                });
            });
            var cc = document.getElementById('ob-accent-custom');
            if (cc) {
                var saved = localStorage.getItem('homun-accent-custom');
                if (saved) cc.value = saved;
                cc.addEventListener('input', function () {
                    document.querySelectorAll('.ob-accent-dot').forEach(function (d) { d.classList.remove('is-active'); });
                    cc.classList.add('is-active');
                    window.HomunAccent.applyAccent(cc.value);
                });
            }
        }

        if (currentPhase === 1) {
            document.querySelectorAll('.ob-provider-card').forEach(function (card) {
                card.addEventListener('click', function () {
                    var form = document.getElementById('ob-provider-form');
                    if (form) { form.style.display = 'block'; setHTML(form, renderProviderForm(card.dataset.provider)); bindProviderForm(card.dataset.provider); }
                });
            });
            document.querySelectorAll('[data-ollama-model]').forEach(function (pill) {
                pill.addEventListener('click', async function () {
                    pill.textContent = '…';
                    try {
                        await patchConfig('providers.ollama.api_base', 'http://localhost:11434');
                        await patchConfig('agent.model', 'ollama/' + pill.dataset.ollamaModel);
                        state.providerConfigured = 'ollama';
                        state.modelSelected = 'ollama/' + pill.dataset.ollamaModel;
                        pill.textContent = '✓ ' + pill.dataset.ollamaModel;
                        pill.classList.add('is-selected');
                        showModelSection();
                    } catch (_) { pill.textContent = pill.dataset.ollamaModel; }
                });
            });
            if (state.providerConfigured) showModelSection();
        }

        if (currentPhase === 2) {
            document.querySelectorAll('.ob-channel-card[data-channel]').forEach(function (card) {
                card.addEventListener('click', function () {
                    var form = document.getElementById('ob-ch-form');
                    if (form) { form.style.display = 'block'; setHTML(form, renderChannelForm(card.dataset.channel)); bindChannelForm(card.dataset.channel); }
                });
            });
        }
    }

    function bindProviderForm(key) {
        var close = document.getElementById('ob-pf-close');
        if (close) close.addEventListener('click', function () { var f = document.getElementById('ob-provider-form'); if (f) { f.style.display = 'none'; setHTML(f, ''); } });
        var test = document.getElementById('ob-pf-test');
        if (test) test.addEventListener('click', async function () {
            var apiKey = (document.getElementById('ob-apikey') || {}).value || '';
            if (!apiKey) return;
            var s = document.getElementById('ob-pf-status');
            test.disabled = true;
            if (s) { s.textContent = t('provider.testing'); s.className = 'ob-test-status'; }
            try {
                await patchConfig('providers.' + key + '.api_key', apiKey);
                var ok = await testProvider(key);
                if (s) { s.textContent = ok ? t('provider.connected') : t('provider.failed'); s.className = 'ob-test-status ' + (ok ? 'is-ok' : 'is-err'); }
                if (ok) { state.providerConfigured = key; renderPhase(); }
            } catch (_) { if (s) { s.textContent = t('provider.failed'); s.className = 'ob-test-status is-err'; } }
            test.disabled = false;
        });
    }

    function bindChannelForm(key) {
        var close = document.getElementById('ob-ch-close');
        if (close) close.addEventListener('click', function () { var f = document.getElementById('ob-ch-form'); if (f) { f.style.display = 'none'; setHTML(f, ''); } });
        var save = document.getElementById('ob-ch-save');
        if (save) save.addEventListener('click', async function () {
            save.disabled = true;
            var s = document.getElementById('ob-ch-status');
            if (s) { s.textContent = 'Saving…'; s.className = 'ob-test-status'; }
            try {
                if (key === 'telegram' || key === 'discord' || key === 'slack') {
                    var token = (document.getElementById('ob-ch-token') || {}).value;
                    if (token) { await patchConfig('channels.' + key + '.token', token); await patchConfig('channels.' + key + '.enabled', true); }
                } else if (key === 'email') {
                    var ih = (document.getElementById('ob-imap') || {}).value, sh = (document.getElementById('ob-smtp') || {}).value;
                    var eu = (document.getElementById('ob-email-user') || {}).value, ep = (document.getElementById('ob-email-pass') || {}).value;
                    if (ih) await patchConfig('channels.email.imap_host', ih);
                    if (sh) await patchConfig('channels.email.smtp_host', sh);
                    if (eu) { await patchConfig('channels.email.username', eu); await patchConfig('channels.email.from_address', eu); }
                    if (ep) await patchConfig('channels.email.password', ep);
                    await patchConfig('channels.email.enabled', true);
                }
                if (s) { s.textContent = '✓ Saved'; s.className = 'ob-test-status is-ok'; }
            } catch (_) { if (s) { s.textContent = 'Failed'; s.className = 'ob-test-status is-err'; } }
            save.disabled = false;
        });
    }

    // ═══ Phase transitions ═══

    async function savePhase() {
        if (currentPhase === 0) {
            var name = (document.getElementById('ob-name') || {}).value || '';
            var lang = (document.getElementById('ob-lang') || {}).value || 'en';
            var tz = (document.getElementById('ob-tz') || {}).value || '';
            if (name) await patchConfig('agent.user_name', name);
            await patchConfig('ui.language', lang);
            await patchConfig('ui.theme', localStorage.getItem('homun-theme') || 'system');
            await patchConfig('ui.accent', localStorage.getItem('homun-accent') || '');
            if (tz) await patchConfig('agent.timezone', tz);
            localStorage.setItem('homun-language', lang);
        }
        if (currentPhase === 1 && state.modelSelected) await patchConfig('agent.model', state.modelSelected);
    }

    async function goNext() {
        await savePhase();
        if (!state.completed.includes(PHASES[currentPhase])) state.completed.push(PHASES[currentPhase]);
        if (currentPhase === PHASES.length - 1) {
            await completeOnboarding();
            window.location.href = state.chatConvId ? '/chat?c=' + state.chatConvId : '/chat';
            return;
        }
        currentPhase++;
        renderPhase();
        if (currentPhase === 1) { await detectOllama(); renderPhase(); }
    }

    function goBack() { if (currentPhase > 0) { if (state.chatWs) { state.chatWs.close(); state.chatWs = null; } currentPhase--; renderPhase(); } }

    // ═══ Init ═══

    async function init() {
        await loadStrings();
        var s = await fetchOnboardingStatus();
        if (s && s.completed) { window.location.href = '/chat'; return; }
        if (s && s.has_provider) state.providerConfigured = 'configured';
        renderPhase();

        document.getElementById('ob-next').addEventListener('click', goNext);
        document.getElementById('ob-back').addEventListener('click', goBack);
        document.getElementById('ob-skip').addEventListener('click', async function (e) {
            e.preventDefault();
            await completeOnboarding();
            window.location.href = '/chat';
        });
    }

    init();
})();
