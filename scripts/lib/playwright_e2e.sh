#!/usr/bin/env bash

set -euo pipefail

PLAYWRIGHT_E2E_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PLAYWRIGHT_E2E_ROOT_DIR="$(cd "$PLAYWRIGHT_E2E_LIB_DIR/../.." && pwd)"

if [[ -f "$PLAYWRIGHT_E2E_ROOT_DIR/.env" ]]; then
    set -a
    # shellcheck disable=SC1091
    source "$PLAYWRIGHT_E2E_ROOT_DIR/.env"
    set +a
fi

CODEX_HOME="${CODEX_HOME:-$HOME/.codex}"
DEFAULT_PWCLI="$CODEX_HOME/skills/playwright/scripts/playwright_cli.sh"
if [[ -z "${PWCLI:-}" ]]; then
    if [[ -x "$DEFAULT_PWCLI" ]]; then
        PWCLI="$DEFAULT_PWCLI"
    else
        PWCLI="$PLAYWRIGHT_E2E_ROOT_DIR/scripts/playwright_cli.sh"
    fi
fi
HOMUN_E2E_BASE_URL="${HOMUN_E2E_BASE_URL:-https://ui.homun.bot}"
HOMUN_E2E_SESSION="${HOMUN_E2E_SESSION:-homun-e2e-$$}"
HOMUN_E2E_ARTIFACT_DIR="${HOMUN_E2E_ARTIFACT_DIR:-output/playwright}"
HOMUN_E2E_WAIT_MS="${HOMUN_E2E_WAIT_MS:-15000}"
HOMUN_E2E_CONVERSATION_EXPR="new URLSearchParams(window.location.search).get('c') || localStorage.getItem('homun.chat.currentConversation') || 'default'"
HOMUN_E2E_BROWSER_OPENED="${HOMUN_E2E_BROWSER_OPENED:-0}"
HOMUN_E2E_VERBOSE="${HOMUN_E2E_VERBOSE:-0}"
HOMUN_E2E_SCRIPT_NAME="${HOMUN_E2E_SCRIPT_NAME:-$(basename "$0" .sh)}"
HOMUN_E2E_LOG_FILE="${HOMUN_E2E_LOG_FILE:-$HOMUN_E2E_ARTIFACT_DIR/${HOMUN_E2E_SCRIPT_NAME}.cli.log}"

require_playwright_cli() {
    if ! command -v npx >/dev/null 2>&1; then
        echo "npx is required to drive Playwright MCP/CLI." >&2
        echo "Install Node.js/npm first." >&2
        exit 1
    fi

    if [[ ! -x "$PWCLI" ]]; then
        echo "Playwright wrapper not found: $PWCLI" >&2
        echo "Expected Codex skill wrapper from ~/.codex/skills/playwright." >&2
        exit 1
    fi

    mkdir -p "$HOMUN_E2E_ARTIFACT_DIR"
    : > "$HOMUN_E2E_LOG_FILE"
}

log_step() {
    printf '\n[%s] %s\n' "e2e" "$*"
}

json_quote() {
    node -e 'process.stdout.write(JSON.stringify(process.argv[1]))' "$1"
}

extract_cli_result() {
    awk 'f{ print; exit } /^### Result$/{ f=1 }'
}

decode_cli_result() {
    node -e 'const value = process.argv[1] ?? ""; try { const parsed = JSON.parse(value); process.stdout.write(typeof parsed === "string" ? parsed : JSON.stringify(parsed)); } catch { process.stdout.write(value); }' "$1"
}

now_ms() {
    node -e 'process.stdout.write(String(Date.now()))'
}

sanitize_cli_text() {
    node -e '
const fs = require("fs");
let text = fs.readFileSync(0, "utf8");
const replacements = [
  [process.env.HOMUN_E2E_PASSWORD, "[REDACTED_PASSWORD]"],
  [process.env.HOMUN_E2E_USERNAME, "[REDACTED_USERNAME]"],
];
for (const [from, to] of replacements) {
  if (from) text = text.split(from).join(to);
}
process.stdout.write(text);
'
}

