//! Web authentication module — session-based auth + Bearer token + rate limiting.
//!
//! SEC-1: Login/session management with PBKDF2 password hashing
//! SEC-3: Per-IP rate limiting for auth and API endpoints
//! SEC-4: Bearer token auth for programmatic access

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use axum::extract::{ConnectInfo, Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};
use axum::Json;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64, Engine};
use ring::hmac;
use ring::pbkdf2;
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use super::server::AppState;

// ─── Constants ──────────────────────────────────────────────────

const PBKDF2_ITERATIONS: u32 = 600_000;
const SALT_LEN: usize = 16;
const CREDENTIAL_LEN: usize = 32;
const SESSION_COOKIE_NAME: &str = "homun_session";
pub const DEFAULT_SESSION_TTL_SECS: u64 = 86400; // 24 hours
const SESSION_ID_LEN: usize = 32;

static PBKDF2_ALG: pbkdf2::Algorithm = pbkdf2::PBKDF2_HMAC_SHA256;

// ─── Password Hashing (ring::pbkdf2) ───────────────────────────

/// Hash a password with a random salt. Returns "base64(salt):base64(hash)".
pub fn hash_password(password: &str) -> Result<String> {
    let rng = SystemRandom::new();

    let mut salt = [0u8; SALT_LEN];
    rng.fill(&mut salt)
        .map_err(|_| anyhow::anyhow!("RNG failed generating salt"))?;

    let mut hash = [0u8; CREDENTIAL_LEN];
    pbkdf2::derive(
        PBKDF2_ALG,
        std::num::NonZeroU32::new(PBKDF2_ITERATIONS).unwrap(),
        &salt,
        password.as_bytes(),
        &mut hash,
    );

    Ok(format!("{}:{}", B64.encode(salt), B64.encode(hash)))
}

/// Verify a password against a stored "salt:hash" string.
pub fn verify_password(password: &str, stored: &str) -> bool {
    let parts: Vec<&str> = stored.splitn(2, ':').collect();
    if parts.len() != 2 {
        return false;
    }

    let salt = match B64.decode(parts[0]) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let expected_hash = match B64.decode(parts[1]) {
        Ok(h) => h,
        Err(_) => return false,
    };

    pbkdf2::verify(
        PBKDF2_ALG,
        std::num::NonZeroU32::new(PBKDF2_ITERATIONS).unwrap(),
        &salt,
        password.as_bytes(),
        &expected_hash,
    )
    .is_ok()
}

// ─── Session Store ──────────────────────────────────────────────

/// An active web session.
#[derive(Debug, Clone)]
pub struct WebSession {
    pub user_id: String,
    pub username: String,
    pub roles: Vec<String>,
    pub created_at: Instant,
    pub ttl: Duration,
}

impl WebSession {
    pub fn is_valid(&self) -> bool {
        self.created_at.elapsed() < self.ttl
    }
}

/// In-memory session store (same pattern as TwoFactorSessionManager).
pub struct SessionStore {
    sessions: RwLock<HashMap<String, WebSession>>,
    signing_key: hmac::Key,
    default_ttl: Duration,
}

impl SessionStore {
    /// Create a new session store with a random signing key.
    pub fn new(ttl_secs: u64) -> Result<Self> {
        let signing_key = Self::load_or_create_signing_key()?;
        Ok(Self {
            sessions: RwLock::new(HashMap::new()),
            signing_key,
            default_ttl: Duration::from_secs(ttl_secs),
        })
    }

