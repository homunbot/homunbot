// Homun — Chat WebSocket client with streaming, markdown, and tool indicators

const messagesEl = document.getElementById('messages');
const threadWrapEl = document.querySelector('.chat-thread-wrap');
const chatForm = document.getElementById('chat-form');
const chatText = document.getElementById('chat-text');
const wsStatus = document.getElementById('ws-status');
const chatPlanPanel = document.getElementById('chat-plan-panel');
const chatPlanToggle = document.getElementById('chat-plan-toggle');
const chatPlanObjective = document.getElementById('chat-plan-objective');
const chatPlanDoneWrap = document.getElementById('chat-plan-done-wrap');
const chatPlanDone = document.getElementById('chat-plan-done');
const chatPlanRemainingWrap = document.getElementById('chat-plan-remaining-wrap');
const chatPlanRemaining = document.getElementById('chat-plan-remaining');
const chatPlanConstraintsWrap = document.getElementById('chat-plan-constraints-wrap');
const chatPlanConstraints = document.getElementById('chat-plan-constraints');
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
const chatModalCancel = document.getElementById('chat-modal-cancel');
const chatModalConfirm = document.getElementById('chat-modal-confirm');

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
let renamingConversationId = null;
let renameDraft = '';
let multiSelectMode = false;
let selectedConversations = new Set();
let searchDebounceTimer = null;
let modalState = null;
let pendingAttachments = [];
let pendingMcpServers = [];
let availableMcpServers = [];
let mcpPickerOpen = false;
let mcpSearchQuery = '';
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

function scrollThreadToBottom() {
    const scroller = threadWrapEl || messagesEl;
    if (!scroller) return;
    window.requestAnimationFrame(() => {
        scroller.scrollTop = scroller.scrollHeight;
    });
}

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
}

// sidebar menu removed — search modal replaces it

function closeConversationMenu() {
    openConversationMenuId = null;
    renderConversationList();
}

