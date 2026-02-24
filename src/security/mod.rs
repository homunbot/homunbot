//! Security module — TOTP 2FA, session management, vault protection, and exfiltration prevention.
//!
//! This module implements security features for Homun:
//!
//! ## 1. Two-Factor Authentication (Vault 2FA)
//!
//! TOTP-based authentication for vault access using Google Authenticator,
//! Authy, 1Password, Bitwarden, and other authenticator apps.
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                        VAULT 2FA FLOW                           │
//! ├─────────────────────────────────────────────────────────────────┤
//! │   1. Setup (Settings → Enable 2FA)                              │
//! │      ├── Generate TOTP secret (Base32)                         │
//! │      ├── Generate QR code (server-side PNG)                    │
//! │      ├── User scans with authenticator app                     │
//! │      └── Confirm with first code → save to 2fa.enc             │
//! │                                                                 │
//! │   2. Authentication (vault retrieve)                            │
//! │      ├── Check if 2FA enabled                                   │
//! │      ├── If enabled + no valid session → require code          │
//! │      ├── Verify code (±1 window for clock skew)                │
//! │      └── Create session (5 min TTL by default)                 │
//! │                                                                 │
//! │   3. Session Management                                         │
//! │      ├── In-memory sessions with configurable TTL              │
//! │      ├── Auto-expiry after timeout                             │
//! │      └── Rate limiting (5 attempts, 5 min lockout)             │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## 2. Exfiltration Prevention (T-SEC-02)
//!
//! Detects and redacts secrets in LLM output before they reach the user.
//!
//! ```text
//! LLM Response → ExfilFilter → Pattern Match → Redact/Block → User
//! ```
//!
//! # Security Properties
//!
//! - **TOTP secret encrypted** in `~/.homun/2fa.enc` (same master key as vault)
//! - **Session timeout** configurable (default 5 min)
//! - **Rate limiting** to prevent brute force
//! - **Recovery codes** for account recovery
//! - **No bypass** — even disabling 2FA requires 2FA!
//! - **Exfiltration detection** for API keys, tokens, passwords

mod exfiltration;
mod totp;
mod two_factor;
mod vault_leak;

pub use exfiltration::{
    scan, redact, global_filter, init_global_filter,
    Detection, ExfilConfig, ExfilFilter, ScanResult, Severity,
};
pub use vault_leak::redact_vault_values;
pub use totp::{generate_recovery_codes, TotpError, TotpManager};
pub use two_factor::{
    global_session_manager, TwoFactorConfig, TwoFactorSession, TwoFactorSessionManager,
    TwoFactorStorage,
};
