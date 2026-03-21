// Homun — Chat WebSocket client with streaming, markdown, and tool indicators

const messagesEl = document.getElementById('messages');
const threadWrapEl = document.querySelector('.chat-thread-wrap');
const chatForm = document.getElementById('chat-form');
const chatText = document.getElementById('chat-text');
const wsStatus = document.getElementById('ws-status');
const chatPlanPanel = document.getElementById('chat-plan-panel');
const chatPlanToggle = document.getElementById('chat-plan-toggle');
const chatPlanSummary = document.getElementById('chat-plan-summary');
const chatPlanTasklist = document.getElementById('chat-plan-tasklist');
const btnSend = document.getElementById('btn-send');
const chatEmptyState = document.getElementById('chat-empty-state');
const chatShellEl = document.querySelector('.chat-shell');
const conversationListEl = document.getElementById('chat-conversation-list');
const conversationTitleEl = document.getElementById('chat-conversation-title');
const btnChatSidebar = document.getElementById('btn-chat-sidebar');
// Search modal
const btnChatSearch = document.getElementById('btn-chat-search');
const chatSearchModal = document.getElementById('chat-search-modal');
const chatSearchInput = document.getElementById('chat-search-input');
const chatSearchResults = document.getElementById('chat-search-results');
const btnChatSearchClose = document.getElementById('btn-chat-search-close');
const chatSearchIncludeArchived = document.getElementById('chat-search-include-archived');
// Bulk actions
const chatBulkActions = document.getElementById('chat-bulk-actions');
const chatBulkCount = document.getElementById('chat-bulk-count');
const btnBulkArchive = document.getElementById('btn-bulk-archive');
const btnBulkDelete = document.getElementById('btn-bulk-delete');
const btnBulkCancel = document.getElementById('btn-bulk-cancel');
const runBadgeEl = document.getElementById('chat-run-badge');
const runModelEl = document.getElementById('chat-run-model');
const chatPlusBtn = document.getElementById('btn-chat-plus');
const chatPlusMenu = document.getElementById('chat-plus-menu');
const chatAttachmentStrip = document.getElementById('chat-attachment-strip');
const chatImageInput = document.getElementById('chat-image-input');
const chatDocInput = document.getElementById('chat-doc-input');
const btnChatUploadImage = document.getElementById('btn-chat-upload-image');
const btnChatUploadDoc = document.getElementById('btn-chat-upload-doc');
const btnChatOpenMcp = document.getElementById('btn-chat-open-mcp');
const chatMcpPicker = document.getElementById('chat-mcp-picker');
const chatMcpSearch = document.getElementById('chat-mcp-search');
const chatMcpPickerList = document.getElementById('chat-mcp-picker-list');
const chatModalBackdrop = document.getElementById('chat-modal-backdrop');
const chatModalTitle = document.getElementById('chat-modal-title');
const chatModalCopy = document.getElementById('chat-modal-copy');
let chatModalInput = document.getElementById('chat-modal-input');
// Ensure input exists even if HTML template hasn't been recompiled
if (!chatModalInput) {
    const copy = document.getElementById('chat-modal-copy');
    if (copy) {
        chatModalInput = document.createElement('input');
        chatModalInput.type = 'text';
        chatModalInput.className = 'chat-modal-input';
        chatModalInput.id = 'chat-modal-input';
        chatModalInput.hidden = true;
        chatModalInput.autocomplete = 'off';
        copy.insertAdjacentElement('afterend', chatModalInput);
    }
}
const chatModalCancel = document.getElementById('chat-modal-cancel');
const chatModalConfirm = document.getElementById('chat-modal-confirm');
const chatMainEl = document.querySelector('.chat-main');
const chatWelcomeGreeting = document.getElementById('chat-welcome-greeting');
const chatWelcomePhrase = document.getElementById('chat-welcome-phrase');
const chatDragOverlay = document.getElementById('chat-drag-overlay');
const btnChatTools = document.getElementById('btn-chat-tools');
const chatToolsLabel = document.getElementById('chat-tools-label');
const chatToolsDismiss = document.getElementById('chat-tools-dismiss');

let ws = null;
let reconnectTimer = null;
let loadedConversationId = null;
let currentConversationId = null;
let socketConversationId = null;
let suppressReconnect = false;
let conversations = [];
let conversationSearch = '';
let showArchived = false;
let sidebarCollapsed = false;
let openConversationMenuId = null;
let conversationPollTimer = null;
let previouslyRunningIds = new Set();
let multiSelectMode = false;
let selectedConversations = new Set();
let searchDebounceTimer = null;
let modalState = null;
let pendingAttachments = [];
let pendingMcpServers = [];
let availableMcpServers = [];
let mcpPickerOpen = false;
let mcpSearchQuery = '';
let isRecording = false;
let recognition = null;
let activeToolMode = null;
let currentPlanState = null;
let planExpanded = false;
const GENERIC_PLAN_CONSTRAINTS = new Set([
    'Cover every requested option/source and compare them before finalizing.',
    'Treat date/time-sensitive details as current and verify them from fresh evidence.',
    'Respect explicit numeric, date, price, time, and threshold constraints from the request.',
    'For multi-step forms, confirm each required field/widget before submitting.',
    'Complete all distinct sub-requests in the prompt before stopping.',
]);

