#!/usr/bin/env bash

set -euo pipefail

npx --yes --package @playwright/cli playwright-cli "$@"
