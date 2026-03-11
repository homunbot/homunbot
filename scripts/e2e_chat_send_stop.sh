#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=/dev/null
source "$ROOT_DIR/scripts/lib/playwright_e2e.sh"

CHAT_PROMPT="${HOMUN_E2E_CHAT_PROMPT:-Produce exactly 120 numbered lines. Keep writing until all 120 are complete. Do not summarize early.}"
ARTIFACT_PREFIX="${HOMUN_E2E_ARTIFACT_DIR}/chat-send-stop"
CHAT_RUN_WAIT_MS="${HOMUN_E2E_CHAT_RUN_WAIT_MS:-20000}"
CHAT_STOP_WAIT_MS="${HOMUN_E2E_CHAT_STOP_WAIT_MS:-20000}"
RUN_FETCH_EXPR="fetch('/api/v1/chat/run?conversation_id=' + encodeURIComponent(new URLSearchParams(window.location.search).get('c') || localStorage.getItem('homun.chat.currentConversation') || 'default'), { credentials: 'same-origin' }).then(r => r.json())"

require_playwright_cli

open_relative "/login"
setup_or_login_if_needed

log_step "Open chat UI"
open_relative "/chat"
wait_for_selector "#chat-form"
wait_for_selector "#chat-text"
wait_for_selector "#btn-send"
wait_for_function "() => { const el = document.getElementById('ws-status'); return !!el && !/Connecting/i.test(el.textContent || ''); }" 20000

log_step "Send chat prompt"
fill_selector "#chat-text" "$CHAT_PROMPT"
click_selector "#btn-send"
wait_until_eval_true "document.querySelectorAll('#messages .chat-msg.user').length >= 1" "$CHAT_RUN_WAIT_MS"
wait_until_eval_true "Boolean(document.querySelector('#btn-send.is-processing'))" "$CHAT_RUN_WAIT_MS"
wait_until_eval_async_true "const response = await fetch('/api/v1/chat/run?conversation_id=' + encodeURIComponent(new URLSearchParams(window.location.search).get('c') || localStorage.getItem('homun.chat.currentConversation') || 'default'), { credentials: 'same-origin' }); const run = await response.json(); return !!(run && run.run_id)" "$CHAT_RUN_WAIT_MS"

log_step "Request stop from the UI"
click_selector "#btn-send"
wait_until_eval_async_true "const messages = document.getElementById('messages'); if (messages && messages.textContent.includes('Stopped by user.')) return true; const response = await fetch('/api/v1/chat/run?conversation_id=' + encodeURIComponent(new URLSearchParams(window.location.search).get('c') || localStorage.getItem('homun.chat.currentConversation') || 'default'), { credentials: 'same-origin' }); const run = await response.json(); return !run || run.status === 'stopping' || run.status === 'interrupted' || run.status === 'completed' || run.status === 'failed'" "$CHAT_STOP_WAIT_MS"

RUN_STATE="$(eval_js_async "const response = await fetch('/api/v1/chat/run?conversation_id=' + encodeURIComponent(new URLSearchParams(window.location.search).get('c') || localStorage.getItem('homun.chat.currentConversation') || 'default'), { credentials: 'same-origin' }); const run = await response.json(); return run ? run.status : 'none'")"
save_snapshot "${ARTIFACT_PREFIX}.snapshot.txt"
save_screenshot "${ARTIFACT_PREFIX}.png"

case "$RUN_STATE" in
    *stopping*|*interrupted*|*completed*|*failed*|*none*)
        ;;
    *)
        echo "Unexpected run state after stop: $RUN_STATE" >&2
        exit 1
        ;;
esac

log_step "Chat send/stop smoke passed"
echo "Final run state: $RUN_STATE"
echo "Artifacts:"
echo "  ${ARTIFACT_PREFIX}.snapshot.txt"
echo "  ${ARTIFACT_PREFIX}.png"
