//! Connection Recipes — simplified MCP onboarding.
//!
//! A `ConnectionRecipe` is a self-contained TOML definition that transforms
//! "configure an MCP server" into "connect a service". Recipes sit on top of
//! the existing MCP infrastructure ([`crate::mcp_setup`]) without replacing it.

pub mod connect;
pub mod recipes;

use serde::{Deserialize, Serialize};

// ── Recipe types (parsed from TOML) ──────────────────────────────────

/// A connection recipe — one TOML file per service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionRecipe {
    pub id: String,
    pub display_name: String,
    pub subtitle: String,
    /// Icon identifier (used by frontend for rendering).
    pub icon: String,
    /// Logical category for filtering (e.g. "Developer", "Communication").
    pub category: String,
    /// Auth mode hint for the UI: "api_key", "oauth", "manual".
    pub auth_mode: String,
    /// One-line intro shown in the catalog card.
    pub capability_intro: String,
    /// Human-facing credential fields.
    #[serde(default)]
    pub fields: Vec<RecipeField>,
    /// MCP server configuration.
    pub mcp: RecipeMcpConfig,
    /// Copy shown after successful connection.
    pub success: SuccessCopy,
}

/// A single credential / config field in the recipe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeField {
    /// Machine id (e.g. "personal_access_token").
    pub id: String,
    /// Human-facing label (e.g. "GitHub Token").
    pub label: String,
    /// Help text shown below the field.
    #[serde(default)]
    pub help: String,
    /// Whether the value is sensitive and should be stored in vault.
    #[serde(default)]
    pub secret: bool,
    /// Whether the field is required.
    #[serde(default = "default_true")]
    pub required: bool,
    /// HTML input type hint: "text", "password", "url".
    #[serde(default = "default_text")]
    pub input: String,
    /// Hint on where to obtain this value (e.g. link to settings page).
    #[serde(default)]
    pub source_hint: String,
    /// The MCP env var this field maps to (1:1 mapping).
    pub env_key: String,
}

/// MCP server configuration embedded in the recipe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeMcpConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default = "default_stdio")]
    pub transport: String,
}

/// Success screen copy after a service is connected.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuccessCopy {
    pub title: String,
    pub body: String,
}

// ── Runtime types ────────────────────────────────────────────────────

/// Connection status for a recipe.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum ConnectionStatus {
    #[serde(rename = "not_connected")]
    NotConnected,
    #[serde(rename = "connected")]
    Connected { tool_count: usize },
    #[serde(rename = "error")]
    Error { message: String },
}

/// A recipe bundled with its live connection status (for API responses).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionCatalogItem {
    #[serde(flatten)]
    pub recipe: ConnectionRecipe,
    pub connection_status: ConnectionStatus,
}

// ── Helpers ──────────────────────────────────────────────────────────

fn default_true() -> bool {
    true
}

fn default_text() -> String {
    "text".to_string()
}

fn default_stdio() -> String {
    "stdio".to_string()
}
