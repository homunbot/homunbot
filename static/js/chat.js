// Homun — Chat WebSocket client with streaming, markdown, and tool indicators

const messagesEl = document.getElementById('messages');
const chatForm = document.getElementById('chat-form');
const chatText = document.getElementById('chat-text');
const wsStatus = document.getElementById('ws-status');
const btnSend = document.getElementById('btn-send');
const btnStop = document.getElementById('btn-stop');

let ws = null;
let reconnectTimer = null;

// Track the currently streaming message element so we can
// append incremental deltas as they arrive from the LLM.
let streamingEl = null;
let streamingContent = '';

// Tool call activity indicator element
let toolIndicatorEl = null;

// Current tool call blocks for display (shown before streaming text)
let currentToolCallsEl = null;
let currentToolCalls = [];

// Thinking block element
let thinkingEl = null;
let thinkingContent = '';

// Browser screenshot gallery
let browserGalleryEl = null;
let browserScreenshots = [];

// Processing state (true when agent is working)
let isProcessing = false;

// Configure marked.js for LLM output
if (typeof marked !== 'undefined') {
    marked.setOptions({ breaks: true, gfm: true });
}

// ─── Textarea auto-resize ────────────────────────────────────────

/** Auto-resize textarea to fit content, up to a max height. */
function autoResizeTextarea() {
    chatText.style.height = 'auto';
    chatText.style.height = Math.min(chatText.scrollHeight, 200) + 'px';
}

chatText.addEventListener('input', autoResizeTextarea);

// Submit on Enter, allow Shift+Enter for newline
chatText.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        chatForm.dispatchEvent(new Event('submit'));
    }
});

// ─── Markdown rendering ────────────────────────────────────────

/** Render markdown content safely into an element.
 *  User messages stay plain text; assistant messages get markdown.
 *  Uses DOMPurify to sanitize HTML output from marked.js.
 */
function renderContent(el, content, role) {
    if (role === 'assistant' && typeof marked !== 'undefined' && typeof DOMPurify !== 'undefined') {
        // Convert screenshot URLs to inline images before markdown parsing
        let processedContent = content.replace(
            /\/api\/v1\/browser\/screenshots\/(\S+\.png)/g,
            '\n\n![Screenshot](/api/v1/browser/screenshots/$1)\n\n'
        );

        const rawHtml = marked.parse(processedContent);
        // DOMPurify sanitizes the HTML to prevent XSS attacks (safe: uses sanitize)
        el.innerHTML = DOMPurify.sanitize(rawHtml);

        // Add click-to-expand for screenshots
        el.querySelectorAll('img[src*="/api/v1/browser/screenshots/"]').forEach(img => {
            img.style.cursor = 'pointer';
            img.style.maxWidth = '100%';
            img.style.borderRadius = '8px';
            img.style.marginTop = '8px';
            img.style.boxShadow = 'var(--shadow-md)';
            img.addEventListener('click', () => {
                window.open(img.src, '_blank');
            });
        });
    } else {
        el.textContent = content;
    }
}

// ─── Chat history ──────────────────────────────────────────────

/** Load previous messages from the server on connect. */
async function loadHistory() {
    try {
        const res = await fetch('/api/v1/chat/history?limit=50');
        if (!res.ok) return;
        const messages = await res.json();
        if (messages.length === 0) return;

        messages.forEach(m => {
            addMessage(m.role, m.content, m.tools_used);
        });
    } catch (e) {
        console.error('Failed to load chat history:', e);
    }
}

// ─── Tool indicators ───────────────────────────────────────────

// List of tool names currently executing (for multi-tool sequences)
let activeTools = [];

// Reasoning section element (collapsible container for tool calls)
let reasoningSectionEl = null;
let reasoningContentEl = null;
let reasoningCount = 0;

/** Create the collapsible reasoning section */
function createReasoningSection() {
    if (reasoningSectionEl) return reasoningSectionEl;

    reasoningSectionEl = document.createElement('div');
    reasoningSectionEl.className = 'chat-reasoning collapsed';
    reasoningSectionEl.innerHTML = `
        <div class="chat-reasoning-header" onclick="toggleReasoning(this)">
            <span class="chat-reasoning-icon">⚙️</span>
            <span class="chat-reasoning-label">Reasoning</span>
            <span class="chat-reasoning-count">(0)</span>
            <span class="chat-reasoning-toggle">▶</span>
        </div>
        <div class="chat-reasoning-content"></div>
    `;

    reasoningContentEl = reasoningSectionEl.querySelector('.chat-reasoning-content');
    messagesEl.appendChild(reasoningSectionEl);
    messagesEl.scrollTop = messagesEl.scrollHeight;

    return reasoningSectionEl;
}