// Track the currently streaming message element so we can
// append incremental deltas as they arrive from the LLM.
let streamingEl = null;
let streamingContent = '';
let streamRenderRafId = null;
let lastStreamRenderTime = 0;
const STREAM_RENDER_INTERVAL = 150; // ms — throttle gate for progressive markdown

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
        // safe: DOMPurify sanitizes all HTML; ADD_ATTR allows target for new-tab links
        const sanitized = DOMPurify.sanitize(rawHtml, { ADD_ATTR: ['target'] });
        el.innerHTML = sanitized; // safe: sanitized by DOMPurify above

        // All links open in new tab
        el.querySelectorAll('a[href]').forEach(a => {
            a.setAttribute('target', '_blank');
            a.setAttribute('rel', 'noopener noreferrer');
        });

        // Images: click-to-expand lightbox overlay
        el.querySelectorAll('img').forEach(img => {
            img.style.cursor = 'zoom-in';
            img.addEventListener('click', () => openImageLightbox(img.src, img.alt));
        });

        // Syntax highlighting
        if (typeof hljs !== 'undefined') {
            el.querySelectorAll('pre code').forEach(block => {
                hljs.highlightElement(block);
            });
        }

        // Code blocks: add copy button
        el.querySelectorAll('pre').forEach(pre => {
            if (pre.querySelector('.chat-code-copy-btn')) return;
            const btn = document.createElement('button');
            btn.type = 'button';
            btn.className = 'chat-code-copy-btn';
            btn.textContent = 'Copy';
            btn.addEventListener('click', () => {
                const code = pre.querySelector('code');
                navigator.clipboard.writeText(code ? code.textContent : pre.textContent);
                btn.textContent = 'Copied!';
                setTimeout(() => { btn.textContent = 'Copy'; }, 1500);
            });
            pre.style.position = 'relative';
            pre.appendChild(btn);
        });
    } else if (role === 'user' && typeof DOMPurify !== 'undefined') {
        // User messages: light inline markdown (bold, italic, code) but no block elements
        let html = escapeHtml(content);
        html = html.replace(/`([^`]+)`/g, '<code>$1</code>');
        html = html.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
        html = html.replace(/\*([^*]+)\*/g, '<em>$1</em>');
        // safe: escapeHtml first, then only add known safe tags
        el.innerHTML = DOMPurify.sanitize(html, { ALLOWED_TAGS: ['code', 'strong', 'em'] });
    } else {
        el.textContent = content;
    }
}

/** Whether the user is near the bottom of the scroll area. */
let userNearBottom = true;
/** Flag to ignore scroll events triggered by programmatic scrollTo. */
let programmaticScroll = false;

function isNearBottom(scroller) {
    return scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 120;
}

/** Only auto-scroll if the user hasn't scrolled up. */
function scrollThreadToBottom() {
    const scroller = threadWrapEl || messagesEl;
    if (!scroller) return;
    if (!userNearBottom) {
        syncScrollBtn();
        return;
    }
    programmaticScroll = true;
    scroller.scrollTop = scroller.scrollHeight;
    syncScrollBtn();
    // Reset flag after browser processes the scroll
    requestAnimationFrame(() => { programmaticScroll = false; });
}

/** Force scroll to bottom (user action: send message, click arrow, load history). */
function forceScrollToBottom() {
    const scroller = threadWrapEl || messagesEl;
    if (!scroller) return;
    userNearBottom = true;
    programmaticScroll = true;
    scroller.scrollTop = scroller.scrollHeight;
    syncScrollBtn();
    requestAnimationFrame(() => { programmaticScroll = false; });
}

/** Show/hide the scroll-to-bottom floating button. */
function syncScrollBtn() {
    const btn = document.getElementById('chat-scroll-bottom');
    if (!btn) return;
    const scroller = threadWrapEl || messagesEl;
    if (!scroller) return;
    btn.hidden = isNearBottom(scroller);
}

// Track ONLY user-initiated scroll, ignore programmatic scroll
(function initScrollTracking() {
    const scroller = threadWrapEl || messagesEl;
    if (!scroller) return;
    scroller.addEventListener('scroll', () => {
        if (programmaticScroll) return;
        userNearBottom = isNearBottom(scroller);
        syncScrollBtn();
    }, { passive: true });
})();

// ─── Chat history ──────────────────────────────────────────────

function conversationApi(path) {
    const url = new URL(path, window.location.origin);
    if (currentConversationId) {
        url.searchParams.set('conversation_id', currentConversationId);
    }
    return url.pathname + url.search;
}

function conversationResourceUrl(conversationId) {
    return `/api/v1/chat/conversations/${encodeURIComponent(conversationId)}`;
}

function setConversationUrl(conversationId) {
    const url = new URL(window.location.href);
    url.searchParams.set('c', conversationId);
    window.history.replaceState({}, '', url);
}

function currentConversationTitle() {
    const active = conversations.find((item) => item.conversation_id === currentConversationId);
    if (active && active.title) return active.title;
    if (conversationTitleEl && conversationTitleEl.textContent.trim()) {
        return conversationTitleEl.textContent.trim();
    }
    return 'New conversation';
}

function truncateConversationText(value, max = 48) {
    const compact = String(value || '').trim().replace(/\s+/g, ' ');
    if (!compact) return '';
    return compact.length > max ? `${compact.slice(0, max).trimEnd()}…` : compact;
}

function capitalizeFirst(text) {
    if (!text) return text;
    return text.charAt(0).toUpperCase() + text.slice(1);
}

function formatConversationTimestamp(value) {
    if (!value) return '';
    const parsed = new Date(value);
    if (Number.isNaN(parsed.getTime())) return '';
    const now = new Date();
    const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
    const yesterday = new Date(today); yesterday.setDate(yesterday.getDate() - 1);
    const itemDay = new Date(parsed.getFullYear(), parsed.getMonth(), parsed.getDate());
    if (itemDay >= today) {
        return parsed.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
    }
    if (itemDay >= yesterday) {
        return 'Ieri';
    }
    return parsed.toLocaleDateString([], { day: 'numeric', month: 'short' });
}

function groupConversationsByDate(convos) {
    const now = new Date();
    const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
    const yesterday = new Date(today); yesterday.setDate(yesterday.getDate() - 1);
    const groups = [];
    let todayItems = [], yesterdayItems = [], olderItems = [];
    for (const c of convos) {
        const d = new Date(c.updated_at);
        const itemDay = new Date(d.getFullYear(), d.getMonth(), d.getDate());
        if (itemDay >= today) todayItems.push(c);
        else if (itemDay >= yesterday) yesterdayItems.push(c);
        else olderItems.push(c);
    }
    if (todayItems.length) groups.push({ label: 'Oggi', items: todayItems });
    if (yesterdayItems.length) groups.push({ label: 'Ieri', items: yesterdayItems });
    if (olderItems.length) groups.push({ label: 'Meno recenti', items: olderItems });
    return groups;
}

function syncConversationHeader() {
    if (!conversationTitleEl) return;
    conversationTitleEl.textContent = capitalizeFirst(currentConversationTitle());
}

function applySidebarState() {
    if (!chatShellEl) return;
    chatShellEl.classList.toggle('is-sidebar-collapsed', sidebarCollapsed);
    // Expose sidebar width as CSS var so welcome layout can center on viewport
    chatShellEl.style.setProperty('--sidebar-w', sidebarCollapsed ? '0px' : '272px');
}

// sidebar menu removed — search modal replaces it

function dismissDropdown() {
    const existing = document.querySelector('.chat-conv-dropdown');
    if (existing) existing.remove();
    openConversationMenuId = null;
}

function closeConversationMenu() {
    dismissDropdown();
    renderConversationList();
}

function openConversationDropdown(conversation, anchorEl) {
    closeConversationMenu();
    openConversationMenuId = conversation.conversation_id;
    renderConversationList();

    var icRename = '<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M15.2 5.2l-1.8-1.8a2.5 2.5 0 00-3.5 0L3.5 9.9V14.5h4.6l6.4-6.4a2.5 2.5 0 000-3.5z"/></svg>';
    var icArchive = '<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="3" width="14" height="3" rx="1"/><path d="M3 6v8a1 1 0 001 1h10a1 1 0 001-1V6"/><path d="M7 10h4"/></svg>';
    var icDelete = '<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M3 5h12"/><path d="M7 5V3h4v2"/><path d="M5 5v10a1 1 0 001 1h6a1 1 0 001-1V5"/></svg>';
    var icSelect = '<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="12" height="12" rx="2"/><path d="M6 9l2 2 4-4"/></svg>';

    const menu = document.createElement('div');
    menu.className = 'chat-conv-dropdown';

    function addItem(icon, label, cls, handler) {
        const btn = document.createElement('button');
        btn.type = 'button';
        btn.className = 'chat-conv-dropdown-item' + (cls ? ' ' + cls : '');
        btn.appendChild(parseSvg(icon));
        const span = document.createElement('span');
        span.textContent = label;
        btn.appendChild(span);
        btn.addEventListener('click', function(e) {
            e.stopPropagation();
            handler();
        });
        menu.appendChild(btn);
    }

    addItem(icRename, 'Rename', '', () => renameConversation(conversation));
    addItem(icArchive, conversation.archived ? 'Restore' : 'Archive', '', () => setConversationArchived(conversation, !conversation.archived));

    const sep = document.createElement('div');
    sep.className = 'chat-conv-dropdown-sep';
    menu.appendChild(sep);

    addItem(icDelete, 'Delete', 'is-danger', () => deleteConversation(conversation));

    const sep2 = document.createElement('div');
    sep2.className = 'chat-conv-dropdown-sep';
    menu.appendChild(sep2);

    addItem(icSelect, 'Select', '', () => {
        closeConversationMenu();
        enterMultiSelectMode(conversation.conversation_id);
    });

    document.body.appendChild(menu);

    // anchorEl may be detached after renderConversationList() rebuilt the DOM,
    // so find the live button via its .is-open class instead.
    requestAnimationFrame(() => {
        const liveAnchor = document.querySelector('.chat-conv-more-btn.is-open') || anchorEl;
        const rect = liveAnchor.getBoundingClientRect();
        let top = rect.bottom + 4;
        let left = rect.right - menu.offsetWidth;
        if (left < 8) left = 8;
        if (top + menu.offsetHeight > window.innerHeight - 8) {
            top = rect.top - menu.offsetHeight - 4;
        }
        menu.style.top = top + 'px';
        menu.style.left = left + 'px';
    });
}

function openModal({ title, copy, confirmLabel = 'Confirm', destructive = false, inputValue, inputPlaceholder, onConfirm }) {
    modalState = { onConfirm, hasInput: inputValue !== undefined };
    if (chatModalTitle) chatModalTitle.textContent = title;
    if (chatModalCopy) chatModalCopy.textContent = copy || '';
    if (chatModalInput) {
        if (inputValue !== undefined) {
            chatModalInput.hidden = false;
            chatModalInput.value = inputValue;
            chatModalInput.placeholder = inputPlaceholder || '';
            setTimeout(() => { chatModalInput.focus(); chatModalInput.select(); }, 0);
        } else {
            chatModalInput.hidden = true;
            chatModalInput.value = '';
        }
    }
    if (chatModalConfirm) {
        chatModalConfirm.textContent = confirmLabel;
        chatModalConfirm.classList.toggle('btn-danger', destructive);
        chatModalConfirm.classList.toggle('btn-primary', !destructive);
    }
    if (chatModalBackdrop) {
        chatModalBackdrop.hidden = false;
    }
}

function closeModal() {
    modalState = null;
    if (chatModalBackdrop) chatModalBackdrop.hidden = true;
    if (chatModalInput) { chatModalInput.hidden = true; chatModalInput.value = ''; }
}

/** Open a full-screen lightbox for an image. */
function openImageLightbox(src, alt) {
    // Reuse existing or create overlay
    let overlay = document.getElementById('chat-lightbox');
    if (!overlay) {
        overlay = document.createElement('div');
        overlay.id = 'chat-lightbox';
        overlay.className = 'chat-lightbox';
        overlay.addEventListener('click', () => { overlay.hidden = true; });
        document.body.appendChild(overlay);
    }
    const img = document.createElement('img');
    img.src = src;
    img.alt = alt || '';
    overlay.textContent = '';
    overlay.appendChild(img);
    overlay.hidden = false;
}

function updateConversationSummary(mutator) {
    const conversation = conversations.find((item) => item.conversation_id === currentConversationId);
    if (!conversation) return;
    mutator(conversation);
    renderConversationList();
    syncConversationHeader();
}

function renderConversationList() {
    if (!conversationListEl) return;
    if (conversations.length === 0) {
        conversationListEl.textContent = '';
        const empty = document.createElement('div');
        empty.className = 'chat-conversation-empty';
        empty.textContent = 'No conversations yet.';
        conversationListEl.appendChild(empty);
        return;
    }

    conversationListEl.textContent = '';
    const groups = groupConversationsByDate(conversations);
    groups.forEach((group) => {
        const header = document.createElement('div');
        header.className = 'chat-date-group';
        header.textContent = group.label;
        conversationListEl.appendChild(header);

        group.items.forEach((conversation) => {
            conversationListEl.appendChild(buildConversationItem(conversation));
        });
    });
    syncBulkActions();
}

function buildConversationItem(conversation) {
    var icMore = '<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><circle cx="9" cy="4" r="1.2"/><circle cx="9" cy="9" r="1.2"/><circle cx="9" cy="14" r="1.2"/></svg>';

    const item = document.createElement('div');
    item.className = 'chat-conversation-item';
    if (conversation.conversation_id === currentConversationId) item.classList.add('is-active');
    if (conversation.active_run && (conversation.active_run.status === 'running' || conversation.active_run.status === 'stopping')) {
        item.classList.add('is-running');
    }
    if (multiSelectMode) {
        item.classList.add('is-selectable');
        if (selectedConversations.has(conversation.conversation_id)) item.classList.add('is-selected');
    }
    if (openConversationMenuId === conversation.conversation_id) item.classList.add('has-menu-open');

    // Checkbox — only visible in multi-select mode via CSS
    const checkWrap = document.createElement('div');
    checkWrap.className = 'chat-conv-check';
    const cb = document.createElement('input');
    cb.type = 'checkbox';
    cb.checked = multiSelectMode && selectedConversations.has(conversation.conversation_id);
    cb.addEventListener('click', function(e) {
        e.stopPropagation();
        toggleConversationSelection(conversation.conversation_id);
    });
    checkWrap.appendChild(cb);
    item.appendChild(checkWrap);

    // Name
    const nameEl = document.createElement('span');
    nameEl.className = 'chat-conversation-name';
    nameEl.textContent = capitalizeFirst(conversation.title) || 'New conversation';
    nameEl.addEventListener('click', function() {
        if (multiSelectMode) { toggleConversationSelection(conversation.conversation_id); return; }
        if (conversation.conversation_id !== currentConversationId) selectConversation(conversation.conversation_id);
    });
    item.appendChild(nameEl);

    // Trailing: timestamp (default) / 3-dot button (hover)
    const trailing = document.createElement('div');
    trailing.className = 'chat-conv-trailing';

    const dateEl = document.createElement('span');
    dateEl.className = 'chat-conversation-date';
    dateEl.textContent = formatConversationTimestamp(conversation.updated_at);
    trailing.appendChild(dateEl);
    item.appendChild(trailing);

    // 3-dot menu button (appears on hover)
    const moreBtn = document.createElement('button');
    moreBtn.type = 'button';
    moreBtn.className = 'chat-conv-more-btn';
    if (openConversationMenuId === conversation.conversation_id) moreBtn.classList.add('is-open');
    moreBtn.title = 'Actions';
    moreBtn.appendChild(parseSvg(icMore));
    moreBtn.addEventListener('click', function(e) {
        e.stopPropagation();
        if (openConversationMenuId === conversation.conversation_id) {
            closeConversationMenu();
        } else {
            openConversationDropdown(conversation, moreBtn);
        }
    });
    item.appendChild(moreBtn);

    return item;
}

function parseSvg(str) {
    var t = document.createElement('template');
    t.innerHTML = str.trim();
    return t.content.firstChild;
}

async function refreshConversationList() {
    try {
        const url = new URL('/api/v1/chat/conversations', window.location.origin);
        url.searchParams.set('limit', '50');
        if (conversationSearch) url.searchParams.set('q', conversationSearch);
        if (showArchived) url.searchParams.set('include_archived', 'true');
        const res = await fetch(url.pathname + url.search);
        if (!res.ok) return;
        conversations = await res.json();
        if (!currentConversationId && conversations[0]) {
            currentConversationId = conversations[0].conversation_id;
        }

        // Detect background run completions: conversations that were running
        // on the previous poll but are no longer running now (INFRA-2).
        const nowRunning = new Set();
        for (const c of conversations) {
            if (c.active_run && (c.active_run.status === 'running' || c.active_run.status === 'stopping')) {
                nowRunning.add(c.conversation_id);
            }
        }
        for (const id of previouslyRunningIds) {
            if (!nowRunning.has(id) && id !== currentConversationId) {
                showToast('Background conversation completed', 'info');
                break; // One toast per poll cycle is enough
            }
        }
        previouslyRunningIds = nowRunning;

        renderConversationList();
        syncConversationHeader();
    } catch (e) {
        console.error('Failed to load conversations:', e);
    }
}

async function createConversation() {
    const res = await fetch('/api/v1/chat/conversations', { method: 'POST' });
    if (!res.ok) {
        throw new Error('Failed to create conversation');
    }
    return await res.json();
}

async function ensureConversationSelected() {
    await refreshConversationList();

    const params = new URLSearchParams(window.location.search);
    const requestedId = params.get('c') || window.localStorage.getItem('homun.chat.currentConversation');
    const requestedExists = requestedId && conversations.some((item) => item.conversation_id === requestedId);

    if (requestedExists) {
        currentConversationId = requestedId;
    } else if (conversations.length > 0) {
        currentConversationId = conversations[0].conversation_id;
    } else {
        const created = await createConversation();
        conversations = [created];
        currentConversationId = created.conversation_id;
        renderConversationList();
    }

    if (currentConversationId) {
        window.localStorage.setItem('homun.chat.currentConversation', currentConversationId);
        setConversationUrl(currentConversationId);
        renderConversationList();
        syncConversationHeader();
    }
}

async function updateConversation(conversationId, payload) {
    const res = await fetch(conversationResourceUrl(conversationId), {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
    });
    if (!res.ok) {
        throw new Error('Failed to update conversation');
    }
    return await res.json();
}

async function renameConversation(conversation) {
    closeConversationMenu();
    openModal({
        title: 'Rename conversation',
        copy: '',
        inputValue: conversation.title || 'New conversation',
        inputPlaceholder: 'Conversation name',
        confirmLabel: 'Save',
        onConfirm: async (newTitle) => {
            const nextTitle = String(newTitle || '').trim();
            if (!nextTitle || nextTitle === conversation.title) return;
            const oldTitle = conversation.title;
            conversation.title = nextTitle;
            const conv = conversations.find(c => c.conversation_id === conversation.conversation_id);
            if (conv) conv.title = nextTitle;
            renderConversationList();
            syncConversationHeader();
            try {
                await updateConversation(conversation.conversation_id, { title: nextTitle });
            } catch (e) {
                console.error('Failed to rename conversation:', e);
                conversation.title = oldTitle;
                if (conv) conv.title = oldTitle;
                renderConversationList();
                syncConversationHeader();
                showToast('Failed to rename conversation', 'error');
            }
        },
    });
}

async function setConversationArchived(conversation, archived) {
    closeConversationMenu();
    openModal({
        title: archived ? 'Archive conversation' : 'Restore conversation',
        copy: archived
            ? 'This conversation will move out of the main list until archived items are shown again.'
            : 'This conversation will return to the main list.',
        confirmLabel: archived ? 'Archive' : 'Restore',
        onConfirm: async () => {
            try {
                await updateConversation(conversation.conversation_id, { archived });
                await refreshConversationList();
                if (conversation.conversation_id === currentConversationId && archived && !showArchived) {
                    await ensureConversationSelectedAfterRemoval(conversation.conversation_id);
                }
                showToast(archived ? 'Conversation archived' : 'Conversation restored', 'success');
            } catch (e) {
                console.error('Failed to update archive state:', e);
                showToast('Failed to update conversation', 'error');
            }
        },
    });
}

async function ensureConversationSelectedAfterRemoval(removedId) {
    await refreshConversationList();
    const next = conversations.find((item) => item.conversation_id !== removedId);
    if (next) {
        await selectConversation(next.conversation_id);
        return;
    }
    const created = await createConversation();
    conversations = [created];
    currentConversationId = created.conversation_id;
    window.localStorage.setItem('homun.chat.currentConversation', currentConversationId);
    setConversationUrl(currentConversationId);
    renderConversationList();
    syncConversationHeader();
    disconnectSocket();
    connect();
}

async function deleteConversation(conversation) {
    closeConversationMenu();
    openModal({
        title: 'Delete conversation',
        copy: 'This will permanently remove the conversation history and cannot be undone.',
        confirmLabel: 'Delete',
        destructive: true,
        onConfirm: async () => {
            try {
                const res = await fetch(conversationResourceUrl(conversation.conversation_id), { method: 'DELETE' });
                const data = await res.json();
                if (!data.ok) throw new Error(data.message || 'Delete failed');
                if (conversation.conversation_id === currentConversationId) {
                    resetConversationView();
                    disconnectSocket();
                    await ensureConversationSelectedAfterRemoval(conversation.conversation_id);
                } else {
                    await refreshConversationList();
                }
                showToast('Conversation deleted', 'success');
            } catch (e) {
                console.error('Failed to delete conversation:', e);
                showToast('Failed to delete conversation', 'error');
            }
        },
    });
}

/** Load previous messages from the server on connect. */
async function loadHistory() {
    if (!currentConversationId || loadedConversationId === currentConversationId) return;
    try {
        const res = await fetch(conversationApi('/api/v1/chat/history?limit=50'));
        if (!res.ok) return;
        const messages = await res.json();
        loadedConversationId = currentConversationId;
        messagesEl.textContent = '';
        clearBrowserGallery();
        clearTransientRunUi();
        if (messages.length === 0) {
            syncEmptyState();
            return;
        }

        messages.forEach(m => {
            addMessage(m.role, m.content, m.tools_used, {
                messageId: m.id,
                attachments: m.attachments || [],
                mcpServers: m.mcp_servers || [],
                timestamp: m.timestamp || m.created_at,
            });
        });
        syncEmptyState();
        forceScrollToBottom();
    } catch (e) {
        console.error('Failed to load chat history:', e);
    }
}

const WELCOME_PHRASES = [
    'What would you like to explore?',
    'How can I help you today?',
    'Ready when you are.',
    'Ask me anything.',
    'Let\'s get something done.',
    'What\'s on your mind?',
    'Where shall we start?',
    'I\'m here to help.',
];

function syncEmptyState() {
    if (!messagesEl) return;
    const isEmpty = messagesEl.children.length === 0;

    // Toggle welcome class on chat-main for centered layout
    if (chatMainEl) chatMainEl.classList.toggle('is-welcome', isEmpty);

    // Populate greeting + random phrase on first show
    if (isEmpty && chatWelcomeGreeting) {
        const chatConfig = document.getElementById('chat-config');
        const username = chatConfig?.dataset?.username || '';
        const displayName = username ? username.charAt(0).toUpperCase() + username.slice(1) : '';
        chatWelcomeGreeting.textContent = displayName ? `Ciao ${displayName}` : 'Ciao';
    }
    if (isEmpty && chatWelcomePhrase) {
        chatWelcomePhrase.textContent = WELCOME_PHRASES[Math.floor(Math.random() * WELCOME_PHRASES.length)];
    }
}

function isUsefulPlanConstraint(item) {
    const text = String(item || '').trim();
    return Boolean(text) && !GENERIC_PLAN_CONSTRAINTS.has(text);
}

/** Apply or update the execution plan panel as a numbered task list. */
function applyExecutionPlan(plan) {
    currentPlanState = plan && typeof plan === 'object' ? plan : null;
    if (!chatPlanPanel || !chatPlanSummary || !chatPlanTasklist) return;

    // Hide if no plan or no meaningful content
    const hasExplicitSteps = Array.isArray(currentPlanState?.explicit_steps) && currentPlanState.explicit_steps.length > 0;
    const hasCompletedSteps = Array.isArray(currentPlanState?.completed_steps) && currentPlanState.completed_steps.length > 0;
    const hasBlockers = Array.isArray(currentPlanState?.active_blockers) && currentPlanState.active_blockers.length > 0;
    const hasSources = Array.isArray(currentPlanState?.required_sources) && currentPlanState.required_sources.length > 0;
    const hasPhase = Boolean(currentPlanState?.phase);
    const hasContent = hasExplicitSteps || hasCompletedSteps || hasBlockers || hasSources || hasPhase || currentPlanState?.current_source;
    if (!currentPlanState || (!currentPlanState.objective && !hasPhase) || (!hasContent && !isProcessing)) {
        chatPlanPanel.hidden = true;
        chatPlanPanel.classList.add('collapsed');
        if (chatPlanToggle) chatPlanToggle.setAttribute('aria-expanded', 'false');
        chatPlanSummary.textContent = '';
        chatPlanTasklist.textContent = '';
        return;
    }

    chatPlanPanel.hidden = false;

    // Auto-expand panel when orchestrator sends an explicit plan.
    const isOrchestrated = Boolean(currentPlanState?.phase);
    if (isOrchestrated && !planExpanded) {
        planExpanded = true;
    }
    chatPlanPanel.classList.toggle('collapsed', !planExpanded);
    if (chatPlanToggle) chatPlanToggle.setAttribute('aria-expanded', String(planExpanded));

    // Build unified task list
    chatPlanTasklist.textContent = '';
    let steps = [];

    if (hasExplicitSteps) {
        steps = currentPlanState.explicit_steps;
    } else {
        // Inferred plan — build steps from completed + remaining
        const doneItems = Array.isArray(currentPlanState.completed_steps) ? currentPlanState.completed_steps : [];
        const remainingItems = [
            ...(Array.isArray(currentPlanState.active_blockers) ? currentPlanState.active_blockers : []),
            ...(Array.isArray(currentPlanState.constraints) ? currentPlanState.constraints.filter(c => isUsefulPlanConstraint(c) && !doneItems.includes(c)) : []),
        ].filter((v, i, a) => v && a.indexOf(v) === i);
        steps = [
            ...doneItems.map(d => ({ status: 'completed', description: d })),
            ...remainingItems.map(r => ({ status: 'pending', description: r })),
        ];
    }

    const total = steps.length;
    const done = steps.filter(s => s.status === 'completed').length;

    // Show phase-aware summary for orchestrated tasks.
    const phase = currentPlanState?.phase || '';
    if (phase === 'planning') {
        chatPlanSummary.textContent = 'Analisi e pianificazione…';
    } else if (phase === 'synthesizing') {
        chatPlanSummary.textContent = 'Sintesi dei risultati…';
    } else if (total > 0) {
        chatPlanSummary.textContent = `${done} su ${total} attività completate`;
    } else {
        chatPlanSummary.textContent = '';
    }

    steps.forEach((step) => {
        const li = document.createElement('li');
        li.className = `chat-plan-task is-${step.status || 'pending'}`;

        const circle = document.createElement('span');
        circle.className = 'chat-plan-task-icon';
        if (step.status === 'completed') {
            circle.textContent = '\u2713';
        }

        const label = document.createElement('span');
        label.className = 'chat-plan-task-label';
        label.textContent = step.description || '';

        li.appendChild(circle);
        li.appendChild(label);
        chatPlanTasklist.appendChild(li);
    });
}

function clearExecutionPlan() {
    applyExecutionPlan(null);
}

chatPlanToggle?.addEventListener('click', () => {
    planExpanded = !planExpanded;
    if (chatPlanPanel) {
        chatPlanPanel.classList.toggle('collapsed', !planExpanded);
    }
    chatPlanToggle.setAttribute('aria-expanded', String(planExpanded));
});

function parsePlanPayload(raw) {
    if (!raw) return null;
    try {
        const parsed = JSON.parse(raw);
        return parsed && typeof parsed === 'object' ? parsed : null;
    } catch (_) {
        return null;
    }
}

function purgeOrphanLiveArtifacts() {
    if (!messagesEl) return;

    messagesEl.querySelectorAll('.chat-msg.assistant.streaming').forEach((el) => {
        if (el === streamingEl) {
            if (!streamingContent.trim() && !el.textContent.trim()) {
                el.remove();
                streamingEl = null;
                streamingContent = '';
            }
            return;
        }
        if (!el.textContent.trim()) {
            el.remove();
        } else {
            el.classList.remove('streaming');
        }
    });

    messagesEl.querySelectorAll('.chat-thinking').forEach((el) => {
        const contentEl = el.querySelector('.chat-thinking-content');
        const hasContent = !!(contentEl && contentEl.textContent.trim());
        if (el === thinkingEl) {
            if (!thinkingContent.trim() && !hasContent) {
                el.remove();
                thinkingEl = null;
                thinkingContent = '';
            }
            return;
        }
        if (!hasContent) {
            el.remove();
        }
    });

    messagesEl.querySelectorAll('.chat-reasoning').forEach((el) => {
        const hasCards = el.querySelectorAll('.chat-tool-call').length > 0;
        if (el === reasoningSectionEl) {
            if (!hasCards && reasoningCount === 0) {
                el.remove();
                resetReasoning();
            }
            return;
        }
        if (!hasCards) {
            el.remove();
        }
    });
}

function clearTransientRunUi() {
    purgeOrphanLiveArtifacts();
    removeCognition();
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
    return userMessages.reverse().find((el) => {
        if (el.dataset.runId) return false;
        const body = el.querySelector('.chat-msg-body');
        return (body ? body.textContent : '').trim() === content.trim();
    }) || null;
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
    clearExecutionPlan();
    let lastStatusHint = '';

    // Find the last plan event upfront — only apply the final plan snapshot
    // to avoid DOM thrashing from intermediate states.
    let lastPlanEvent = null;
    for (const event of run.events || []) {
        if (event.event_type === 'cognition_start') {
            showCognitionStep(event.name || 'Analyzing request...');
        } else if (event.event_type === 'cognition_step') {
            addCognitionStep(event.name || '');
        } else if (event.event_type === 'cognition_result') {
            finalizeCognition(event.name || 'Ready');
        } else if (event.event_type === 'subagent_start') {
            showToolIndicator('subagent', {
                id: 'subagent-hydrate',
                name: 'subagent',
                arguments: { task: event.name || 'Background task' },
            });
        } else if (event.event_type === 'subagent_end') {
            endToolIndicator('subagent', event.tool_call || null);
        } else if (event.event_type === 'tool_start') {
            showToolIndicator(event.name, event.tool_call || null);
        } else if (event.event_type === 'tool_end') {
            endToolIndicator(event.name, event.tool_call || null);
        } else if (event.event_type === 'status') {
            lastStatusHint = event.name || '';
            setRunBadge('working', lastStatusHint || 'Running');
        } else if (event.event_type === 'model') {
            setExecutionModel(event.name || run.effective_model || '');
        } else if (event.event_type === 'plan') {
            lastPlanEvent = event;
        }
    }
    if (lastPlanEvent) {
        applyExecutionPlan(parsePlanPayload(lastPlanEvent.name));
    }

    if (run.effective_model) {
        setExecutionModel(run.effective_model);
    }

    if (run.assistant_response) {
        if (toolIndicatorEl) {
            morphIndicatorToStreaming();
        }
        handleStreamChunk(run.assistant_response);
    }

    if (run.status === 'stopping') {
        setProcessing(true);
        setRunBadge('stopping', lastStatusHint || 'Stopping');
    } else if (run.status === 'running') {
        setProcessing(true);
        setRunBadge('working', lastStatusHint || 'Running');
    } else if (run.status === 'interrupted') {
        setProcessing(false);
        setRunBadge('warning', 'Interrupted');
    } else if (run.status === 'failed') {
        setProcessing(false);
        setRunBadge('warning', 'Failed');
    }

    // Hide stale plan panel if run is no longer active
    if (run.status !== 'running' && run.status !== 'stopping') {
        clearExecutionPlan();
    }

    syncEmptyState();
}

async function restoreActiveRun() {
    if (!currentConversationId) return;
    try {
        const res = await fetch(conversationApi('/api/v1/chat/run'));
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

function resetConversationView() {
    messagesEl.textContent = '';
    clearBrowserGallery();
    clearTransientRunUi();
    pendingAttachments = [];
    pendingMcpServers = [];
    renderComposerContextStrip();
    closeMcpPicker();
    activeRunId = null;
    loadedConversationId = null;
    setProcessing(false);
    setExecutionModel('');
    clearExecutionPlan();
    syncEmptyState();
}

async function selectConversation(conversationId) {
    currentConversationId = conversationId;
    dismissDropdown();
    window.localStorage.setItem('homun.chat.currentConversation', conversationId);
    setConversationUrl(conversationId);
    syncConversationHeader();
    renderConversationList();
    resetConversationView();
    disconnectSocket();
    connect();
    await refreshConversationList();
    chatText?.focus();
}

// ─── Command palette: chat-specific actions ───
if (window.homunCommandPalette) {
    homunCommandPalette.register({ id: 'chat-new', label: 'New Conversation', icon: '➕', fn: handleNewChat });
    homunCommandPalette.register({ id: 'chat-search', label: 'Search Conversations', icon: '🔍', fn: function() { openSearchModal(); } });
    homunCommandPalette.register({ id: 'chat-focus', label: 'Focus Chat Input', icon: '⌨️', fn: function() { chatText?.focus(); } });
}

// ─── Search modal ───
btnChatSearch?.addEventListener('click', () => openSearchModal());
btnChatSearchClose?.addEventListener('click', () => closeSearchModal());
chatSearchModal?.querySelector('.chat-search-modal-backdrop')?.addEventListener('click', () => closeSearchModal());
chatSearchInput?.addEventListener('input', () => {
    clearTimeout(searchDebounceTimer);
    searchDebounceTimer = setTimeout(() => performSearch(), 300);
});
chatSearchIncludeArchived?.addEventListener('change', () => performSearch());

function openSearchModal() {
    if (!chatSearchModal) return;
    chatSearchModal.hidden = false;
    if (chatSearchInput) { chatSearchInput.value = ''; chatSearchInput.focus(); }
    if (chatSearchResults) chatSearchResults.textContent = '';
}
function closeSearchModal() {
    if (chatSearchModal) chatSearchModal.hidden = true;
}
async function performSearch() {
    const q = chatSearchInput?.value.trim() || '';
    const inclArchived = chatSearchIncludeArchived?.checked || false;
    const url = new URL('/api/v1/chat/conversations', window.location.origin);
    url.searchParams.set('limit', '20');
    if (q) url.searchParams.set('q', q);
    if (inclArchived) url.searchParams.set('include_archived', 'true');
    try {
        const res = await fetch(url.pathname + url.search);
        if (!res.ok) return;
        const results = await res.json();
        renderSearchResults(results);
    } catch (_) { /* ignore */ }
}
function renderSearchResults(results) {
    if (!chatSearchResults) return;
    chatSearchResults.textContent = '';
    if (results.length === 0) {
        const empty = document.createElement('div');
        empty.className = 'chat-search-result-empty';
        empty.textContent = 'No results found.';
        chatSearchResults.appendChild(empty);
        return;
    }
    results.forEach((c) => {
        const el = document.createElement('button');
        el.type = 'button';
        el.className = 'chat-search-result-item';
        if (c.archived) el.classList.add('is-archived');

        const name = document.createElement('span');
        name.className = 'chat-search-result-name';
        name.textContent = capitalizeFirst(c.title) || 'New conversation';
        el.appendChild(name);

        if (c.archived) {
            const badge = document.createElement('span');
            badge.className = 'chat-search-result-badge';
            badge.textContent = 'Archived';
            el.appendChild(badge);

            const restoreBtn = document.createElement('button');
            restoreBtn.type = 'button';
            restoreBtn.className = 'chat-search-restore-btn';
            restoreBtn.textContent = 'Restore';
            restoreBtn.addEventListener('click', async (e) => {
                e.stopPropagation();
                try {
                    await updateConversation(c.conversation_id, { archived: false });
                    showToast('Conversation restored', 'success');
                    closeSearchModal();
                    selectConversation(c.conversation_id);
                } catch (_) {
                    showToast('Failed to restore conversation', 'error');
                }
            });
            el.appendChild(restoreBtn);
        }

        const date = document.createElement('span');
        date.className = 'chat-search-result-date';
        date.textContent = formatConversationTimestamp(c.updated_at);
        el.appendChild(date);

        el.addEventListener('click', () => {
            closeSearchModal();
            // Auto-restore archived conversations when opened
            if (c.archived) {
                updateConversation(c.conversation_id, { archived: false }).then(() => {
                    refreshConversationList();
                }).catch(() => {});
            }
            selectConversation(c.conversation_id);
        });
        chatSearchResults.appendChild(el);
    });
}

// ─── Multi-select ───
function enterMultiSelectMode(initialId) {
    dismissDropdown();
    multiSelectMode = true;
    selectedConversations.clear();
    if (initialId) selectedConversations.add(initialId);
    renderConversationList();
}
function exitMultiSelectMode() {
    multiSelectMode = false;
    selectedConversations.clear();
    renderConversationList();
}
function toggleConversationSelection(id) {
    if (selectedConversations.has(id)) selectedConversations.delete(id);
    else selectedConversations.add(id);
    if (selectedConversations.size === 0) exitMultiSelectMode();
    else renderConversationList();
}
function syncBulkActions() {
    if (!chatBulkActions) return;
    chatBulkActions.hidden = !multiSelectMode || selectedConversations.size === 0;
    if (chatBulkCount) chatBulkCount.textContent = `${selectedConversations.size} selected`;
}

btnBulkCancel?.addEventListener('click', () => exitMultiSelectMode());
btnBulkDelete?.addEventListener('click', async () => {
    const count = selectedConversations.size;
    if (count === 0) return;
    openModal({
        title: 'Delete conversations',
        copy: `Delete ${count} conversation${count > 1 ? 's' : ''}? This cannot be undone.`,
        confirmLabel: 'Delete',
        destructive: true,
        onConfirm: async () => {
            const ids = [...selectedConversations];
            for (const id of ids) {
                try { await fetch(`/api/v1/chat/conversations/${encodeURIComponent(id)}`, { method: 'DELETE' }); } catch (_) { /* ignore */ }
            }
            exitMultiSelectMode();
            await refreshConversationList();
            if (ids.includes(currentConversationId)) {
                await ensureConversationSelectedAfterRemoval(currentConversationId);
            }
            showToast(`Deleted ${count} conversation${count > 1 ? 's' : ''}`, 'success');
        },
    });
});
btnBulkArchive?.addEventListener('click', async () => {
    const count = selectedConversations.size;
    if (count === 0) return;
    const ids = [...selectedConversations];
    for (const id of ids) {
        try {
            await fetch(`/api/v1/chat/conversations/${encodeURIComponent(id)}`, {
                method: 'PATCH',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ archived: true }),
            });
        } catch (_) { /* ignore */ }
    }
    exitMultiSelectMode();
    await refreshConversationList();
    showToast(`Archived ${count} conversation${count > 1 ? 's' : ''}`, 'success');
});

btnChatSidebar?.addEventListener('click', () => {
    sidebarCollapsed = !sidebarCollapsed;
    window.localStorage.setItem('homun.chat.sidebarCollapsed', sidebarCollapsed ? '1' : '0');
    applySidebarState();
});

document.addEventListener('click', (e) => {
    if (openConversationMenuId && !e.target.closest('.chat-conversation-item') && !e.target.closest('.chat-conv-dropdown')) {
        closeConversationMenu();
    }
    if (mcpPickerOpen && !e.target.closest('.chat-plus-wrap')) {
        closeMcpPicker();
    }
});

document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') {
        if (!chatSearchModal?.hidden) { closeSearchModal(); return; }
        if (openConversationMenuId) { closeConversationMenu(); return; }
        if (modelPickerBackdrop) { closeModelPicker(); return; }
        if (toolsDropdownEl) { closeToolsDropdown(); return; }
        if (multiSelectMode) { exitMultiSelectMode(); return; }
        if (mcpPickerOpen) { closeMcpPicker(); }
        if (!chatModalBackdrop?.hidden) { closeModal(); }
    }
});

chatModalCancel?.addEventListener('click', closeModal);
chatModalBackdrop?.addEventListener('click', (e) => {
    if (e.target === chatModalBackdrop) {
        closeModal();
    }
});
chatModalConfirm?.addEventListener('click', async () => {
    const action = modalState?.onConfirm;
    const inputVal = modalState?.hasInput && chatModalInput ? chatModalInput.value : undefined;
    closeModal();
    if (action) {
        await action(inputVal);
    }
});
chatModalInput?.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') { e.preventDefault(); chatModalConfirm?.click(); }
});

function setRunBadge(mode, label) {
    if (!runBadgeEl) return;
    const safeLabel = label || '';
    runBadgeEl.textContent = safeLabel;
    runBadgeEl.className = `chat-run-badge is-${mode}${safeLabel ? '' : ' is-dot-only'}`;
    runBadgeEl.setAttribute('aria-label', safeLabel || mode);
    runBadgeEl.title = safeLabel || mode;
}

function setExecutionModel(model) {
    if (!runModelEl) return;
    const normalized = (model || '').trim();
    if (!normalized || normalized === currentModel) {
        runModelEl.hidden = true;
        runModelEl.textContent = '';
        runModelEl.removeAttribute('title');
        return;
    }

    const shortLabel = normalized.split('/').pop() || normalized;
    runModelEl.hidden = false;
    runModelEl.textContent = `via ${shortLabel}`;
    runModelEl.title = normalized;
}

// ─── Cognition indicator ──────────────────────────────────────

let cognitionEl = null;

/** Show or update the cognition phase indicator. */
function showCognitionStep(label) {
    if (!cognitionEl) {
        cognitionEl = document.createElement('div');
        cognitionEl.className = 'chat-cognition is-active';

        const header = document.createElement('div');
        header.className = 'chat-cognition-header';
        header.onclick = function() { toggleCognition(this); };

        const dot = document.createElement('span');
        dot.className = 'chat-cognition-dot';
        const lbl = document.createElement('span');
        lbl.className = 'chat-cognition-label';
        const toggle = document.createElement('span');
        toggle.className = 'chat-cognition-toggle';
        toggle.textContent = '\u203A';

        header.append(dot, lbl, toggle);

        const steps = document.createElement('div');
        steps.className = 'chat-cognition-steps';

        cognitionEl.append(header, steps);
        messagesEl.appendChild(cognitionEl);
        scrollThreadToBottom();
    }
    const labelEl = cognitionEl.querySelector('.chat-cognition-label');
    if (labelEl) labelEl.textContent = label;
}

/** Add a discovery step to the cognition indicator. */
function addCognitionStep(step) {
    if (!cognitionEl) showCognitionStep('Analyzing...');
    const stepsEl = cognitionEl.querySelector('.chat-cognition-steps');
    if (!stepsEl) return;
    const stepEl = document.createElement('div');
    stepEl.className = 'chat-cognition-step';
    stepEl.textContent = formatCognitionStep(step);
    stepsEl.appendChild(stepEl);
    const labelEl = cognitionEl.querySelector('.chat-cognition-label');
    if (labelEl) labelEl.textContent = friendlyCognitionStep(step);
    scrollThreadToBottom();
}

/** Finalize the cognition indicator with the result summary. */
function finalizeCognition(summary) {
    if (!cognitionEl) showCognitionStep(summary);
    cognitionEl.classList.remove('is-active');
    cognitionEl.classList.add('collapsed');
    const labelEl = cognitionEl.querySelector('.chat-cognition-label');
    if (labelEl) labelEl.textContent = compactCognitionLabel(summary);
}

/** Compact the cognition result into a short label for the collapsed header. */
function compactCognitionLabel(raw) {
    if (!raw || raw.length < 60) return raw || 'Analysis complete';
    // Extract key parts from "Understanding... | Tools: x, y | Plan: N steps"
    const toolsMatch = raw.match(/Tools:\s*([^|]+)/);
    const planMatch = raw.match(/Plan:\s*(\d+)\s*steps?/);
    const parts = [];
    if (toolsMatch) {
        const names = toolsMatch[1].trim().split(/,\s*/);
        parts.push(names.length + ' tool' + (names.length > 1 ? 's' : ''));
    }
    if (planMatch) parts.push(planMatch[1] + ' steps');
    if (raw.includes('Memory: loaded')) parts.push('memory');
    if (parts.length > 0) return 'Analyzed \u00b7 ' + parts.join(', ');
    // Fallback: truncate
    return raw.length > 50 ? raw.substring(0, 50) + '\u2026' : raw;
}

/** Toggle cognition detail visibility. */
window.toggleCognition = function(headerEl) {
    const section = headerEl.closest('.chat-cognition');
    if (section) section.classList.toggle('collapsed');
};

/** Map discovery tool calls to user-friendly labels (used for header during activity). */
function friendlyCognitionStep(raw) {
    if (raw.startsWith('discover_tools')) return 'Searching tools...';
    if (raw.startsWith('discover_skills')) return 'Searching skills...';
    if (raw.startsWith('discover_mcp')) return 'Checking services...';
    if (raw.startsWith('search_memory')) return 'Checking memory...';
    if (raw.startsWith('search_knowledge')) return 'Searching knowledge...';
    return raw;
}

/** Format a cognition step for display in the expanded details. */
function formatCognitionStep(raw) {
    // "discover_tools(query) → N found: tool1, tool2" → "Tools: 7 found (browser, web_search, ...)"
    const arrowIdx = raw.indexOf('\u2192');
    if (arrowIdx === -1) return raw;
    const result = raw.substring(arrowIdx + 1).trim();
    if (raw.startsWith('discover_tools')) return 'Tools: ' + cleanToolNames(result);
    if (raw.startsWith('discover_skills')) return 'Skills: ' + result;
    if (raw.startsWith('discover_mcp')) return 'Services: ' + cleanToolNames(result);
    if (raw.startsWith('search_memory')) return 'Memory: ' + result;
    if (raw.startsWith('search_knowledge')) return 'Knowledge: ' + result;
    return raw;
}

/** Clean MCP-prefixed tool names for display: "brave-search__brave_local_search" → "local_search" */
function cleanToolNames(text) {
    return text.replace(/\b[\w-]+__(\w+)/g, '$1');
}

/** Remove the cognition indicator from the DOM. */
function removeCognition() {
    if (cognitionEl) { cognitionEl.remove(); cognitionEl = null; }
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
    purgeOrphanLiveArtifacts();
    if (reasoningSectionEl) return reasoningSectionEl;

    reasoningSectionEl = document.createElement('div');
    reasoningSectionEl.className = 'chat-reasoning collapsed';
    reasoningSectionEl.innerHTML = `
        <div class="chat-reasoning-header" onclick="toggleReasoning(this)">
            <span class="chat-reasoning-summary">
                <span class="chat-reasoning-label">Tool activity</span>
                <span class="chat-reasoning-count">0</span>
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
    scrollThreadToBottom();

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
            countEl.textContent = `${reasoningCount}`;
        }
        const labelEl = reasoningSectionEl.querySelector('.chat-reasoning-label');
        if (labelEl) {
            labelEl.textContent = reasoningHeadline();
        }
    }
}

