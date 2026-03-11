#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=/dev/null
source "$ROOT_DIR/scripts/lib/playwright_e2e.sh"

ARTIFACT_PREFIX="${HOMUN_E2E_ARTIFACT_DIR}/chat-mcp-picker"

require_playwright_cli

open_relative "/login"
setup_or_login_if_needed
open_relative "/chat"
wait_for_selector "#chat-form"
wait_for_selector "#btn-chat-plus"

log_step "Open MCP picker"
click_selector "#btn-chat-plus"
wait_for_function "() => { const el = document.getElementById('btn-chat-open-mcp'); return !!el && !el.hidden && el.offsetParent !== null; }" 15000
click_selector "#btn-chat-open-mcp"
wait_for_function "() => { const picker = document.getElementById('chat-mcp-picker'); return !!picker && !picker.hidden; }" 15000
wait_for_function "() => document.querySelector('.chat-mcp-empty') || document.querySelector('.chat-mcp-option')" 15000

RESULT="empty"
if eval_js "Boolean(document.querySelector('.chat-mcp-option'))" | grep -qi "true"; then
    log_step "Select first MCP server from picker"
    run_code "await page.locator('.chat-mcp-option').first().click()"
    wait_for_function "() => { const strip = document.getElementById('chat-attachment-strip'); return !!strip && !strip.hidden && !!strip.querySelector('.chat-context-chip'); }" 10000
    RESULT="selected"
fi

save_snapshot "${ARTIFACT_PREFIX}.snapshot.txt"
save_screenshot "${ARTIFACT_PREFIX}.png"

log_step "Chat MCP picker smoke passed"
echo "Picker result: $RESULT"
echo "Artifacts:"
echo "  ${ARTIFACT_PREFIX}.snapshot.txt"
echo "  ${ARTIFACT_PREFIX}.png"
