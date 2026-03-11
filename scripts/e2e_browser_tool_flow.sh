#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=/dev/null
source "$ROOT_DIR/scripts/lib/playwright_e2e.sh"

ARTIFACT_PREFIX="${HOMUN_E2E_ARTIFACT_DIR}/browser-tool-flow"
CHAT_WAIT_MS="${HOMUN_E2E_CHAT_WAIT_MS:-90000}"
FIXTURE_PATH="$ROOT_DIR/scripts/fixtures/browser_tool_flow.html"
EXPECTED_TOKEN="${HOMUN_E2E_BROWSER_FLOW_TOKEN:-browser-smoke-token-42}"

if [[ ! -f "$FIXTURE_PATH" ]]; then
    echo "Missing browser fixture: $FIXTURE_PATH" >&2
    exit 1
fi

FIXTURE_URL="$(node -e '
const fs = require("fs");
const path = process.argv[1];
const html = fs.readFileSync(path, "utf8");
process.stdout.write("data:text/html;charset=utf-8," + encodeURIComponent(html));
' "$FIXTURE_PATH")"

CHAT_PROMPT="$(cat <<EOF
Use the browser tool only and answer with the final token only.
Open this exact URL:
$FIXTURE_URL

Type the required challenge code into the input, click "Reveal answer", then read the final token.
Do not use web_search or web_fetch.
EOF
)"

require_playwright_cli

open_relative "/login"
setup_or_login_if_needed

log_step "Open chat UI for browser tool flow"
open_relative "/chat"
wait_for_selector "#chat-form"
wait_for_selector "#chat-text"
wait_for_selector "#btn-send"
wait_for_selector "#btn-new-chat"
wait_for_function "() => { const el = document.getElementById('ws-status'); return !!el && !/Connecting/i.test(el.textContent || ''); }" 20000

log_step "Start a fresh conversation"
click_selector "#btn-new-chat"
wait_for_function "() => document.querySelectorAll('#messages .chat-msg').length === 0" 10000

log_step "Ask the agent to complete a deterministic browser flow"
fill_selector "#chat-text" "$CHAT_PROMPT"
click_selector "#btn-send"
wait_until_eval_true "document.querySelectorAll('#messages .chat-msg.user').length >= 1" 15000
wait_until_eval_true "Boolean(document.querySelector('.chat-tool-call[data-tool-name=\"browser\"]'))" "$CHAT_WAIT_MS"
wait_until_eval_true "Boolean(document.getElementById('messages') && document.getElementById('messages').textContent.includes($(json_quote "$EXPECTED_TOKEN")))" "$CHAT_WAIT_MS"

ASSISTANT_TEXT="$(eval_js_value "document.getElementById('messages')?.textContent || ''")"
TOOL_LABELS="$(eval_js_value "document.querySelector('.chat-tool-call[data-tool-name=\"browser\"] .chat-tool-call-name')?.textContent || ''")"

save_snapshot "${ARTIFACT_PREFIX}.snapshot.txt"
save_screenshot "${ARTIFACT_PREFIX}.png"

if ! printf '%s' "$ASSISTANT_TEXT" | grep -q "$EXPECTED_TOKEN"; then
    echo "Browser tool flow failed: expected token not found in assistant output" >&2
    echo "$ASSISTANT_TEXT" >&2
    exit 1
fi

if ! printf '%s' "$TOOL_LABELS" | grep -Eqi 'Opened a page|Typed into the page|Read the page|Used the browser'; then
    echo "Browser tool flow failed: browser tool activity not visible in chat UI" >&2
    echo "$TOOL_LABELS" >&2
    exit 1
fi

log_step "Browser tool flow smoke passed"
echo "Observed browser tool labels: $TOOL_LABELS"
echo "Artifacts:"
echo "  ${ARTIFACT_PREFIX}.snapshot.txt"
echo "  ${ARTIFACT_PREFIX}.png"
