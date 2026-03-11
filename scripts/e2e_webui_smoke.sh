#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=/dev/null
source "$ROOT_DIR/scripts/lib/playwright_e2e.sh"

CHAT_PROMPT="${HOMUN_E2E_CHAT_PROMPT:-}"
CHAT_WAIT_MS="${HOMUN_E2E_CHAT_WAIT_MS:-30000}"
ARTIFACT_PREFIX="${HOMUN_E2E_ARTIFACT_DIR}/webui-chat-smoke"

require_playwright_cli

open_relative "/login"
setup_or_login_if_needed

log_step "Open chat UI"
open_relative "/chat"
wait_for_selector "#chat-form"
wait_for_selector "#chat-text"
wait_for_selector "#btn-send"
wait_for_selector "#chat-conversation-list"
wait_for_function "() => { const el = document.getElementById('ws-status'); return !!el && !/Connecting/i.test(el.textContent || ''); }" 20000

if [[ -n "$CHAT_PROMPT" ]]; then
    log_step "Send a smoke prompt through the chat composer"
    fill_selector "#chat-text" "$CHAT_PROMPT"
    click_selector "#btn-send"
    wait_until_eval_true "document.querySelectorAll('#messages .chat-msg.user').length >= 1" "$CHAT_WAIT_MS"
    wait_until_eval_true "document.querySelectorAll('#messages .chat-msg.assistant').length >= 1" "$CHAT_WAIT_MS"
fi

save_snapshot "${ARTIFACT_PREFIX}.snapshot.txt"
save_screenshot "${ARTIFACT_PREFIX}.png"

log_step "Web UI chat smoke passed"
echo "Artifacts:"
echo "  ${ARTIFACT_PREFIX}.snapshot.txt"
echo "  ${ARTIFACT_PREFIX}.png"