    /// Create with an explicit key (for testing).
    #[cfg(test)]
    pub fn with_key(key: hmac::Key, ttl_secs: u64) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            signing_key: key,
            default_ttl: Duration::from_secs(ttl_secs),
        }
    }

    /// Load or create signing key from vault.
    fn load_or_create_signing_key() -> Result<hmac::Key> {
        // Try to get existing key from vault
        let secrets = crate::storage::global_secrets()?;
        let key_name = crate::storage::SecretKey::custom("web.session.signing_key");

        if let Some(existing) = secrets.get(&key_name)? {
            let key_bytes = B64.decode(&existing)?;
            return Ok(hmac::Key::new(hmac::HMAC_SHA256, &key_bytes));
        }

        // Generate new key and store it
        let rng = SystemRandom::new();
        let mut key_bytes = [0u8; 32];
        rng.fill(&mut key_bytes)
            .map_err(|_| anyhow::anyhow!("RNG failed generating signing key"))?;

        let encoded = B64.encode(key_bytes);
        secrets.set(&key_name, &encoded)?;

        tracing::info!("Generated new web session signing key");
        Ok(hmac::Key::new(hmac::HMAC_SHA256, &key_bytes))
    }

    /// Create a new session, returns the session ID.
    pub async fn create(&self, user_id: &str, username: &str, roles: &[String]) -> String {
        let rng = SystemRandom::new();
        let mut id_bytes = [0u8; SESSION_ID_LEN];
        rng.fill(&mut id_bytes).expect("RNG failed");
        let session_id = B64.encode(id_bytes);

        let session = WebSession {
            user_id: user_id.to_string(),
            username: username.to_string(),
            roles: roles.to_vec(),
            created_at: Instant::now(),
            ttl: self.default_ttl,
        };

        self.sessions
            .write()
            .await
            .insert(session_id.clone(), session);
        session_id
    }

    /// Validate a session by ID.
    pub async fn get(&self, session_id: &str) -> Option<WebSession> {
        let sessions = self.sessions.read().await;
        sessions.get(session_id).filter(|s| s.is_valid()).cloned()
    }

    /// Destroy a session (logout).
    pub async fn destroy(&self, session_id: &str) {
        self.sessions.write().await.remove(session_id);
    }

    /// Remove expired sessions.
    pub async fn cleanup_expired(&self) {
        let mut sessions = self.sessions.write().await;
        let before = sessions.len();
        sessions.retain(|_, s| s.is_valid());
        let removed = before - sessions.len();
        if removed > 0 {
            tracing::debug!(removed, "Cleaned up expired web sessions");
        }
    }

    /// Sign a session ID for cookie value: "session_id.signature"
    pub fn sign_cookie(&self, session_id: &str) -> String {
        let tag = hmac::sign(&self.signing_key, session_id.as_bytes());
        format!("{}.{}", session_id, B64.encode(tag.as_ref()))
    }

    /// Verify and extract session ID from signed cookie value.
    pub fn verify_cookie(&self, cookie_value: &str) -> Option<String> {
        let dot_pos = cookie_value.rfind('.')?;
        let session_id = &cookie_value[..dot_pos];
        let signature = &cookie_value[dot_pos + 1..];

        let sig_bytes = B64.decode(signature).ok()?;
        hmac::verify(&self.signing_key, session_id.as_bytes(), &sig_bytes).ok()?;

        Some(session_id.to_string())
    }
}

// ─── Auth User (injected into request extensions) ───────────────

/// Authenticated user info, available to handlers via request extensions.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: String,
    pub username: String,
    pub roles: Vec<String>,
    pub auth_method: AuthMethod,
}

#[derive(Debug, Clone)]
pub enum AuthMethod {
    Session,
    BearerToken { scope: String },
}

impl AuthUser {
    /// Check if this auth context allows write operations.
    pub fn can_write(&self) -> bool {
        match &self.auth_method {
            AuthMethod::Session => true,
            AuthMethod::BearerToken { scope } => scope != "read",
        }
    }
}

// ─── Auth Middleware ────────────────────────────────────────────

/// Extract session cookie from Cookie header.
fn extract_session_cookie(req: &Request) -> Option<String> {
    let cookie_header = req.headers().get(header::COOKIE)?.to_str().ok()?;
    for cookie in cookie_header.split(';') {
        let cookie = cookie.trim();
        if let Some(value) = cookie.strip_prefix(SESSION_COOKIE_NAME) {
            let value = value.strip_prefix('=')?;
            return Some(value.to_string());
        }
    }
    None
}

/// Extract Bearer token from Authorization header.
fn extract_bearer_token(req: &Request) -> Option<String> {
    let auth_header = req.headers().get(header::AUTHORIZATION)?.to_str().ok()?;
    let token = auth_header.strip_prefix("Bearer ")?.trim();
    if !token.is_empty() {
        Some(token.to_string())
    } else {
        None
    }
}