/** Toggle reasoning section visibility */
window.toggleReasoning = function(headerEl) {
    const section = headerEl.closest('.chat-reasoning');
    if (section) {
        section.classList.toggle('collapsed');
        const toggle = section.querySelector('.chat-reasoning-toggle');
        if (toggle) {
            toggle.textContent = section.classList.contains('collapsed') ? '▶' : '▼';
        }
    }
};

/** Update reasoning count */
function updateReasoningCount() {
    if (reasoningSectionEl) {
        const countEl = reasoningSectionEl.querySelector('.chat-reasoning-count');
        if (countEl) {
            countEl.textContent = `(${reasoningCount})`;
        }
    }
}

function showToolIndicator(toolName, toolCallData) {
    activeTools.push(toolName);

    if (!toolIndicatorEl) {
        toolIndicatorEl = document.createElement('div');
        toolIndicatorEl.className = 'chat-msg tool-indicator';
        messagesEl.appendChild(toolIndicatorEl);
    }

    toolIndicatorEl.textContent = `Using ${activeTools[activeTools.length - 1]}\u2026`;
    messagesEl.scrollTop = messagesEl.scrollHeight;

    // Add to reasoning section
    if (toolCallData) {
        addToolCallCard(toolCallData);
    }
}

/** Add a tool call card to the reasoning section */
function addToolCallCard(toolCallData) {
    // Ensure reasoning section exists
    if (!reasoningSectionEl) {
        createReasoningSection();
    }

    const card = document.createElement('div');
    card.className = 'chat-tool-call';
    card.id = `tool-call-${toolCallData.id}`;

    // Build card content - compact format
    let argsDisplay = '';
    if (toolCallData.arguments && Object.keys(toolCallData.arguments).length > 0) {
        // Compact one-line display for common actions
        const args = toolCallData.arguments;
        if (toolCallData.name === 'browser' && args.action) {
            const action = args.action;
            let summary = action;
            if (action === 'navigate' && args.url) {
                summary = 'navigate \u2192 ' + truncate(args.url, 50);
            } else if (action === 'click' && args.ref) {
                summary = 'click [' + args.ref + ']';
            } else if (action === 'type' && args.ref && args.text) {
                summary = 'type "' + truncate(args.text, 30) + '" \u2192 [' + args.ref + ']';
            } else if (action === 'snapshot') {
                summary = 'get page snapshot';
            }
            argsDisplay = '<span class="chat-tool-summary">' + escapeHtml(summary) + '</span>';
        } else if (toolCallData.name === 'web_search' && args.query) {
            argsDisplay = '<span class="chat-tool-summary">"' + escapeHtml(truncate(args.query, 50)) + '"</span>';
        } else if (toolCallData.name === 'web_fetch' && args.url) {
            argsDisplay = '<span class="chat-tool-summary">' + escapeHtml(truncate(args.url, 50)) + '</span>';
        } else {
            // Fallback to JSON for other tools
            argsDisplay = '<pre class="chat-tool-args">' + JSON.stringify(args, null, 2) + '</pre>';
        }
    }

    card.innerHTML = '<div class="chat-tool-call-compact">' +
        '<span class="chat-tool-call-icon">\u26a1</span>' +
        '<span class="chat-tool-call-name">' + escapeHtml(toolCallData.name) + '</span>' +
        argsDisplay + '</div>';

    reasoningContentEl.appendChild(card);
    reasoningCount++;
    updateReasoningCount();
    currentToolCalls.push(toolCallData.id);

    // Auto-expand while tools are running
    reasoningSectionEl.classList.remove('collapsed');
    const toggle = reasoningSectionEl.querySelector('.chat-reasoning-toggle');
    if (toggle) toggle.textContent = '\u25bc';

    messagesEl.scrollTop = messagesEl.scrollHeight;
}

/** Truncate a string with ellipsis */
function truncate(str, maxLen) {
    if (!str) return '';
    return str.length > maxLen ? str.substring(0, maxLen) + '...' : str;
}

// ─── Browser Screenshot Gallery ────────────────────────────────────

