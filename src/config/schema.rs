use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Root configuration — loaded from ~/.homun/config.toml
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub agent: AgentConfig,
    pub providers: ProvidersConfig,
    pub channels: ChannelsConfig,
    pub tools: ToolsConfig,
    pub storage: StorageConfig,
    pub mcp: McpConfig,
}

impl Config {
    /// Load config from the default path (~/.homun/config.toml)
    pub fn load() -> Result<Self> {
        let path = Self::default_path();
        if path.exists() {
            Self::load_from(&path)
        } else {
            tracing::warn!("Config file not found at {}, using defaults", path.display());
            Ok(Self::default())
        }
    }

    /// Load config from a specific path
    pub fn load_from(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config from {}", path.display()))?;
        let config: Config = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config from {}", path.display()))?;
        Ok(config)
    }

    /// Save config to the default path
    pub fn save(&self) -> Result<()> {
        let path = Self::default_path();
        self.save_to(&path)
    }

    /// Save config to a specific path
    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config directory {}", parent.display()))?;
        }
        let content = toml::to_string_pretty(self)
            .context("Failed to serialize config")?;
        std::fs::write(path, content)
            .with_context(|| format!("Failed to write config to {}", path.display()))?;
        Ok(())
    }

    /// Default config file path: ~/.homun/config.toml
    pub fn default_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".homun")
            .join("config.toml")
    }

    /// Data directory: ~/.homun/
    pub fn data_dir() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".homun")
    }

    /// Workspace directory: ~/.homun/workspace/
    pub fn workspace_dir() -> PathBuf {
        Self::data_dir().join("workspace")
    }

    /// Check if provider has credentials (API key in secure storage, in config, or base URL)
    pub fn is_provider_configured(&self, name: &str) -> bool {
        // Check secure storage for encrypted API key
        if let Ok(secrets) = crate::storage::global_secrets() {
            let key = crate::storage::SecretKey::provider_api_key(name);
            let result: std::result::Result<Option<String>, anyhow::Error> = secrets.get(&key);
            if matches!(result, Ok(Some(_))) {
                return true;
            }
        }

        // Check if provider has api_key in config (legacy plaintext or marker) or base URL
        if let Some(provider) = self.providers.get(name) {
            return !provider.api_key.is_empty() || provider.api_base.is_some();
        }

        false
    }

    /// Check if a channel is configured and ready to use.
    pub fn is_channel_configured(&self, name: &str) -> bool {
        match name {
            "telegram" => {
                // Check encrypted storage first, then config
                if let Ok(secrets) = crate::storage::global_secrets() {
                    let key = crate::storage::SecretKey::channel_token("telegram");
                    if matches!(secrets.get(&key), Ok(Some(_))) {
                        return true;
                    }
                }
                !self.channels.telegram.token.is_empty()
            }
            "discord" => {
                if let Ok(secrets) = crate::storage::global_secrets() {
                    let key = crate::storage::SecretKey::channel_token("discord");
                    if matches!(secrets.get(&key), Ok(Some(_))) {
                        return true;
                    }
                }
                !self.channels.discord.token.is_empty()
            }
            "whatsapp" => {
                // WhatsApp is "configured" if it has a phone number and the session DB exists
                !self.channels.whatsapp.phone_number.is_empty()
                    && self.channels.whatsapp.resolved_db_path().exists()
            }
            "web" => true, // Always configured
            _ => false,
        }
    }

    /// Resolve the provider config for a given model string.
    ///
    /// Matching priority:
    /// 1. Direct keyword match (model name contains provider keyword)
    /// 2. Gateway providers (OpenRouter, AiHubMix — route any model)
    /// 3. Local providers (Ollama, vLLM — no api_key needed)
    /// 4. Fallback: first provider with credentials
    pub fn resolve_provider(&self, model: &str) -> Option<(&str, &ProviderConfig)> {
        let m = model.to_lowercase();

        // --- 1. Direct keyword matching (ordered by specificity) ---
        let keyword_providers: &[(&[&str], &str, &ProviderConfig)] = &[
            (&["anthropic/", "claude"],           "anthropic",  &self.providers.anthropic),
            (&["openai/", "gpt"],                 "openai",     &self.providers.openai),
            (&["deepseek"],                       "deepseek",   &self.providers.deepseek),
            (&["groq/"],                          "groq",       &self.providers.groq),
            (&["gemini"],                         "gemini",     &self.providers.gemini),
            (&["minimax"],                        "minimax",    &self.providers.minimax),
            (&["dashscope/", "qwen"],             "dashscope",  &self.providers.dashscope),
            (&["moonshot", "kimi"],               "moonshot",   &self.providers.moonshot),
            (&["zhipu/", "glm"],                  "zhipu",      &self.providers.zhipu),
        ];

        for (keywords, name, config) in keyword_providers {
            if keywords.iter().any(|kw| m.contains(kw)) && self.is_provider_configured(name) {
                return Some((name, config));
            }
        }

        // --- 2. Local providers — explicit prefix always wins ---
        // These have unambiguous prefixes so they must match before gateways
        if m.starts_with("ollama/") {
            return Some(("ollama", &self.providers.ollama));
        }
        if m.starts_with("vllm/") {
            return Some(("vllm", &self.providers.vllm));
        }
        if m.starts_with("custom/") {
            return Some(("custom", &self.providers.custom));
        }

        // --- 3. Gateways (route any model) ---
        if self.is_provider_configured("openrouter") {
            return Some(("openrouter", &self.providers.openrouter));
        }
        if self.is_provider_configured("aihubmix") {
            return Some(("aihubmix", &self.providers.aihubmix));
        }

        // --- 4. Fallback: first provider with credentials ---
        for (name, provider) in self.providers.iter() {
            if self.is_provider_configured(name) {
                return Some((name, provider));
            }
        }

        None
    }
}