/// Auth middleware: check session cookie or Bearer token.
/// Redirects to /login for HTML requests, returns 401 for API requests.
pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    mut req: Request,
    next: Next,
) -> Response {
    // 1. Try session cookie
    if let Some(session_store) = &state.session_store {
        if let Some(cookie_value) = extract_session_cookie(&req) {
            if let Some(session_id) = session_store.verify_cookie(&cookie_value) {
                if let Some(session) = session_store.get(&session_id).await {
                    req.extensions_mut().insert(AuthUser {
                        user_id: session.user_id,
                        username: session.username,
                        roles: session.roles,
                        auth_method: AuthMethod::Session,
                    });
                    return next.run(req).await;
                }
            }
        }
    }

    // 2. Try Bearer token (SEC-4)
    if let Some(token) = extract_bearer_token(&req) {
        if token.starts_with("wh_") {
            if let Some(db) = &state.db {
                if let Ok(Some(token_row)) = db.load_webhook_token(&token).await {
                    if token_row.enabled {
                        if let Ok(Some(user_row)) = db.lookup_user_by_webhook_token(&token).await {
                            // Update last_used (fire and forget)
                            let db_clone = db.clone();
                            let token_clone = token.clone();
                            tokio::spawn(async move {
                                let _ = db_clone.touch_webhook_token(&token_clone).await;
                            });

                            let roles: Vec<String> = serde_json::from_str(&user_row.roles)
                                .unwrap_or_else(|_| vec!["user".to_string()]);

                            req.extensions_mut().insert(AuthUser {
                                user_id: user_row.id,
                                username: user_row.username,
                                roles,
                                auth_method: AuthMethod::BearerToken {
                                    scope: token_row.scope,
                                },
                            });
                            return next.run(req).await;
                        }
                    }
                }
            }
        }
    }

    // 3. First-run: if no user has a password, redirect to setup wizard
    if let Some(db) = &state.db {
        if let Ok(0) = db.count_users_with_password().await {
            let path = req.uri().path();
            if !path.starts_with("/setup-wizard")
                && !path.starts_with("/api/auth/setup")
                && !path.starts_with("/static/")
            {
                return Redirect::to("/setup-wizard").into_response();
            }
            // Allow setup wizard and its API through
            return next.run(req).await;
        }
    }

    // 4. No valid auth — reject
    let path = req.uri().path().to_string();
    if path.starts_with("/api/") || path.starts_with("/ws/") {
        // API and WebSocket paths get 401 JSON (WS can't follow HTTP redirects)
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": "unauthorized",
                "message": "Authentication required. Provide a session cookie or Bearer token."
            })),
        )
            .into_response()
    } else {
        Redirect::to("/login").into_response()
    }
}

// ─── Rate Limiter ──────────────────────────────────────────────

/// Simple in-memory rate limiter per IP address.
pub struct RateLimiter {
    state: RwLock<HashMap<IpAddr, (u32, Instant)>>,
    max_requests: u32,
    window: Duration,
}

impl RateLimiter {
    pub fn new(max_requests: u32, window_secs: u64) -> Self {
        Self {
            state: RwLock::new(HashMap::new()),
            max_requests,
            window: Duration::from_secs(window_secs),
        }
    }

    /// Check if a request from this IP is allowed. Returns remaining count or retry-after duration.
    pub async fn check(&self, ip: IpAddr) -> std::result::Result<u32, Duration> {
        let mut state = self.state.write().await;
        let now = Instant::now();

        let entry = state.entry(ip).or_insert((0, now));

        // Reset window if expired
        if now.duration_since(entry.1) >= self.window {
            *entry = (0, now);
        }

        if entry.0 >= self.max_requests {
            let retry_after = self.window.saturating_sub(now.duration_since(entry.1));
            return Err(retry_after);
        }

        entry.0 += 1;
        Ok(self.max_requests - entry.0)
    }

    /// Periodic cleanup of expired entries.
    pub async fn cleanup(&self) {
        let mut state = self.state.write().await;
        let now = Instant::now();
        state.retain(|_, (_, start)| now.duration_since(*start) < self.window);
    }
}

