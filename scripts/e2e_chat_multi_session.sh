#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=/dev/null
source "$ROOT_DIR/scripts/lib/playwright_e2e.sh"

PROMPT_ONE="${HOMUN_E2E_CHAT_MULTI_PROMPT_ONE:-Session smoke marker one. Reply briefly.}"
PROMPT_TWO="${HOMUN_E2E_CHAT_MULTI_PROMPT_TWO:-Session smoke marker two. Reply briefly.}"
ARTIFACT_PREFIX="${HOMUN_E2E_ARTIFACT_DIR}/chat-multi-session"
CHAT_WAIT_MS="${HOMUN_E2E_CHAT_WAIT_MS:-30000}"

require_playwright_cli

open_relative "/login"
setup_or_login_if_needed
open_relative "/chat"
wait_for_selector "#chat-form"
wait_for_selector "#btn-new-chat"
wait_for_function "() => { const el = document.getElementById('ws-status'); return !!el && !/Connecting/i.test(el.textContent || ''); }" 20000

INITIAL_CONV="$(current_conversation_id)"

log_step "Create first fresh conversation"
click_selector "#btn-new-chat"
wait_for_function "() => document.querySelectorAll('#messages .chat-msg').length === 0" 10000
if [[ -n "$INITIAL_CONV" ]]; then
    wait_for_function "() => (new URLSearchParams(window.location.search).get('c') || localStorage.getItem('homun.chat.currentConversation') || '') !== $(json_quote "$INITIAL_CONV")" 10000
fi
CONV1="$(current_conversation_id)"

log_step "Send message in first conversation"
fill_selector "#chat-text" "$PROMPT_ONE"
click_selector "#btn-send"
wait_until_eval_true "document.querySelectorAll('#messages .chat-msg.assistant').length >= 1" "$CHAT_WAIT_MS"

log_step "Create second conversation"
click_selector "#btn-new-chat"
wait_for_function "() => document.querySelectorAll('#messages .chat-msg').length === 0" 10000
wait_for_function "() => (new URLSearchParams(window.location.search).get('c') || localStorage.getItem('homun.chat.currentConversation') || '') !== $(json_quote "$CONV1")" 10000
CONV2="$(current_conversation_id)"

if [[ -z "$CONV1" || -z "$CONV2" || "$CONV1" == "$CONV2" ]]; then
    echo "Failed to obtain two distinct conversation ids: conv1=$CONV1 conv2=$CONV2" >&2
    exit 1
fi

log_step "Send message in second conversation"
fill_selector "#chat-text" "$PROMPT_TWO"
click_selector "#btn-send"
wait_until_eval_true "document.querySelectorAll('#messages .chat-msg.assistant').length >= 1" "$CHAT_WAIT_MS"

log_step "Re-open first conversation and verify isolated history"
open_relative "/chat?c=${CONV1}"
wait_for_selector "#messages"
wait_until_eval_true "Boolean(document.querySelector('#messages') && document.querySelector('#messages').textContent.includes($(json_quote "$PROMPT_ONE")))" 15000
if [[ "$(eval_js_value "Boolean(document.querySelector('#messages') && document.querySelector('#messages').textContent.includes($(json_quote "$PROMPT_TWO")))")" == "true" ]]; then
    echo "Second prompt leaked into first conversation history" >&2
    exit 1
fi

log_step "Re-open second conversation and verify isolated history"
open_relative "/chat?c=${CONV2}"
wait_for_selector "#messages"
wait_until_eval_true "Boolean(document.querySelector('#messages') && document.querySelector('#messages').textContent.includes($(json_quote "$PROMPT_TWO")))" 15000
if [[ "$(eval_js_value "Boolean(document.querySelector('#messages') && document.querySelector('#messages').textContent.includes($(json_quote "$PROMPT_ONE")))")" == "true" ]]; then
    echo "First prompt leaked into second conversation history" >&2
    exit 1
fi

save_snapshot "${ARTIFACT_PREFIX}.snapshot.txt"
save_screenshot "${ARTIFACT_PREFIX}.png"

log_step "Chat multi-session smoke passed"
echo "Conversation ids:"
echo "  first:  ${CONV1}"
echo "  second: ${CONV2}"
echo "Artifacts:"
echo "  ${ARTIFACT_PREFIX}.snapshot.txt"
echo "  ${ARTIFACT_PREFIX}.png"
