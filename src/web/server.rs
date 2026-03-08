use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use axum::Router;
use tokio::sync::mpsc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::bus::{InboundMessage, OutboundMessage, StreamMessage};
use crate::config::Config;
use crate::storage::Database;

use super::api;
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
        }
    }

    /// Start the web server. Runs until the server is shut down.
    pub async fn start(self) -> Result<()> {
        let (host, port) = {
            let cfg = self.config.read().await;
            (cfg.channels.web.host.clone(), cfg.channels.web.port)
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

        let app = Router::new()
            // Pages (HTML)
            .merge(pages::router())
            // API endpoints
            .nest("/api", api::router())
            // WebSocket
            .merge(ws::router())
            // Static assets
            .merge(static_assets())
            .layer(TraceLayer::new_for_http())
            .layer(CorsLayer::permissive())
            .with_state(state);

        let addr: SocketAddr = format!("{host}:{port}")
            .parse()
            .unwrap_or_else(|_| SocketAddr::from(([0, 0, 0, 0], 18080)));

        tracing::info!(%addr, "Web UI starting");

        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app).await?;

        Ok(())
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
