use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use axum::Router;
use tokio::sync::mpsc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

#[cfg(feature = "web-ui")]
use std::path::Path;

use crate::bus::{InboundMessage, OutboundMessage, StreamMessage};
use crate::config::Config;
use crate::provider::ProviderHealthTracker;
use crate::security::EStopHandles;
use crate::storage::Database;
use crate::workflows::engine::WorkflowEngine;

use super::api;
use super::auth;
use super::pages;
use super::run_state::WebRunStore;
use super::ws;

/// Shared state accessible by all web handlers.
/// The config Arc is shared with the AgentLoop for hot-reload:
/// web UI writes → agent reads on next request.
pub struct AppState {
    pub config: Arc<tokio::sync::RwLock<Config>>,
    pub started_at: Instant,
    pub inbound_tx: Option<mpsc::Sender<InboundMessage>>,
    pub web_runs: Arc<WebRunStore>,
    /// Active WebSocket sessions: chat_id → sender for outbound messages
    pub ws_sessions: tokio::sync::RwLock<std::collections::HashMap<String, mpsc::Sender<String>>>,
    /// Stream sessions: chat_id → sender for real-time stream chunks and tool events.
    /// Used to deliver incremental text as the LLM generates it,
    /// plus tool_start/tool_end notifications.
    pub stream_sessions: tokio::sync::RwLock<
        std::collections::HashMap<String, mpsc::Sender<super::ws::WsStreamEvent>>,
    >,
    /// Database handle — used by memory/vault API endpoints.
    /// `None` in setup-only mode (no agent, just config UI).
    pub db: Option<Database>,
    /// Shared memory searcher for hybrid vector + FTS5 search.
    /// Shared with the AgentLoop — both use the same HNSW index.
    #[cfg(feature = "local-embeddings")]
    pub memory_searcher: Option<Arc<tokio::sync::Mutex<crate::agent::MemorySearcher>>>,
    /// Shared RAG knowledge base engine.
    #[cfg(feature = "local-embeddings")]
    pub rag_engine: Option<Arc<tokio::sync::Mutex<crate::rag::RagEngine>>>,
    /// Provider health tracker for circuit breaker metrics.
    pub health_tracker: Option<Arc<ProviderHealthTracker>>,
    /// Workflow engine for multi-step orchestration.
    pub workflow_engine: Option<Arc<WorkflowEngine>>,
    /// Business engine for autonomous business management.
    pub business_engine: Option<Arc<crate::business::engine::BusinessEngine>>,
    /// Emergency stop handles — shared with the estop module.
    pub estop_handles: Arc<tokio::sync::RwLock<EStopHandles>>,
    /// Web authentication session store (SEC-1).
    pub session_store: Option<Arc<auth::SessionStore>>,
    /// Rate limiter for auth endpoints — 5 req/min per IP (SEC-3).
    pub auth_rate_limiter: Arc<auth::RateLimiter>,
    /// Rate limiter for general API — 60 req/min per IP (SEC-3).
    pub api_rate_limiter: Arc<auth::RateLimiter>,
}

impl AppState {
    /// Save config to disk AND update the in-memory copy atomically.
    /// Since config is shared via Arc, the AgentLoop sees changes on next request.
    pub async fn save_config(&self, config: Config) -> anyhow::Result<()> {
        config.save()?;
        *self.config.write().await = config;
        Ok(())
    }
}

/// Web server — embedded dashboard + REST API + WebSocket chat
pub struct WebServer {
    config: Arc<tokio::sync::RwLock<Config>>,
    inbound_tx: Option<mpsc::Sender<InboundMessage>>,
    outbound_rx: Option<mpsc::Receiver<OutboundMessage>>,
    stream_rx: Option<mpsc::Receiver<StreamMessage>>,
    db: Option<Database>,
    #[cfg(feature = "local-embeddings")]
    memory_searcher: Option<Arc<tokio::sync::Mutex<crate::agent::MemorySearcher>>>,
    #[cfg(feature = "local-embeddings")]
    rag_engine: Option<Arc<tokio::sync::Mutex<crate::rag::RagEngine>>>,
    health_tracker: Option<Arc<ProviderHealthTracker>>,
    workflow_engine: Option<Arc<WorkflowEngine>>,
    business_engine: Option<Arc<crate::business::engine::BusinessEngine>>,
    estop_handles: Arc<tokio::sync::RwLock<EStopHandles>>,
}