function showToolIndicator(toolName, toolCallData) {
    purgeOrphanLiveArtifacts();
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
    scrollThreadToBottom();

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

    // Auto-expand for first few tools, then collapse to reduce noise
    if (reasoningCount <= 3) {
        reasoningSectionEl.classList.remove('collapsed');
    } else {
        reasoningSectionEl.classList.add('collapsed');
    }

    scrollThreadToBottom();
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
            detail: args.query ? `"${truncate(String(args.query), 40)}"` : '',
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
            detail: args.command ? truncate(String(args.command), 40) : '',
        };
    }

    if (name === 'subagent') {
        return {
            label: 'Background task',
            detail: args.task ? truncate(String(args.task), 50) : '',
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
        return truncate(String(url), 30);
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
    scrollThreadToBottom();
}

/** Clear the browser gallery for a new session */
function clearBrowserGallery() {
    if (browserGalleryEl) {
        browserGalleryEl.remove();
        browserGalleryEl = null;
    }
    browserScreenshots = [];
}

function endToolIndicator(toolName, toolCallData) {
    activeTools = activeTools.filter(t => t !== toolName);
    const completedCard = Array.from(document.querySelectorAll('.chat-tool-call'))
        .reverse()
        .find((card) => card.dataset.toolName === toolName && !card.classList.contains('is-complete'));
    if (completedCard) {
        completedCard.classList.add('is-complete');
        completedCard.dataset.toolStatus = 'done';
        const meta = completedCard.querySelector('.chat-tool-call-meta');
        if (meta) {
            const resultText = toolCallData?.result;
            if (resultText) {
                meta.textContent = '\u2713';
                meta.title = resultText;
                // Add truncated result as a summary line
                const summary = document.createElement('span');
                summary.className = 'chat-tool-summary';
                summary.textContent = resultText.length > 80
                    ? resultText.substring(0, 80) + '…'
                    : resultText;
                const compact = completedCard.querySelector('.chat-tool-call-compact');
                if (compact) compact.appendChild(summary);
            } else {
                meta.textContent = '\u2713';
            }
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
        // Add body child for progressive markdown rendering
        const body = document.createElement('div');
        body.className = 'chat-msg-body';
        toolIndicatorEl.appendChild(body);
        streamingEl = toolIndicatorEl;
        streamingContent = '';
        lastStreamRenderTime = 0;
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
        reasoningSectionEl.classList.add('collapsed', 'is-done');
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
    purgeOrphanLiveArtifacts();
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
    scrollThreadToBottom();

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

    scrollThreadToBottom();
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

function formatAttachmentSize(sizeBytes) {
    if (!sizeBytes || sizeBytes < 1024) return `${sizeBytes || 0} B`;
    const kib = sizeBytes / 1024;
    if (kib < 1024) return `${Math.round(kib)} KB`;
    return `${(kib / 1024).toFixed(1)} MB`;
}

function createAttachmentNode(attachment, { removable = false, compact = false } = {}) {
    const item = document.createElement('div');
    item.className = `chat-attachment-card${compact ? ' is-compact' : ''}`;

    if (attachment.kind === 'image' && attachment.preview_url) {
        const img = document.createElement('img');
        img.className = 'chat-attachment-thumb';
        img.src = attachment.preview_url;
        img.alt = attachment.name || 'Image attachment';
        item.appendChild(img);
    } else {
        const icon = document.createElement('div');
        icon.className = 'chat-attachment-icon';
        icon.textContent = attachment.kind === 'document' ? 'DOC' : 'FILE';
        item.appendChild(icon);
    }

    const meta = document.createElement('div');
    meta.className = 'chat-attachment-meta';

    const name = document.createElement('div');
    name.className = 'chat-attachment-name';
    name.textContent = attachment.name || 'Attachment';
    meta.appendChild(name);

    const info = document.createElement('div');
    info.className = 'chat-attachment-info';
    info.textContent = compact
        ? attachment.kind
        : `${attachment.kind} • ${formatAttachmentSize(attachment.size_bytes)}`;
    meta.appendChild(info);

    item.appendChild(meta);

    if (removable) {
        const removeBtn = document.createElement('button');
        removeBtn.type = 'button';
        removeBtn.className = 'chat-attachment-remove';
        removeBtn.textContent = '×';
        removeBtn.title = 'Remove attachment';
        removeBtn.addEventListener('click', () => {
            pendingAttachments = pendingAttachments.filter((entry) => entry !== attachment);
            renderComposerContextStrip();
        });
        item.appendChild(removeBtn);
    }

    return item;
}

function createMcpServerNode(server, { removable = false, compact = false } = {}) {
    const item = document.createElement('div');
    item.className = `chat-context-chip${compact ? ' is-compact' : ''}`;

    const label = document.createElement('div');
    label.className = 'chat-context-chip-label';
    label.textContent = server.name;
    item.appendChild(label);

    const meta = document.createElement('div');
    meta.className = 'chat-context-chip-meta';
    meta.textContent = `MCP • ${server.transport || 'stdio'}`;
    item.appendChild(meta);

    if (removable) {
        const removeBtn = document.createElement('button');
        removeBtn.type = 'button';
        removeBtn.className = 'chat-attachment-remove';
        removeBtn.textContent = '×';
        removeBtn.title = 'Remove MCP server';
        removeBtn.addEventListener('click', () => {
            pendingMcpServers = pendingMcpServers.filter((entry) => entry.name !== server.name);
            renderComposerContextStrip();
            renderMcpPickerList();
        });
        item.appendChild(removeBtn);
    }

    return item;
}

function renderComposerContextStrip() {
    if (!chatAttachmentStrip) return;
    chatAttachmentStrip.textContent = '';
    if (!pendingAttachments.length && !pendingMcpServers.length) {
        chatAttachmentStrip.hidden = true;
        return;
    }

    pendingMcpServers.forEach((server) => {
        chatAttachmentStrip.appendChild(createMcpServerNode(server, { removable: true }));
    });
    pendingAttachments.forEach((attachment) => {
        chatAttachmentStrip.appendChild(createAttachmentNode(attachment, { removable: true }));
    });
    chatAttachmentStrip.hidden = false;
}

async function uploadChatFiles(kind, fileList) {
    const files = Array.from(fileList || []);
    if (!files.length) return;
    if (!currentConversationId) {
        showToast('Open a conversation first', 'error');
        return;
    }

    let uploadedCount = 0;
    for (const file of files) {
        const formData = new FormData();
        formData.append('conversation_id', currentConversationId);
        formData.append('kind', kind);
        formData.append('file', file);

        try {
            const res = await fetch('/api/v1/chat/uploads', {
                method: 'POST',
                body: formData,
            });
            if (!res.ok) {
                throw new Error(`Upload failed (${res.status})`);
            }
            const data = await res.json();
            if (!data.ok || !data.attachment) {
                throw new Error('Upload failed');
            }
            pendingAttachments.push(data.attachment);
            uploadedCount += 1;
        } catch (error) {
            console.error('Failed to upload chat attachment:', error);
            showToast(`Failed to upload ${file.name}`, 'error');
        }
    }

    renderComposerContextStrip();
    if (uploadedCount > 0) {
        showToast(
            kind === 'image'
                ? `Added ${uploadedCount} image${uploadedCount === 1 ? '' : 's'}`
                : `Added ${uploadedCount} document${uploadedCount === 1 ? '' : 's'}`,
            'success'
        );
    }
}

async function ensureMcpServersLoaded() {
    var servers = await McpLoader.fetchServers();
    availableMcpServers = Array.isArray(servers) ? servers : [];
    return availableMcpServers;
}

function closeMcpPicker() {
    mcpPickerOpen = false;
    if (chatMcpPicker) {
        chatMcpPicker.hidden = true;
    }
}

function renderMcpPickerList() {
    if (!chatMcpPickerList) return;
    const query = (mcpSearchQuery || '').trim().toLowerCase();
    const items = availableMcpServers
        .filter((server) => server && server.enabled)
        .filter((server) => {
            if (!query) return true;
            const haystack = `${server.name} ${server.transport} ${(server.command || '')} ${(server.url || '')} ${((server.capabilities || []).join(' '))}`.toLowerCase();
            return haystack.includes(query);
        });

    if (!items.length) {
        chatMcpPickerList.innerHTML = '<div class="chat-mcp-empty">No active MCP servers</div>';
        return;
    }

    chatMcpPickerList.innerHTML = '';
    items.forEach((server) => {
        const button = document.createElement('button');
        button.type = 'button';
        button.className = 'chat-mcp-option';
        if (pendingMcpServers.some((entry) => entry.name === server.name)) {
            button.classList.add('is-selected');
        }
        button.innerHTML = `
            <span class="chat-mcp-option-main">
                <span class="chat-mcp-option-name">${escapeHtml(server.name)}</span>
                <span class="chat-mcp-option-meta">${escapeHtml(server.transport || 'stdio')}</span>
            </span>
            <span class="chat-mcp-option-check">${pendingMcpServers.some((entry) => entry.name === server.name) ? 'Selected' : 'Use'}</span>
        `;
        button.addEventListener('click', () => {
            const existing = pendingMcpServers.some((entry) => entry.name === server.name);
            if (existing) {
                pendingMcpServers = pendingMcpServers.filter((entry) => entry.name !== server.name);
            } else {
                pendingMcpServers.push({
                    name: server.name,
                    transport: server.transport || 'stdio',
                });
            }
            renderComposerContextStrip();
            renderMcpPickerList();
        });
        chatMcpPickerList.appendChild(button);
    });
}

// ─── WebSocket ─────────────────────────────────────────────────

function disconnectSocket() {
    if (reconnectTimer) {
        clearTimeout(reconnectTimer);
        reconnectTimer = null;
    }
    if (ws) {
        suppressReconnect = true;
        ws.close();
        ws = null;
    }
}

function connect() {
    if (!currentConversationId) return;
    const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
    socketConversationId = currentConversationId;
    const socket = new WebSocket(`${proto}//${location.host}/ws/chat?conversation_id=${encodeURIComponent(currentConversationId)}`);
    ws = socket;

    socket.onopen = () => {
        suppressReconnect = false;
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

    socket.onmessage = (event) => {
        try {
            const data = JSON.parse(event.data);

            if (data.type === 'connected') {
                if (data.conversation_id && data.conversation_id !== currentConversationId) {
                    return;
                }
                syncEmptyState();
                restoreActiveRun();

            } else if (data.type === 'thinking_start') {
                removeTypingIndicator();
                // Start a new thinking block
                createThinkingBlock();

            } else if (data.type === 'thinking_chunk') {
                // Append to thinking content
                appendThinking(data.delta);

            } else if (data.type === 'thinking_end') {
                // Finalize thinking block
                finalizeThinking();

            } else if (data.type === 'tool_start') {
                removeTypingIndicator();
                // Agent is calling a tool
                showToolIndicator(data.name, data.tool_call);

            } else if (data.type === 'tool_end') {
                // Tool finished — update indicator but keep it visible
                endToolIndicator(data.name, data.tool_call || null);

            } else if (data.type === 'cognition_start') {
                removeTypingIndicator();
                removeCognition();
                showCognitionStep(data.name || 'Analyzing request...');
                setRunBadge('working', 'Analyzing');

            } else if (data.type === 'cognition_step') {
                addCognitionStep(data.name || '');

            } else if (data.type === 'cognition_result') {
                finalizeCognition(data.name || 'Ready');

            } else if (data.type === 'subagent_start') {
                showToolIndicator('subagent', {
                    id: 'subagent-' + Date.now(),
                    name: 'subagent',
                    arguments: { task: data.name || 'Background task' },
                });

            } else if (data.type === 'subagent_end') {
                endToolIndicator('subagent', data.tool_call || null);

            } else if (data.type === 'status') {
                setRunBadge('working', data.name || 'Running');
            } else if (data.type === 'model') {
                setExecutionModel(data.name || '');
            } else if (data.type === 'plan') {
                applyExecutionPlan(parsePlanPayload(data.name));

            } else if (data.type === 'screenshot') {
                // Screenshot URL from browser tool — add to gallery
                addBrowserScreenshot(data.delta);

            } else if (data.type === 'stream') {
                removeTypingIndicator();
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
                // Force-collapse reasoning and cognition sections
                if (reasoningSectionEl) {
                    reasoningSectionEl.classList.add('collapsed', 'is-done');
                    updateReasoningCount();
                }
                // Remove cognition indicator — its job is done
                removeCognition();
                activeRunId = null;
                setExecutionModel('');
            } else if (data.type === 'workflow_progress') {
                handleWorkflowProgress(data.progress);
            } else if (data.type === 'error') {
                settleLiveArtifacts();
                setProcessing(false);
                setExecutionModel('');
                showToast(data.message || 'Chat error', 'error');
            }
        } catch (e) {
            console.error('Failed to parse message:', e);
        }
    };

    socket.onclose = () => {
        // Only reset UI state if this socket is still the active one.
        // When the user switches conversations, disconnectSocket() sets ws=null
        // then connect() sets ws=new_socket. The old socket's async onclose
        // must NOT clobber the new conversation's processing state.
        if (ws !== socket) return;

        wsStatus.textContent = 'Disconnected';
        wsStatus.className = 'chat-connection is-offline';
        settleLiveArtifacts();
        setRunBadge('offline', 'Offline');
        setProcessing(false);
        setExecutionModel('');
        if (!suppressReconnect && socketConversationId === currentConversationId) {
            reconnectTimer = setTimeout(connect, 3000);
        }
        suppressReconnect = false;
    };

    socket.onerror = () => {
        socket.close();
    };
}

// ─── Streaming (progressive markdown rendering) ────────────────

/** Render accumulated streaming content as markdown with fence patching.
 *  All HTML output is sanitized via DOMPurify.sanitize() to prevent XSS.
 */
function renderStreamingMarkdown() {
    if (!streamingEl || !streamingContent) return;
    let content = streamingContent;

    // Patch unclosed code fences — count lines starting with 3+ backticks
    const fenceCount = (content.match(/^`{3,}/gm) || []).length;
    if (fenceCount % 2 !== 0) content += '\n```';

    // Patch unclosed inline code (simple heuristic: odd single backticks)
    const inlineCount = (content.match(/(?<!`)`(?!`)/g) || []).length;
    if (inlineCount % 2 !== 0) content += '`';

    const bodyEl = streamingEl.querySelector('.chat-msg-body') || streamingEl;

    if (typeof marked === 'undefined' || typeof DOMPurify === 'undefined') {
        // Fallback if libs not loaded yet — safe plain text
        bodyEl.textContent = streamingContent;
        return;
    }

    const rawHtml = marked.parse(content);
    // DOMPurify sanitizes all HTML to prevent XSS (safe: uses DOMPurify.sanitize)
    const safeHtml = DOMPurify.sanitize(rawHtml);
    bodyEl.innerHTML = safeHtml;
}

/** Schedule a throttled markdown render (max once per STREAM_RENDER_INTERVAL). */
function scheduleStreamRender() {
    const now = Date.now();
    if (now - lastStreamRenderTime >= STREAM_RENDER_INTERVAL) {
        renderStreamingMarkdown();
        lastStreamRenderTime = now;
    } else if (!streamRenderRafId) {
        streamRenderRafId = requestAnimationFrame(() => {
            streamRenderRafId = null;
            if (Date.now() - lastStreamRenderTime >= STREAM_RENDER_INTERVAL) {
                renderStreamingMarkdown();
                lastStreamRenderTime = Date.now();
            }
        });
    }
}

/** Cancel any pending streaming render frame. */
function cancelStreamRender() {
    if (streamRenderRafId) {
        cancelAnimationFrame(streamRenderRafId);
        streamRenderRafId = null;
    }
    lastStreamRenderTime = 0;
}

/** Handle an incremental streaming chunk from the LLM. */
function handleStreamChunk(delta) {
    if (!delta) return;
    purgeOrphanLiveArtifacts();

    if (!streamingEl) {
        // First chunk — create a new assistant message bubble with body child
        streamingEl = document.createElement('div');
        streamingEl.className = 'chat-msg assistant streaming';
        const body = document.createElement('div');
        body.className = 'chat-msg-body';
        streamingEl.appendChild(body);
        streamingContent = '';
        lastStreamRenderTime = 0;
        messagesEl.appendChild(streamingEl);
    }

    // Accumulate and schedule throttled markdown render
    streamingContent += delta;
    scheduleStreamRender();
    scrollThreadToBottom();
}

function settleLiveArtifacts() {
    cancelStreamRender();
    purgeOrphanLiveArtifacts();
    if (streamingEl) {
        if (streamingContent.trim()) {
            const bodyEl = streamingEl.querySelector('.chat-msg-body') || streamingEl;
            renderContent(bodyEl, streamingContent, 'assistant');
            streamingEl.classList.remove('streaming');
        } else {
            streamingEl.remove();
        }
        streamingEl = null;
        streamingContent = '';
    }

    if (toolIndicatorEl) {
        removeToolIndicator();
    }

    if (thinkingEl && !thinkingContent.trim()) {
        thinkingEl.remove();
        thinkingEl = null;
    }

    if (reasoningSectionEl && reasoningCount === 0 && !(reasoningContentEl && reasoningContentEl.textContent.trim())) {
        reasoningSectionEl.remove();
        resetReasoning();
    }

    purgeOrphanLiveArtifacts();
}

/** Finalize the streaming message with the complete response.
 *  Render markdown on the final content for proper formatting.
 */
function finalizeStream(content) {
    // Cancel any pending progressive render
    cancelStreamRender();

    // Detect and add screenshots to the browser gallery
    const screenshotPattern = /\/api\/v1\/browser\/screenshots\/([a-zA-Z0-9_-]+\.png)/g;
    let match;
    while ((match = screenshotPattern.exec(content)) !== null) {
        addBrowserScreenshot(match[0]);
    }

    if (streamingEl) {
        // Final render into the body child (or fallback to streamingEl)
        const bodyEl = streamingEl.querySelector('.chat-msg-body') || streamingEl;
        renderContent(bodyEl, content, 'assistant');
        streamingEl.classList.remove('streaming');
        streamingEl = null;
        streamingContent = '';
    } else {
        addMessage('assistant', content);
    }
    scrollThreadToBottom();

    // Reset processing state
    setProcessing(false);
    syncEmptyState();
    updateConversationSummary((conversation) => {
        conversation.preview = truncateConversationText(content);
        conversation.updated_at = new Date().toISOString();
        conversation.message_count = (conversation.message_count || 0) + 1;
    });
    maybeAutoCompact();
}

// ─── Message actions (copy, edit/resend) ────────────────────────

const ICON_COPY = '<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="6" y="6" width="9" height="9" rx="1.5"/><path d="M3 12V4a1.5 1.5 0 011.5-1.5H12"/></svg>';
const ICON_EDIT = '<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M11 3l4 4-9 9H2v-4z"/><path d="M9.5 4.5l4 4"/></svg>';
const ICON_CHECK = '<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 9l4 4 6-7"/></svg>';

function createMessageActions(role, msgDiv) {
    const actions = document.createElement('div');
    actions.className = 'chat-msg-actions';

    // Copy button (all messages)
    const copyBtn = document.createElement('button');
    copyBtn.className = 'chat-msg-action-btn';
    copyBtn.title = 'Copy';
    copyBtn.innerHTML = ICON_COPY;
    copyBtn.addEventListener('click', () => copyMessageContent(msgDiv, copyBtn));
    actions.appendChild(copyBtn);

    // Edit button (user messages only)
    if (role === 'user') {
        const editBtn = document.createElement('button');
        editBtn.className = 'chat-msg-action-btn';
        editBtn.title = 'Edit & Resend';
        editBtn.innerHTML = ICON_EDIT;
        editBtn.addEventListener('click', () => startEditMessage(msgDiv));
        actions.appendChild(editBtn);
    }

    return actions;
}

function copyMessageContent(msgDiv, btn) {
    const raw = msgDiv.dataset.rawContent || '';
    if (!raw) return;
    navigator.clipboard.writeText(raw).then(() => {
        // Brief visual feedback — swap icon to checkmark
        btn.classList.add('is-copied');
        btn.innerHTML = ICON_CHECK;
        setTimeout(() => {
            btn.classList.remove('is-copied');
            btn.innerHTML = ICON_COPY;
        }, 1500);
    });
}

function startEditMessage(msgDiv) {
    const bodyEl = msgDiv.querySelector('.chat-msg-body');
    if (!bodyEl || msgDiv.querySelector('.chat-msg-edit-area')) return;

    const originalContent = msgDiv.dataset.rawContent || bodyEl.textContent || '';
    const originalHTML = bodyEl.innerHTML;

    // Replace body content with inline textarea — looks like editable text
    const textarea = document.createElement('textarea');
    textarea.className = 'chat-msg-edit-area';
    textarea.value = originalContent;

    // Auto-size: grow with content
    function autoSize() {
        textarea.style.height = 'auto';
        textarea.style.height = textarea.scrollHeight + 'px';
    }
    textarea.addEventListener('input', autoSize);

    // Ctrl/Cmd+Enter to send
    textarea.addEventListener('keydown', (e) => {
        if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
            e.preventDefault();
            const newContent = textarea.value.trim();
            if (newContent) resendFromMessage(msgDiv, newContent);
        }
        if (e.key === 'Escape') {
            e.preventDefault();
            bodyEl.innerHTML = originalHTML; // Security: restoring own prior content
        }
    });

    const actionsBar = document.createElement('div');
    actionsBar.className = 'chat-msg-edit-actions';

    const cancelBtn = document.createElement('button');
    cancelBtn.className = 'btn btn-ghost btn-sm';
    cancelBtn.textContent = 'Cancel';
    cancelBtn.addEventListener('click', () => {
        bodyEl.innerHTML = originalHTML; // Security: restoring own prior content
    });

    const sendBtn = document.createElement('button');
    sendBtn.className = 'btn btn-primary btn-sm';
    sendBtn.textContent = 'Resend';
    sendBtn.addEventListener('click', () => {
        const newContent = textarea.value.trim();
        if (!newContent) return;
        resendFromMessage(msgDiv, newContent);
    });

    actionsBar.appendChild(cancelBtn);
    actionsBar.appendChild(sendBtn);

    bodyEl.textContent = '';
    bodyEl.appendChild(textarea);
    bodyEl.appendChild(actionsBar);

    // Initial auto-size after DOM insertion
    requestAnimationFrame(() => {
        autoSize();
        textarea.focus();
        textarea.setSelectionRange(textarea.value.length, textarea.value.length);
    });
}

async function resendFromMessage(msgDiv, newContent) {
    // Remove all messages after (and including) this message from DOM
    const allMsgs = Array.from(messagesEl.querySelectorAll('.chat-msg, .chat-thinking, .chat-reasoning'));
    const idx = allMsgs.indexOf(msgDiv);
    if (idx >= 0) {
        for (let i = allMsgs.length - 1; i >= idx; i--) {
            allMsgs[i].remove();
        }
    }

    // Backend truncation — await so the DB is clean before sending the new message
    const messageId = msgDiv.dataset.messageId;
    if (messageId && currentConversationId) {
        try {
            const res = await fetch(conversationApi('/api/v1/chat/truncate'), {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    conversation_id: currentConversationId,
                    from_message_id: parseInt(messageId, 10)
                })
            });
            if (!res.ok) {
                console.warn('Truncate failed:', res.status);
            }
        } catch (e) {
            console.warn('Truncate error:', e);
        }
    }

    // Re-add the edited user message and send via WebSocket
    addMessage('user', newContent);
    if (ws && ws.readyState === WebSocket.OPEN) {
        const payload = { content: newContent };
        if (thinkingEnabled !== null) payload.thinking = thinkingEnabled;
        ws.send(JSON.stringify(payload));
        setProcessing(true);
    }
}

// ─── Message rendering ─────────────────────────────────────────

function addMessage(role, content, toolsUsed, options = {}) {
    const div = document.createElement('div');
    div.className = `chat-msg ${role}`;
    // First user message gets hero styling (matches empty-state title)
    if (role === 'user' && !messagesEl.querySelector('.chat-msg.user')) {
        div.classList.add('is-hero');
    }
    if (options.runId) {
        div.dataset.runId = options.runId;
    }
    // Store raw content for copy and message ID for edit/resend
    div.dataset.rawContent = content || '';
    if (options.messageId) {
        div.dataset.messageId = options.messageId;
    }

    if (options.mcpServers && options.mcpServers.length > 0) {
        const mcpEl = document.createElement('div');
        mcpEl.className = 'chat-message-attachments';
        options.mcpServers.forEach((server) => {
            mcpEl.appendChild(createMcpServerNode(server, { compact: role !== 'user' }));
        });
        div.appendChild(mcpEl);
    }

    if (options.attachments && options.attachments.length > 0) {
        const attachmentsEl = document.createElement('div');
        attachmentsEl.className = 'chat-message-attachments';
        options.attachments.forEach((attachment) => {
            attachmentsEl.appendChild(createAttachmentNode(attachment, { compact: role !== 'user' }));
        });
        div.appendChild(attachmentsEl);
    }

    if (content) {
        const contentEl = document.createElement('div');
        contentEl.className = 'chat-msg-body';
        renderContent(contentEl, content, role);
        div.appendChild(contentEl);
    }

    // Show tools as a collapsible section (matches live tool timeline)
    if (toolsUsed && toolsUsed.length > 0) {
        const section = document.createElement('div');
        section.className = 'chat-reasoning collapsed';
        const uniqueTools = [...new Set(toolsUsed)];
        const label = uniqueTools.length === 1
            ? `Used ${uniqueTools[0]}`
            : `Used tools`;
        const headerHtml = '<div class="chat-reasoning-header" onclick="toggleReasoning(this)">' +
            '<span class="chat-reasoning-summary">' +
            '' +
            '<span class="chat-reasoning-label">' + escapeHtml(label) + '</span>' +
            '<span class="chat-reasoning-count">' + toolsUsed.length + '</span>' +
            '</span>' +
            '<span class="chat-reasoning-toggle">\u203a</span>' +
            '</div>' +
            '<div class="chat-reasoning-content"></div>';
        section.innerHTML = headerHtml; // safe: no user content in template
        const contentEl = section.querySelector('.chat-reasoning-content');
        toolsUsed.forEach(toolName => {
            const card = document.createElement('div');
            card.className = 'chat-tool-call';
            card.dataset.toolStatus = 'done';
            const compact = document.createElement('div');
            compact.className = 'chat-tool-call-compact';
            const nameSpan = document.createElement('span');
            nameSpan.className = 'chat-tool-call-name';
            nameSpan.textContent = toolName;
            compact.appendChild(nameSpan);
            const metaSpan = document.createElement('span');
            metaSpan.className = 'chat-tool-call-meta';
            metaSpan.textContent = 'Done';
            compact.appendChild(metaSpan);
            card.appendChild(compact);
            contentEl.appendChild(card);
        });
        div.prepend(section);
    }

    // Timestamp tooltip (shown on hover)
    const ts = options.timestamp || new Date().toISOString();
    const tsEl = document.createElement('time');
    tsEl.className = 'chat-msg-timestamp';
    tsEl.dateTime = ts;
    try {
        const d = new Date(ts);
        tsEl.textContent = d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
        tsEl.title = d.toLocaleString();
    } catch (_) {
        tsEl.textContent = '';
    }
    div.appendChild(tsEl);

    // Add hover action buttons (copy, edit)
    div.appendChild(createMessageActions(role, div));

    messagesEl.appendChild(div);
    scrollThreadToBottom();
    syncEmptyState();
    return div;
}

// ─── Form submission ───────────────────────────────────────────

function sendCurrentMessage() {
    const text = chatText.value.trim();
    const attachments = pendingAttachments.slice();
    const mcpServers = pendingMcpServers.slice();
    if ((!text && attachments.length === 0 && mcpServers.length === 0) || isProcessing || !ws || ws.readyState !== WebSocket.OPEN) return;

    addMessage('user', text, null, { attachments, mcpServers });
    forceScrollToBottom();
    const payload = { content: text, attachments, mcp_servers: mcpServers };
    if (thinkingEnabled !== null) {
        payload.thinking = thinkingEnabled;
    }
    ws.send(JSON.stringify(payload));
    chatText.value = '';
    chatText.style.height = 'auto';
    chatText.focus();
    // Auto-collapse sidebar to maximize reading area
    if (!sidebarCollapsed) {
        sidebarCollapsed = true;
        window.localStorage.setItem('homun.chat.sidebarCollapsed', '1');
        applySidebarState();
    }
    pendingAttachments = [];
    pendingMcpServers = [];
    if (activeToolMode) {
        activeToolMode = null;
        if (btnChatTools) btnChatTools.classList.remove('is-active');
        if (chatToolsLabel) chatToolsLabel.textContent = 'Tools';
        if (chatToolsDismiss) chatToolsDismiss.hidden = true;
    }
    clearExecutionPlan();
    renderComposerContextStrip();
    closeMcpPicker();
    if (chatImageInput) chatImageInput.value = '';
    if (chatDocInput) chatDocInput.value = '';
    setProcessing(true);
    closeChatPlusMenu();

    // Mark setup wizard step 4 complete on first message
    try {
        var ck = localStorage.getItem('homun-wizard-checkpoint');
        if (ck) {
            var d = JSON.parse(ck);
            if (d && d.step === 'chat') {
                localStorage.setItem('homun-wizard-checkpoint', JSON.stringify({ step: 'done', ts: Date.now() }));
            }
        }
    } catch(_) {}
    updateConversationSummary((conversation) => {
        if (!conversation.message_count) {
            conversation.title = truncateConversationText(text)
                || attachments[0]?.name
                || mcpServers[0]?.name
                || 'New conversation';
        }
        conversation.preview = truncateConversationText(text)
            || (attachments.length ? `${attachments.length} file${attachments.length === 1 ? '' : 's'}` : '')
            || (mcpServers.length ? `${mcpServers.length} MCP server${mcpServers.length === 1 ? '' : 's'}` : '');
        conversation.updated_at = new Date().toISOString();
        conversation.message_count = (conversation.message_count || 0) + 1;
    });
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
        if (!processing) updateSendButtonState();
    }
    if (chatText) chatText.disabled = false;
    // Toggle logo pulse on the app shell
    const appEl = document.querySelector('.app');
    if (appEl) appEl.classList.toggle('is-agent-working', processing);
    if (!processing) {
        setRunBadge(ws && ws.readyState === WebSocket.OPEN ? 'idle' : 'offline', ws && ws.readyState === WebSocket.OPEN ? '' : 'Offline');
        removeTypingIndicator();
    } else {
        showTypingIndicator();
    }
}

/** Show animated typing dots before content starts streaming */
function showTypingIndicator() {
    if (document.getElementById('chat-typing-indicator')) return;
    const el = document.createElement('div');
    el.id = 'chat-typing-indicator';
    el.className = 'chat-typing-indicator';
    el.innerHTML = '<span></span><span></span><span></span>'; // safe: static content
    messagesEl.appendChild(el);
    scrollThreadToBottom();
}

function removeTypingIndicator() {
    const el = document.getElementById('chat-typing-indicator');
    if (el) el.remove();
}

/** Update send button icon state: mic (empty) vs send arrow (has text) */
function updateSendButtonState() {
    if (!btnSend || isProcessing) return;
    const hasText = chatText && chatText.value.trim().length > 0;
    const hasAttachments = pendingAttachments && pendingAttachments.length > 0;
    btnSend.classList.toggle('has-text', hasText || hasAttachments);
    btnSend.setAttribute('aria-label', hasText || hasAttachments ? 'Send message' : 'Voice input');
}

/** Handle stop button click */
async function handleStop() {
    try {
        const res = await fetch(conversationApi('/api/v1/chat/stop'), { method: 'POST' });
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
    if (isRecording) {
        stopRecording();
        return;
    }
    const hasContent = (chatText && chatText.value.trim()) || (pendingAttachments && pendingAttachments.length > 0);
    if (hasContent) {
        sendCurrentMessage();
    } else {
        startRecording();
    }
});

// ─── Chat actions (New / Compact / Clear) ─────────────────────────

/** Clear the screen (UI only, keeps DB history) */
function handleClearChat() {
    messagesEl.textContent = '';
    clearBrowserGallery();
    clearTransientRunUi();
    clearExecutionPlan();
    pendingAttachments = [];
    pendingMcpServers = [];
    renderComposerContextStrip();
    closeMcpPicker();
    activeRunId = null;
    syncEmptyState();
    showToast('Screen cleared', 'info');
}

/** Start a new conversation (clears DB and UI) */
async function handleNewChat() {
    try {
        const conversation = await createConversation();
        conversations.unshift(conversation);
        renderConversationList();
        await selectConversation(conversation.conversation_id);
        showToast('Started new conversation', 'success');
    } catch (e) {
        console.error('Failed to start new chat:', e);
        showToast('Failed to start new conversation', 'error');
    }
}

/** Compact the conversation (trigger memory consolidation) */
async function handleCompactChat(silent = false) {
    try {
        const res = await fetch(conversationApi('/api/v1/chat/compact'), { method: 'POST' });
        const data = await res.json();

        if (data.ok) {
            showToast('Conversation compacted', 'success');
        } else if (!silent) {
            showToast(data.message || 'Cannot compact yet', 'warning');
        }
    } catch (e) {
        if (!silent) {
            console.error('Failed to compact chat:', e);
            showToast('Failed to compact conversation', 'error');
        }
    }
}

/** Auto-compact when conversation exceeds message threshold */
function maybeAutoCompact() {
    if (!currentConversationId) return;
    const msgCount = messagesEl.querySelectorAll('.chat-msg').length;
    if (msgCount >= 30) {
        handleCompactChat(true);
    }
}

// Wire up action buttons
document.getElementById('btn-clear-chat')?.addEventListener('click', handleClearChat);
document.getElementById('btn-new-chat')?.addEventListener('click', handleNewChat);
document.getElementById('btn-new-chat-topbar')?.addEventListener('click', handleNewChat);
document.getElementById('btn-compact-chat')?.addEventListener('click', handleCompactChat);

// ─── Model Selector ─────────────────────────────────────────────

const chatConfig = document.getElementById('chat-config');
const chatModelCapabilitiesEl = document.getElementById('chat-model-capabilities');
let currentModel = '';
let currentVisionModel = '';
let chatModelCapabilities = {};

function normalizeModelCapabilities(capabilities) {
    return {
        multimodal: !!(capabilities && capabilities.multimodal),
        image_input: !!(capabilities && capabilities.image_input),
        tool_calls: !(capabilities && capabilities.tool_calls === false),
        thinking: !!(capabilities && capabilities.thinking),
    };
}

/** Per-conversation thinking override: null = model default, true/false = explicit */
let thinkingEnabled = null;

async function hydrateChatModelCapabilities(data) {
    const known = {
        ...(data.model_capabilities || {}),
        ...(data.effective_model_capabilities || {}),
    };
    const modelIds = [];

    if (Array.isArray(data.models)) {
        data.models.forEach((model) => modelIds.push(model.model));
    }
    if (data.current) modelIds.push(data.current);
    if (data.vision_model) modelIds.push(data.vision_model);

    const missing = Array.from(new Set(modelIds.filter(Boolean))).filter((modelId) => !known[modelId]);
    if (missing.length > 0) {
        try {
            const resp = await fetch('/api/v1/providers/model-capabilities', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ models: missing, apply_overrides: true }),
            });
            const payload = await resp.json();
            if (payload && payload.ok && payload.model_capabilities) {
                Object.assign(known, payload.model_capabilities);
            }
        } catch (err) {
            console.error('Failed to resolve chat model capabilities:', err);
        }
    }

    chatModelCapabilities = known;
}

function modelCapabilityLabels(modelId) {
    const capabilities = normalizeModelCapabilities(chatModelCapabilities[modelId]);
    const labels = [];
    if (capabilities.image_input || capabilities.multimodal) labels.push('image');
    if (capabilities.tool_calls) labels.push('tools');
    if (capabilities.thinking) labels.push('thinking');
    return labels;
}

function formatModelOptionLabel(label, _modelId) {
    return label;
}

function renderActiveModelCapabilities(modelId) {
    if (!chatModelCapabilitiesEl) return;
    const caps = normalizeModelCapabilities(chatModelCapabilities[modelId]);
    const labels = modelCapabilityLabels(modelId);
    if (labels.length === 0) {
        chatModelCapabilitiesEl.hidden = true;
        chatModelCapabilitiesEl.textContent = '';
        return;
    }
    chatModelCapabilitiesEl.hidden = false;
    chatModelCapabilitiesEl.textContent = '';

    // Thinking switch first (if model supports it)
    if (caps.thinking) {
        const isOn = thinkingEnabled !== false;
        const wrapper = document.createElement('label');
        wrapper.className = 'thinking-switch';
        wrapper.title = 'Toggle thinking for this conversation';

        const checkbox = document.createElement('input');
        checkbox.type = 'checkbox';
        checkbox.checked = isOn;
        checkbox.addEventListener('change', () => {
            thinkingEnabled = checkbox.checked ? null : false;
        });

        const slider = document.createElement('span');
        slider.className = 'thinking-slider';

        const text = document.createElement('span');
        text.className = 'thinking-label';
        text.textContent = 'thinking';

        wrapper.appendChild(checkbox);
        wrapper.appendChild(slider);
        wrapper.appendChild(text);
        chatModelCapabilitiesEl.appendChild(wrapper);
    }

    // Static capability badges after thinking
    labels.filter(l => l !== 'thinking').forEach((label) => {
        const span = document.createElement('span');
        span.className = 'chat-model-capability-badge';
        span.textContent = label;
        chatModelCapabilitiesEl.appendChild(span);
    });
}

/** Fetch model data and init current model (no DOM select needed) */
async function loadChatModelDropdown() {
    if (chatConfig) {
        currentModel = chatConfig.dataset.model || '';
        currentVisionModel = chatConfig.dataset.visionModel || '';
    }
    try {
        const result = window.ModelLoader
            ? await ModelLoader.fetchGrouped({ fresh: true })
            : { groups: {}, raw: {} };
        await hydrateChatModelCapabilities(result.raw);
        renderActiveModelCapabilities(currentModel);
        const pillName = document.getElementById('chat-model-pill-name');
        if (pillName) pillName.textContent = currentModel.split('/').pop() || currentModel;
    } catch (err) {
        console.error('Failed to load models:', err);
    }
}

// ─── Model Picker Modal ─────────────────────────────────────────

let modelPickerBackdrop = null;
const RECENT_MODELS_KEY = 'homun.chat.recentModels';
const MAX_RECENT = 5;

function getRecentModels() {
    try {
        return JSON.parse(localStorage.getItem(RECENT_MODELS_KEY) || '[]');
    } catch { return []; }
}

function addRecentModel(modelId) {
    const recent = getRecentModels().filter(m => m !== modelId);
    recent.unshift(modelId);
    if (recent.length > MAX_RECENT) recent.length = MAX_RECENT;
    localStorage.setItem(RECENT_MODELS_KEY, JSON.stringify(recent));
}

async function openModelPicker() {
    if (modelPickerBackdrop) return;

    const result = window.ModelLoader
        ? await ModelLoader.fetchGrouped()
        : { groups: {}, raw: {} };
    const groups = result.groups;

    // Build modal DOM
    const backdrop = document.createElement('div');
    backdrop.className = 'chat-model-picker-backdrop';

    const modal = document.createElement('div');
    modal.className = 'chat-model-picker';

    // Header
    const header = document.createElement('div');
    header.className = 'chat-model-picker-header';
    const title = document.createElement('h3');
    title.textContent = 'Choose a Model';
    const closeBtn = document.createElement('button');
    closeBtn.type = 'button';
    closeBtn.className = 'chat-model-picker-close';
    closeBtn.textContent = '\u00d7';
    closeBtn.addEventListener('click', closeModelPicker);
    header.appendChild(title);
    header.appendChild(closeBtn);
    modal.appendChild(header);

    // Search
    const searchInput = document.createElement('input');
    searchInput.type = 'text';
    searchInput.className = 'chat-model-picker-search';
    searchInput.placeholder = 'Search models\u2026';
    searchInput.autocomplete = 'off';
    modal.appendChild(searchInput);

    // Body (scrollable list)
    const body = document.createElement('div');
    body.className = 'chat-model-picker-body';
    modal.appendChild(body);

    backdrop.appendChild(modal);
    document.body.appendChild(backdrop);
    modelPickerBackdrop = backdrop;

    // Close on backdrop click
    backdrop.addEventListener('click', (e) => {
        if (e.target === backdrop) closeModelPicker();
    });

    // Render models
    function renderList(filter) {
        body.textContent = '';
        const q = (filter || '').toLowerCase();
        const providerNames = window.ModelLoader ? ModelLoader.PROVIDER_NAMES : {};
        const recent = getRecentModels();
        let totalRendered = 0;

        // Recently used section
        if (recent.length > 0 && !q) {
            const label = document.createElement('div');
            label.className = 'chat-model-group-label';
            label.textContent = 'Recently Used';
            body.appendChild(label);
            recent.forEach(modelId => {
                body.appendChild(buildModelOption(modelId, modelId.split('/').pop() || modelId));
                totalRendered++;
            });
        }

        // Provider groups
        Object.keys(groups).forEach(provider => {
            const models = groups[provider];
            const filtered = q
                ? models.filter(m => m.label.toLowerCase().includes(q) || m.value.toLowerCase().includes(q))
                : models;
            if (filtered.length === 0) return;

            const label = document.createElement('div');
            label.className = 'chat-model-group-label';
            label.textContent = providerNames[provider] || provider;
            body.appendChild(label);

            filtered.forEach(m => {
                body.appendChild(buildModelOption(m.value, m.label));
                totalRendered++;
            });
        });

        if (totalRendered === 0) {
            const empty = document.createElement('div');
            empty.className = 'chat-model-picker-empty';
            empty.textContent = q ? 'No models match your search' : 'No models configured';
            body.appendChild(empty);
        }
    }

    function buildModelOption(modelId, label) {
        const btn = document.createElement('button');
        btn.type = 'button';
        btn.className = 'chat-model-option';
        if (modelId === currentModel) btn.classList.add('is-current');

        const nameSpan = document.createElement('span');
        nameSpan.className = 'chat-model-option-name';
        nameSpan.textContent = label;
        btn.appendChild(nameSpan);

        // Capability badges
        const caps = modelCapabilityLabels(modelId);
        if (caps.length > 0) {
            const badge = document.createElement('span');
            badge.className = 'chat-model-option-badge';
            badge.textContent = caps.join(' \u00b7 ');
            btn.appendChild(badge);
        }

        // Current check
        if (modelId === currentModel) {
            const check = document.createElement('span');
            check.className = 'chat-model-option-check';
            check.textContent = '\u2713';
            btn.appendChild(check);
        }

        btn.addEventListener('click', () => selectModel(modelId));
        return btn;
    }

    renderList('');
    searchInput.focus();

    searchInput.addEventListener('input', () => renderList(searchInput.value));
}

function closeModelPicker() {
    if (modelPickerBackdrop) {
        modelPickerBackdrop.remove();
        modelPickerBackdrop = null;
    }
}

async function selectModel(newModel) {
    if (newModel === currentModel) {
        closeModelPicker();
        return;
    }
    try {
        const res = await fetch('/api/v1/config', {
            method: 'PATCH',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ key: 'agent.model', value: newModel })
        });
        if (res.ok) {
            currentModel = newModel;
            thinkingEnabled = null;
            if (chatConfig) chatConfig.dataset.model = newModel;
            addRecentModel(newModel);
            const pillName = document.getElementById('chat-model-pill-name');
            if (pillName) pillName.textContent = newModel.split('/').pop() || newModel;
            renderActiveModelCapabilities(newModel);
            showToast('Model switched to ' + newModel.split('/').pop(), 'success');
        } else {
            showToast('Failed to switch model', 'error');
        }
    } catch (e) {
        console.error('Failed to switch model:', e);
        showToast('Failed to switch model', 'error');
    }
    closeModelPicker();
}

// providerDisplayName removed — now uses ModelLoader.PROVIDER_NAMES (DRY)

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
    closeMcpPicker();
    if (!chatPlusMenu) return;
    chatPlusMenu.hidden = !chatPlusMenu.hidden;
});

