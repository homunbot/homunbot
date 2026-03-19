#!/usr/bin/env bash
# CI E2E suite — runs UI-structural tests that don't require an LLM provider.
# Validates: login/setup flow, page rendering, selector presence, WebSocket connection.
# Used by .github/workflows/e2e-ci.yml on every push/PR touching web UI files.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# No chat prompt → webui smoke just checks selectors without sending a message
export HOMUN_E2E_CHAT_PROMPT=""

tests=(
    "$ROOT_DIR/scripts/e2e_webui_smoke.sh"
)

for test_script in "${tests[@]}"; do
    printf '\n[ci-suite] Running %s\n' "$(basename "$test_script")"
    "$test_script"
done

printf '\n[ci-suite] All CI E2E tests passed\n'
printf '[ci-suite] Artifacts saved in output/playwright/\n'
