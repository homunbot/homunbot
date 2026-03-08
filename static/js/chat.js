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
const conversationSearchEl = document.getElementById('chat-conversation-search');
const btnChatSidebar = document.getElementById('btn-chat-sidebar');
const btnChatSidebarMenu = document.getElementById('btn-chat-sidebar-menu');
const chatSidebarMenu = document.getElementById('chat-sidebar-menu');
const btnChatToggleArchived = document.getElementById('btn-chat-toggle-archived');
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
let sidebarMenuOpen = false;
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

function formatConversationTimestamp(value) {
    if (!value) return '';
    const parsed = new Date(value);
    if (Number.isNaN(parsed.getTime())) return '';
    return parsed.toLocaleString([], {
        month: 'short',
        day: 'numeric',
        hour: '2-digit',
        minute: '2-digit',
    });
}

function syncConversationHeader() {
    if (!conversationTitleEl) return;
    conversationTitleEl.textContent = currentConversationTitle();
}

function applySidebarState() {
    if (!chatShellEl) return;
    chatShellEl.classList.toggle('is-sidebar-collapsed', sidebarCollapsed);
}

function syncSidebarMenuLabel() {
    if (!btnChatToggleArchived) return;
    btnChatToggleArchived.textContent = showArchived ? 'Hide archived' : 'Show archived';
}

function closeSidebarMenu() {
    sidebarMenuOpen = false;
    if (chatSidebarMenu) {
        chatSidebarMenu.hidden = true;
    }
}

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
        conversationListEl.innerHTML = '<div class="chat-conversation-empty">No conversations yet.</div>';
        return;
    }

    conversationListEl.innerHTML = '';
    conversations.forEach((conversation) => {
        const item = document.createElement('div');
        item.className = 'chat-conversation-item';
        if (conversation.conversation_id === currentConversationId) {
            item.classList.add('is-active');
        }
        if (conversation.active_run && (conversation.active_run.status === 'running' || conversation.active_run.status === 'stopping')) {
            item.classList.add('is-running');
        }
        const formattedDate = formatConversationTimestamp(conversation.updated_at);

        const titleHtml = renamingConversationId === conversation.conversation_id
            ? `<input type="text" class="input chat-rename-input" value="${escapeHtml(renameDraft || conversation.title || 'New conversation')}" aria-label="Rename conversation">`
            : `<span class="chat-conversation-name">${escapeHtml(conversation.title || 'New conversation')}</span>`;

        item.innerHTML = `
            <button type="button" class="chat-conversation-item-body">
                ${titleHtml}
                <span class="chat-conversation-meta">${escapeHtml(formattedDate || '')}</span>
            </button>
            <button type="button" class="chat-conversation-menu-btn" aria-label="Conversation actions">•••</button>
            <div class="chat-conversation-menu" ${openConversationMenuId === conversation.conversation_id ? '' : 'hidden'}>
                <button type="button" class="chat-conversation-menu-item" data-action="rename">Rename</button>
                <button type="button" class="chat-conversation-menu-item" data-action="${conversation.archived ? 'unarchive' : 'archive'}">${conversation.archived ? 'Unarchive' : 'Archive'}</button>
                <button type="button" class="chat-conversation-menu-item is-danger" data-action="delete">Delete</button>
            </div>
        `;
        item.querySelector('.chat-conversation-item-body')?.addEventListener('click', () => {
            if (renamingConversationId === conversation.conversation_id) {
                return;
            }
            if (conversation.conversation_id !== currentConversationId) {
                selectConversation(conversation.conversation_id);
            }
        });
        item.querySelector('.chat-conversation-menu-btn')?.addEventListener('click', (e) => {
            e.stopPropagation();
            openConversationMenuId = openConversationMenuId === conversation.conversation_id ? null : conversation.conversation_id;
            renderConversationList();
        });
        item.querySelectorAll('.chat-conversation-menu-item').forEach((button) => {
            button.addEventListener('click', async (e) => {
                e.stopPropagation();
                const action = button.dataset.action;
                if (action === 'rename') {
                    await renameConversation(conversation);
                } else if (action === 'archive') {
                    await setConversationArchived(conversation, true);
                } else if (action === 'unarchive') {
                    await setConversationArchived(conversation, false);
                } else if (action === 'delete') {
                    await deleteConversation(conversation);
                }
            });
        });
        const renameInput = item.querySelector('.chat-rename-input');
        if (renameInput) {
            renameInput.addEventListener('click', (e) => e.stopPropagation());
            renameInput.addEventListener('input', () => {
                renameDraft = renameInput.value;
            });
            renameInput.addEventListener('keydown', async (e) => {
                if (e.key === 'Enter') {
                    e.preventDefault();
                    await commitRenameConversation(conversation);
                } else if (e.key === 'Escape') {
                    e.preventDefault();
                    cancelRenameConversation();
                }
            });
            renameInput.addEventListener('blur', async () => {
                await commitRenameConversation(conversation);
            });
            setTimeout(() => {
                renameInput.focus();
                renameInput.select();
            }, 0);
        }
        conversationListEl.appendChild(item);
    });
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

    if (!currentPlanState || !currentPlanState.objective) {
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
        return;
    }

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
}