document.addEventListener('click', () => {
    closeChatPlusMenu();
    closeMcpPicker();
    closeToolsDropdown();
});

chatPlusMenu?.addEventListener('click', (e) => {
    e.stopPropagation();
});

chatMcpPicker?.addEventListener('click', (e) => {
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
    ensureMcpServersLoaded()
        .then(() => {
            mcpPickerOpen = !mcpPickerOpen;
            if (chatMcpPicker) {
                chatMcpPicker.hidden = !mcpPickerOpen;
            }
            if (mcpPickerOpen) {
                renderMcpPickerList();
                chatMcpSearch?.focus();
            }
        })
        .catch((error) => {
            console.error('Failed to load MCP servers:', error);
            showToast('Failed to load MCP servers', 'error');
        });
});

chatMcpSearch?.addEventListener('input', () => {
    mcpSearchQuery = chatMcpSearch.value || '';
    renderMcpPickerList();
});

chatImageInput?.addEventListener('change', async () => {
    if (chatImageInput.files && chatImageInput.files.length > 0) {
        await uploadChatFiles('image', chatImageInput.files);
        chatImageInput.value = '';
    }
});

chatDocInput?.addEventListener('change', () => {
    if (chatDocInput.files && chatDocInput.files.length > 0) {
        uploadChatFiles('document', chatDocInput.files);
        chatDocInput.value = '';
    }
});

