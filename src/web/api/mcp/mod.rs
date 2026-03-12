pub(crate) mod catalog;
pub(crate) mod crud;
pub(crate) mod helpers;
pub(crate) mod install;
pub(crate) mod oauth;

use std::sync::Arc;

use axum::routing::get;
use axum::Router;
use serde::{Deserialize, Serialize};

use crate::web::server::AppState;

/// Combined MCP router — mounted by the parent API module.
pub(crate) fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/mcp/catalog", get(catalog::list_mcp_catalog))
        .route("/v1/mcp/suggest", get(catalog::suggest_mcp_catalog))
        .route("/v1/mcp/search", get(catalog::search_mcp_catalog))
        .route(
            "/v1/mcp/install-guide",
            axum::routing::post(install::mcp_install_guide),
        )
        .route(
            "/v1/mcp/oauth/google/start",
            axum::routing::post(oauth::start_google_mcp_oauth),
        )
        .route(
            "/v1/mcp/oauth/google/exchange",
            axum::routing::post(oauth::exchange_google_mcp_oauth_code),
        )
        .route(
            "/v1/mcp/oauth/github/start",
            axum::routing::post(oauth::start_github_mcp_oauth),
        )
        .route(
            "/v1/mcp/oauth/github/exchange",
            axum::routing::post(oauth::exchange_github_mcp_oauth_code),
        )
        .route(
            "/v1/mcp/servers",
            get(crud::list_mcp_servers).post(crud::upsert_mcp_server),
        )
        .route("/v1/mcp/setup", axum::routing::post(crud::setup_mcp_server))
        .route(
            "/v1/mcp/servers/{name}/toggle",
            axum::routing::post(crud::toggle_mcp_server),
        )
        .route(
            "/v1/mcp/servers/{name}/test",
            axum::routing::post(crud::test_mcp_server),
        )
        .route(
            "/v1/mcp/servers/{name}",
            axum::routing::delete(crud::delete_mcp_server),
        )
}

// ═══════════════════════════════════════════════════
// Shared types used across multiple MCP sub-modules
// ═══════════════════════════════════════════════════

#[derive(Clone, Serialize)]
pub(crate) struct McpCatalogEnvView {
    pub key: String,
    pub description: String,
    pub required: bool,
    pub secret: bool,
}

#[derive(Clone, Serialize)]
pub(crate) struct McpCatalogItemView {
    pub kind: String, // preset | npm | registry
    pub source: String,
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub command: String,
    pub args: Vec<String>,
    pub transport: Option<String>, // stdio | http
    pub url: Option<String>,       // set when transport=http
    pub install_supported: bool,
    pub package_name: Option<String>,
    pub downloads_monthly: Option<u64>,
    pub score: Option<f64>,
    pub popularity_rank: Option<u32>,
    pub popularity_value: Option<u64>,
    pub popularity_source: Option<String>,
    pub env: Vec<McpCatalogEnvView>,
    pub docs_url: Option<String>,
    pub aliases: Vec<String>,
    pub keywords: Vec<String>,
    pub recommended: bool,
    pub recommended_reason: Option<String>,
    pub decision_tags: Vec<String>,
    pub setup_effort: String,
    pub auth_profile: String,
    pub preflight_checks: Vec<String>,
    pub why_choose: Option<String>,
    pub tradeoff: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct McpLeaderboardEntry {
    pub rank: u32,
    pub popularity: u64,
    pub url: String,
}

#[derive(Debug)]
pub(crate) struct McpMarketItem {
    pub rank: u32,
    pub name: String,
    pub slug: String,
    pub popularity: u64,
    pub url: String,
}

#[derive(Deserialize)]
pub(crate) struct McpMarketFallback {
    pub items: Vec<McpMarketFallbackItem>,
}

#[derive(Deserialize)]
pub(crate) struct McpMarketFallbackItem {
    pub rank: u32,
    pub name: String,
    pub slug: String,
    pub popularity: u64,
    pub url: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct McpServerEnvView {
    pub key: String,
    pub value_preview: String,
    pub is_vault_ref: bool,
}

#[derive(Serialize)]
pub(crate) struct McpServerView {
    pub name: String,
    pub transport: String,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub url: Option<String>,
    pub capabilities: Vec<String>,
    pub enabled: bool,
    pub env: Vec<McpServerEnvView>,
}

#[derive(Deserialize)]
pub(crate) struct OfficialRegistryResponse {
    pub servers: Option<Vec<OfficialRegistryServerEntry>>,
}

#[derive(Deserialize)]
pub(crate) struct OfficialRegistryServerEntry {
    pub server: OfficialRegistryServer,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OfficialRegistryServer {
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub website_url: Option<String>,
    pub repository: Option<OfficialRegistryRepository>,
    pub packages: Option<Vec<OfficialRegistryPackage>>,
    pub remotes: Option<Vec<OfficialRegistryRemote>>,
}

#[derive(Deserialize)]
pub(crate) struct OfficialRegistryRepository {
    pub url: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OfficialRegistryPackage {
    pub registry_type: Option<String>,
    pub identifier: Option<String>,
    pub version: Option<String>,
    pub runtime_hint: Option<String>,
    pub environment_variables: Option<Vec<OfficialRegistryInput>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OfficialRegistryRemote {
    #[serde(rename = "type")]
    pub transport_type: Option<String>,
    pub url: Option<String>,
    pub headers: Option<Vec<OfficialRegistryInput>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OfficialRegistryInput {
    pub name: Option<String>,
    pub description: Option<String>,
    pub is_required: Option<bool>,
    pub is_secret: Option<bool>,
}

#[derive(Deserialize)]
pub(crate) struct NpmSearchResponse {
    pub objects: Vec<NpmSearchObject>,
}

#[derive(Deserialize)]
pub(crate) struct NpmSearchObject {
    pub package: NpmPackage,
    pub score: NpmScore,
    pub downloads: Option<NpmDownloads>,
}

#[derive(Deserialize)]
pub(crate) struct NpmPackage {
    pub name: String,
    pub description: Option<String>,
    pub links: Option<NpmLinks>,
}

#[derive(Deserialize)]
pub(crate) struct NpmLinks {
    pub npm: Option<String>,
    pub repository: Option<String>,
    pub homepage: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct NpmScore {
    #[serde(rename = "final")]
    pub final_score: f64,
}

#[derive(Deserialize)]
pub(crate) struct NpmDownloads {
    pub monthly: Option<u64>,
}