/// Rate limiting middleware for auth endpoints (login, setup).
pub async fn auth_rate_limit_middleware(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req: Request,
    next: Next,
) -> Response {
    match state.auth_rate_limiter.check(addr.ip()).await {
        Ok(remaining) => {
            let mut response = next.run(req).await;
            if let Ok(val) = remaining.to_string().parse() {
                response.headers_mut().insert("X-RateLimit-Remaining", val);
            }
            response
        }
        Err(retry_after) => (
            StatusCode::TOO_MANY_REQUESTS,
            [("Retry-After", retry_after.as_secs().max(1).to_string())],
            Json(serde_json::json!({
                "error": "rate_limited",
                "message": "Too many requests. Please try again later.",
                "retry_after_secs": retry_after.as_secs().max(1)
            })),
        )
            .into_response(),
    }
}

/// Rate limiting middleware for general API endpoints.
pub async fn api_rate_limit_middleware(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req: Request,
    next: Next,
) -> Response {
    match state.api_rate_limiter.check(addr.ip()).await {
        Ok(remaining) => {
            let mut response = next.run(req).await;
            if let Ok(val) = remaining.to_string().parse() {
                response.headers_mut().insert("X-RateLimit-Remaining", val);
            }
            response
        }
        Err(retry_after) => (
            StatusCode::TOO_MANY_REQUESTS,
            [("Retry-After", retry_after.as_secs().max(1).to_string())],
            Json(serde_json::json!({
                "error": "rate_limited",
                "message": "Too many API requests. Please try again later.",
                "retry_after_secs": retry_after.as_secs().max(1)
            })),
        )
            .into_response(),
    }
}

// ─── Auth API Handlers ─────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// POST /api/auth/login
pub async fn login_handler(
    State(state): State<Arc<AppState>>,
    Json(body): Json<LoginRequest>,
) -> Response {
    let db = match &state.db {
        Some(db) => db,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthResponse {
                    success: false,
                    redirect: None,
                    error: Some("Database not available".into()),
                }),
            )
                .into_response()
        }
    };

    let session_store = match &state.session_store {
        Some(s) => s,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthResponse {
                    success: false,
                    redirect: None,
                    error: Some("Session store not available".into()),
                }),
            )
                .into_response()
        }
    };

    // Look up user
    let user = match db.load_user_by_username(&body.username).await {
        Ok(Some(u)) => u,
        _ => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(AuthResponse {
                    success: false,
                    redirect: None,
                    error: Some("Invalid username or password".into()),
                }),
            )
                .into_response()
        }
    };

    // Verify password
    let password_hash = match &user.password_hash {
        Some(h) if !h.is_empty() => h,
        _ => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(AuthResponse {
                    success: false,
                    redirect: None,
                    error: Some("Account has no password set".into()),
                }),
            )
                .into_response()
        }
    };

    if !verify_password(&body.password, password_hash) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(AuthResponse {
                success: false,
                redirect: None,
                error: Some("Invalid username or password".into()),
            }),
        )
            .into_response();
    }

    // Create session
    let roles: Vec<String> =
        serde_json::from_str(&user.roles).unwrap_or_else(|_| vec!["user".to_string()]);
    let session_id = session_store.create(&user.id, &user.username, &roles).await;
    let signed_cookie = session_store.sign_cookie(&session_id);

    tracing::info!(username = %user.username, "User logged in via web UI");

    let cookie = format!(
        "{}={}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age={}",
        SESSION_COOKIE_NAME, signed_cookie, DEFAULT_SESSION_TTL_SECS
    );

    (
        StatusCode::OK,
        [(header::SET_COOKIE, cookie)],
        Json(AuthResponse {
            success: true,
            redirect: Some("/".into()),
            error: None,
        }),
    )
        .into_response()
}

/// POST /api/auth/logout
pub async fn logout_handler(State(state): State<Arc<AppState>>, req: Request) -> Response {
    if let Some(session_store) = &state.session_store {
        if let Some(cookie_value) = extract_session_cookie(&req) {
            if let Some(session_id) = session_store.verify_cookie(&cookie_value) {
                session_store.destroy(&session_id).await;
            }
        }
    }

    // Clear cookie
    let clear_cookie = format!(
        "{}=; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=0",
        SESSION_COOKIE_NAME
    );

    (
        StatusCode::OK,
        [(header::SET_COOKIE, clear_cookie)],
        Json(AuthResponse {
            success: true,
            redirect: Some("/login".into()),
            error: None,
        }),
    )
        .into_response()
}