impl WebServer {
    /// Create a web server that shares config with the agent for hot-reload.
    pub fn new(
        config: Arc<tokio::sync::RwLock<Config>>,
        inbound_tx: mpsc::Sender<InboundMessage>,
        outbound_rx: mpsc::Receiver<OutboundMessage>,
        db: Database,
    ) -> Self {
        Self {
            config,
            inbound_tx: Some(inbound_tx),
            outbound_rx: Some(outbound_rx),
            stream_rx: None,
            db: Some(db),
            #[cfg(feature = "local-embeddings")]
            memory_searcher: None,
            #[cfg(feature = "local-embeddings")]
            rag_engine: None,
            health_tracker: None,
            workflow_engine: None,
            business_engine: None,
            estop_handles: Arc::new(tokio::sync::RwLock::new(EStopHandles::default())),
        }
    }

    /// Set the shared memory searcher for hybrid search in the web API.
    #[cfg(feature = "local-embeddings")]
    pub fn set_memory_searcher(
        &mut self,
        searcher: Arc<tokio::sync::Mutex<crate::agent::MemorySearcher>>,
    ) {
        self.memory_searcher = Some(searcher);
    }

    /// Set the shared RAG engine for knowledge base API endpoints.
    #[cfg(feature = "local-embeddings")]
    pub fn set_rag_engine(
        &mut self,
        engine: Arc<tokio::sync::Mutex<crate::rag::RagEngine>>,
    ) {
        self.rag_engine = Some(engine);
    }

    /// Set the provider health tracker for the `/api/v1/providers/health` endpoint.
    pub fn set_health_tracker(&mut self, tracker: Arc<ProviderHealthTracker>) {
        self.health_tracker = Some(tracker);
    }

    /// Set the workflow engine for multi-step orchestration API endpoints.
    pub fn set_workflow_engine(&mut self, engine: Arc<WorkflowEngine>) {
        self.workflow_engine = Some(engine);
    }

    /// Set the business engine for autonomous business management API endpoints.
    pub fn set_business_engine(
        &mut self,
        engine: Arc<crate::business::engine::BusinessEngine>,
    ) {
        self.business_engine = Some(engine);
    }

    /// Set the emergency stop handles (shared with the estop module).
    pub fn set_estop_handles(&mut self, handles: Arc<tokio::sync::RwLock<EStopHandles>>) {
        self.estop_handles = handles;
    }

    /// Set the receiver for streaming chunks from the gateway.
    /// When the agent streams text for a web chat session, the gateway
    /// sends StreamMessage chunks here so they can be forwarded to the
    /// correct WebSocket connection.
    pub fn set_stream_rx(&mut self, rx: mpsc::Receiver<StreamMessage>) {
        self.stream_rx = Some(rx);
    }

    /// Create a setup-only server (no agent, just config UI).
    /// Wraps config in its own Arc — not shared with any agent.
    pub fn setup_only(config: Config) -> Self {
        Self {
            config: Arc::new(tokio::sync::RwLock::new(config)),
            inbound_tx: None,
            outbound_rx: None,
            stream_rx: None,
            db: None,
            #[cfg(feature = "local-embeddings")]
            memory_searcher: None,
            #[cfg(feature = "local-embeddings")]
            rag_engine: None,
            health_tracker: None,
            workflow_engine: None,
            business_engine: None,
            estop_handles: Arc::new(tokio::sync::RwLock::new(EStopHandles::default())),
        }
    }