/** Model pill click → open modal picker */
document.getElementById('chat-model-pill')?.addEventListener('click', (e) => {
    e.stopPropagation();
    openModelPicker();
});

// Load model dropdown on connect
loadChatModelDropdown();
closeChatPlusMenu();
setRunBadge('offline', 'Offline');
syncEmptyState();

// ─── Send button state: mic ↔ send arrow ────────────────────────

chatText?.addEventListener('input', () => updateSendButtonState());
// Initial state
updateSendButtonState();

// ─── Tools dropdown ─────────────────────────────────────────────

let toolsDropdownEl = null;

/** Create an SVG element from a template string (safe: all content is static) */
function svgFromTemplate(tmpl) {
    const container = document.createElement('template');
    container.innerHTML = tmpl.trim(); // eslint-disable-line -- static SVG only
    return container.content.firstChild;
}

const TOOLS_ITEMS = [
    { label: 'Create Skill', svg: '<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M9 2v14"/><path d="M2 9h14"/></svg>', prompt: 'Create a new skill that ' },
    { label: 'Create Automation', svg: '<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M13 2L5 10l3 3 8-8z"/><path d="M2 16h4"/></svg>', prompt: 'Create a new automation that ' },
    { label: 'Create Workflow', svg: '<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="4" cy="9" r="2"/><circle cx="14" cy="5" r="2"/><circle cx="14" cy="13" r="2"/><path d="M6 9h4l2-4M10 9l2 4"/></svg>', prompt: 'Create a workflow that ' },
    { label: 'Browse Web', svg: '<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="9" cy="9" r="7"/><path d="M2 9h14"/><path d="M9 2a11 11 0 014 7 11 11 0 01-4 7 11 11 0 01-4-7 11 11 0 014-7z"/></svg>', prompt: 'Search with the browser for ' },
    { label: 'MCP Servers', svg: '<svg viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="3" width="14" height="5" rx="1"/><rect x="2" y="10" width="14" height="5" rx="1"/><circle cx="5" cy="5.5" r="0.8" fill="currentColor"/><circle cx="5" cy="12.5" r="0.8" fill="currentColor"/></svg>', prompt: null },
];