#[derive(Debug, Deserialize)]
pub struct SetupRequest {
    pub username: String,
    pub password: String,
}

/// POST /api/auth/setup — first-run admin creation
pub async fn setup_handler(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SetupRequest>,
) -> Response {
    let db = match &state.db {
        Some(db) => db,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthResponse {
                    success: false,
                    redirect: None,
                    error: Some("Database not available".into()),
                }),
            )
                .into_response()
        }
    };

    // Only allow setup if no user has a password yet
    match db.count_users_with_password().await {
        Ok(0) => {}
        Ok(_) => {
            return (
                StatusCode::FORBIDDEN,
                Json(AuthResponse {
                    success: false,
                    redirect: None,
                    error: Some("Setup already completed. An admin account already exists.".into()),
                }),
            )
                .into_response()
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthResponse {
                    success: false,
                    redirect: None,
                    error: Some(format!("Database error: {e}")),
                }),
            )
                .into_response()
        }
    }

    // Validate input
    if body.username.trim().is_empty() || body.password.len() < 6 {
        return (
            StatusCode::BAD_REQUEST,
            Json(AuthResponse {
                success: false,
                redirect: None,
                error: Some("Username required and password must be at least 6 characters".into()),
            }),
        )
            .into_response();
    }

    // Hash password
    let password_hash = match hash_password(&body.password) {
        Ok(h) => h,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthResponse {
                    success: false,
                    redirect: None,
                    error: Some(format!("Failed to hash password: {e}")),
                }),
            )
                .into_response()
        }
    };

    // Look up existing user or create new one
    let username = body.username.trim();
    let user_id = match db.load_user_by_username(username).await {
        Ok(Some(existing)) => {
            // User exists (e.g. from channel pairing) — just set password
            tracing::info!(%username, "Setting password for existing user during first-run setup");
            existing.id
        }
        _ => {
            // No existing user — create new admin
            let new_id = uuid::Uuid::new_v4().to_string();
            if let Err(e) = db.create_user(&new_id, username, &["admin"]).await {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(AuthResponse {
                        success: false,
                        redirect: None,
                        error: Some(format!("Failed to create user: {e}")),
                    }),
                )
                    .into_response();
            }
            tracing::info!(%username, "Admin account created during first-run setup");
            new_id
        }
    };

    if let Err(e) = db.set_user_password_hash(&user_id, &password_hash).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(AuthResponse {
                success: false,
                redirect: None,
                error: Some(format!("Failed to set password: {e}")),
            }),
        )
            .into_response();
    }

    // Auto-login: create session
    let session_store = match &state.session_store {
        Some(s) => s,
        None => {
            return (
                StatusCode::OK,
                Json(AuthResponse {
                    success: true,
                    redirect: Some("/login".into()),
                    error: None,
                }),
            )
                .into_response()
        }
    };

    let roles = vec!["admin".to_string()];
    let session_id = session_store
        .create(&user_id, body.username.trim(), &roles)
        .await;
    let signed_cookie = session_store.sign_cookie(&session_id);

    let cookie = format!(
        "{}={}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age={}",
        SESSION_COOKIE_NAME, signed_cookie, DEFAULT_SESSION_TTL_SECS
    );

    (
        StatusCode::OK,
        [(header::SET_COOKIE, cookie)],
        Json(AuthResponse {
            success: true,
            redirect: Some("/".into()),
            error: None,
        }),
    )
        .into_response()
}

