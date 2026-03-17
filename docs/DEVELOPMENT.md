# Development Guide

This guide covers building, testing, and releasing Homun.

## Table of Contents

- [Prerequisites](#prerequisites)
- [Building](#building)
- [Feature Flags](#feature-flags)
- [Testing](#testing)
- [CI/CD Pipeline](#cicd-pipeline)
- [Installing Pre-built Binaries](#installing-pre-built-binaries)
- [Creating Releases](#creating-releases)

## Prerequisites

- **Rust 1.85+** (MSRV)
- **SQLite 3** (for database)
- **macOS/Linux/Windows**

Install Rust via [rustup](https://rustup.rs/):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

## Building

### Quick Start

```bash
# Development build (fast, with debug info)
cargo build

# Release build (optimized, smaller binary)
cargo build --release
```

### Binary Size Comparison

| Build Command | Binary Size | Description |
|---------------|-------------|-------------|
| `cargo build --release` | ~7 MB | Default features (CLI + Web UI) |
| `cargo build --release --features gateway` | ~29 MB | All channels + embeddings |
| `cargo build --release --features full` | ~31 MB | Everything including browser |

### Release Profile

The release profile in `Cargo.toml` is optimized for size:

```toml
[profile.release]
opt-level = "z"        # Optimize for size
lto = "fat"            # Maximum cross-crate optimization
codegen-units = 1      # Serialized codegen for smaller binary
strip = true           # Remove debug symbols
panic = "abort"        # Reduce binary size
```

## Feature Flags

Homun uses feature flags to reduce binary size by only including what you need.

### Default Features

```bash
cargo build --release
```

Includes:
- `cli` - Command-line interface with TUI
- `web-ui` - Web dashboard (Axum + WebSocket)
- `file-tools` - Read/Write/Edit file tools
- `shell-tool` - Shell command execution
- `web-tools` - Web search and fetch
- `cron-tool` - Scheduled tasks
- `message-tool` - Proactive messaging
- `vault-2fa` - Two-factor authentication
- `mcp` - Model Context Protocol client

### Optional Features

| Feature | Description | Dependencies Added |
|---------|-------------|-------------------|
| `channel-telegram` | Telegram bot | `teloxide` |
| `channel-discord` | Discord bot | `serenity` |
| `channel-whatsapp` | WhatsApp native client | `wa-rs` crates |
| `embeddings` | Vector search (Ollama/OpenAI + HNSW) | `usearch`, `lru` |
| `browser` | Browser automation | `chromiumoxide` |

### Meta Features

| Feature | Includes |
|---------|----------|
| `gateway` | All channels + embeddings + default tools |
| `full` | Gateway + browser |

### Build Variants

```bash
# Minimal CLI-only build
cargo build --release --no-default-features --features cli

# Web UI only (no CLI TUI)
cargo build --release --no-default-features --features web-ui

# Gateway mode (all messaging channels)
cargo build --release --features gateway

# Everything
cargo build --release --features full

# Custom combination
cargo build --release --features "cli,channel-telegram,embeddings"
```

## Testing

```bash
# Run all tests with all features
cargo test --all-features

# Run specific test
cargo test --test <test_name>

# Run tests with output
cargo test --all-features -- --nocapture
```

## CI/CD Pipeline

The GitHub Actions pipeline (`.github/workflows/ci.yml`) runs on every push and PR.

### Jobs

| Job | Trigger | Description |
|-----|---------|-------------|
| `check` | Push, PR | Clippy linting + rustfmt check |
| `test` | Push, PR | `cargo test --all-features` |
| `build` | Push to main, tags | Build 5 platform binaries |
| `feature-test` | Push, PR | Test all feature combinations |
| `release` | Tag `v*` | Create GitHub release with binaries |

### Build Matrix

| Target | OS | Artifact |
|--------|-----|----------|
| `x86_64-apple-darwin` | macOS | `homun-macos-x64` |
| `aarch64-apple-darwin` | macOS | `homun-macos-arm64` |
| `x86_64-unknown-linux-gnu` | Ubuntu | `homun-linux-x64` |
| `aarch64-unknown-linux-gnu` | Ubuntu | `homun-linux-arm64` |
| `x86_64-pc-windows-msvc` | Windows | `homun-windows-x64.exe` |

### Feature Test Matrix

The `feature-test` job verifies builds with:
- No features (`--no-default-features`)
- `cli` only
- `web-ui` only
- `gateway` meta-feature
- `full` meta-feature

### Viewing CI Status

```bash
# Via GitHub CLI
gh run list

# Via web
https://github.com/homunbot/homun/actions
```

## Installing Pre-built Binaries

### From GitHub Releases

```bash
# macOS (Apple Silicon)
curl -sL https://github.com/homunbot/homun/releases/latest/download/homun-macos-arm64 -o homun
chmod +x homun
sudo mv homun /usr/local/bin/

# macOS (Intel)
curl -sL https://github.com/homunbot/homun/releases/latest/download/homun-macos-x64 -o homun
chmod +x homun
sudo mv homun /usr/local/bin/

# Linux (x64)
curl -sL https://github.com/homunbot/homun/releases/latest/download/homun-linux-x64 -o homun
chmod +x homun
sudo mv homun /usr/local/bin/

# Linux (ARM64)
curl -sL https://github.com/homunbot/homun/releases/latest/download/homun-linux-arm64 -o homun
chmod +x homun
sudo mv homun /usr/local/bin/

# Windows (PowerShell)
Invoke-WebRequest -Uri https://github.com/homunbot/homun/releases/latest/download/homun-windows-x64.exe -OutFile homun.exe
```

### Install Script

```bash
# One-liner install (macOS/Linux)
curl -sL https://raw.githubusercontent.com/homunbot/homun/main/scripts/install.sh | bash
```

### From Source

```bash
# Clone and build
git clone https://github.com/homunbot/homun.git
cd homun
cargo build --release

# Install to ~/.cargo/bin
cargo install --path .
```

## Creating Releases

### Automatic Release

1. Update version in `Cargo.toml`
2. Create and push a tag:

```bash
git tag v0.2.0
git push origin v0.2.0
```

3. GitHub Actions will:
   - Build all 5 platform binaries
   - Create a GitHub release
   - Upload binaries as release assets

### Manual Release Notes

Edit the release on GitHub to add:
- Changelog
- Breaking changes
- New features

### Version Naming

- `vMAJOR.MINOR.PATCH` (semver)
- Example: `v0.1.0`, `v1.0.0`

## Development Workflow

### 1. Create Feature Branch

```bash
git checkout -b feature/my-feature
```

### 2. Make Changes

```bash
# Check formatting
cargo fmt --check

# Run linter
cargo clippy --all-features -- -D warnings

# Run tests
cargo test --all-features
```

### 3. Commit and Push

```bash
git add .
git commit -m "feat(scope): description"
git push origin feature/my-feature
```

### 4. Create PR

```bash
gh pr create --title "feat(scope): description" --body "..."
```

### 5. Merge and Release

After merge to `main`:
- CI runs tests and checks
- Create version tag for release

## Dependabot

Dependabot automatically creates PRs for:
- Cargo dependencies (weekly)
- GitHub Actions (weekly)

Review and merge dependency updates regularly to keep the project secure.

## Troubleshooting

### Build Fails with "linker not found"

Install a C compiler:
```bash
# Ubuntu/Debian
sudo apt install build-essential

# macOS (Xcode Command Line Tools)
xcode-select --install
```

### SQLite Link Errors

```bash
# Ubuntu/Debian
sudo apt install libsqlite3-dev

# macOS (Homebrew)
brew install sqlite
```

### Large Binary Size

Ensure you're using `--release`:
```bash
cargo build --release
```

Check which features are enabled:
```bash
cargo tree --features full | head -50
```

### CI Fails on Clippy

Run locally first:
```bash
cargo clippy --all-features -- -D warnings
cargo clippy --fix --allow-dirty
```
