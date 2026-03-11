#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

: "${HOMUN_E2E_CHAT_WAIT_MS:=60000}"
: "${HOMUN_E2E_CHAT_RUN_WAIT_MS:=30000}"
: "${HOMUN_E2E_CHAT_STOP_WAIT_MS:=30000}"
: "${HOMUN_E2E_CHAT_RESTORE_WAIT_MS:=15000}"

export HOMUN_E2E_CHAT_WAIT_MS
export HOMUN_E2E_CHAT_RUN_WAIT_MS
export HOMUN_E2E_CHAT_STOP_WAIT_MS
export HOMUN_E2E_CHAT_RESTORE_WAIT_MS

tests=(
    "$ROOT_DIR/scripts/e2e_webui_smoke.sh"
    "$ROOT_DIR/scripts/e2e_chat_send_stop.sh"
    "$ROOT_DIR/scripts/e2e_chat_multi_session.sh"
    "$ROOT_DIR/scripts/e2e_chat_restore_run.sh"
    "$ROOT_DIR/scripts/e2e_chat_attachment_smoke.sh"
    "$ROOT_DIR/scripts/e2e_chat_mcp_picker_smoke.sh"
    "$ROOT_DIR/scripts/e2e_browser_tool_flow.sh"
    "$ROOT_DIR/scripts/e2e_browser_smoke.sh"
)

for test_script in "${tests[@]}"; do
    printf '\n[suite] Running %s\n' "$(basename "$test_script")"
    "$test_script"
done

printf '\n[suite] All E2E smoke scripts completed successfully\n'
printf '[suite] CLI logs saved in output/playwright/*.cli.log\n'
