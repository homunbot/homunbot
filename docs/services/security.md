# Security

## Purpose

This subsystem owns the runtime security controls that protect secrets, outputs, access, and emergency shutdown behavior.

## Primary Code

- `src/security/mod.rs`
- `src/security/estop.rs`
- `src/security/exfiltration.rs`
- `src/security/pairing.rs`
- `src/security/vault_leak.rs`
- `src/security/totp.rs`
- `src/security/two_factor.rs`
- `src/storage/secrets.rs`

## Major Security Functions

### Encrypted Secrets Storage

Secrets are stored in `~/.homun/secrets.enc` using AES-256-GCM. The master key is resolved from:

1. OS keychain if available
2. file fallback at `~/.homun/master.key`

This is the backing store for many provider, channel, vault, and MCP secret flows.

### Exfiltration Prevention

The runtime scans LLM output for likely secrets before they reach the user. This sits in the response path, not just at input time.

### Vault Value Redaction

Known vault values can be redacted from outputs and memory flows so secrets are not accidentally reflected back.

### Pairing

Pairing support is used for channels or flows that need an explicit trust/onboarding step.

### Emergency Stop

The E-stop system is shared across gateway and web runtime. It is the main kill-switch mechanism for stopping autonomous activity.

### Optional Vault 2FA

When `vault-2fa` is enabled, the security layer also provides:

- TOTP setup
- recovery codes
- short-lived vault access sessions
- rate limiting for 2FA verification

## Security Boundaries

Security logic is spread across several layers on purpose:

- encrypted persistence in storage
- output redaction in the agent/runtime path
- approval and permission gates in the tools layer
- auth and rate limiting in the web layer

This document covers the dedicated security modules, not every security-relevant check in the repo.

## Failure Modes And Limits

- some controls are feature-gated
- security posture depends on whether secrets are actually moved out of plain config into encrypted storage
- sandbox hardening is complete: modular architecture (`src/tools/sandbox/`, 11 files), Linux native backend (Bubblewrap) with CI validation, Windows native backend (Job Objects) with post-spawn enforcement, runtime image lifecycle with CI validation, cross-platform E2E suite
- CI workflow `.github/workflows/sandbox-validation.yml` validates sandbox behavior across Linux (bwrap), Docker (runtime image), and cross-platform (macOS/Windows/Linux)

## Change Checklist

Update this document when you change:

- secret storage format or key handling
- exfiltration detection/redaction rules
- E-stop semantics
- 2FA session behavior