    /// Start the web server. Runs until the server is shut down.
    pub async fn start(self) -> Result<()> {
        let (host, port, domain, rate_limit, auth_rate_limit, tls_cert, tls_key, auto_tls) = {
            let cfg = self.config.read().await;
            (
                cfg.channels.web.host.clone(),
                cfg.channels.web.port,
                cfg.channels.web.domain.clone(),
                cfg.channels.web.rate_limit_per_minute,
                cfg.channels.web.auth_rate_limit_per_minute,
                cfg.channels.web.tls_cert.clone(),
                cfg.channels.web.tls_key.clone(),
                cfg.channels.web.auto_tls,
            )
        };

        if let Some(db) = self.db.as_ref() {
            let interrupted = db.mark_incomplete_web_chat_runs_interrupted().await?;
            if interrupted > 0 {
                tracing::warn!(
                    count = interrupted,
                    "Marked stale web chat runs as interrupted"
                );
            }
        }

        // Initialize session store (may fail if vault is not available — that's OK in setup mode)
        let session_store = match auth::SessionStore::new(auth::DEFAULT_SESSION_TTL_SECS) {
            Ok(store) => {
                tracing::info!("Web session store initialized");
                Some(Arc::new(store))
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to initialize session store (auth disabled)");
                None
            }
        };

        let auth_rate_limiter = Arc::new(auth::RateLimiter::new(auth_rate_limit, 60));
        let api_rate_limiter = Arc::new(auth::RateLimiter::new(rate_limit, 60));

        let state = Arc::new(AppState {
            config: self.config,
            started_at: Instant::now(),
            inbound_tx: self.inbound_tx,
            web_runs: Arc::new(WebRunStore::default()),
            ws_sessions: tokio::sync::RwLock::new(std::collections::HashMap::new()),
            stream_sessions: tokio::sync::RwLock::new(std::collections::HashMap::new()),
            db: self.db,
            #[cfg(feature = "local-embeddings")]
            memory_searcher: self.memory_searcher,
            #[cfg(feature = "local-embeddings")]
            rag_engine: self.rag_engine,
            health_tracker: self.health_tracker,
            workflow_engine: self.workflow_engine,
            business_engine: self.business_engine,
            estop_handles: self.estop_handles,
            session_store: session_store.clone(),
            auth_rate_limiter: auth_rate_limiter.clone(),
            api_rate_limiter: api_rate_limiter.clone(),
        });

        // If we have outbound messages, spawn task to route them to WebSocket sessions
        if let Some(mut outbound_rx) = self.outbound_rx {
            let state_for_outbound = state.clone();
            tokio::spawn(async move {
                while let Some(msg) = outbound_rx.recv().await {
                    if msg.channel == "web" {
                        let session_key = format!("web:{}", msg.chat_id);
                        if let Some(run) = state_for_outbound
                            .web_runs
                            .complete_run(&session_key, &msg.content)
                        {
                            if let Some(db) = state_for_outbound.db.as_ref() {
                                if let Err(error) = db.upsert_web_chat_run(&run).await {
                                    tracing::error!(run_id = %run.run_id, %error, "Failed to persist completed web chat run");
                                }
                            }
                        }
                    }
                    let sessions = state_for_outbound.ws_sessions.read().await;
                    if let Some(tx) = sessions.get(&msg.chat_id) {
                        if tx.send(msg.content).await.is_err() {
                            tracing::warn!(chat_id = %msg.chat_id, "WebSocket session closed");
                        }
                    } else {
                        tracing::debug!(
                            chat_id = %msg.chat_id,
                            "No WebSocket session found for outbound message"
                        );
                    }
                }
            });
        }

        // If we have stream messages, spawn task to forward chunks to WebSocket stream sessions
        if let Some(mut stream_rx) = self.stream_rx {
            let state_for_stream = state.clone();
            tokio::spawn(async move {
                while let Some(msg) = stream_rx.recv().await {
                    if !msg.chat_id.is_empty() {
                        let session_key = format!("web:{}", msg.chat_id);
                        if let Some(run) = state_for_stream
                            .web_runs
                            .append_stream_message(&session_key, &msg)
                        {
                            if let Some(db) = state_for_stream.db.as_ref() {
                                if let Err(error) = db.upsert_web_chat_run(&run).await {
                                    tracing::error!(run_id = %run.run_id, %error, "Failed to persist streaming web chat run");
                                }
                            }
                        }
                    }
                    let streams = state_for_stream.stream_sessions.read().await;
                    if let Some(tx) = streams.get(&msg.chat_id) {
                        let event = super::ws::WsStreamEvent {
                            delta: msg.delta,
                            event_type: msg.event_type,
                            tool_call_data: msg.tool_call_data,
                        };
                        if tx.send(event).await.is_err() {
                            tracing::debug!(chat_id = %msg.chat_id, "Stream session closed");
                        }
                    }
                }
            });
        }

        // Spawn session + rate limiter cleanup task (every 5 minutes)
        {
            let session_store_clone = session_store.clone();
            let auth_rl = auth_rate_limiter.clone();
            let api_rl = api_rate_limiter.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(300));
                loop {
                    interval.tick().await;
                    if let Some(ref store) = session_store_clone {
                        store.cleanup_expired().await;
                    }
                    auth_rl.cleanup().await;
                    api_rl.cleanup().await;
                }
            });
        }

        // ─── Router: public vs protected ────────────────────────────

        // Auth routes with strict rate limiting (SEC-3: 5 req/min)
        let auth_routes = Router::new()
            .route("/api/auth/login", axum::routing::post(auth::login_handler))
            .route("/api/auth/setup", axum::routing::post(auth::setup_handler))
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                auth::auth_rate_limit_middleware,
            ));

        // Public routes — no auth required
        let public = Router::new()
            .route("/login", axum::routing::get(pages::login_page))
            .route(
                "/setup-wizard",
                axum::routing::get(pages::setup_wizard_page),
            )
            .route("/api/health", axum::routing::get(api::health))
            .route(
                "/api/auth/logout",
                axum::routing::post(auth::logout_handler),
            )
            .route(
                "/api/v1/webhook/{token}",
                axum::routing::post(api::webhook_ingress),
            )
            .merge(static_assets())
            .merge(auth_routes);

        // Protected routes — require auth (SEC-1 middleware + SEC-3 API rate limit)
        let protected = Router::new()
            .merge(pages::router())
            .nest("/api", api::router())
            .merge(ws::router())
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                auth::api_rate_limit_middleware,
            ))
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                auth::auth_middleware,
            ));

        let app = Router::new()
            .merge(public)
            .merge(protected)
            .layer(TraceLayer::new_for_http())
            .layer(CorsLayer::permissive())
            .with_state(state);

        let addr: SocketAddr = format!("{host}:{port}")
            .parse()
            .unwrap_or_else(|_| SocketAddr::from(([127, 0, 0, 1], 18443)));

        // Try to set up TLS (SEC-2)
        let tls_config = build_tls_config(&tls_cert, &tls_key, auto_tls, &domain).await;

        let listener = tokio::net::TcpListener::bind(addr).await?;

        if let Some(tls_cfg) = tls_config {
            // One-shot system setup: hosts entry + cert trust + port forward
            // All privileged operations are batched into a single admin prompt.
            if !domain.is_empty() {
                let cert_path = if auto_tls && tls_cert.is_empty() {
                    Some(
                        dirs::home_dir()
                            .unwrap_or_default()
                            .join(".homun/tls/cert.pem"),
                    )
                } else {
                    None
                };
                let forward_port = if port != 443 { Some(port) } else { None };
                setup_system(&domain, &host, cert_path.as_deref(), forward_port);
                tracing::info!(%addr, url = %format!("https://{domain}"), "Web UI starting (HTTPS)");
            } else {
                tracing::info!(%addr, "Web UI starting (HTTPS)");
            }
            let acceptor = tokio_rustls::TlsAcceptor::from(tls_cfg);
            let make_service = app.into_make_service_with_connect_info::<SocketAddr>();

            // Manual accept loop for TLS
            loop {
                let (stream, remote_addr) = listener.accept().await?;
                let acceptor = acceptor.clone();
                let mut make_service = make_service.clone();
                tokio::spawn(async move {
                    match acceptor.accept(stream).await {
                        Ok(tls_stream) => {
                            use tower::Service;
                            let io = hyper_util::rt::TokioIo::new(tls_stream);
                            let Ok(svc) =
                                tower::Service::<SocketAddr>::call(
                                    &mut make_service,
                                    remote_addr,
                                )
                                .await;
                            let hyper_svc =
                                hyper_util::service::TowerToHyperService::new(svc);
                            let _ = hyper_util::server::conn::auto::Builder::new(
                                hyper_util::rt::TokioExecutor::new(),
                            )
                            .serve_connection(io, hyper_svc)
                            .await;
                        }
                        Err(e) => {
                            tracing::debug!(error = %e, "TLS handshake failed");
                        }
                    }
                });
            }
        } else {
            tracing::info!(%addr, "Web UI starting (HTTP)");
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await?;
        }

        Ok(())
    }
}