/** Add a screenshot to the browser gallery */
function addBrowserScreenshot(url) {
    if (!browserGalleryEl) {
        browserGalleryEl = document.createElement('div');
        browserGalleryEl.className = 'browser-gallery';
        browserGalleryEl.innerHTML = '<div class="browser-gallery-header">📷 Browser Screenshots</div><div class="browser-gallery-images"></div>';
        messagesEl.appendChild(browserGalleryEl);
    }

    const imagesEl = browserGalleryEl.querySelector('.browser-gallery-images');
    const img = document.createElement('img');
    img.src = url;
    img.className = 'browser-gallery-img';
    img.loading = 'lazy';
    img.addEventListener('click', () => window.open(url, '_blank'));

    imagesEl.appendChild(img);
    browserScreenshots.push(url);
    messagesEl.scrollTop = messagesEl.scrollHeight;
}

/** Clear the browser gallery for a new session */
function clearBrowserGallery() {
    if (browserGalleryEl) {
        browserGalleryEl.remove();
        browserGalleryEl = null;
    }
    browserScreenshots = [];
}

function endToolIndicator(toolName) {
    activeTools = activeTools.filter(t => t !== toolName);
    if (activeTools.length > 0 && toolIndicatorEl) {
        // Still tools running — update label to the current one
        toolIndicatorEl.textContent = `Using ${activeTools[activeTools.length - 1]}\u2026`;
    }
    // Don't remove the indicator here — it morphs into the streaming bubble
}

/** Morph the tool indicator into the streaming assistant bubble.
 *  This avoids a DOM remove+insert that causes layout jumps.
 */
function morphIndicatorToStreaming() {
    if (toolIndicatorEl) {
        // Reuse the same DOM element — just change its class and clear content
        toolIndicatorEl.className = 'chat-msg assistant streaming';
        toolIndicatorEl.textContent = '';
        streamingEl = toolIndicatorEl;
        streamingContent = '';
        toolIndicatorEl = null;
        activeTools = [];
    }
}

function removeToolIndicator() {
    if (toolIndicatorEl) {
        toolIndicatorEl.remove();
        toolIndicatorEl = null;
        activeTools = [];
    }
}

/** Finalize and collapse reasoning section */
function finalizeReasoning() {
    if (reasoningSectionEl && reasoningCount > 0) {
        // Auto-collapse after completion
        reasoningSectionEl.classList.add('collapsed');
        const toggle = reasoningSectionEl.querySelector('.chat-reasoning-toggle');
        if (toggle) toggle.textContent = '\u25b6';

        // Update label to show it's done
        const labelEl = reasoningSectionEl.querySelector('.chat-reasoning-label');
        if (labelEl) {
            labelEl.textContent = 'Reasoning';
        }
    }
}

/** Reset reasoning section for next message */
function resetReasoning() {
    reasoningSectionEl = null;
    reasoningContentEl = null;
    reasoningCount = 0;
}

/** Clear tool calls container after response */
function clearToolCalls() {
    // Finalize and collapse reasoning section
    finalizeReasoning();
    // Reset for next message
    resetReasoning();
    if (currentToolCallsEl) {
        currentToolCallsEl = null;
    }
    currentToolCalls = [];
}

// ─── Thinking blocks ────────────────────────────────────────────

/** Create a collapsible thinking block */
function createThinkingBlock() {
    if (thinkingEl) return thinkingEl;
    
    thinkingEl = document.createElement('div');
    thinkingEl.className = 'chat-thinking collapsed';
    thinkingEl.innerHTML = `
        <div class="chat-thinking-header" onclick="toggleThinking(this)">
            <span class="chat-thinking-icon">💭</span>
            <span class="chat-thinking-label">Thinking...</span>
            <span class="chat-thinking-toggle">▶</span>
        </div>
        <div class="chat-thinking-content"></div>
    `;
    
    // Insert before tool indicator or at the end
    if (toolIndicatorEl && toolIndicatorEl.parentElement === messagesEl) {
        messagesEl.insertBefore(thinkingEl, toolIndicatorEl);
    } else {
        messagesEl.appendChild(thinkingEl);
    }
    messagesEl.scrollTop = messagesEl.scrollHeight;
    
    return thinkingEl;
}