function openModal({ title, copy, confirmLabel = 'Confirm', destructive = false, onConfirm }) {
    modalState = { onConfirm };
    if (chatModalTitle) chatModalTitle.textContent = title;
    if (chatModalCopy) chatModalCopy.textContent = copy;
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
    if (chatModalBackdrop) {
        chatModalBackdrop.hidden = true;
    }
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

    // Checkbox (multi-select only)
    if (multiSelectMode) {
        const cb = document.createElement('input');
        cb.type = 'checkbox';
        cb.className = 'chat-select-checkbox';
        cb.checked = selectedConversations.has(conversation.conversation_id);
        cb.addEventListener('click', (e) => { e.stopPropagation(); toggleConversationSelection(conversation.conversation_id); });
        item.appendChild(cb);
    }

    // Body
    const body = document.createElement('button');
    body.type = 'button';
    body.className = 'chat-conversation-item-body';

    if (renamingConversationId === conversation.conversation_id) {
        const inp = document.createElement('input');
        inp.type = 'text';
        inp.className = 'input chat-rename-input';
        inp.value = renameDraft || conversation.title || 'New conversation';
        inp.setAttribute('aria-label', 'Rename conversation');
        inp.addEventListener('click', (e) => e.stopPropagation());
        inp.addEventListener('input', () => { renameDraft = inp.value; });
        inp.addEventListener('keydown', async (e) => {
            if (e.key === 'Enter') { e.preventDefault(); await commitRenameConversation(conversation); }
            else if (e.key === 'Escape') { e.preventDefault(); cancelRenameConversation(); }
        });
        inp.addEventListener('blur', async () => { await commitRenameConversation(conversation); });
        body.appendChild(inp);
        setTimeout(() => { inp.focus(); inp.select(); }, 0);
    } else {
        const nameEl = document.createElement('span');
        nameEl.className = 'chat-conversation-name';
        nameEl.textContent = capitalizeFirst(conversation.title) || 'New conversation';
        body.appendChild(nameEl);
    }
    const dateEl = document.createElement('span');
    dateEl.className = 'chat-conversation-date';
    dateEl.textContent = formatConversationTimestamp(conversation.updated_at);
    body.appendChild(dateEl);

    body.addEventListener('click', () => {
        if (renamingConversationId === conversation.conversation_id) return;
        if (multiSelectMode) { toggleConversationSelection(conversation.conversation_id); return; }
        if (conversation.conversation_id !== currentConversationId) selectConversation(conversation.conversation_id);
    });
    item.appendChild(body);

    // Menu button (three dots)
    const menuBtn = document.createElement('button');
    menuBtn.type = 'button';
    menuBtn.className = 'chat-conversation-menu-btn';
    menuBtn.setAttribute('aria-label', 'Conversation actions');
    menuBtn.innerHTML = '<svg viewBox="0 0 18 18" fill="currentColor" width="14" height="14"><circle cx="4" cy="9" r="1.4"/><circle cx="9" cy="9" r="1.4"/><circle cx="14" cy="9" r="1.4"/></svg>';
    menuBtn.addEventListener('click', (e) => {
        e.stopPropagation();
        openConversationMenuId = openConversationMenuId === conversation.conversation_id ? null : conversation.conversation_id;
        renderConversationList();
    });
    item.appendChild(menuBtn);

    // Menu dropdown
    const menu = document.createElement('div');
    menu.className = 'chat-conversation-menu';
    if (openConversationMenuId !== conversation.conversation_id) menu.hidden = true;
    const actions = [
        { action: 'rename', label: 'Rename', cls: '' },
        { action: 'select', label: multiSelectMode ? 'Deselect' : 'Select', cls: '' },
        { action: conversation.archived ? 'unarchive' : 'archive', label: conversation.archived ? 'Unarchive' : 'Archive', cls: '' },
        { action: 'delete', label: 'Delete', cls: 'is-danger' },
    ];
    actions.forEach(({ action, label, cls }) => {
        const btn = document.createElement('button');
        btn.type = 'button';
        btn.className = 'chat-conversation-menu-item' + (cls ? ' ' + cls : '');
        btn.dataset.action = action;
        btn.textContent = label;
        btn.addEventListener('click', async (e) => {
            e.stopPropagation();
            if (action === 'rename') await renameConversation(conversation);
            else if (action === 'select') enterMultiSelectMode(conversation.conversation_id);
            else if (action === 'archive') await setConversationArchived(conversation, true);
            else if (action === 'unarchive') await setConversationArchived(conversation, false);
            else if (action === 'delete') await deleteConversation(conversation);
        });
        menu.appendChild(btn);
    });
    item.appendChild(menu);

    return item;
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
    renamingConversationId = conversation.conversation_id;
    renameDraft = conversation.title || 'New conversation';
    renderConversationList();
}

function cancelRenameConversation() {
    renamingConversationId = null;
    renameDraft = '';
    renderConversationList();
}

async function commitRenameConversation(conversation) {
    if (renamingConversationId !== conversation.conversation_id) return;
    const nextTitle = String(renameDraft || '').trim();
    renamingConversationId = null;
    renameDraft = '';
    renderConversationList();
    try {
        await updateConversation(conversation.conversation_id, { title: nextTitle });
        await refreshConversationList();
        showToast('Conversation renamed', 'success');
    } catch (e) {
        console.error('Failed to rename conversation:', e);
        showToast('Failed to rename conversation', 'error');
    }
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
                attachments: m.attachments || [],
                mcpServers: m.mcp_servers || [],
            });
        });
        syncEmptyState();
        scrollThreadToBottom();
    } catch (e) {
        console.error('Failed to load chat history:', e);
    }
}

function syncEmptyState() {
    if (!chatEmptyState || !messagesEl) return;
    chatEmptyState.style.display = messagesEl.children.length > 0 ? 'none' : '';
}