/// Build a rustls ServerConfig from cert/key paths, or generate self-signed if `auto_tls` is set.
/// Returns `None` if TLS is not configured.
async fn build_tls_config(
    tls_cert: &str,
    tls_key: &str,
    auto_tls: bool,
    domain: &str,
) -> Option<Arc<rustls::ServerConfig>> {
    // Ensure ring CryptoProvider is installed
    let _ = rustls::crypto::ring::default_provider().install_default();

    let (cert_path, key_path) = if !tls_cert.is_empty() && !tls_key.is_empty() {
        // User-provided cert/key
        (std::path::PathBuf::from(tls_cert), std::path::PathBuf::from(tls_key))
    } else if auto_tls {
        // Auto-generate self-signed cert
        let tls_dir = dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".homun")
            .join("tls");
        let cert_path = tls_dir.join("cert.pem");
        let key_path = tls_dir.join("key.pem");

        // Only generate if files don't already exist
        if !cert_path.exists() || !key_path.exists() {
            // Collect extra domains for the certificate
            let extra_domains: Vec<&str> = if domain.is_empty() {
                vec![]
            } else {
                vec![domain]
            };
            if let Err(e) = generate_self_signed(&cert_path, &key_path, &extra_domains) {
                tracing::error!(error = %e, "Failed to generate self-signed TLS certificate");
                return None;
            }
            tracing::info!(cert = %cert_path.display(), "Generated self-signed TLS certificate");
        } else {
            tracing::info!(cert = %cert_path.display(), "Using existing self-signed TLS certificate");
        }
        (cert_path, key_path)
    } else {
        return None;
    };

    // Load cert chain
    let cert_data = match std::fs::read(&cert_path) {
        Ok(d) => d,
        Err(e) => {
            tracing::error!(path = %cert_path.display(), error = %e, "Failed to read TLS cert");
            return None;
        }
    };
    let certs: Vec<_> = rustls_pemfile::certs(&mut cert_data.as_slice())
        .filter_map(|r| r.ok())
        .collect();
    if certs.is_empty() {
        tracing::error!("No valid certificates found in PEM file");
        return None;
    }

    // Load private key
    let key_data = match std::fs::read(&key_path) {
        Ok(d) => d,
        Err(e) => {
            tracing::error!(path = %key_path.display(), error = %e, "Failed to read TLS key");
            return None;
        }
    };
    let key = match rustls_pemfile::private_key(&mut key_data.as_slice()) {
        Ok(Some(k)) => k,
        Ok(None) => {
            tracing::error!("No private key found in PEM file");
            return None;
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to parse TLS private key");
            return None;
        }
    };

    // Build ServerConfig
    match rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
    {
        Ok(config) => Some(Arc::new(config)),
        Err(e) => {
            tracing::error!(error = %e, "Failed to build TLS config");
            None
        }
    }
}

