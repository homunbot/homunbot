#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=/dev/null
source "$ROOT_DIR/scripts/lib/playwright_e2e.sh"

CHAT_PROMPT="${HOMUN_E2E_CHAT_RESTORE_PROMPT:-Restore-run smoke marker. Produce exactly 150 numbered lines and do not stop early.}"
ARTIFACT_PREFIX="${HOMUN_E2E_ARTIFACT_DIR}/chat-restore-run"
CHAT_RUN_WAIT_MS="${HOMUN_E2E_CHAT_RUN_WAIT_MS:-20000}"
RESTORE_WAIT_MS="${HOMUN_E2E_CHAT_RESTORE_WAIT_MS:-10000}"
RUN_FETCH_EXPR="fetch('/api/v1/chat/run?conversation_id=' + encodeURIComponent(${HOMUN_E2E_CONVERSATION_EXPR}), { credentials: 'same-origin' }).then(r => r.json())"

require_playwright_cli

open_relative "/login"
setup_or_login_if_needed
open_relative "/chat"
wait_for_selector "#chat-form"
wait_for_function "() => { const el = document.getElementById('ws-status'); return !!el && !/Connecting/i.test(el.textContent || ''); }" 20000

log_step "Start a run intended for reload/restore"
fill_selector "#chat-text" "$CHAT_PROMPT"
click_selector "#btn-send"
wait_until_eval_true "Boolean(document.querySelector('#btn-send.is-processing'))" "$CHAT_RUN_WAIT_MS"
wait_until_eval_async_true "const response = await fetch('/api/v1/chat/run?conversation_id=' + encodeURIComponent(${HOMUN_E2E_CONVERSATION_EXPR}), { credentials: 'same-origin' }); const run = await response.json(); return !!(run && run.run_id)" "$CHAT_RUN_WAIT_MS"
RUN_ID="$(eval_js_async "const response = await fetch('/api/v1/chat/run?conversation_id=' + encodeURIComponent(${HOMUN_E2E_CONVERSATION_EXPR}), { credentials: 'same-origin' }); const run = await response.json(); return run && run.run_id ? run.run_id : ''")"

if [[ -z "$RUN_ID" ]]; then
    echo "No active run id found before reload" >&2
    exit 1
fi

reload_page
wait_for_selector "#chat-form"
wait_for_function "() => { const el = document.getElementById('ws-status'); return !!el && !/Connecting/i.test(el.textContent || ''); }" 20000
wait_until_eval_true "Boolean(document.querySelector('#messages') && document.querySelector('#messages').textContent.includes($(json_quote "Restore-run smoke marker")))" "$RESTORE_WAIT_MS"

RESTORE_RESULT="completed_before_restore"
if wait_until_eval_async_true "const response = await fetch('/api/v1/chat/run?conversation_id=' + encodeURIComponent(${HOMUN_E2E_CONVERSATION_EXPR}), { credentials: 'same-origin' }); const run = await response.json(); return !!(run && run.run_id === $(json_quote "$RUN_ID"))" "$RESTORE_WAIT_MS"; then
    RESTORE_RESULT="active_run_restored"
elif wait_until_eval_true "Boolean(document.querySelector('#btn-send.is-processing'))" 5000; then
    RESTORE_RESULT="processing_state_restored"
fi

save_snapshot "${ARTIFACT_PREFIX}.snapshot.txt"
save_screenshot "${ARTIFACT_PREFIX}.png"

log_step "Chat restore-run smoke passed"
echo "Initial run id: $RUN_ID"
echo "Restore result: $RESTORE_RESULT"
echo "Artifacts:"
echo "  ${ARTIFACT_PREFIX}.snapshot.txt"
echo "  ${ARTIFACT_PREFIX}.png"