function renderPlanList(target, items) {
    if (!target) return;
    target.textContent = '';
    items.slice(0, 6).forEach((item) => {
        const li = document.createElement('li');
        li.textContent = item;
        target.appendChild(li);
    });
}

function isUsefulPlanConstraint(item) {
    const text = String(item || '').trim();
    return Boolean(text) && !GENERIC_PLAN_CONSTRAINTS.has(text);
}

function applyExecutionPlan(plan) {
    currentPlanState = plan && typeof plan === 'object' ? plan : null;
    if (!chatPlanPanel || !chatPlanObjective || !chatPlanDoneWrap || !chatPlanRemainingWrap) return;

    // Hide if no plan, no objective, or plan has no meaningful content
    const hasExplicitSteps = Array.isArray(currentPlanState?.explicit_steps) && currentPlanState.explicit_steps.length > 0;
    const hasCompletedSteps = Array.isArray(currentPlanState?.completed_steps) && currentPlanState.completed_steps.length > 0;
    const hasBlockers = Array.isArray(currentPlanState?.active_blockers) && currentPlanState.active_blockers.length > 0;
    const hasSources = Array.isArray(currentPlanState?.required_sources) && currentPlanState.required_sources.length > 0;
    const hasContent = hasExplicitSteps || hasCompletedSteps || hasBlockers || hasSources || currentPlanState?.current_source;
    if (!currentPlanState || !currentPlanState.objective || (!hasContent && !isProcessing)) {
        chatPlanPanel.hidden = true;
        chatPlanPanel.classList.add('collapsed');
        if (chatPlanToggle) chatPlanToggle.setAttribute('aria-expanded', 'false');
        chatPlanObjective.textContent = '';
        if (chatPlanDone) chatPlanDone.textContent = '';
        if (chatPlanRemaining) chatPlanRemaining.textContent = '';
        if (chatPlanConstraints) chatPlanConstraints.textContent = '';
        chatPlanDoneWrap.hidden = true;
        chatPlanRemainingWrap.hidden = true;
        if (chatPlanConstraintsWrap) chatPlanConstraintsWrap.hidden = true;
        // Reset label in case explicit plan changed it
        resetPlanLabels();
        return;
    }

    // If an explicit plan is present, use the dedicated renderer
    if (Array.isArray(currentPlanState.explicit_steps) && currentPlanState.explicit_steps.length > 0) {
        renderExplicitPlan(currentPlanState);
        return;
    }

    // --- Inferred mode (existing behavior) ---
    chatPlanPanel.hidden = false;
    if (
        !planExpanded &&
        (
            (Array.isArray(currentPlanState.required_sources) && currentPlanState.required_sources.length > 0) ||
            currentPlanState.current_source
        )
    ) {
        planExpanded = true;
    }
    chatPlanPanel.classList.toggle('collapsed', !planExpanded);
    if (chatPlanToggle) chatPlanToggle.setAttribute('aria-expanded', String(planExpanded));
    chatPlanObjective.textContent = currentPlanState.objective || '';
    resetPlanLabels();

    const doneItems = Array.isArray(currentPlanState.completed_steps)
        ? currentPlanState.completed_steps
        : [];
    const sourceDoneItems = Array.isArray(currentPlanState.completed_sources)
        ? currentPlanState.completed_sources.map((item) => `Source completed: ${item}`)
        : [];
    const remainingItems = [
        ...(Array.isArray(currentPlanState.active_blockers) ? currentPlanState.active_blockers : []),
        ...(currentPlanState.current_source && !(Array.isArray(currentPlanState.completed_sources) ? currentPlanState.completed_sources : []).includes(currentPlanState.current_source)
            ? [`Current source in progress: ${currentPlanState.current_source}`]
            : []),
        ...(Array.isArray(currentPlanState.required_sources)
            ? currentPlanState.required_sources
                .filter((item) => !(Array.isArray(currentPlanState.completed_sources) ? currentPlanState.completed_sources : []).includes(item))
                .map((item) => `Source still required: ${item}`)
            : []),
        ...(Array.isArray(currentPlanState.constraints)
            ? currentPlanState.constraints.filter((item) => !doneItems.includes(item))
            : []),
    ].filter((item, index, items) => item && items.indexOf(item) === index);
    const constraintItems = Array.isArray(currentPlanState.constraints)
        ? currentPlanState.constraints.filter((item) => (
            isUsefulPlanConstraint(item) && !remainingItems.includes(item)
        ))
        : [];

    chatPlanDoneWrap.hidden = doneItems.length === 0;
    chatPlanRemainingWrap.hidden = remainingItems.length === 0;
    if (chatPlanConstraintsWrap) chatPlanConstraintsWrap.hidden = !planExpanded || constraintItems.length === 0;
    renderPlanList(chatPlanDone, [...sourceDoneItems, ...doneItems]);
    renderPlanList(chatPlanRemaining, remainingItems);
    renderPlanList(chatPlanConstraints, constraintItems);
    updatePlanProgressBadge();
}