/// Generate a self-signed TLS certificate for localhost and optional extra domains.
fn generate_self_signed(cert_path: &Path, key_path: &Path, extra_domains: &[&str]) -> Result<()> {
    use std::io::Write;

    // Ensure parent directory exists
    if let Some(parent) = cert_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Build SAN list: localhost + any extra domains (e.g., "ui.homun.bot")
    let mut dns_names = vec!["localhost".to_string()];
    for domain in extra_domains {
        if !domain.is_empty() && *domain != "localhost" {
            dns_names.push(domain.to_string());
        }
    }

    let mut params = rcgen::CertificateParams::new(dns_names)?;
    params.distinguished_name = rcgen::DistinguishedName::new();
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, rcgen::DnValue::Utf8String("Homun Self-Signed".into()));
    // Add IP SANs for localhost
    params
        .subject_alt_names
        .push(rcgen::SanType::IpAddress(std::net::IpAddr::V4(
            std::net::Ipv4Addr::new(127, 0, 0, 1),
        )));
    params
        .subject_alt_names
        .push(rcgen::SanType::IpAddress(std::net::IpAddr::V6(
            std::net::Ipv6Addr::LOCALHOST,
        )));
    // Valid for 10 years
    params.not_after = rcgen::date_time_ymd(2036, 1, 1);

    let key_pair = rcgen::KeyPair::generate()?;
    let cert = params.self_signed(&key_pair)?;

    // Write cert PEM
    let mut cert_file = std::fs::File::create(cert_path)?;
    cert_file.write_all(cert.pem().as_bytes())?;

    // Write key PEM
    let mut key_file = std::fs::File::create(key_path)?;
    key_file.write_all(key_pair.serialize_pem().as_bytes())?;

    // Set restrictive permissions on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(key_path, std::fs::Permissions::from_mode(0o600))?;
        std::fs::set_permissions(cert_path, std::fs::Permissions::from_mode(0o600))?;
    }

    Ok(())
}