format_pw_command() {
    local command="${1:-}"
    local binary
    binary="$(basename "$PWCLI")"
    case "$command" in
        open|goto)
            printf '%s --session %s %s %s' "$binary" "$HOMUN_E2E_SESSION" "$command" "${2:-}"
            ;;
        "")
            printf '%s --session %s' "$binary" "$HOMUN_E2E_SESSION"
            ;;
        *)
            printf '%s --session %s %s [args omitted]' "$binary" "$HOMUN_E2E_SESSION" "$command"
            ;;
    esac
}

append_cli_log() {
    local command="$1"
    local status="$2"
    local output="$3"
    {
        printf '\n$ %s\n' "$command"
        printf '[exit=%s]\n' "$status"
        if [[ -n "$output" ]]; then
            printf '%s\n' "$output"
        fi
    } >> "$HOMUN_E2E_LOG_FILE"
}

_pw_invoke() {
    local output status command sanitized_command sanitized_output
    set +e
    output="$("$PWCLI" --session "$HOMUN_E2E_SESSION" "$@" 2>&1)"
    status=$?
    set -e

    command="$(format_pw_command "$@")"
    sanitized_command="$(printf '%s' "$command" | sanitize_cli_text)"
    sanitized_output="$(printf '%s' "$output" | sanitize_cli_text)"
    if [[ $status -eq 0 ]] && printf '%s\n' "$output" | grep -q '^### Error$'; then
        status=1
    fi
    append_cli_log "$sanitized_command" "$status" "$sanitized_output"

    if [[ $status -ne 0 ]]; then
        if [[ -n "$sanitized_output" ]]; then
            printf '%s\n' "$sanitized_output" >&2
        fi
        return "$status"
    fi

    printf '%s' "$sanitized_output"
}

pw() {
    local output
    output="$(_pw_invoke "$@")"
    if [[ "$HOMUN_E2E_VERBOSE" == "1" && -n "$output" ]]; then
        printf '%s\n' "$output"
    fi
}

pw_capture() {
    _pw_invoke "$@"
}

open_page() {
    local url="${1:-$HOMUN_E2E_BASE_URL}"
    log_step "Open $url"
    if [[ "$HOMUN_E2E_BROWSER_OPENED" == "1" ]]; then
        pw goto "$url"
    else
        pw open "$url"
        HOMUN_E2E_BROWSER_OPENED=1
    fi
}

run_code() {
    local body="$1"
    pw run-code "async (page) => { ${body}; }"
}

run_code_capture() {
    local body="$1"
    pw_capture run-code "async (page) => { ${body}; }"
}

eval_js() {
    pw_capture eval "$1"
}

eval_js_value() {
    local expression="$1"
    local output raw
    output="$(eval_js "$expression")"
    raw="$(printf '%s\n' "$output" | extract_cli_result)"
    if [[ -z "$raw" ]]; then
        printf '%s\n' "$output" >&2
        return 1
    fi
    decode_cli_result "$raw"
}

eval_js_async() {
    local body="$1"
    local output raw
    output="$(run_code_capture "return await page.evaluate(async () => { ${body}; })")"
    raw="$(printf '%s\n' "$output" | extract_cli_result)"
    if [[ -z "$raw" ]]; then
        printf '%s\n' "$output" >&2
        return 1
    fi
    decode_cli_result "$raw"
}

current_conversation_id() {
    eval_js_value "$HOMUN_E2E_CONVERSATION_EXPR"
}

wait_for_selector() {
    local selector="$1"
    local timeout_ms="${2:-$HOMUN_E2E_WAIT_MS}"
    local selector_json
    selector_json="$(json_quote "$selector")"
    run_code "await page.waitForSelector(${selector_json}, { timeout: ${timeout_ms} })"
}

wait_for_function() {
    local body="$1"
    local timeout_ms="${2:-$HOMUN_E2E_WAIT_MS}"
    run_code "await page.waitForFunction(${body}, { timeout: ${timeout_ms} })"
}

