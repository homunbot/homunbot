#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IMAGE_TAG="${1:-homun/runtime-core:2026.03}"
DOCKERFILE_PATH="$ROOT_DIR/docker/sandbox-runtime/Dockerfile"

if ! command -v docker >/dev/null 2>&1; then
    echo "docker is required to build the sandbox runtime image" >&2
    exit 1
fi

echo "[sandbox-runtime] building $IMAGE_TAG from $DOCKERFILE_PATH"
docker build -t "$IMAGE_TAG" -f "$DOCKERFILE_PATH" "$ROOT_DIR"
echo "[sandbox-runtime] build completed: $IMAGE_TAG"