/** Append content to the thinking block */
function appendThinking(delta) {
    if (!thinkingEl) createThinkingBlock();
    
    thinkingContent += delta;
    const contentEl = thinkingEl.querySelector('.chat-thinking-content');
    if (contentEl) {
        contentEl.textContent = thinkingContent;
    }
    
    // Update label
    const labelEl = thinkingEl.querySelector('.chat-thinking-label');
    if (labelEl) {
        labelEl.textContent = 'Thinking...';
    }
    
    messagesEl.scrollTop = messagesEl.scrollHeight;
}

/** Finalize thinking block (collapses it if there's content) */
function finalizeThinking() {
    if (thinkingEl && thinkingContent) {
        thinkingEl.classList.remove('collapsed');
        thinkingEl.classList.add('has-content');
        
        const labelEl = thinkingEl.querySelector('.chat-thinking-label');
        if (labelEl) {
            labelEl.textContent = 'Thought process';
        }
        
        // Auto-collapse if content is long
        if (thinkingContent.length > 200) {
            thinkingEl.classList.add('collapsed');
        }
    }
    thinkingEl = null;
    thinkingContent = '';
}

/** Toggle thinking block visibility */
window.toggleThinking = function(headerEl) {
    const thinkingBlock = headerEl.closest('.chat-thinking');
    if (thinkingBlock) {
        thinkingBlock.classList.toggle('collapsed');
        const toggle = thinkingBlock.querySelector('.chat-thinking-toggle');
        if (toggle) {
            toggle.textContent = thinkingBlock.classList.contains('collapsed') ? '▶' : '▼';
        }
    }
};

// ─── Utility functions ───────────────────────────────────────────

function escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}

// ─── WebSocket ─────────────────────────────────────────────────

function connect() {
    const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
    ws = new WebSocket(`${proto}//${location.host}/ws/chat`);

    ws.onopen = () => {
        wsStatus.textContent = 'Connected';
        wsStatus.className = 'badge badge-success';
        if (reconnectTimer) {
            clearTimeout(reconnectTimer);
            reconnectTimer = null;
        }
        // Load conversation history from DB
        loadHistory();
    };

    ws.onmessage = (event) => {
        try {
            const data = JSON.parse(event.data);

            if (data.type === 'connected') {
                addMessage('system', `Session ${data.session_id}`);

            } else if (data.type === 'thinking_start') {
                // Start a new thinking block
                createThinkingBlock();

            } else if (data.type === 'thinking_chunk') {
                // Append to thinking content
                appendThinking(data.delta);

            } else if (data.type === 'thinking_end') {
                // Finalize thinking block
                finalizeThinking();

            } else if (data.type === 'tool_start') {
                // Agent is calling a tool
                showToolIndicator(data.name, data.tool_call);

            } else if (data.type === 'tool_end') {
                // Tool finished — update indicator but keep it visible
                endToolIndicator(data.name);

            } else if (data.type === 'screenshot') {
                // Screenshot URL from browser tool — add to gallery
                addBrowserScreenshot(data.delta);

            } else if (data.type === 'stream') {
                // First text chunk: morph indicator into streaming bubble (no layout jump)
                if (toolIndicatorEl) morphIndicatorToStreaming();
                // Incremental text chunk from the LLM
                handleStreamChunk(data.delta);

            } else if (data.type === 'response') {
                // Final complete response — morph or replace streaming draft
                if (toolIndicatorEl) morphIndicatorToStreaming();
                finalizeStream(data.content);
                // Clear tool calls for next message
                clearToolCalls();
            }
        } catch (e) {
            console.error('Failed to parse message:', e);
        }
    };

    ws.onclose = () => {
        wsStatus.textContent = 'Disconnected';
        wsStatus.className = 'badge badge-error';
        streamingEl = null;
        streamingContent = '';
        removeToolIndicator();
        reconnectTimer = setTimeout(connect, 3000);
    };

    ws.onerror = () => {
        ws.close();
    };
}

// ─── Streaming ─────────────────────────────────────────────────

/** Handle an incremental streaming chunk from the LLM. */
function handleStreamChunk(delta) {
    if (!delta) return;

    if (!streamingEl) {
        // First chunk — create a new assistant message bubble
        streamingEl = document.createElement('div');
        streamingEl.className = 'chat-msg assistant streaming';
        streamingContent = '';
        messagesEl.appendChild(streamingEl);
    }

    // During streaming, use textContent for performance
    // (markdown rendered at finalization)
    streamingContent += delta;
    streamingEl.textContent = streamingContent;
    messagesEl.scrollTop = messagesEl.scrollHeight;
}