function openToolsDropdown() {
    if (toolsDropdownEl) { closeToolsDropdown(); return; }
    const anchor = btnChatTools;
    if (!anchor) return;

    const menu = document.createElement('div');
    menu.className = 'chat-tools-dropdown';
    TOOLS_ITEMS.forEach(item => {
        const btn = document.createElement('button');
        btn.type = 'button';
        btn.className = 'chat-tools-dropdown-item';
        btn.appendChild(svgFromTemplate(item.svg));
        const span = document.createElement('span');
        span.textContent = item.label;
        btn.appendChild(span);
        btn.addEventListener('click', () => {
            closeToolsDropdown();
            if (item.prompt) {
                insertPrompt(item.prompt, item.label);
            } else {
                openMcpPickerFromTools();
            }
        });
        menu.appendChild(btn);
    });

    document.body.appendChild(menu);
    toolsDropdownEl = menu;

    requestAnimationFrame(() => {
        const rect = anchor.getBoundingClientRect();
        const mw = menu.offsetWidth;
        const mh = menu.offsetHeight;
        let left = rect.left;
        let top = rect.top - mh - 8;
        if (left + mw > window.innerWidth - 8) left = window.innerWidth - mw - 8;
        if (top < 8) top = rect.bottom + 8;
        menu.style.left = left + 'px';
        menu.style.top = top + 'px';
    });
}

