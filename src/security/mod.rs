//! Security module — TOTP 2FA, session management, and vault protection.
//!
//! This module implements two-factor authentication for vault access using
//! TOTP (Time-based One-Time Password) compatible with Google Authenticator,
//! Authy, 1Password, Bitwarden, and other authenticator apps.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                        VAULT 2FA FLOW                           │
//! ├─────────────────────────────────────────────────────────────────┤
//! │                                                                 │
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
//! │                                                                 │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Security Properties
//!
//! - **TOTP secret encrypted** in `~/.homun/2fa.enc` (same master key as vault)
//! - **Session timeout** configurable (default 5 min)
//! - **Rate limiting** to prevent brute force
//! - **Recovery codes** for account recovery
//! - **No bypass** — even disabling 2FA requires 2FA!

mod totp;
mod two_factor;

pub use totp::{generate_recovery_codes, TotpError, TotpManager};
pub use two_factor::{
    global_session_manager, TwoFactorConfig, TwoFactorSession, TwoFactorSessionManager,
    TwoFactorStorage,
};
