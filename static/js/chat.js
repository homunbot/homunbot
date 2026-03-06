// Homun — Chat WebSocket client with streaming, markdown, and tool indicators

const messagesEl = document.getElementById('messages');
const chatForm = document.getElementById('chat-form');
const chatText = document.getElementById('chat-text');
const wsStatus = document.getElementById('ws-status');
const btnSend = document.getElementById('btn-send');
const chatEmptyState = document.getElementById('chat-empty-state');
const runBadgeEl = document.getElementById('chat-run-badge');
const chatPlusBtn = document.getElementById('btn-chat-plus');
const chatPlusMenu = document.getElementById('chat-plus-menu');
const chatImageInput = document.getElementById('chat-image-input');
const chatDocInput = document.getElementById('chat-doc-input');
const btnChatUploadImage = document.getElementById('btn-chat-upload-image');
const btnChatUploadDoc = document.getElementById('btn-chat-upload-doc');
const btnChatOpenMcp = document.getElementById('btn-chat-open-mcp');

let ws = null;
let reconnectTimer = null;
let historyLoaded = false;

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
let activeRunId = null;

// Configure marked.js for LLM output
if (typeof marked !== 'undefined') {
    marked.setOptions({ breaks: true, gfm: true });
}

// ─── Textarea auto-resize ────────────────────────────────────────

/** Auto-resize textarea to fit content, up to a max height. */
function autoResizeTextarea() {
    if (!chatText) return;
    chatText.style.height = 'auto';
    chatText.style.height = Math.min(chatText.scrollHeight, 200) + 'px';
}

chatText?.addEventListener('input', autoResizeTextarea);