/** Render an explicit plan created via plan_task with status icons. */
function renderExplicitPlan(plan) {
    chatPlanPanel.hidden = false;
    // Keep collapsed as pill — user clicks to expand
    chatPlanPanel.classList.toggle('collapsed', !planExpanded);
    if (chatPlanToggle) chatPlanToggle.setAttribute('aria-expanded', String(planExpanded));
    chatPlanObjective.textContent = plan.objective || '';

    // Render steps with status icons in the "Done" column (relabeled to "Plan")
    const doneLabel = chatPlanDoneWrap.querySelector('.chat-plan-label');
    if (doneLabel) doneLabel.textContent = 'Plan';
    chatPlanDoneWrap.hidden = false;

    const stepItems = plan.explicit_steps.map((step) => {
        const icon = step.status === 'completed' ? '\u2705'
            : step.status === 'in_progress' ? '\uD83D\uDD04'
            : '\u2B1C';
        return `${icon} ${step.description}`;
    });
    renderPlanList(chatPlanDone, stepItems);

    // Hide "Remaining" column — step statuses already convey progress
    chatPlanRemainingWrap.hidden = true;
    if (chatPlanRemaining) chatPlanRemaining.textContent = '';

    // Show active blockers if any
    const blockers = Array.isArray(plan.active_blockers) ? plan.active_blockers : [];
    if (chatPlanConstraintsWrap) {
        chatPlanConstraintsWrap.hidden = blockers.length === 0;
        if (blockers.length > 0) {
            const label = chatPlanConstraintsWrap.querySelector('.chat-plan-label');
            if (label) label.textContent = 'Blockers';
            renderPlanList(chatPlanConstraints, blockers);
        }
    }

    // Show verification note when all steps completed
    if (plan.verification && plan.explicit_steps.every((s) => s.status === 'completed')) {
        if (chatPlanConstraintsWrap) {
            chatPlanConstraintsWrap.hidden = false;
            const label = chatPlanConstraintsWrap.querySelector('.chat-plan-label');
            if (label) label.textContent = 'Verification';
            renderPlanList(chatPlanConstraints, [plan.verification]);
        }
    }
    updatePlanProgressBadge();
}

/** Reset column labels to their defaults (after explicit plan cleanup). */
function resetPlanLabels() {
    const doneLabel = chatPlanDoneWrap?.querySelector('.chat-plan-label');
    if (doneLabel) doneLabel.textContent = 'Done';
    const constraintLabel = chatPlanConstraintsWrap?.querySelector('.chat-plan-label');
    if (constraintLabel) constraintLabel.textContent = 'Constraints';
}

