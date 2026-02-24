# Homun — Security Implementation Tasks

> Last updated: 2026-02-24
> Context: OpenClaw ClawHavoc attack (Feb 2026) — 2,419 malicious skills in marketplace

---

## Security Philosophy

**Defense in Depth**: Multiple layers of protection so if one fails, others catch the threat.

**Zero Trust**: Never trust skills, scripts, or external inputs. Always verify.

**Data Sovereignty**: Sensitive data never leaves the device without explicit consent.

---

## P0 — Critical Security

### T-SEC-01: Vault 2FA ✅ DONE

**Priority: P0 | Complexity: Medium | Completed: 2026-02-23**

**Problem**: Anyone with access to the terminal can read vault secrets.

**Solution**: TOTP-based two-factor authentication using authenticator apps (Google Authenticator, Authy, 1Password, etc.)

**Implementation**:
- TOTP with 6-digit codes, 30-second period, ±1 window for clock skew
- Session-based authentication (5 min TTL by default, configurable)
- 10 recovery codes (format: XXXX-XXXX)
- Rate limiting: 5 attempts, 5 min lockout
- QR code generation for easy setup
- Web UI integration with modal dialogs

**Files created/modified**:
- `src/security/mod.rs` — Security module
- `src/security/totp.rs` — TOTP manager (generate, verify, QR)
- `src/security/two_factor.rs` — Config, sessions, storage
- `src/tools/vault.rs` — 2FA integration in VaultTool
- `src/web/api.rs` — 8 new API endpoints for 2FA
- `src/web/pages.rs` — Web UI 2FA section
- `static/js/vault.js` — Frontend 2FA handling

**Storage**: `~/.homun/2fa.enc` (JSON, to be encrypted)

**API Endpoints**:
- `GET /api/v1/vault/2fa/status`
- `POST /api/v1/vault/2fa/setup`
- `POST /api/v1/vault/2fa/confirm`
- `POST /api/v1/vault/2fa/verify`
- `POST /api/v1/vault/2fa/disable`
- `POST /api/v1/vault/2fa/recovery`
- `PATCH /api/v1/vault/2fa/settings`

---

### T-SEC-02: Data Exfiltration Prevention ✅ DONE

**Priority: P0 | Complexity: High | Completed: 2026-02-24**

**Problem**: Malicious skill or compromised agent could exfiltrate secrets via network or LLM output.

**Solution**: Multi-layer protection — pattern detection + vault value redaction.

**Implementation**:

1. **Regex Pattern Detection** (`src/security/exfiltration.rs`):
   - 15+ patterns for API keys, tokens, passwords
   - OpenAI, Anthropic, AWS, Telegram, Discord, GitHub, JWT, private keys
   - Applied to LLM output before returning to user

2. **Vault Leak Prevention** (`src/security/vault_leak.rs`):
   - Redact vault values from memory files during consolidation
   - Redact vault values from LLM output before returning to user
   - Replace with `vault://key_name` references

**Files created/modified**:
- `src/security/exfiltration.rs` — Pattern detection engine + 14 tests
- `src/security/vault_leak.rs` — Vault value redaction + 6 tests
- `src/security/mod.rs` — Exports
- `src/agent/agent_loop.rs` — Apply redaction to LLM output
- `src/agent/memory.rs` — Redact during consolidation

**Config**:
```toml
[security.exfiltration]
enabled = true
block_on_detection = false  # true = block output, false = redact only
log_attempts = true
custom_patterns = []
```

---

## P1 — Skill Security

### T-SEC-03: VirusTotal Integration

**Priority: P1 | Complexity: Medium | Est: 2 days**

**Problem**: Downloaded skills could contain malware.

**Solution**: Scan skill archives with VirusTotal before installation.

**Flow**:
```
homun skills add owner/repo
  ↓
Download to temp location
  ↓
Calculate SHA256 hash
  ↓
Query VirusTotal API
  ↓
If clean → install
If malicious → reject, alert user
If unknown → warn, ask for confirmation
```

**Implementation**:
- `src/skills/scanner.rs` — VirusTotal client
- `src/skills/installer.rs` — integrate scan step
- Cache results locally (24h TTL)

**API**: https://www.virustotal.com/api/v3/files/{hash}

**Config**:
```toml
[security.skill_scan]
enabled = true
virustotal_api_key = "***ENCRYPTED***"
# Minimum engines that must flag as malicious to block
malicious_threshold = 2
# Block on scan failure (network error)
block_on_error = false
```

---

### T-SEC-04: Skill Compatibility Analysis

**Priority: P1 | Complexity: High | Est: 3-4 days**

**Problem**: A skill may be safe but incompatible or risky for Homun.

**Solution**: Static analysis of skill before installation.