// Submit on Enter, allow Shift+Enter for newline
chatText?.addEventListener('keydown', (e) => {
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
    if (historyLoaded) return;
    try {
        const res = await fetch('/api/v1/chat/history?limit=50');
        if (!res.ok) return;
        const messages = await res.json();
        historyLoaded = true;
        if (messages.length === 0) {
            syncEmptyState();
            return;
        }

        messages.forEach(m => {
            addMessage(m.role, m.content, m.tools_used);
        });
        syncEmptyState();
    } catch (e) {
        console.error('Failed to load chat history:', e);
    }
}

function syncEmptyState() {
    if (!chatEmptyState || !messagesEl) return;
    chatEmptyState.style.display = messagesEl.children.length > 0 ? 'none' : '';
}

function clearTransientRunUi() {
    if (toolIndicatorEl) {
        toolIndicatorEl.remove();
        toolIndicatorEl = null;
    }
    if (streamingEl) {
        streamingEl.remove();
        streamingEl = null;
        streamingContent = '';
    }
    if (reasoningSectionEl) {
        reasoningSectionEl.remove();
    }
    reasoningSectionEl = null;
    reasoningContentEl = null;
    reasoningCount = 0;
    reasoningTools = [];
    activeTools = [];
    if (thinkingEl) {
        thinkingEl.remove();
        thinkingEl = null;
        thinkingContent = '';
    }
}

function findMatchingUserMessage(content) {
    const userMessages = Array.from(messagesEl.querySelectorAll('.chat-msg.user'));
    return userMessages.reverse().find((el) => !el.dataset.runId && el.textContent.trim() === content.trim()) || null;
}

function ensureRunUserMessage(run) {
    let userEl = messagesEl.querySelector(`.chat-msg.user[data-run-id="${run.run_id}"]`);
    if (!userEl) {
        userEl = findMatchingUserMessage(run.user_message);
    }
    if (userEl) {
        userEl.dataset.runId = run.run_id;
        return userEl;
    }
    return addMessage('user', run.user_message, null, { runId: run.run_id });
}

function hydrateActiveRun(run) {
    if (!run || !run.run_id) return;

    activeRunId = run.run_id;
    ensureRunUserMessage(run);
    clearTransientRunUi();

    for (const event of run.events || []) {
        if (event.event_type === 'tool_start') {
            showToolIndicator(event.name, event.tool_call || null);
        } else if (event.event_type === 'tool_end') {
            endToolIndicator(event.name);
        }
    }

    if (run.assistant_response) {
        if (toolIndicatorEl) {
            morphIndicatorToStreaming();
        }
        handleStreamChunk(run.assistant_response);
    }

    if (run.status === 'stopping') {
        setProcessing(true);
        setRunBadge('stopping', 'Stopping');
    } else if (run.status === 'running') {
        setProcessing(true);
        setRunBadge('working', 'Running');
    }

    syncEmptyState();
}

async function restoreActiveRun() {
    try {
        const res = await fetch('/api/v1/chat/run');
        if (!res.ok) return;
        const run = await res.json();
        if (!run) {
            activeRunId = null;
            return;
        }
        hydrateActiveRun(run);
    } catch (e) {
        console.error('Failed to restore active run:', e);
    }
}

function setRunBadge(mode, label) {
    if (!runBadgeEl) return;
    const safeLabel = label || '';
    runBadgeEl.textContent = safeLabel;
    runBadgeEl.className = `chat-run-badge is-${mode}${safeLabel ? '' : ' is-dot-only'}`;
    runBadgeEl.setAttribute('aria-label', safeLabel || mode);
    runBadgeEl.title = safeLabel || mode;
}

// ─── Tool indicators ───────────────────────────────────────────

// List of tool names currently executing (for multi-tool sequences)
let activeTools = [];

// Reasoning section element (collapsible container for tool calls)
let reasoningSectionEl = null;
let reasoningContentEl = null;
let reasoningCount = 0;
let reasoningTools = [];

/** Create the collapsible reasoning section */
function createReasoningSection() {
    if (reasoningSectionEl) return reasoningSectionEl;

    reasoningSectionEl = document.createElement('div');
    reasoningSectionEl.className = 'chat-reasoning collapsed';
    reasoningSectionEl.innerHTML = `
        <div class="chat-reasoning-header" onclick="toggleReasoning(this)">
            <span class="chat-reasoning-summary">
                <span class="chat-reasoning-label">Tool activity</span>
                <span class="chat-reasoning-count">0 steps</span>
            </span>
            <span class="chat-reasoning-toggle">›</span>
        </div>
        <div class="chat-reasoning-content"></div>
    `;

    reasoningContentEl = reasoningSectionEl.querySelector('.chat-reasoning-content');
    if (toolIndicatorEl && toolIndicatorEl.parentElement === messagesEl) {
        messagesEl.insertBefore(reasoningSectionEl, toolIndicatorEl);
    } else if (streamingEl && streamingEl.parentElement === messagesEl) {
        messagesEl.insertBefore(reasoningSectionEl, streamingEl);
    } else {
        messagesEl.appendChild(reasoningSectionEl);
    }
    messagesEl.scrollTop = messagesEl.scrollHeight;

    return reasoningSectionEl;
}

/** Toggle reasoning section visibility */
window.toggleReasoning = function(headerEl) {
    const section = headerEl.closest('.chat-reasoning');
    if (section) {
        section.classList.toggle('collapsed');
    }
};

/** Update reasoning count */
function updateReasoningCount() {
    if (reasoningSectionEl) {
        const countEl = reasoningSectionEl.querySelector('.chat-reasoning-count');
        if (countEl) {
            countEl.textContent = `${reasoningCount} step${reasoningCount === 1 ? '' : 's'}`;
        }
        const labelEl = reasoningSectionEl.querySelector('.chat-reasoning-label');
        if (labelEl) {
            labelEl.textContent = reasoningHeadline();
        }
    }
}

function showToolIndicator(toolName, toolCallData) {
    activeTools.push(toolName);

    if (toolCallData && !reasoningSectionEl) {
        createReasoningSection();
    }

    if (!toolIndicatorEl) {
        toolIndicatorEl = document.createElement('div');
        toolIndicatorEl.className = 'chat-msg tool-indicator';
        if (reasoningSectionEl && reasoningSectionEl.parentElement === messagesEl) {
            if (reasoningSectionEl.nextSibling) {
                messagesEl.insertBefore(toolIndicatorEl, reasoningSectionEl.nextSibling);
            } else {
                messagesEl.appendChild(toolIndicatorEl);
            }
        } else {
            messagesEl.appendChild(toolIndicatorEl);
        }
    }

    toolIndicatorEl.textContent = toolStatusLabel(
        activeTools[activeTools.length - 1],
        toolCallData?.arguments || null
    );
    setRunBadge('working', 'Running');
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
    card.dataset.toolName = toolCallData.name || '';
    card.dataset.toolStatus = 'running';

    const description = describeToolCall(toolCallData);
    card.innerHTML = '<div class="chat-tool-call-compact">' +
        '<span class="chat-tool-call-name">' + escapeHtml(description.label) + '</span>' +
        (description.detail ? '<span class="chat-tool-summary">' + escapeHtml(description.detail) + '</span>' : '') +
        '<span class="chat-tool-call-meta">Running</span>' +
        '</div>';

    reasoningContentEl.appendChild(card);
    reasoningCount++;
    reasoningTools.push(toolCallData.name || '');
    updateReasoningCount();
    currentToolCalls.push(toolCallData.id);

    // Auto-expand while tools are running
    reasoningSectionEl.classList.remove('collapsed');

    messagesEl.scrollTop = messagesEl.scrollHeight;
}

/** Truncate a string with ellipsis */
function truncate(str, maxLen) {
    if (!str) return '';
    return str.length > maxLen ? str.substring(0, maxLen) + '...' : str;
}

function reasoningHeadline() {
    if (reasoningTools.some((name) => name === 'web_search' || name === 'web_fetch' || name === 'browser')) {
        return 'Searched the web';
    }
    if (reasoningTools.some((name) => name === 'shell')) {
        return 'Ran commands';
    }
    if (reasoningTools.length > 0) {
        return 'Used tools';
    }
    return 'Tool activity';
}

function describeToolCall(toolCallData) {
    const args = toolCallData.arguments || {};
    const name = toolCallData.name || 'tool';

    if (name === 'web_search') {
        return {
            label: 'Searched the web',
            detail: args.query ? `"${truncate(String(args.query), 56)}"` : '',
        };
    }

    if (name === 'web_fetch') {
        return {
            label: 'Opened a source',
            detail: summarizeUrl(args.url),
        };
    }

    if (name === 'browser') {
        const action = args.action ? String(args.action) : '';
        if (action === 'navigate') {
            return { label: 'Opened a page', detail: summarizeUrl(args.url) };
        }
        if (action === 'click') {
            return { label: 'Followed a result', detail: args.ref ? `[${args.ref}]` : '' };
        }
        if (action === 'type') {
            return {
                label: 'Typed into the page',
                detail: args.text ? `"${truncate(String(args.text), 40)}"` : '',
            };
        }
        if (action === 'snapshot') {
            return { label: 'Read the page', detail: '' };
        }
        return { label: 'Used the browser', detail: action || '' };
    }

    if (name === 'shell') {
        return {
            label: 'Ran a command',
            detail: args.command ? truncate(String(args.command), 56) : '',
        };
    }

    return {
        label: prettifyToolName(name),
        detail: '',
    };
}

function summarizeUrl(url) {
    if (!url) return '';
    try {
        const parsed = new URL(String(url));
        return parsed.hostname.replace(/^www\./, '');
    } catch (_) {
        return truncate(String(url), 56);
    }
}

function prettifyToolName(name) {
    if (!name) return 'Used a tool';
    return String(name)
        .split(/[_-]+/)
        .filter(Boolean)
        .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
        .join(' ');
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
    const completedCard = Array.from(document.querySelectorAll('.chat-tool-call'))
        .reverse()
        .find((card) => card.dataset.toolName === toolName && !card.classList.contains('is-complete'));
    if (completedCard) {
        completedCard.classList.add('is-complete');
        completedCard.dataset.toolStatus = 'done';
        const meta = completedCard.querySelector('.chat-tool-call-meta');
        if (meta) {
            meta.textContent = 'Done';
        }
    }
    if (activeTools.length > 0 && toolIndicatorEl) {
        // Still tools running — update label to the current one
        toolIndicatorEl.textContent = toolStatusLabel(activeTools[activeTools.length - 1]);
    } else if (activeTools.length === 0 && toolIndicatorEl) {
        toolIndicatorEl.textContent = 'Preparing response…';
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
        reasoningSectionEl.classList.add('collapsed');
        updateReasoningCount();
    }
}

/** Reset reasoning section for next message */
function resetReasoning() {
    reasoningSectionEl = null;
    reasoningContentEl = null;
    reasoningCount = 0;
    reasoningTools = [];
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
            <span class="chat-thinking-label">Thinking</span>
            <span class="chat-thinking-toggle">›</span>
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
    thinkingEl.classList.add('has-content', 'is-live');
    const contentEl = thinkingEl.querySelector('.chat-thinking-content');
    if (contentEl) {
        contentEl.textContent = thinkingContent;
    }

    messagesEl.scrollTop = messagesEl.scrollHeight;
}

/** Finalize thinking block (collapses it if there's content) */
function finalizeThinking() {
    if (thinkingEl && thinkingContent) {
        thinkingEl.classList.remove('collapsed');
        thinkingEl.classList.remove('is-live');
        thinkingEl.classList.add('has-content');

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
        wsStatus.textContent = 'Live';
        wsStatus.className = 'chat-connection is-live';
        setRunBadge(isProcessing ? 'working' : 'idle', isProcessing ? 'Running' : '');
        if (reconnectTimer) {
            clearTimeout(reconnectTimer);
            reconnectTimer = null;
        }
        // Load conversation history from DB
        loadHistory();
        restoreActiveRun();
    };

    ws.onmessage = (event) => {
        try {
            const data = JSON.parse(event.data);

            if (data.type === 'connected') {
                syncEmptyState();
                restoreActiveRun();

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
                activeRunId = null;
            } else if (data.type === 'error') {
                setProcessing(false);
                showToast(data.message || 'Chat error', 'error');
            }
        } catch (e) {
            console.error('Failed to parse message:', e);
        }
    };

    ws.onclose = () => {
        wsStatus.textContent = 'Disconnected';
        wsStatus.className = 'chat-connection is-offline';
        streamingEl = null;
        streamingContent = '';
        removeToolIndicator();
        setRunBadge('offline', 'Offline');
        setProcessing(false);
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
    syncEmptyState();
}

// ─── Message rendering ─────────────────────────────────────────

function addMessage(role, content, toolsUsed, options = {}) {
    const div = document.createElement('div');
    div.className = `chat-msg ${role}`;
    if (options.runId) {
        div.dataset.runId = options.runId;
    }
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
    syncEmptyState();
    return div;
}

// ─── Form submission ───────────────────────────────────────────

function sendCurrentMessage() {
    const text = chatText.value.trim();
    if (!text || isProcessing || !ws || ws.readyState !== WebSocket.OPEN) return;

    addMessage('user', text);
    ws.send(JSON.stringify({ content: text }));
    chatText.value = '';
    chatText.style.height = 'auto';
    chatText.focus();
    setProcessing(true);
    closeChatPlusMenu();
}

chatForm.addEventListener('submit', (e) => {
    e.preventDefault();
    if (isProcessing) {
        return;
    }
    sendCurrentMessage();
});

// ─── Stop button ─────────────────────────────────────────────────

/** Set processing state and toggle Send/Stop buttons */
function setProcessing(processing) {
    isProcessing = processing;
    if (btnSend) {
        btnSend.classList.toggle('is-processing', processing);
        btnSend.classList.remove('is-stopping');
        btnSend.setAttribute('aria-label', processing ? 'Stop current run' : 'Send message');
        btnSend.title = processing ? 'Stop' : 'Send';
    }
    if (chatText) chatText.disabled = false;
    if (!processing) {
        setRunBadge(ws && ws.readyState === WebSocket.OPEN ? 'idle' : 'offline', ws && ws.readyState === WebSocket.OPEN ? '' : 'Offline');
    }
}

/** Handle stop button click */
async function handleStop() {
    try {
        const res = await fetch('/api/v1/chat/stop', { method: 'POST' });
        const data = await res.json();
        if (data.ok) {
            if (btnSend) {
                btnSend.classList.add('is-processing', 'is-stopping');
            }
            showToast('Stopping...', 'warning');
        } else {
            showToast('Failed to stop', 'error');
        }
    } catch (e) {
        console.error('Failed to send stop:', e);
        showToast('Failed to stop', 'error');
    }
}

btnSend?.addEventListener('click', () => {
    if (isProcessing) {
        handleStop();
        return;
    }
    sendCurrentMessage();
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
    clearBrowserGallery();
    clearTransientRunUi();
    activeRunId = null;
    syncEmptyState();
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
            clearTransientRunUi();
            activeRunId = null;
            syncEmptyState();
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
        currentOpt.value = currentModel;
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

function toolStatusLabel(toolName, args) {
    if (toolName === 'web_search') return 'Ricerca web in corso';
    if (toolName === 'browser') {
        const action = args && args.action ? String(args.action) : '';
        if (action === 'navigate') return 'Navigazione browser in corso';
        if (action === 'snapshot') return 'Lettura pagina in corso';
        return 'Browser in corso';
    }
    if (toolName === 'web_fetch') return 'Apertura fonte in corso';
    if (toolName === 'shell') return 'Esecuzione comandi in corso';
    return `Uso ${toolName} in corso`;
}

function closeChatPlusMenu() {
    if (chatPlusMenu) {
        chatPlusMenu.hidden = true;
    }
}

chatPlusBtn?.addEventListener('click', (e) => {
    e.stopPropagation();
    if (!chatPlusMenu) return;
    chatPlusMenu.hidden = !chatPlusMenu.hidden;
});

document.addEventListener('click', () => {
    closeChatPlusMenu();
});

chatPlusMenu?.addEventListener('click', (e) => {
    e.stopPropagation();
});

btnChatUploadImage?.addEventListener('click', () => {
    closeChatPlusMenu();
    chatImageInput?.click();
});

btnChatUploadDoc?.addEventListener('click', () => {
    closeChatPlusMenu();
    chatDocInput?.click();
});

btnChatOpenMcp?.addEventListener('click', () => {
    closeChatPlusMenu();
    window.location.href = '/mcp';
});

chatImageInput?.addEventListener('change', () => {
    if (chatImageInput.files && chatImageInput.files.length > 0) {
        showToast('Image uploads are next on the chat roadmap', 'info');
    }
});

chatDocInput?.addEventListener('change', () => {
    if (chatDocInput.files && chatDocInput.files.length > 0) {
        showToast('Document uploads are next on the chat roadmap', 'info');
    }
});

/** Handle model change */
if (chatModelSelect) {
    chatModelSelect.addEventListener('change', async function() {
        const newModel = chatModelSelect.value;
        if (!newModel || newModel === currentModel) return;

        try {
            const res = await fetch('/api/v1/config', {
                method: 'PATCH',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ key: 'agent.model', value: newModel })
            });

            if (res.ok) {
                currentModel = newModel;
                if (chatConfig) {
                    chatConfig.dataset.model = newModel;
                }
                showToast('Model switched to ' + newModel.split('/').pop(), 'success');

                // Update display
                const opt = chatModelSelect.options[0];
                opt.value = newModel;
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
closeChatPlusMenu();
setRunBadge('offline', 'Offline');
syncEmptyState();

// Start connection
connect();