// --- Agent Config ---

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentConfig {
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f32,
    pub max_iterations: u32,
    pub memory_window: u32,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            model: "anthropic/claude-sonnet-4-20250514".to_string(),
            max_tokens: 8192,
            temperature: 0.7,
            max_iterations: 20,
            memory_window: 50,
        }
    }
}

// --- Provider Config ---

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderConfig {
    pub api_key: String,
    pub api_base: Option<String>,
    #[serde(default)]
    pub extra_headers: HashMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProvidersConfig {
    pub anthropic: ProviderConfig,
    pub openai: ProviderConfig,
    pub openrouter: ProviderConfig,
    pub ollama: ProviderConfig,
    pub deepseek: ProviderConfig,
    pub groq: ProviderConfig,
    pub gemini: ProviderConfig,
    pub minimax: ProviderConfig,
    pub aihubmix: ProviderConfig,
    pub dashscope: ProviderConfig,
    pub moonshot: ProviderConfig,
    pub zhipu: ProviderConfig,
    pub vllm: ProviderConfig,
    pub custom: ProviderConfig,
}

impl ProvidersConfig {
    /// Iterate over all providers as (name, config) pairs
    pub fn iter(&self) -> impl Iterator<Item = (&str, &ProviderConfig)> {
        [
            ("anthropic", &self.anthropic),
            ("openai", &self.openai),
            ("openrouter", &self.openrouter),
            ("ollama", &self.ollama),
            ("deepseek", &self.deepseek),
            ("groq", &self.groq),
            ("gemini", &self.gemini),
            ("minimax", &self.minimax),
            ("aihubmix", &self.aihubmix),
            ("dashscope", &self.dashscope),
            ("moonshot", &self.moonshot),
            ("zhipu", &self.zhipu),
            ("vllm", &self.vllm),
            ("custom", &self.custom),
        ]
        .into_iter()
    }

    /// Get a reference to a provider config by name
    pub fn get(&self, name: &str) -> Option<&ProviderConfig> {
        match name {
            "anthropic" => Some(&self.anthropic),
            "openai" => Some(&self.openai),
            "openrouter" => Some(&self.openrouter),
            "ollama" => Some(&self.ollama),
            "deepseek" => Some(&self.deepseek),
            "groq" => Some(&self.groq),
            "gemini" => Some(&self.gemini),
            "minimax" => Some(&self.minimax),
            "aihubmix" => Some(&self.aihubmix),
            "dashscope" => Some(&self.dashscope),
            "moonshot" => Some(&self.moonshot),
            "zhipu" => Some(&self.zhipu),
            "vllm" => Some(&self.vllm),
            "custom" => Some(&self.custom),
            _ => None,
        }
    }

    /// Get a mutable reference to a provider config by name
    pub fn get_mut(&mut self, name: &str) -> Option<&mut ProviderConfig> {
        match name {
            "anthropic" => Some(&mut self.anthropic),
            "openai" => Some(&mut self.openai),
            "openrouter" => Some(&mut self.openrouter),
            "ollama" => Some(&mut self.ollama),
            "deepseek" => Some(&mut self.deepseek),
            "groq" => Some(&mut self.groq),
            "gemini" => Some(&mut self.gemini),
            "minimax" => Some(&mut self.minimax),
            "aihubmix" => Some(&mut self.aihubmix),
            "dashscope" => Some(&mut self.dashscope),
            "moonshot" => Some(&mut self.moonshot),
            "zhipu" => Some(&mut self.zhipu),
            "vllm" => Some(&mut self.vllm),
            "custom" => Some(&mut self.custom),
            _ => None,
        }
    }

    /// List of all known provider names
    pub fn known_names() -> &'static [&'static str] {
        &[
            "anthropic", "openai", "openrouter", "ollama", "deepseek",
            "groq", "gemini", "minimax", "aihubmix", "dashscope",
            "moonshot", "zhipu", "vllm", "custom",
        ]
    }
}

