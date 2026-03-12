#[cfg(feature = "browser")]
mod inner {
    use std::sync::Arc;

    use axum::extract::State;
    use axum::response::Json;
    use axum::routing::post;
    use axum::Router;
    use serde::Serialize;

    use crate::web::server::AppState;

    #[derive(Serialize)]
    struct BrowserTestResponse {
        success: bool,
        message: String,
    }

    /// Test if browser can be launched
    async fn test_browser(State(state): State<Arc<AppState>>) -> Json<BrowserTestResponse> {
        let config = state.config.read().await;
        let status = config.browser.runtime_status();
        if !status.available {
            return Json(BrowserTestResponse {
                success: false,
                message: status.reason.unwrap_or_else(|| {
                    "Browser automation is unavailable in the current configuration".to_string()
                }),
            });
        }

        // With MCP-based browser, the Playwright server starts on demand when the agent
        // first calls a browser tool. We just confirm prerequisites are met.
        let exe_info = status
            .executable_path
            .map(|p| format!(" (Chrome: {})", p))
            .unwrap_or_default();
        Json(BrowserTestResponse {
            success: true,
            message: format!(
                "Browser prerequisites OK. MCP server (@playwright/mcp) will start on first use{}.",
                exe_info
            ),
        })
    }

    pub(crate) fn routes() -> Router<Arc<AppState>> {
        Router::new().route("/v1/browser/test", post(test_browser))
    }
}

#[cfg(feature = "browser")]
pub(super) use inner::routes;