function closeToolsDropdown() {
    if (toolsDropdownEl) {
        toolsDropdownEl.remove();
        toolsDropdownEl = null;
    }
}

function insertPrompt(text, label) {
    if (!chatText) return;
    chatText.value = text;
    chatText.focus();
    chatText.setSelectionRange(text.length, text.length);
    updateSendButtonState();
    // Activate tool mode indicator
    if (label) {
        activeToolMode = label;
        if (btnChatTools) btnChatTools.classList.add('is-active');
        if (chatToolsLabel) chatToolsLabel.textContent = label;
        if (chatToolsDismiss) chatToolsDismiss.hidden = false;
    }
}

function clearToolMode() {
    activeToolMode = null;
    if (btnChatTools) btnChatTools.classList.remove('is-active');
    if (chatToolsLabel) chatToolsLabel.textContent = 'Tools';
    if (chatToolsDismiss) chatToolsDismiss.hidden = true;
    if (chatText) {
        chatText.value = '';
        chatText.focus();
    }
    updateSendButtonState();
}

function openMcpPickerFromTools() {
    closeChatPlusMenu();
    if (typeof ensureMcpServersLoaded === 'function') {
        ensureMcpServersLoaded().then(() => {
            mcpPickerOpen = true;
            if (chatMcpPicker) chatMcpPicker.hidden = false;
            renderMcpPickerList();
            chatMcpSearch?.focus();
        }).catch(() => showToast('Failed to load MCP servers', 'error'));
    }
}

