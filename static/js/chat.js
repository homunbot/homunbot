// Homun — Chat WebSocket client with streaming, markdown, and tool indicators

const messagesEl = document.getElementById('messages');
const chatForm = document.getElementById('chat-form');
const chatText = document.getElementById('chat-text');
const wsStatus = document.getElementById('ws-status');

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
        const rawHtml = marked.parse(content);
        // DOMPurify sanitizes the HTML to prevent XSS attacks
        el.innerHTML = DOMPurify.sanitize(rawHtml);
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

function showToolIndicator(toolName, toolCallData) {
    activeTools.push(toolName);

    if (!toolIndicatorEl) {
        toolIndicatorEl = document.createElement('div');
        toolIndicatorEl.className = 'chat-msg tool-indicator';
        messagesEl.appendChild(toolIndicatorEl);
    }

    toolIndicatorEl.textContent = `Using ${activeTools[activeTools.length - 1]}\u2026`;
    messagesEl.scrollTop = messagesEl.scrollHeight;
    
    // Also add a tool call card if we have data
    if (toolCallData) {
        addToolCallCard(toolCallData);
    }
}

/** Add a tool call card to the current tool calls container */
function addToolCallCard(toolCallData) {
    if (!currentToolCallsEl) {
        currentToolCallsEl = document.createElement('div');
        currentToolCallsEl.className = 'chat-tool-calls';
        messagesEl.appendChild(currentToolCallsEl);
    }
    
    const card = document.createElement('div');
    card.className = 'chat-tool-call';
    card.id = `tool-call-${toolCallData.id}`;
    
    // Build card content
    let argsDisplay = '';
    if (toolCallData.arguments && Object.keys(toolCallData.arguments).length > 0) {
        argsDisplay = `<pre class="chat-tool-args">${JSON.stringify(toolCallData.arguments, null, 2)}</pre>`;
    }
    
    card.innerHTML = `
        <div class="chat-tool-call-header">
            <span class="chat-tool-call-icon">⚡</span>
            <span class="chat-tool-call-name">${escapeHtml(toolCallData.name)}</span>
        </div>
        ${argsDisplay}
    `;
    
    currentToolCallsEl.appendChild(card);
    currentToolCalls.push(toolCallData.id);
    messagesEl.scrollTop = messagesEl.scrollHeight;
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

/** Clear tool calls container after response */
function clearToolCalls() {
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
    if (streamingEl) {
        renderContent(streamingEl, content, 'assistant');
        streamingEl.classList.remove('streaming');
        streamingEl = null;
        streamingContent = '';
    } else {
        addMessage('assistant', content);
    }
    messagesEl.scrollTop = messagesEl.scrollHeight;
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
});

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
    showToast('Screen cleared', 'info');
}

/** Start a new conversation (clears DB and UI) */
async function handleNewChat() {
    try {
        const res = await fetch('/api/v1/chat/history', { method: 'DELETE' });
        const data = await res.json();
        
        if (data.ok) {
            messagesEl.textContent = '';
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

// Start connection
connect();