/// One-shot system configuration: hosts entry + cert trust + port forward.
/// All privileged operations are batched into a **single** admin prompt.
///
/// `cert_path`: if `Some`, trust the self-signed cert (first-time only).
/// `forward_port`: if `Some(actual_port)`, redirect 443 → actual_port.
fn setup_system(domain: &str, host: &str, cert_path: Option<&Path>, forward_port: Option<u16>) {
    let hosts_path = if cfg!(windows) {
        r"C:\Windows\System32\drivers\etc\hosts"
    } else {
        "/etc/hosts"
    };

    // ── Check what actually needs to be done ──────────────────────────
    let needs_hosts = !std::fs::read_to_string(hosts_path)
        .map(|c| c.contains(domain))
        .unwrap_or(false);

    let marker_path = dirs::home_dir()
        .unwrap_or_default()
        .join(".homun/tls/.trusted");
    let needs_cert_trust = cert_path.is_some() && !marker_path.exists();

    let needs_port_forward = forward_port.map_or(false, |actual_port| {
        if cfg!(target_os = "macos") {
            // Check pfctl NAT rules
            !std::process::Command::new("pfctl")
                .args(["-s", "nat"])
                .output()
                .map(|o| {
                    String::from_utf8_lossy(&o.stdout)
                        .contains(&format!("port {actual_port}"))
                })
                .unwrap_or(false)
        } else {
            true // Assume needed on Linux/Windows (iptables -C will check inside)
        }
    });

    if !needs_hosts && !needs_cert_trust && !needs_port_forward {
        tracing::debug!(domain, "System already configured");
        return;
    }

    // ── Build a single script with all operations ────────────────────
    let mut commands: Vec<String> = Vec::new();

    if needs_hosts {
        commands.push(format!("echo '{host}\t{domain}' >> {hosts_path}"));
    }

    if needs_cert_trust {
        if let Some(cert) = cert_path {
            let cert_str = cert.to_string_lossy();
            if cfg!(target_os = "macos") {
                commands.push(format!(
                    "security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain {cert_str}"
                ));
            } else if cfg!(target_os = "linux") {
                commands.push(format!(
                    "cp {cert_str} /usr/local/share/ca-certificates/homun-self-signed.crt && update-ca-certificates"
                ));
            } else if cfg!(windows) {
                commands.push(format!(
                    "certutil -addstore -f ROOT {cert_str}"
                ));
            }
        }
    }

    if let Some(actual_port) = forward_port {
        if needs_port_forward {
            if cfg!(target_os = "macos") {
                // Write pf config file first (unprivileged)
                let pf_conf_path = dirs::home_dir()
                    .unwrap_or_default()
                    .join(".homun/pf-homun.conf");
                let pf_rule = format!(
                    "rdr pass on lo0 inet proto tcp from any to {host} port 443 -> {host} port {actual_port}"
                );
                if let Some(parent) = pf_conf_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::write(&pf_conf_path, &pf_rule);
                let pf_path = pf_conf_path.to_string_lossy();
                commands.push(format!(
                    "pfctl -ef {pf_path} 2>/dev/null || pfctl -f {pf_path} -e 2>/dev/null; true"
                ));
            } else if cfg!(target_os = "linux") {
                commands.push(format!(
                    "iptables -t nat -C OUTPUT -p tcp -d {host} --dport 443 -j REDIRECT --to-port {actual_port} 2>/dev/null || \
                     iptables -t nat -A OUTPUT -p tcp -d {host} --dport 443 -j REDIRECT --to-port {actual_port}"
                ));
            } else if cfg!(windows) {
                commands.push(format!(
                    "netsh interface portproxy add v4tov4 listenport=443 listenaddress={host} connectport={actual_port} connectaddress={host}"
                ));
            }
        }
    }

    if commands.is_empty() {
        return;
    }

    let combined = commands.join(" && ");

    // ── Execute with a single privilege escalation ───────────────────
    let success = if cfg!(target_os = "macos") {
        let script = format!(
            r#"do shell script "{combined}" with administrator privileges"#
        );
        std::process::Command::new("osascript")
            .args(["-e", &script])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    } else if cfg!(target_os = "linux") {
        // Try pkexec (GUI dialog), fall back to sudo (terminal)
        std::process::Command::new("pkexec")
            .args(["sh", "-c", &combined])
            .status()
            .map(|s| s.success())
            .unwrap_or_else(|_| {
                std::process::Command::new("sudo")
                    .args(["sh", "-c", &combined])
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false)
            })
    } else if cfg!(windows) {
        // Windows: PowerShell with UAC elevation
        let ps_cmd = format!(
            "Start-Process cmd -ArgumentList '/c {combined}' -Verb RunAs -Wait"
        );
        std::process::Command::new("powershell")
            .args(["-Command", &ps_cmd])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    } else {
        false
    };

    if success {
        // Write marker so we don't re-trust the cert on next boot
        if needs_cert_trust {
            let _ = std::fs::write(&marker_path, "");
        }
        let ops: Vec<&str> = [
            if needs_hosts { Some("hosts") } else { None },
            if needs_cert_trust { Some("cert-trust") } else { None },
            if needs_port_forward { Some("port-forward") } else { None },
        ]
        .into_iter()
        .flatten()
        .collect();
        tracing::info!(domain, operations = ?ops, "System configured");
    } else {
        tracing::warn!(
            domain,
            "Could not configure system (admin prompt declined?) — \
             https://{domain} may not work, try https://{host}:{} directly",
            forward_port.unwrap_or(18443)
        );
    }
}