// ─── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_and_verify_password() {
        let hash = hash_password("test_password_123").unwrap();
        assert!(verify_password("test_password_123", &hash));
        assert!(!verify_password("wrong_password", &hash));
    }

    #[test]
    fn test_hash_different_salts() {
        let h1 = hash_password("same_password").unwrap();
        let h2 = hash_password("same_password").unwrap();
        // Different salts produce different hashes
        assert_ne!(h1, h2);
        // But both verify correctly
        assert!(verify_password("same_password", &h1));
        assert!(verify_password("same_password", &h2));
    }

    #[test]
    fn test_verify_invalid_format() {
        assert!(!verify_password("anything", "not_a_valid_hash"));
        assert!(!verify_password("anything", ""));
        assert!(!verify_password("anything", "::"));
    }

    #[test]
    fn test_cookie_signing_valid() {
        let key = hmac::Key::new(hmac::HMAC_SHA256, &[42u8; 32]);
        let store = SessionStore::with_key(key, 3600);

        let signed = store.sign_cookie("test-session-id");
        assert!(signed.contains('.'));

        let verified = store.verify_cookie(&signed);
        assert_eq!(verified, Some("test-session-id".to_string()));
    }

    #[test]
    fn test_cookie_signing_tampered() {
        let key = hmac::Key::new(hmac::HMAC_SHA256, &[42u8; 32]);
        let store = SessionStore::with_key(key, 3600);

        let signed = store.sign_cookie("real-session");
        // Tamper with session ID but keep signature
        let tampered = format!("fake-session.{}", signed.split('.').last().unwrap());
        assert_eq!(store.verify_cookie(&tampered), None);

        // Tamper with signature
        assert_eq!(store.verify_cookie("real-session.badsig"), None);
    }

    #[tokio::test]
    async fn test_session_lifecycle() {
        let key = hmac::Key::new(hmac::HMAC_SHA256, &[42u8; 32]);
        let store = SessionStore::with_key(key, 3600);

        let id = store
            .create("user-1", "admin", &["admin".to_string()])
            .await;
        assert!(store.get(&id).await.is_some());

        let session = store.get(&id).await.unwrap();
        assert_eq!(session.user_id, "user-1");
        assert_eq!(session.username, "admin");

        store.destroy(&id).await;
        assert!(store.get(&id).await.is_none());
    }

    #[tokio::test]
    async fn test_session_cleanup() {
        let key = hmac::Key::new(hmac::HMAC_SHA256, &[42u8; 32]);
        // TTL of 0 seconds means sessions expire immediately
        let store = SessionStore::with_key(key, 0);

        let id = store
            .create("user-1", "admin", &["admin".to_string()])
            .await;
        // Session should be expired immediately (TTL = 0)
        assert!(store.get(&id).await.is_none());

        store.cleanup_expired().await;
        // After cleanup, the expired entry is removed from the map
        assert!(store.sessions.read().await.is_empty());
    }

    #[test]
    fn test_bearer_scope_read() {
        let user = AuthUser {
            user_id: "u1".into(),
            username: "test".into(),
            roles: vec!["user".into()],
            auth_method: AuthMethod::BearerToken {
                scope: "read".into(),
            },
        };
        assert!(!user.can_write());
    }

    #[test]
    fn test_bearer_scope_admin() {
        let user = AuthUser {
            user_id: "u1".into(),
            username: "test".into(),
            roles: vec!["admin".into()],
            auth_method: AuthMethod::BearerToken {
                scope: "admin".into(),
            },
        };
        assert!(user.can_write());
    }

    #[test]
    fn test_session_auth_can_write() {
        let user = AuthUser {
            user_id: "u1".into(),
            username: "test".into(),
            roles: vec!["user".into()],
            auth_method: AuthMethod::Session,
        };
        assert!(user.can_write());
    }

    #[tokio::test]
    async fn test_rate_limiter_within_limit() {
        let rl = RateLimiter::new(5, 60);
        let ip: IpAddr = "127.0.0.1".parse().unwrap();

        for i in (0..5).rev() {
            assert_eq!(rl.check(ip).await, Ok(i));
        }
    }

    #[tokio::test]
    async fn test_rate_limiter_over_limit() {
        let rl = RateLimiter::new(2, 60);
        let ip: IpAddr = "127.0.0.1".parse().unwrap();

        assert!(rl.check(ip).await.is_ok());
        assert!(rl.check(ip).await.is_ok());
        assert!(rl.check(ip).await.is_err());
    }

    #[tokio::test]
    async fn test_rate_limiter_separate_ips() {
        let rl = RateLimiter::new(1, 60);
        let ip1: IpAddr = "10.0.0.1".parse().unwrap();
        let ip2: IpAddr = "10.0.0.2".parse().unwrap();

        assert!(rl.check(ip1).await.is_ok());
        assert!(rl.check(ip2).await.is_ok());
        assert!(rl.check(ip1).await.is_err());
        assert!(rl.check(ip2).await.is_err());
    }
}