/** Finalize the streaming message with the complete response.
 *  Render markdown on the final content for proper formatting.
 */
function finalizeStream(content) {
    // Detect and add screenshots to the browser gallery
    const screenshotRegex = /\/api\/v1\/browser\/screenshots\/([a-zA-Z0-9_-]+\.png)/g;
    let match;
    while ((match = screenshotRegex.exec(content)) !== null) {
        addBrowserScreenshot(match[0]);
    }

    if (streamingEl) {
        renderContent(streamingEl, content, 'assistant');
        streamingEl.classList.remove('streaming');
        streamingEl = null;
        streamingContent = '';
    } else {
        addMessage('assistant', content);
    }
    messagesEl.scrollTop = messagesEl.scrollHeight;

    // Reset processing state
    setProcessing(false);
}

// ─── Message rendering ─────────────────────────────────────────

function addMessage(role, content, toolsUsed) {
    const div = document.createElement('div');
    div.className = `chat-msg ${role}`;
    renderContent(div, content, role);

    // Show tool badges for messages that used tools
    if (toolsUsed && toolsUsed.length > 0) {
        const badge = document.createElement('div');
        badge.className = 'chat-tools-badge';
        badge.textContent = toolsUsed.join(', ');
        div.prepend(badge);
    }

    messagesEl.appendChild(div);
    messagesEl.scrollTop = messagesEl.scrollHeight;
}

// ─── Form submission ───────────────────────────────────────────

chatForm.addEventListener('submit', (e) => {
    e.preventDefault();
    const text = chatText.value.trim();
    if (!text || !ws || ws.readyState !== WebSocket.OPEN) return;

    addMessage('user', text);
    ws.send(JSON.stringify({ content: text }));
    chatText.value = '';
    chatText.style.height = 'auto';
    chatText.focus();
    setProcessing(true);
});

// ─── Stop button ─────────────────────────────────────────────────

/** Set processing state and toggle Send/Stop buttons */
function setProcessing(processing) {
    isProcessing = processing;
    if (btnSend) btnSend.style.display = processing ? 'none' : '';
    if (btnStop) btnStop.style.display = processing ? '' : 'none';
}

/** Handle stop button click */
async function handleStop() {
    try {
        const res = await fetch('/api/v1/chat/stop', { method: 'POST' });
        const data = await res.json();
        if (data.ok) {
            showToast('Stopping...', 'warning');
        } else {
            showToast('Failed to stop', 'error');
        }
    } catch (e) {
        console.error('Failed to send stop:', e);
        showToast('Failed to stop', 'error');
    }
}

if (btnStop) {
    btnStop.addEventListener('click', handleStop);
}

// ─── Chat actions (New / Compact / Clear) ─────────────────────────

/** Show a temporary toast notification */
function showToast(message, type = 'info') {
    const existing = document.querySelector('.chat-toast');
    if (existing) existing.remove();
    
    const toast = document.createElement('div');
    toast.className = `chat-toast chat-toast--${type}`;
    toast.textContent = message;
    document.body.appendChild(toast);
    
    setTimeout(() => toast.classList.add('chat-toast--visible'), 10);
    setTimeout(() => {
        toast.classList.remove('chat-toast--visible');
        setTimeout(() => toast.remove(), 200);
    }, 2500);
}

/** Clear the screen (UI only, keeps DB history) */
function handleClearChat() {
    messagesEl.textContent = '';
    clearBrowserGallery();
    showToast('Screen cleared', 'info');
}

/** Start a new conversation (clears DB and UI) */
async function handleNewChat() {
    try {
        const res = await fetch('/api/v1/chat/history', { method: 'DELETE' });
        const data = await res.json();

        if (data.ok) {
            messagesEl.textContent = '';
            clearBrowserGallery();
            addMessage('system', 'New conversation started');
            showToast('Started new conversation', 'success');
        } else {
            showToast(data.message || 'Failed to clear history', 'error');
        }
    } catch (e) {
        console.error('Failed to start new chat:', e);
        showToast('Failed to start new conversation', 'error');
    }
}

/** Compact the conversation (trigger memory consolidation) */
async function handleCompactChat() {
    try {
        const res = await fetch('/api/v1/chat/compact', { method: 'POST' });
        const data = await res.json();
        
        if (data.ok) {
            showToast('Conversation compacted', 'success');
        } else {
            showToast(data.message || 'Cannot compact yet', 'warning');
        }
    } catch (e) {
        console.error('Failed to compact chat:', e);
        showToast('Failed to compact conversation', 'error');
    }
}