**Checks**:
1. **SKILL.md validation** — required fields, valid YAML
2. **Script analysis**:
   - Check for dangerous commands (rm -rf, curl | sh, eval)
   - Check for network calls (curl, wget, nc)
   - Check for file access outside workspace
   - Check for credential access (cat ~/.ssh, cat ~/.aws)
3. **Permission declaration** — skill must declare what it needs:
   - `network: true/false`
   - `filesystem: read/write/none`
   - `shell: true/false`
   - `secrets: true/false`
4. **Risk score** — 0-10 scale based on capabilities

**Output**:
```
📊 Skill Analysis: github-notify v1.2

✅ VirusTotal: Clean (0/72 engines)

⚠️  Risk Assessment: MEDIUM (4/10)
  • Network access: YES (api.github.com)
  • Shell commands: YES (git, gh)
  • File access: READ (workspace only)
  • Secrets: NO

🔍 Dangerous Patterns: NONE

📋 Permissions Requested:
  • network:api.github.com
  • shell:git,gh
  • filesystem:read

❓ Install this skill? [y/N]
```

**Implementation**:
- `src/skills/analyzer.rs` — static analysis engine
- `src/skills/permissions.rs` — permission system
- Update SKILL.md spec to include permissions declaration

---

### T-SEC-05: Skill Sandbox Execution

**Priority: P1 | Complexity: High | Est: 4-5 days**

**Problem**: Even "safe" skills could behave maliciously at runtime.

**Solution**: Run skill scripts in isolated environment with restricted permissions.

**Implementation Options**:

1. **Namespace isolation** (Linux only):
   - `unshare --net --pid --mount` for network/filesystem isolation
   - Lowest overhead

2. **Docker container** (portable):
   - `docker run --rm --network none --read-only ...`
   - Highest security, requires Docker

3. **Wasmer/Wasmtime** (future):
   - Compile scripts to WASM
   - Perfect isolation, no system access
   - Limited to supported languages

**Phase 1**: Namespace isolation (Linux) + warning on other OS
**Phase 2**: Docker option
**Phase 3**: WASM runtime

**Config**:
```toml
[security.sandbox]
enabled = true
mode = "namespace"  # or "docker", "none"
# Resources
memory_limit_mb = 256
cpu_limit_percent = 50
timeout_secs = 60
# Access
network_enabled = false
filesystem_readonly = true
```

---

### T-SEC-06: Skill Audit Logging

**Priority: P1 | Complexity: Low | Est: 1 day**

**Problem**: No visibility into what skills do at runtime.

**Solution**: Log all skill executions with full context.

**Log Format**:
```json
{
  "timestamp": "2026-02-23T14:30:00Z",
  "event": "skill_execute",
  "skill": "github-notify",
  "script": "check_prs.py",
  "input": {"repo": "owner/repo"},
  "output": "[truncated]",
  "duration_ms": 1234,
  "exit_code": 0,
  "files_accessed": ["/workspace/.git/config"],
  "network_calls": ["api.github.com"]
}
```

**Storage**: SQLite table `skill_audit_log`

**Files**:
- `src/skills/audit.rs` — audit logger
- `migrations/` — add audit table

---

## P2 — Additional Hardening

### T-SEC-07: Session Encryption

**Priority: P2 | Complexity: Medium | Est: 2 days**

Encrypt conversation history at rest in SQLite.

### T-SEC-08: Secure Memory Handling

**Priority: P2 | Complexity: Low | Est: 1 day**

Use `zeroize` for all secret strings in memory.

### T-SEC-09: Config Integrity Check

**Priority: P2 | Complexity: Low | Est: 1 day**

Hash config.toml, alert if modified outside Homun.

### T-SEC-10: Brute Force Protection

**Priority: P2 | Complexity: Low | Est: 1 day**

Rate limit failed vault PIN attempts, lockout after N failures.

---

## Implementation Order

```
Sprint 1: Critical (P0)
├── T-SEC-01: Vault 2FA ✅
└── T-SEC-02: Exfiltration Prevention ✅

Sprint 2: Skill Security (P1)
├── T-SEC-03: VirusTotal Integration
├── T-SEC-04: Skill Compatibility Analysis
├── T-SEC-05: Skill Sandbox
└── T-SEC-06: Audit Logging

Sprint 3: Hardening (P2)
├── T-SEC-07: Session Encryption
├── T-SEC-08: Secure Memory
├── T-SEC-09: Config Integrity
└── T-SEC-10: Brute Force Protection
```

---

## Dependencies

```toml
# VirusTotal
reqwest = { features = ["json"] }

# Sandbox (Linux)
# Uses std::process::Command with unshare

# Sandbox (Docker)
# Requires Docker installed, no Rust dep

# Memory
zeroize = "1"

# Crypto (session encryption)
aes-gcm = "0.10"
```
