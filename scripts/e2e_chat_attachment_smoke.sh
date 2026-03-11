#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=/dev/null
source "$ROOT_DIR/scripts/lib/playwright_e2e.sh"

ARTIFACT_PREFIX="${HOMUN_E2E_ARTIFACT_DIR}/chat-attachment"
CHAT_WAIT_MS="${HOMUN_E2E_CHAT_WAIT_MS:-30000}"
mkdir -p "$ROOT_DIR/output"
TMP_DIR="$(mktemp -d "$ROOT_DIR/output/e2e-attachment.XXXXXX")"
trap 'rm -rf "$TMP_DIR"' EXIT

DOC_PATH="$TMP_DIR/attachment-smoke.md"
DOC_NAME="$(basename "$DOC_PATH")"
cat > "$DOC_PATH" <<'EOF'
# Attachment Smoke

This is a deterministic E2E attachment fixture for Homun chat.
EOF

require_playwright_cli

open_relative "/login"
setup_or_login_if_needed
open_relative "/chat"
wait_for_selector "#chat-form"
wait_for_function "() => !!document.getElementById('chat-doc-input')" 15000
wait_for_selector "#btn-chat-plus"

log_step "Upload a document attachment"
set_input_files "#chat-doc-input" "$DOC_PATH"
wait_for_function "() => { const strip = document.getElementById('chat-attachment-strip'); return !!strip && !strip.hidden && strip.textContent.includes($(json_quote "$DOC_NAME")); }" 15000

log_step "Send message with attached document"
fill_selector "#chat-text" "Attachment smoke marker. Acknowledge the uploaded file briefly."
click_selector "#btn-send"
wait_until_eval_true "document.querySelectorAll('#messages .chat-msg.user').length >= 1" "$CHAT_WAIT_MS"
wait_until_eval_true "Boolean(document.querySelector('#messages') && document.querySelector('#messages').textContent.includes($(json_quote "$DOC_NAME")))" "$CHAT_WAIT_MS"
wait_until_eval_true "document.querySelectorAll('#messages .chat-msg.assistant').length >= 1" "$CHAT_WAIT_MS"

save_snapshot "${ARTIFACT_PREFIX}.snapshot.txt"
save_screenshot "${ARTIFACT_PREFIX}.png"

log_step "Chat attachment smoke passed"
echo "Uploaded file: $DOC_PATH"
echo "Artifacts:"
echo "  ${ARTIFACT_PREFIX}.snapshot.txt"
echo "  ${ARTIFACT_PREFIX}.png"