// Wire up action buttons
document.getElementById('btn-clear-chat')?.addEventListener('click', handleClearChat);
document.getElementById('btn-new-chat')?.addEventListener('click', handleNewChat);
document.getElementById('btn-compact-chat')?.addEventListener('click', handleCompactChat);

// ─── Model Selector ─────────────────────────────────────────────

const chatModelSelect = document.getElementById('chat-model-select');
const chatConfig = document.getElementById('chat-config');
let currentModel = '';
let currentVisionModel = '';

/** Load available models and populate the dropdown */
async function loadChatModelDropdown() {
    if (!chatModelSelect) return;

    // Get current model from config
    if (chatConfig) {
        currentModel = chatConfig.dataset.model || '';
        currentVisionModel = chatConfig.dataset.visionModel || '';
    }

    try {
        const resp = await fetch('/api/v1/providers/models');
        const data = await resp.json();

        // Clear existing options
        while (chatModelSelect.firstChild) {
            chatModelSelect.removeChild(chatModelSelect.firstChild);
        }

        // Add current model as first option (selected)
        const currentOpt = document.createElement('option');
        currentOpt.value = '';
        const modelDisplay = currentModel.split('/').pop() || currentModel;
        currentOpt.textContent = modelDisplay;
        currentOpt.selected = true;
        chatModelSelect.appendChild(currentOpt);

        // Group models by provider
        const groups = {};

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
                const ollamaResp = await fetch('/api/v1/providers/ollama/models');
                const ollamaData = await ollamaResp.json();
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
                const cloudResp = await fetch('/api/v1/providers/ollama-cloud/models');
                const cloudData = await cloudResp.json();
                if (cloudData.ok && cloudData.models.length > 0) {
                    groups['ollama_cloud'] = cloudData.models.map(function(m) {
                        return { value: 'ollama_cloud/' + m.id, label: m.id };
                    });
                }
            } catch (_) { /* Ollama Cloud might not be reachable */ }
        }

        // Add separator
        const sepOpt = document.createElement('option');
        sepOpt.disabled = true;
        sepOpt.textContent = '── Switch to ──';
        chatModelSelect.appendChild(sepOpt);

        // Add model groups
        Object.keys(groups).forEach(function(provider) {
            const optgroup = document.createElement('optgroup');
            optgroup.label = providerDisplayName(provider);

            groups[provider].forEach(function(m) {
                const option = document.createElement('option');
                option.value = m.value;
                option.textContent = m.label;
                optgroup.appendChild(option);
            });

            chatModelSelect.appendChild(optgroup);
        });

    } catch (err) {
        console.error('Failed to load models:', err);
    }
}

function providerDisplayName(name) {
    const map = {
        anthropic: 'Anthropic', openai: 'OpenAI', openrouter: 'OpenRouter',
        gemini: 'Gemini', deepseek: 'DeepSeek', groq: 'Groq',
        ollama: 'Ollama', ollama_cloud: 'Ollama Cloud',
        mistral: 'Mistral', xai: 'xAI', together: 'Together',
        fireworks: 'Fireworks', perplexity: 'Perplexity', cohere: 'Cohere',
        venice: 'Venice', aihubmix: 'AiHubMix', vllm: 'vLLM', custom: 'Custom',
    };
    return map[name] || name;
}

/** Handle model change */
if (chatModelSelect) {
    chatModelSelect.addEventListener('change', async function() {
        const newModel = chatModelSelect.value;
        if (!newModel) return; // Empty means keep current

        try {
            const res = await fetch('/api/v1/config', {
                method: 'PATCH',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ path: 'agent.model', value: newModel })
            });

            if (res.ok) {
                currentModel = newModel;
                showToast('Model switched to ' + newModel.split('/').pop(), 'success');

                // Update display
                const opt = chatModelSelect.options[0];
                opt.textContent = newModel.split('/').pop();
                opt.selected = true;
            } else {
                showToast('Failed to switch model', 'error');
            }
        } catch (e) {
            console.error('Failed to switch model:', e);
            showToast('Failed to switch model', 'error');
        }
    });
}

// Load model dropdown on connect
loadChatModelDropdown();

// Start connection
connect();