btnChatTools?.addEventListener('click', (e) => {
    e.stopPropagation();
    closeChatPlusMenu();
    closeMcpPicker();
    openToolsDropdown();
});

chatToolsDismiss?.addEventListener('click', (e) => {
    e.stopPropagation();
    clearToolMode();
});

// Tools dropdown also closes on outside click (via existing document click listener)

// ─── Drag & drop ────────────────────────────────────────────────

let dragCounter = 0;

if (chatMainEl) {
    chatMainEl.addEventListener('dragenter', (e) => {
        e.preventDefault();
        dragCounter++;
        if (chatDragOverlay) chatDragOverlay.hidden = false;
    });

    chatMainEl.addEventListener('dragleave', (e) => {
        e.preventDefault();
        dragCounter--;
        if (dragCounter <= 0) {
            dragCounter = 0;
            if (chatDragOverlay) chatDragOverlay.hidden = true;
        }
    });

    chatMainEl.addEventListener('dragover', (e) => {
        e.preventDefault();
        e.dataTransfer.dropEffect = 'copy';
    });

    chatMainEl.addEventListener('drop', (e) => {
        e.preventDefault();
        dragCounter = 0;
        if (chatDragOverlay) chatDragOverlay.hidden = true;

        const files = e.dataTransfer?.files;
        if (!files || files.length === 0) return;

        // Route files by type: images vs documents
        const images = [];
        const docs = [];
        for (const f of files) {
            if (f.type.startsWith('image/')) images.push(f);
            else docs.push(f);
        }
        if (images.length > 0) uploadChatFiles('image', images);
        if (docs.length > 0) uploadChatFiles('document', docs);
    });
}

// ─── Voice input (Web Speech API) ───────────────────────────────

function initSpeechRecognition() {
    const SpeechRecognition = window.SpeechRecognition || window.webkitSpeechRecognition;
    if (!SpeechRecognition) {
        // No browser support — permanently show send icon
        if (btnSend) btnSend.classList.add('no-mic');
        return null;
    }
    const rec = new SpeechRecognition();
    rec.continuous = false;
    rec.interimResults = false;
    rec.lang = navigator.language || 'en-US';

    rec.addEventListener('result', (e) => {
        const transcript = Array.from(e.results)
            .map(r => r[0].transcript)
            .join(' ');
        if (chatText && transcript) {
            chatText.value += (chatText.value ? ' ' : '') + transcript;
            updateSendButtonState();
        }
    });

    rec.addEventListener('end', () => {
        isRecording = false;
        if (btnSend) btnSend.classList.remove('is-recording');
        updateSendButtonState();
    });

    rec.addEventListener('error', (e) => {
        isRecording = false;
        if (btnSend) btnSend.classList.remove('is-recording');
        updateSendButtonState();
        if (e.error === 'not-allowed' || e.error === 'service-not-allowed') {
            if (btnSend) btnSend.classList.add('no-mic');
            showToast('Microphone access denied', 'error');
        }
    });

    return rec;
}

function startRecording() {
    if (isRecording) return;
    if (!recognition) recognition = initSpeechRecognition();
    if (!recognition) return;
    try {
        recognition.start();
        isRecording = true;
        if (btnSend) btnSend.classList.add('is-recording');
    } catch (e) {
        console.error('Speech recognition error:', e);
    }
}

function stopRecording() {
    if (!isRecording || !recognition) return;
    recognition.stop();
}

// Init speech recognition (non-blocking)
recognition = initSpeechRecognition();

// ─── Workflow progress donut chart ─────────────────────────────

/**
 * Build an SVG donut chart showing workflow step progress.
 * Returns an SVG element (36x36) with a ring that fills as steps complete.
 */
function buildWorkflowDonut(completed, total, status) {
    const size = 36;
    const strokeWidth = 4;
    const radius = (size - strokeWidth) / 2;
    const circumference = 2 * Math.PI * radius;
    const progress = total > 0 ? completed / total : 0;
    const dashOffset = circumference * (1 - progress);

    // Status-based colors
    let strokeColor = 'var(--accent)';
    if (status === 'completed') strokeColor = 'var(--ok)';
    else if (status === 'failed') strokeColor = 'var(--err)';
    else if (status === 'paused') strokeColor = 'var(--warn)';

    const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    svg.setAttribute('width', size);
    svg.setAttribute('height', size);
    svg.setAttribute('viewBox', `0 0 ${size} ${size}`);
    svg.classList.add('wf-donut');

    // Background ring
    const bgCircle = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
    bgCircle.setAttribute('cx', size / 2);
    bgCircle.setAttribute('cy', size / 2);
    bgCircle.setAttribute('r', radius);
    bgCircle.setAttribute('fill', 'none');
    bgCircle.setAttribute('stroke', 'var(--border-subtle)');
    bgCircle.setAttribute('stroke-width', strokeWidth);
    svg.appendChild(bgCircle);

    // Progress ring
    const progressCircle = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
    progressCircle.setAttribute('cx', size / 2);
    progressCircle.setAttribute('cy', size / 2);
    progressCircle.setAttribute('r', radius);
    progressCircle.setAttribute('fill', 'none');
    progressCircle.setAttribute('stroke', strokeColor);
    progressCircle.setAttribute('stroke-width', strokeWidth);
    progressCircle.setAttribute('stroke-linecap', 'round');
    progressCircle.setAttribute('stroke-dasharray', circumference);
    progressCircle.setAttribute('stroke-dashoffset', dashOffset);
    progressCircle.setAttribute('transform', `rotate(-90 ${size / 2} ${size / 2})`);
    progressCircle.classList.add('wf-donut-progress');
    svg.appendChild(progressCircle);

    // Center label: step count or checkmark
    const text = document.createElementNS('http://www.w3.org/2000/svg', 'text');
    text.setAttribute('x', size / 2);
    text.setAttribute('y', size / 2);
    text.setAttribute('text-anchor', 'middle');
    text.setAttribute('dominant-baseline', 'central');
    text.setAttribute('font-size', '9');
    text.setAttribute('fill', 'var(--t2)');
    text.setAttribute('font-weight', '600');
    if (status === 'completed') {
        text.textContent = '\u2713'; // checkmark
        text.setAttribute('font-size', '14');
        text.setAttribute('fill', 'var(--ok)');
    } else if (status === 'failed') {
        text.textContent = '\u2717'; // X mark
        text.setAttribute('font-size', '14');
        text.setAttribute('fill', 'var(--err)');
    } else {
        text.textContent = `${completed}/${total}`;
    }
    svg.appendChild(text);

    return svg;
}

/**
 * Handle a workflow_progress WebSocket event.
 * Creates or updates a compact workflow progress card in the chat.
 */
function handleWorkflowProgress(progress) {
    if (!progress || !progress.workflow_id) return;

    const wfId = progress.workflow_id;
    const cardId = `wf-progress-${wfId}`;
    let card = document.getElementById(cardId);

    if (!card) {
        // Create new workflow progress card
        card = document.createElement('div');
        card.id = cardId;
        card.className = 'wf-progress-card';

        const inner = document.createElement('div');
        inner.className = 'wf-progress-inner';

        const donutWrap = document.createElement('div');
        donutWrap.className = 'wf-donut-wrap';
        inner.appendChild(donutWrap);

        const info = document.createElement('div');
        info.className = 'wf-progress-info';
        const nameEl = document.createElement('div');
        nameEl.className = 'wf-progress-name';
        const statusEl = document.createElement('div');
        statusEl.className = 'wf-progress-status';
        info.appendChild(nameEl);
        info.appendChild(statusEl);
        inner.appendChild(info);

        card.appendChild(inner);
        messagesEl.appendChild(card);
    }

    // Update card content
    const donutWrap = card.querySelector('.wf-donut-wrap');
    const nameEl = card.querySelector('.wf-progress-name');
    const statusEl = card.querySelector('.wf-progress-status');

    const completed = progress.completed_steps || 0;
    const total = progress.total_steps || 1;
    const status = progress.status || 'running';

    // Replace donut SVG
    donutWrap.replaceChildren(buildWorkflowDonut(completed, total, status));

    // Update name and status text
    nameEl.textContent = progress.workflow_name || wfId;

    if (status === 'completed') {
        statusEl.textContent = `Completed (${total} steps)`;
        card.dataset.status = 'completed';
    } else if (status === 'failed') {
        statusEl.textContent = progress.error
            ? `Failed at step ${completed + 1}: ${progress.error}`
            : `Failed at step ${completed + 1}/${total}`;
        card.dataset.status = 'failed';
    } else if (status === 'paused') {
        statusEl.textContent = `Waiting for approval \u2014 step ${completed + 1}/${total}: ${progress.current_step || ''}`;
        card.dataset.status = 'paused';
    } else if (status === 'step_started') {
        statusEl.textContent = progress.current_step
            ? `Running step ${completed + 1}/${total}: ${progress.current_step}`
            : `Starting step ${completed + 1}/${total}`;
        card.dataset.status = 'running';
    } else {
        statusEl.textContent = progress.current_step
            ? `Step ${completed}/${total} done \u2014 running: ${progress.current_step}`
            : `Step ${completed}/${total}`;
        card.dataset.status = 'running';
    }

    scrollThreadToBottom();
}

// ─── Conversation list polling ──────────────────────────────
// Periodically refresh the sidebar so "is-running" indicators update
// even when the user is viewing a different conversation (INFRA-2).
function startConversationPolling() {
    if (conversationPollTimer) return;
    conversationPollTimer = setInterval(refreshConversationList, 5000);
}
function stopConversationPolling() {
    if (conversationPollTimer) {
        clearInterval(conversationPollTimer);
        conversationPollTimer = null;
    }
}

async function bootstrapChat() {
    try {
        showArchived = window.localStorage.getItem('homun.chat.showArchived') === '1';
        sidebarCollapsed = window.localStorage.getItem('homun.chat.sidebarCollapsed') === '1';
        applySidebarState();
        await ensureConversationSelected();
        connect();
        startConversationPolling();
    } catch (e) {
        console.error('Failed to bootstrap chat:', e);
        showToast('Failed to load conversations', 'error');
    }
}

bootstrapChat();
