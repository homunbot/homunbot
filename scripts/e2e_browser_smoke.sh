#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=/dev/null
source "$ROOT_DIR/scripts/lib/playwright_e2e.sh"

ARTIFACT_PREFIX="${HOMUN_E2E_ARTIFACT_DIR}/browser-smoke"

require_playwright_cli

open_relative "/login"
setup_or_login_if_needed

log_step "Open browser settings page"
open_relative "/browser"
wait_for_selector "#browser-form"
wait_for_selector "#btn-test-browser"

log_step "Run browser prerequisite test from the UI"
click_selector "#btn-test-browser"
wait_for_function "() => { const el = document.getElementById('browser-result'); return !!el && (el.textContent || '').trim().length > 0; }" 20000

RESULT_TEXT="$(pw_capture eval "document.getElementById('browser-result') && document.getElementById('browser-result').textContent")"
save_snapshot "${ARTIFACT_PREFIX}.snapshot.txt"
save_screenshot "${ARTIFACT_PREFIX}.png"

if ! printf '%s' "$RESULT_TEXT" | grep -qi "Browser prerequisites OK"; then
    echo "Browser smoke failed. UI result:" >&2
    echo "$RESULT_TEXT" >&2
    exit 1
fi

log_step "Browser smoke passed"
echo "Artifacts:"
echo "  ${ARTIFACT_PREFIX}.snapshot.txt"
echo "  ${ARTIFACT_PREFIX}.png"
