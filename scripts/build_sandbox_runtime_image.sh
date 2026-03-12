#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IMAGE_TAG="${1:-homun/runtime-core:2026.03}"
DOCKERFILE_PATH="$ROOT_DIR/docker/sandbox-runtime/Dockerfile"
PUSH="${2:-}"

if ! command -v docker >/dev/null 2>&1; then
    echo "docker is required to build the sandbox runtime image" >&2
    exit 1
fi

if [ "$PUSH" = "--push" ]; then
    echo "[sandbox-runtime] building + pushing $IMAGE_TAG (linux/amd64,linux/arm64)"
    docker buildx build \
        --platform linux/amd64,linux/arm64 \
        -t "$IMAGE_TAG" \
        -f "$DOCKERFILE_PATH" \
        --push \
        "$ROOT_DIR"
    echo "[sandbox-runtime] pushed: $IMAGE_TAG"
else
    echo "[sandbox-runtime] building $IMAGE_TAG (local only)"
    docker build -t "$IMAGE_TAG" -f "$DOCKERFILE_PATH" "$ROOT_DIR"
    echo "[sandbox-runtime] build completed: $IMAGE_TAG"
    echo "[sandbox-runtime] to push: $0 $IMAGE_TAG --push"
fi