conversationSearchEl?.addEventListener('input', async () => {
    conversationSearch = conversationSearchEl.value.trim();
    await refreshConversationList();
});

btnChatSidebarMenu?.addEventListener('click', (e) => {
    e.stopPropagation();
    sidebarMenuOpen = !sidebarMenuOpen;
    if (chatSidebarMenu) {
        chatSidebarMenu.hidden = !sidebarMenuOpen;
    }
});

btnChatToggleArchived?.addEventListener('click', async () => {
    showArchived = !showArchived;
    window.localStorage.setItem('homun.chat.showArchived', showArchived ? '1' : '0');
    syncSidebarMenuLabel();
    closeSidebarMenu();
    await refreshConversationList();
    if (!showArchived && currentConversationId && !conversations.some((item) => item.conversation_id === currentConversationId)) {
        await ensureConversationSelectedAfterRemoval(currentConversationId);
    }
});

btnChatSidebar?.addEventListener('click', () => {
    sidebarCollapsed = !sidebarCollapsed;
    window.localStorage.setItem('homun.chat.sidebarCollapsed', sidebarCollapsed ? '1' : '0');
    applySidebarState();
});

document.addEventListener('click', (e) => {
    if (sidebarMenuOpen && !e.target.closest('.chat-sidebar-menu-wrap')) {
        closeSidebarMenu();
    }
    if (openConversationMenuId && !e.target.closest('.chat-conversation-item')) {
        closeConversationMenu();
    }
    if (mcpPickerOpen && !e.target.closest('.chat-plus-wrap')) {
        closeMcpPicker();
    }
});

document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') {
        if (renamingConversationId) {
            cancelRenameConversation();
        }
        if (sidebarMenuOpen) {
            closeSidebarMenu();
        }
        if (mcpPickerOpen) {
            closeMcpPicker();
        }
        if (!chatModalBackdrop?.hidden) {
            closeModal();
        }
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
    ws.send(JSON.stringify({ content: text, attachments, mcp_servers: mcpServers }));
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
async function handleCompactChat() {
    try {
        const res = await fetch(conversationApi('/api/v1/chat/compact'), { method: 'POST' });
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
    };
}

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
    return labels;
}

function formatModelOptionLabel(label, modelId) {
    const labels = modelCapabilityLabels(modelId);
    return labels.length > 0 ? `${label} · ${labels.join(' · ')}` : label;
}

function renderActiveModelCapabilities(modelId) {
    if (!chatModelCapabilitiesEl) return;
    const labels = modelCapabilityLabels(modelId);
    if (labels.length === 0) {
        chatModelCapabilitiesEl.hidden = true;
        chatModelCapabilitiesEl.textContent = '';
        return;
    }
    chatModelCapabilitiesEl.hidden = false;
    chatModelCapabilitiesEl.textContent = '';
    const settingsUrl = `/setup?model=${encodeURIComponent(modelId)}#section-providers`;
    labels.forEach((label) => {
        const link = document.createElement('a');
        link.className = 'chat-model-capability-badge';
        link.href = settingsUrl;
        link.textContent = label;
        link.title = `Open model settings for ${modelId}`;
        chatModelCapabilitiesEl.appendChild(link);
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

async function bootstrapChat() {
    try {
        showArchived = window.localStorage.getItem('homun.chat.showArchived') === '1';
        sidebarCollapsed = window.localStorage.getItem('homun.chat.sidebarCollapsed') === '1';
        applySidebarState();
        syncSidebarMenuLabel();
        await ensureConversationSelected();
        connect();
    } catch (e) {
        console.error('Failed to bootstrap chat:', e);
        showToast('Failed to load conversations', 'error');
    }
}

bootstrapChat();