// --- Channel Config ---

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TelegramConfig {
    pub enabled: bool,
    pub token: String,
    #[serde(default)]
    pub allow_from: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WhatsAppConfig {
    pub enabled: bool,
    /// Phone number for pair-code authentication (e.g. "393331234567").
    /// If empty, QR code pairing is used instead.
    pub phone_number: String,
    /// Path to the WhatsApp session SQLite database.
    /// Defaults to ~/.homun/whatsapp.db
    pub db_path: String,
    /// Skip processing history sync from phone (recommended for bots).
    pub skip_history_sync: bool,
    #[serde(default)]
    pub allow_from: Vec<String>,
}

impl Default for WhatsAppConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            phone_number: String::new(),
            db_path: "~/.homun/whatsapp.db".to_string(),
            skip_history_sync: true,
            allow_from: Vec::new(),
        }
    }
}

impl WhatsAppConfig {
    /// Resolve the WhatsApp database path, expanding ~ to home directory
    pub fn resolved_db_path(&self) -> std::path::PathBuf {
        if self.db_path.starts_with("~/") {
            dirs::home_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join(&self.db_path[2..])
        } else {
            std::path::PathBuf::from(&self.db_path)
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DiscordConfig {
    pub enabled: bool,
    pub token: String,
    #[serde(default)]
    pub allow_from: Vec<String>,
    /// Default channel ID for proactive/cross-channel messaging.
    /// Without this, Discord can only reply to incoming messages.
    #[serde(default)]
    pub default_channel_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WebConfig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    /// Optional auth token for remote access. Empty = no auth (localhost only).
    pub auth_token: String,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            host: "127.0.0.1".to_string(),
            port: 18080,
            auth_token: String::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ChannelsConfig {
    pub telegram: TelegramConfig,
    pub whatsapp: WhatsAppConfig,
    pub discord: DiscordConfig,
    pub web: WebConfig,
}

impl ChannelsConfig {
    /// Return a list of enabled channels with their default chat IDs.
    /// Used to inject cross-channel routing info into the agent's system prompt.
    pub fn active_channels_with_chat_ids(&self) -> Vec<(String, String)> {
        let mut channels = Vec::new();

        if self.telegram.enabled && !self.telegram.token.is_empty() {
            // Use the first allow_from user as the default chat_id
            if let Some(user_id) = self.telegram.allow_from.first() {
                channels.push(("telegram".to_string(), user_id.clone()));
            }
        }

        if self.whatsapp.enabled && !self.whatsapp.phone_number.is_empty() {
            // WhatsApp JID format: phone@s.whatsapp.net
            let jid = format!("{}@s.whatsapp.net", self.whatsapp.phone_number);
            channels.push(("whatsapp".to_string(), jid));
        }

        if self.discord.enabled && !self.discord.token.is_empty()
            && !self.discord.default_channel_id.is_empty()
        {
            channels.push(("discord".to_string(), self.discord.default_channel_id.clone()));
        }

        channels
    }
}

// --- Tools Config ---

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WebSearchConfig {
    pub provider: String,
    pub api_key: String,
    pub max_results: u32,
}

impl Default for WebSearchConfig {
    fn default() -> Self {
        Self {
            provider: "brave".to_string(),
            api_key: String::new(),
            max_results: 5,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ExecConfig {
    pub timeout: u64,
    pub restrict_to_workspace: bool,
}

impl Default for ExecConfig {
    fn default() -> Self {
        Self {
            timeout: 60,
            restrict_to_workspace: false,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ToolsConfig {
    pub web_search: WebSearchConfig,
    pub exec: ExecConfig,
}

// --- MCP Config ---

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct McpConfig {
    #[serde(default)]
    pub servers: HashMap<String, McpServerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// Transport type: "stdio" or "http"
    pub transport: String,
    /// For stdio: the command to run (e.g., "npx")
    pub command: Option<String>,
    /// For stdio: arguments to the command
    #[serde(default)]
    pub args: Vec<String>,
    /// For http: the server URL
    pub url: Option<String>,
    /// Environment variables to pass to the process
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Whether this server is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl Default for McpServerConfig {
    fn default() -> Self {
        Self {
            transport: "stdio".to_string(),
            command: None,
            args: Vec::new(),
            url: None,
            env: HashMap::new(),
            enabled: true,
        }
    }
}

// --- Storage Config ---

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StorageConfig {
    pub path: String,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            path: "~/.homun/homun.db".to_string(),
        }
    }
}

impl StorageConfig {
    /// Resolve the database path, expanding ~ to home directory
    pub fn resolved_path(&self) -> PathBuf {
        if self.path.starts_with("~/") {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(&self.path[2..])
        } else {
            PathBuf::from(&self.path)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.agent.model, "anthropic/claude-sonnet-4-20250514");
        assert_eq!(config.agent.max_iterations, 20);
        assert_eq!(config.agent.temperature, 0.7);
    }

    #[test]
    fn test_parse_minimal_toml() {
        let toml_str = r#"
[agent]
model = "openai/gpt-4"

[providers.openrouter]
api_key = "sk-or-test"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.agent.model, "openai/gpt-4");
        assert_eq!(config.providers.openrouter.api_key, "sk-or-test");
        // Defaults should be filled in
        assert_eq!(config.agent.max_tokens, 8192);
    }

    #[test]
    fn test_resolve_provider_anthropic() {
        let mut config = Config::default();
        config.providers.anthropic.api_key = "sk-ant-test".to_string();
        let (name, _) = config.resolve_provider("anthropic/claude-sonnet-4-20250514").unwrap();
        assert_eq!(name, "anthropic");
    }

    #[test]
    fn test_resolve_provider_openrouter_fallback() {
        let mut config = Config::default();
        config.providers.openrouter.api_key = "sk-or-test".to_string();
        let (name, _) = config.resolve_provider("some-unknown-model").unwrap();
        assert_eq!(name, "openrouter");
    }

    #[test]
    fn test_resolve_provider_ollama() {
        let mut config = Config::default();
        config.providers.ollama.api_base = Some("http://localhost:11434/v1".to_string());
        let (name, _) = config.resolve_provider("llama3").unwrap();
        assert_eq!(name, "ollama");
    }

    #[test]
    fn test_storage_path_expansion() {
        let storage = StorageConfig::default();
        let resolved = storage.resolved_path();
        assert!(resolved.to_string_lossy().contains(".homun/homun.db"));
        assert!(!resolved.to_string_lossy().starts_with("~"));
    }

    #[test]
    fn test_roundtrip_toml() {
        let config = Config::default();
        let serialized = toml::to_string_pretty(&config).unwrap();
        let deserialized: Config = toml::from_str(&serialized).unwrap();
        assert_eq!(config.agent.model, deserialized.agent.model);
    }
}
