mod account;
mod approvals;
mod automations;
mod browser;
mod business;
mod channels;
mod chat;
mod connections;
mod email_accounts;
mod health;
mod knowledge;
mod logs;
mod mcp;
mod memory;
mod permissions;
mod providers;
mod sandbox;
mod skills;
mod status;
mod usage;
mod vault;
mod workflows;

pub(crate) use chat::{cleanup_chat_upload_dirs, ChatUploadCleanupStats};
pub use health::{health, webhook_ingress};

use std::sync::Arc;

use axum::Router;
use serde::Serialize;

use super::server::AppState;

pub fn router() -> Router<Arc<AppState>> {
    let api_router = Router::new()
        // Note: /health and /v1/webhook/{token} are registered as public routes in server.rs
        .merge(logs::routes())
        .merge(status::routes())
        .merge(skills::routes())
        .merge(providers::routes())
        .merge(mcp::routes())
        // --- Channels ---
        .merge(channels::routes())
        // Note: /v1/webhook/{token} is registered as public route in server.rs
        // --- Account ---
        .merge(account::routes())
        // --- Memory ---
        .merge(memory::routes())
        // --- Chat ---
        .merge(chat::routes())
        // --- Vault + 2FA ---
        .merge(vault::routes())
        .merge(permissions::routes())
        .merge(sandbox::routes())
        // --- Approvals ---
        .merge(approvals::routes())
        // --- Email Accounts (multi-account) ---
        .merge(email_accounts::routes())
        // --- Connection Recipes ---
        .merge(connections::routes())
        .merge(automations::routes())
        // --- Usage ---
        .merge(usage::routes())
        .merge(workflows::routes())
        .merge(business::routes());

    // --- Knowledge Base (RAG) ---
    #[cfg(feature = "local-embeddings")]
    let api_router = api_router.merge(knowledge::routes());

    // --- Browser (optional) ---
    #[cfg(feature = "browser")]
    let api_router = api_router.merge(browser::routes());

    api_router.merge(health::routes())
}

#[derive(Serialize)]
struct OkResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}
