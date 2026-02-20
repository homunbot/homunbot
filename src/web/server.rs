use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use axum::Router;
use tokio::sync::mpsc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::bus::{InboundMessage, OutboundMessage};
use crate::config::Config;

use super::api;
use super::pages;
use super::ws;

/// Shared state accessible by all web handlers
pub struct AppState {
    pub config: tokio::sync::RwLock<Config>,
    pub started_at: Instant,
    pub inbound_tx: Option<mpsc::Sender<InboundMessage>>,
    /// Active WebSocket sessions: chat_id → sender for outbound messages
    pub ws_sessions: tokio::sync::RwLock<std::collections::HashMap<String, mpsc::Sender<String>>>,
}

impl AppState {
    /// Save config to disk AND update the in-memory copy atomically.
    pub async fn save_config(&self, config: Config) -> anyhow::Result<()> {
        config.save()?;
        *self.config.write().await = config;
        Ok(())
    }
}

/// Web server — embedded dashboard + REST API + WebSocket chat
pub struct WebServer {
    config: Config,
    inbound_tx: Option<mpsc::Sender<InboundMessage>>,
    outbound_rx: Option<mpsc::Receiver<OutboundMessage>>,
}

impl WebServer {
    pub fn new(
        config: Config,
        inbound_tx: mpsc::Sender<InboundMessage>,
        outbound_rx: mpsc::Receiver<OutboundMessage>,
    ) -> Self {
        Self {
            config,
            inbound_tx: Some(inbound_tx),
            outbound_rx: Some(outbound_rx),
        }
    }

    /// Create a setup-only server (no agent, just config UI)
    pub fn setup_only(config: Config) -> Self {
        Self {
            config,
            inbound_tx: None,
            outbound_rx: None,
        }
    }

    /// Start the web server. Runs until the server is shut down.
    pub async fn start(self) -> Result<()> {
        let host = self.config.channels.web.host.clone();
        let port = self.config.channels.web.port;

        let state = Arc::new(AppState {
            config: tokio::sync::RwLock::new(self.config),
            started_at: Instant::now(),
            inbound_tx: self.inbound_tx,
            ws_sessions: tokio::sync::RwLock::new(std::collections::HashMap::new()),
        });

        // If we have outbound messages, spawn task to route them to WebSocket sessions
        if let Some(mut outbound_rx) = self.outbound_rx {
            let state_for_outbound = state.clone();
            tokio::spawn(async move {
                while let Some(msg) = outbound_rx.recv().await {
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
    use rust_embed::Embed;

    #[derive(Embed)]
    #[folder = "static/"]
    struct StaticAssets;

    async fn serve_static(Path(path): Path<String>) -> impl IntoResponse {
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