/** Update (or create) the compact progress badge inside the plan header. */
function updatePlanProgressBadge() {
    if (!chatPlanPanel || !currentPlanState) return;
    let el = chatPlanPanel.querySelector('.chat-plan-progress');
    if (!el) {
        el = document.createElement('span');
        el.className = 'chat-plan-progress';
        const hdr = chatPlanPanel.querySelector('.chat-plan-header-copy');
        if (hdr) hdr.appendChild(el);
    }
    if (Array.isArray(currentPlanState.explicit_steps) && currentPlanState.explicit_steps.length) {
        const total = currentPlanState.explicit_steps.length;
        const done = currentPlanState.explicit_steps.filter(s => s.status === 'completed').length;
        el.textContent = done === total ? `\u2705 ${done}/${total}` : `\uD83D\uDD04 ${done}/${total}`;
    } else {
        const done = (currentPlanState.completed_steps || []).length;
        const rem = (currentPlanState.constraints || []).length;
        el.textContent = (done + rem) > 0 ? `(${done}/${done + rem})` : '';
    }
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

    for (const event of run.events || []) {
        if (event.event_type === 'tool_start') {
            showToolIndicator(event.name, event.tool_call || null);
        } else if (event.event_type === 'tool_end') {
            endToolIndicator(event.name);
        } else if (event.event_type === 'status') {
            lastStatusHint = event.name || '';
            setRunBadge('working', lastStatusHint || 'Running');
        } else if (event.event_type === 'model') {
            setExecutionModel(event.name || run.effective_model || '');
        } else if (event.event_type === 'plan') {
            applyExecutionPlan(parsePlanPayload(event.name));
        }
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
    openConversationMenuId = null;
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
        const name = document.createElement('span');
        name.className = 'chat-search-result-name';
        name.textContent = capitalizeFirst(c.title) || 'New conversation';
        el.appendChild(name);
        const date = document.createElement('span');
        date.className = 'chat-search-result-date';
        date.textContent = formatConversationTimestamp(c.updated_at);
        el.appendChild(date);
        el.addEventListener('click', () => {
            closeSearchModal();
            selectConversation(c.conversation_id);
        });
        chatSearchResults.appendChild(el);
    });
}

// ─── Multi-select ───
function enterMultiSelectMode(initialId) {
    openConversationMenuId = null;
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
    if (openConversationMenuId && !e.target.closest('.chat-conversation-item')) {
        closeConversationMenu();
    }
    if (mcpPickerOpen && !e.target.closest('.chat-plus-wrap')) {
        closeMcpPicker();
    }
});