wait_until_eval_true() {
    local expression="$1"
    local timeout_ms="${2:-$HOMUN_E2E_WAIT_MS}"
    local interval_ms="${3:-500}"
    local start_ms now_ms value
    start_ms="$(now_ms)"
    while true; do
        value="$(eval_js_value "$expression" 2>/dev/null || true)"
        if [[ "$value" == "true" || "$value" == "1" ]]; then
            return 0
        fi
        now_ms="$(now_ms)"
        if (( now_ms - start_ms >= timeout_ms )); then
            echo "Timed out waiting for condition: $expression" >&2
            return 1
        fi
        sleep "$(awk "BEGIN { printf \"%.3f\", ${interval_ms}/1000 }")"
    done
}

wait_until_eval_async_true() {
    local body="$1"
    local timeout_ms="${2:-$HOMUN_E2E_WAIT_MS}"
    local interval_ms="${3:-500}"
    local start_ms now_ms value
    start_ms="$(now_ms)"
    while true; do
        value="$(eval_js_async "$body" 2>/dev/null || true)"
        if [[ "$value" == "true" || "$value" == "1" ]]; then
            return 0
        fi
        now_ms="$(now_ms)"
        if (( now_ms - start_ms >= timeout_ms )); then
            echo "Timed out waiting for async condition" >&2
            return 1
        fi
        sleep "$(awk "BEGIN { printf \"%.3f\", ${interval_ms}/1000 }")"
    done
}

page_has_selector() {
    local selector="$1"
    local selector_json
    selector_json="$(json_quote "$selector")"
    pw_capture eval "Boolean(document.querySelector(${selector_json}))" | grep -qi "true"
}

fill_selector() {
    local selector="$1"
    local value="$2"
    local selector_json value_json
    selector_json="$(json_quote "$selector")"
    value_json="$(json_quote "$value")"
    run_code "await page.locator(${selector_json}).fill(${value_json})"
}

click_selector() {
    local selector="$1"
    local selector_json
    selector_json="$(json_quote "$selector")"
    run_code "await page.locator(${selector_json}).click()"
}

open_relative() {
    local path="$1"
    local url="${HOMUN_E2E_BASE_URL%/}${path}"
    open_page "$url"
}

reload_page() {
    log_step "Reload current page"
    pw reload
}

wait_for_path_not() {
    local path="$1"
    local timeout_ms="${2:-$HOMUN_E2E_WAIT_MS}"
    local path_json
    path_json="$(json_quote "$path")"
    wait_for_function "() => window.location.pathname !== ${path_json}" "$timeout_ms"
}

save_screenshot() {
    local path="$1"
    local path_json
    mkdir -p "$(dirname "$path")"
    path_json="$(json_quote "$path")"
    run_code "await page.screenshot({ path: ${path_json}, fullPage: true })"
}

save_snapshot() {
    local path="$1"
    mkdir -p "$(dirname "$path")"
    pw_capture snapshot > "$path"
}

set_input_files() {
    local selector="$1"
    local path="$2"
    local selector_json path_json
    selector_json="$(json_quote "$selector")"
    path_json="$(json_quote "$path")"
    run_code "await page.locator(${selector_json}).setInputFiles(${path_json})"
}

setup_or_login_if_needed() {
    local username="${HOMUN_E2E_USERNAME:-}"
    local password="${HOMUN_E2E_PASSWORD:-}"

    if page_has_selector '#setup-form'; then
        if [[ -z "$username" || -z "$password" ]]; then
            echo "Instance is in setup mode. Set HOMUN_E2E_USERNAME and HOMUN_E2E_PASSWORD." >&2
            exit 2
        fi
        log_step "Create admin account through setup wizard"
        fill_selector '#username' "$username"
        fill_selector '#password' "$password"
        fill_selector '#confirm' "$password"
        click_selector '#setup-btn'
        wait_for_path_not '/setup-wizard' 20000
    fi

    if page_has_selector '#login-form'; then
        if [[ -z "$username" || -z "$password" ]]; then
            echo "Instance requires login. Set HOMUN_E2E_USERNAME and HOMUN_E2E_PASSWORD." >&2
            exit 2
        fi
        log_step "Authenticate through login page"
        fill_selector '#username' "$username"
        fill_selector '#password' "$password"
        click_selector '#login-btn'
        wait_for_path_not '/login' 20000
    fi
}