/// Route for embedded static assets (CSS, JS, images)
fn static_assets() -> Router<Arc<AppState>> {
    use axum::extract::Path;
    use axum::http::{header, StatusCode};
    use axum::response::IntoResponse;

    #[cfg(debug_assertions)]
    async fn serve_static(Path(path): Path<String>) -> impl IntoResponse {
        // In debug mode, serve from filesystem for hot reload
        let static_path = std::path::Path::new("static").join(&path);

        match tokio::fs::read(&static_path).await {
            Ok(content) => {
                let mime = mime_guess::from_path(&path).first_or_octet_stream();
                (
                    StatusCode::OK,
                    [(header::CONTENT_TYPE, mime.as_ref().to_string())],
                    content,
                )
                    .into_response()
            }
            Err(_) => {
                tracing::warn!(path = %static_path.display(), "Static file not found");
                StatusCode::NOT_FOUND.into_response()
            }
        }
    }

    #[cfg(not(debug_assertions))]
    async fn serve_static(Path(path): Path<String>) -> impl IntoResponse {
        use rust_embed::Embed;

        #[derive(Embed)]
        #[folder = "static/"]
        struct StaticAssets;

        match StaticAssets::get(&path) {
            Some(content) => {
                let mime = mime_guess::from_path(&path).first_or_octet_stream();
                (
                    StatusCode::OK,
                    [(header::CONTENT_TYPE, mime.as_ref().to_string())],
                    content.data.to_vec(),
                )
                    .into_response()
            }
            None => StatusCode::NOT_FOUND.into_response(),
        }
    }

    Router::new().route("/static/{*path}", axum::routing::get(serve_static))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn install_crypto_provider() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }

    #[test]
    fn test_self_signed_cert_generation() {
        install_crypto_provider();
        let dir = tempfile::tempdir().unwrap();
        let cert_path = dir.path().join("cert.pem");
        let key_path = dir.path().join("key.pem");

        generate_self_signed(&cert_path, &key_path, &["ui.homun.bot"]).unwrap();

        // Verify files exist and contain valid PEM
        let cert_data = std::fs::read_to_string(&cert_path).unwrap();
        let key_data = std::fs::read_to_string(&key_path).unwrap();

        assert!(cert_data.contains("BEGIN CERTIFICATE"));
        assert!(cert_data.contains("END CERTIFICATE"));
        assert!(key_data.contains("BEGIN PRIVATE KEY"));
        assert!(key_data.contains("END PRIVATE KEY"));

        // Verify rustls can parse them
        let certs: Vec<_> = rustls_pemfile::certs(&mut cert_data.as_bytes())
            .filter_map(|r| r.ok())
            .collect();
        assert_eq!(certs.len(), 1, "Should produce exactly one certificate");

        let key = rustls_pemfile::private_key(&mut key_data.as_bytes())
            .unwrap()
            .expect("Should contain a private key");

        // Verify we can build a valid ServerConfig
        let config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key);
        assert!(config.is_ok(), "Certificate and key should form valid TLS config");
    }

    #[test]
    fn test_self_signed_cert_with_custom_domain() {
        install_crypto_provider();
        let dir = tempfile::tempdir().unwrap();
        let cert_path = dir.path().join("cert.pem");
        let key_path = dir.path().join("key.pem");

        generate_self_signed(&cert_path, &key_path, &["ui.homun.bot", "my.custom.dev"]).unwrap();

        let cert_data = std::fs::read(&cert_path).unwrap();
        let certs: Vec<_> = rustls_pemfile::certs(&mut cert_data.as_slice())
            .filter_map(|r| r.ok())
            .collect();
        assert_eq!(certs.len(), 1);

        // Parse the DER certificate to verify SANs
        // The cert should be valid for localhost, ui.homun.bot, and my.custom.dev
        // We just verify it parses and builds a valid TLS config
        let key_data = std::fs::read(&key_path).unwrap();
        let key = rustls_pemfile::private_key(&mut key_data.as_slice())
            .unwrap()
            .unwrap();
        let config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key);
        assert!(config.is_ok(), "Multi-domain cert should be valid");
    }

    #[test]
    fn test_self_signed_cert_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let cert_path = dir.path().join("cert.pem");
        let key_path = dir.path().join("key.pem");

        generate_self_signed(&cert_path, &key_path, &[]).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let key_perms = std::fs::metadata(&key_path).unwrap().permissions().mode();
            // Check that only owner has permissions (mode & 0o077 == 0)
            assert_eq!(key_perms & 0o077, 0, "Key file should have 0600 permissions");
        }
    }

    #[tokio::test]
    async fn test_build_tls_config_no_tls() {
        let result = build_tls_config("", "", false, "").await;
        assert!(result.is_none(), "No TLS config when not configured");
    }

    #[tokio::test]
    async fn test_build_tls_config_auto_tls() {
        install_crypto_provider();
        let dir = tempfile::tempdir().unwrap();
        let cert_path = dir.path().join("cert.pem");
        let key_path = dir.path().join("key.pem");

        // Pre-generate certs in the temp dir
        generate_self_signed(&cert_path, &key_path, &["ui.homun.bot"]).unwrap();

        let result = build_tls_config(
            cert_path.to_str().unwrap(),
            key_path.to_str().unwrap(),
            false,
            "ui.homun.bot",
        )
        .await;
        assert!(result.is_some(), "Should produce valid TLS config from provided cert/key");
    }
}