document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') {
        if (!chatSearchModal?.hidden) { closeSearchModal(); return; }
        if (multiSelectMode) { exitMultiSelectMode(); return; }
        if (renamingConversationId) { cancelRenameConversation(); }
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
    closeModal();
    if (action) {
        await action();
    }
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
            countEl.textContent = `${reasoningCount} step${reasoningCount === 1 ? '' : 's'}`;
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

    // Auto-expand while tools are running
    reasoningSectionEl.classList.remove('collapsed');

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
    const res = await fetch('/api/v1/mcp/servers');
    if (!res.ok) {
        throw new Error('Failed to load MCP servers');
    }
    const servers = await res.json();
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
    ws = new WebSocket(`${proto}//${location.host}/ws/chat?conversation_id=${encodeURIComponent(currentConversationId)}`);

    ws.onopen = () => {
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

    ws.onmessage = (event) => {
        try {
            const data = JSON.parse(event.data);

            if (data.type === 'connected') {
                if (data.conversation_id && data.conversation_id !== currentConversationId) {
                    return;
                }
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

    ws.onclose = () => {
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

    ws.onerror = () => {
        ws.close();
    };
}

// ─── Streaming ─────────────────────────────────────────────────

/** Handle an incremental streaming chunk from the LLM. */
function handleStreamChunk(delta) {
    if (!delta) return;
    purgeOrphanLiveArtifacts();

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
    scrollThreadToBottom();
}

function settleLiveArtifacts() {
    purgeOrphanLiveArtifacts();
    if (streamingEl) {
        if (streamingContent.trim()) {
            renderContent(streamingEl, streamingContent, 'assistant');
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

// ─── Message rendering ─────────────────────────────────────────

function addMessage(role, content, toolsUsed, options = {}) {
    const div = document.createElement('div');
    div.className = `chat-msg ${role}`;
    if (options.runId) {
        div.dataset.runId = options.runId;
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

    // Show tool badges for messages that used tools
    if (toolsUsed && toolsUsed.length > 0) {
        const badge = document.createElement('div');
        badge.className = 'chat-tools-badge';
        badge.textContent = toolsUsed.join(', ');
        div.prepend(badge);
    }

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
    const payload = { content: text, attachments, mcp_servers: mcpServers };
    if (thinkingEnabled !== null) {
        payload.thinking = thinkingEnabled;
    }
    ws.send(JSON.stringify(payload));
    chatText.value = '';
    chatText.style.height = 'auto';
    chatText.focus();
    pendingAttachments = [];
    pendingMcpServers = [];
    clearExecutionPlan();
    renderComposerContextStrip();
    closeMcpPicker();
    if (chatImageInput) chatImageInput.value = '';
    if (chatDocInput) chatDocInput.value = '';
    setProcessing(true);
    closeChatPlusMenu();
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
        btnSend.setAttribute('aria-label', processing ? 'Stop current run' : 'Send message');
        btnSend.title = processing ? 'Stop' : 'Send';
    }
    if (chatText) chatText.disabled = false;
    // Toggle logo pulse on the app shell
    const appEl = document.querySelector('.app');
    if (appEl) appEl.classList.toggle('is-agent-working', processing);
    if (!processing) {
        setRunBadge(ws && ws.readyState === WebSocket.OPEN ? 'idle' : 'offline', ws && ws.readyState === WebSocket.OPEN ? '' : 'Offline');
    }
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

const chatModelSelect = document.getElementById('chat-model-select');
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

        await hydrateChatModelCapabilities(data);

        // Clear existing options
        while (chatModelSelect.firstChild) {
            chatModelSelect.removeChild(chatModelSelect.firstChild);
        }

        // Add current model as first option (selected)
        const currentOpt = document.createElement('option');
        currentOpt.value = currentModel;
        const modelDisplay = currentModel.split('/').pop() || currentModel;
        currentOpt.textContent = formatModelOptionLabel(modelDisplay, currentModel);
        currentOpt.selected = true;
        chatModelSelect.appendChild(currentOpt);

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
                option.textContent = formatModelOptionLabel(m.label, m.value);
                optgroup.appendChild(option);
            });

            chatModelSelect.appendChild(optgroup);
        });

        renderActiveModelCapabilities(currentModel);

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
    closeMcpPicker();
    if (!chatPlusMenu) return;
    chatPlusMenu.hidden = !chatPlusMenu.hidden;
});

document.addEventListener('click', () => {
    closeChatPlusMenu();
    closeMcpPicker();
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
                thinkingEnabled = null;
                if (chatConfig) {
                    chatConfig.dataset.model = newModel;
                }
                showToast('Model switched to ' + newModel.split('/').pop(), 'success');

                // Update display
                const opt = chatModelSelect.options[0];
                opt.value = newModel;
                opt.textContent = formatModelOptionLabel(newModel.split('/').pop() || newModel, newModel);
                opt.selected = true;
                renderActiveModelCapabilities(newModel);
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
    } else {
        statusEl.textContent = progress.current_step
            ? `Step ${completed}/${total} done \u2014 running: ${progress.current_step}`
            : `Step ${completed}/${total}`;
        card.dataset.status = 'running';
    }

    scrollThreadToBottom();
}

async function bootstrapChat() {
    try {
        showArchived = window.localStorage.getItem('homun.chat.showArchived') === '1';
        sidebarCollapsed = window.localStorage.getItem('homun.chat.sidebarCollapsed') === '1';
        applySidebarState();
        await ensureConversationSelected();
        connect();
    } catch (e) {
        console.error('Failed to bootstrap chat:', e);
        showToast('Failed to load conversations', 'error');
    }
}

bootstrapChat();
