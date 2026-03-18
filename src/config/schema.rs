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
    pub memory: MemoryConfig,
    pub knowledge: KnowledgeConfig,
    pub mcp: McpConfig,
    pub permissions: PermissionsConfig,
    pub security: SecurityConfig,
    pub browser: BrowserConfig,
    pub ui: UiConfig,
    pub business: BusinessConfig,
    pub skills: SkillsConfig,
}

impl Config {
    /// Load config from the default path (~/.homun/config.toml)
    pub fn load() -> Result<Self> {
        let path = Self::default_path();
        if path.exists() {
            let mut config = Self::load_from(&path)?;
            if config.maybe_migrate_legacy_browser_defaults() {
                config.save_to(&path)?;
            }
            Ok(config)
        } else {
            tracing::warn!(
                "Config file not found at {}, using defaults",
                path.display()
            );
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

    /// Save config to a specific path.
    ///
    /// Strips auto-injected virtual MCP servers (e.g. the browser MCP server
    /// generated from `[browser]` config) so they are not persisted to disk.
    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create config directory {}", parent.display())
            })?;
        }
        // Clone to strip virtual servers before serialising.
        // The browser MCP server is auto-injected from [browser] config at startup
        // and must not be persisted to disk.
        let mut snapshot = self.clone();
        snapshot.mcp.servers.remove("playwright");
        let content = toml::to_string_pretty(&snapshot).context("Failed to serialize config")?;
        std::fs::write(path, content)
            .with_context(|| format!("Failed to write config to {}", path.display()))?;
        Ok(())
    }

    fn maybe_migrate_legacy_browser_defaults(&mut self) -> bool {
        self.browser.maybe_auto_enable_for_legacy_config()
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

    /// Determine if XML tool dispatch should be used for a given provider/model.
    ///
    /// Priority:
    /// 1. Provider-specific `force_xml_tools` setting (if set)
    /// 2. Global `agent.force_xml_tools` setting
    /// 3. Auto-detect: Ollama provider defaults to XML dispatch
    pub fn should_use_xml_dispatch(&self, provider_name: &str, model: &str) -> bool {
        // 1. Check provider-specific setting
        if let Some(provider) = self.providers.get(provider_name) {
            if let Some(force) = provider.force_xml_tools {
                return force;
            }
        }

        // 2. Check global setting
        if self.agent.force_xml_tools {
            return true;
        }

        // 3. Per-model capability override
        if let Some(overrides) = self.agent.model_overrides.get(model) {
            if let Some(tool_calls) = overrides.tool_calls {
                return !tool_calls;
            }
        }

        // 4. Auto-detect from capabilities: models without verified tool support use XML
        if !crate::provider::capabilities::supports_tool_calls(provider_name, model) {
            return true;
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
            "slack" => {
                if let Ok(secrets) = crate::storage::global_secrets() {
                    let key = crate::storage::SecretKey::channel_token("slack");
                    if matches!(secrets.get(&key), Ok(Some(_))) {
                        return true;
                    }
                }
                !self.channels.slack.token.is_empty()
            }
            "whatsapp" => {
                // WhatsApp is "configured" if it has a phone number and the session DB exists
                !self.channels.whatsapp.phone_number.is_empty()
                    && self.channels.whatsapp.resolved_db_path().exists()
            }
            "email" => {
                // Check vault for password, then config
                let has_password = if let Ok(secrets) = crate::storage::global_secrets() {
                    let key = crate::storage::SecretKey::channel_token("email");
                    matches!(secrets.get(&key), Ok(Some(_)))
                } else {
                    false
                };
                self.channels.email.is_configured()
                    || (has_password && !self.channels.email.imap_host.is_empty())
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
            (
                &["anthropic/", "claude"],
                "anthropic",
                &self.providers.anthropic,
            ),
            (
                &["openai/", "gpt-", "o1-", "o3-"],
                "openai",
                &self.providers.openai,
            ),
            (
                &["mistral/", "mixtral", "codestral"],
                "mistral",
                &self.providers.mistral,
            ),
            (&["deepseek"], "deepseek", &self.providers.deepseek),
            (&["groq/"], "groq", &self.providers.groq),
            (&["gemini"], "gemini", &self.providers.gemini),
            (&["xai/", "grok"], "xai", &self.providers.xai),
            (&["together/"], "together", &self.providers.together),
            (&["fireworks/"], "fireworks", &self.providers.fireworks),
            (
                &["perplexity/", "sonar"],
                "perplexity",
                &self.providers.perplexity,
            ),
            (&["cohere/", "command"], "cohere", &self.providers.cohere),
            (&["venice/"], "venice", &self.providers.venice),
            (&["minimax"], "minimax", &self.providers.minimax),
            (
                &["dashscope/", "qwen"],
                "dashscope",
                &self.providers.dashscope,
            ),
            (&["moonshot", "kimi"], "moonshot", &self.providers.moonshot),
            (&["zhipu/", "glm"], "zhipu", &self.providers.zhipu),
        ];

        for (keywords, name, config) in keyword_providers {
            if keywords.iter().any(|kw| m.contains(kw)) && self.is_provider_configured(name) {
                return Some((name, config));
            }
        }

        // --- 2. Local/cloud providers — explicit prefix always wins ---
        // These have unambiguous prefixes so they must match before gateways
        if m.starts_with("ollama/") {
            // Check if ollama_cloud is configured (for Ollama cloud), otherwise use local
            if self.is_provider_configured("ollama_cloud") {
                return Some(("ollama_cloud", &self.providers.ollama_cloud));
            }
            return Some(("ollama", &self.providers.ollama));
        }
        if m.starts_with("ollama_cloud/") {
            return Some(("ollama_cloud", &self.providers.ollama_cloud));
        }
        if m.starts_with("vllm/") {
            return Some(("vllm", &self.providers.vllm));
        }
        if m.starts_with("custom/") || m.starts_with("custom:") {
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
    /// Model to use for vision/image analysis. Falls back to `model` if empty.
    pub vision_model: String,
    pub max_tokens: u32,
    pub temperature: f32,
    pub max_iterations: u32,
    /// How many recent messages to include in the LLM context window.
    pub memory_window: u32,
    /// Message count threshold that triggers memory consolidation.
    /// Lower than memory_window so consolidation runs before the context fills up.
    pub consolidation_threshold: u32,
    /// Force XML tool dispatch instead of native function calling.
    /// Useful for models that accept tool definitions but don't reliably call them
    /// (e.g., some Ollama models like GLM-5, Qwen2.5).
    /// When true, tools are injected into the system prompt as XML and parsed
    /// from the LLM's text response.
    pub force_xml_tools: bool,
    /// Fallback models tried in order when the primary model fails.
    /// Each entry is a full model string (e.g. "openai/gpt-4o", "ollama/llama3").
    /// The provider is resolved automatically from the model name.
    #[serde(default)]
    pub fallback_models: Vec<String>,
    /// Per-model parameter overrides. When a model is active, these
    /// values replace the global defaults for temperature/max_tokens.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub model_overrides: HashMap<String, ModelOverrides>,
    /// Delay in milliseconds before retrying after switching to XML tool dispatch.
    /// When a model doesn't support native tool use, the agent switches to XML mode
    /// and retries — this delay prevents hitting rate limits (especially on free-tier models).
    /// Default: 1000ms. Set to 0 to disable.
    pub xml_fallback_delay_ms: u64,
    /// Message debounce window in milliseconds. When multiple messages arrive
    /// for the same session within this window, they are aggregated into one.
    /// Set to 0 to disable debounce. Default: 2000.
    pub debounce_window_ms: u64,
    /// Maximum messages to aggregate before force-flushing the debounce buffer.
    /// Prevents unbounded buffering if messages keep arriving rapidly.
    /// Default: 10.
    pub debounce_max_batch: usize,
    /// Rolling window size for loop/cycle detection.
    /// The agent tracks the last N tool-call signatures and detects repeating
    /// patterns (period 1–3). Set to 0 to disable. Default: 8.
    pub loop_detection_window: u8,
    /// Maximum tokens (prompt + completion) allowed per agent session.
    /// When reached the agent gracefully stops. At 80% a wrap-up hint is
    /// injected. Set to 0 for unlimited (backward-compatible default).
    pub max_session_tokens: u32,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ModelCapabilities {
    pub multimodal: bool,
    pub image_input: bool,
    pub tool_calls: bool,
    pub thinking: bool,
}

/// Per-model parameter overrides.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelOverrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multimodal: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_input: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<bool>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            model: "anthropic/claude-sonnet-4-20250514".to_string(),
            vision_model: String::new(),
            max_tokens: 8192,
            temperature: 0.7,
            max_iterations: 20,
            memory_window: 50,
            consolidation_threshold: 20,
            force_xml_tools: false,
            fallback_models: Vec::new(),
            model_overrides: HashMap::new(),
            xml_fallback_delay_ms: 1000,
            debounce_window_ms: 2000,
            debounce_max_batch: 10,
            loop_detection_window: 8,
            max_session_tokens: 0,
        }
    }
}

impl AgentConfig {
    /// Get effective temperature for a model, checking per-model overrides first.
    pub fn effective_temperature(&self, model: &str) -> f32 {
        self.model_overrides
            .get(model)
            .and_then(|o| o.temperature)
            .unwrap_or(self.temperature)
    }

    /// Get effective max_tokens for a model, checking per-model overrides first.
    pub fn effective_max_tokens(&self, model: &str) -> u32 {
        self.model_overrides
            .get(model)
            .and_then(|o| o.max_tokens)
            .unwrap_or(self.max_tokens)
    }

    pub fn effective_model_capabilities(
        &self,
        provider_name: &str,
        model: &str,
    ) -> ModelCapabilities {
        let mut capabilities =
            crate::provider::capabilities::detect_model_capabilities(provider_name, model);

        if let Some(overrides) = self.model_overrides.get(model) {
            if let Some(multimodal) = overrides.multimodal {
                capabilities.multimodal = multimodal;
            }
            if let Some(image_input) = overrides.image_input {
                capabilities.image_input = image_input;
            }
            if let Some(tool_calls) = overrides.tool_calls {
                capabilities.tool_calls = tool_calls;
            }
            if let Some(thinking) = overrides.thinking {
                capabilities.thinking = thinking;
            }
        }

        capabilities
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
    /// Force XML tool dispatch for this provider.
    /// When set, overrides the global `agent.force_xml_tools` setting.
    /// Useful for providers/models with unreliable native tool calling.
    /// - `true`: Always use XML dispatch
    /// - `false`: Always use native tool calling
    /// - `None`: Use global setting or auto-detect
    pub force_xml_tools: Option<bool>,
    /// Models hidden from the UI lists by the user.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hidden_models: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProvidersConfig {
    // Primary providers
    pub anthropic: ProviderConfig,
    pub openai: ProviderConfig,
    pub openrouter: ProviderConfig,
    pub gemini: ProviderConfig,
    // Local/self-hosted
    pub ollama: ProviderConfig,
    pub ollama_cloud: ProviderConfig,
    pub vllm: ProviderConfig,
    pub custom: ProviderConfig,
    // Cloud providers (OpenAI-compatible)
    pub deepseek: ProviderConfig,
    pub groq: ProviderConfig,
    pub mistral: ProviderConfig,
    pub xai: ProviderConfig,
    pub together: ProviderConfig,
    pub fireworks: ProviderConfig,
    pub perplexity: ProviderConfig,
    pub cohere: ProviderConfig,
    pub venice: ProviderConfig,
    // Gateways/aggregators
    pub aihubmix: ProviderConfig,
    pub vercel: ProviderConfig,
    pub cloudflare: ProviderConfig,
    pub copilot: ProviderConfig,
    pub bedrock: ProviderConfig,
    // Chinese providers
    pub minimax: ProviderConfig,
    pub dashscope: ProviderConfig,
    pub moonshot: ProviderConfig,
    pub zhipu: ProviderConfig,
}

impl ProvidersConfig {
    /// Iterate over all providers as (name, config) pairs
    pub fn iter(&self) -> impl Iterator<Item = (&str, &ProviderConfig)> {
        [
            // Primary
            ("anthropic", &self.anthropic),
            ("openai", &self.openai),
            ("openrouter", &self.openrouter),
            ("gemini", &self.gemini),
            // Local
            ("ollama", &self.ollama),
            ("ollama_cloud", &self.ollama_cloud),
            ("vllm", &self.vllm),
            ("custom", &self.custom),
            // Cloud
            ("deepseek", &self.deepseek),
            ("groq", &self.groq),
            ("mistral", &self.mistral),
            ("xai", &self.xai),
            ("together", &self.together),
            ("fireworks", &self.fireworks),
            ("perplexity", &self.perplexity),
            ("cohere", &self.cohere),
            ("venice", &self.venice),
            // Gateways
            ("aihubmix", &self.aihubmix),
            ("vercel", &self.vercel),
            ("cloudflare", &self.cloudflare),
            ("copilot", &self.copilot),
            ("bedrock", &self.bedrock),
            // Chinese
            ("minimax", &self.minimax),
            ("dashscope", &self.dashscope),
            ("moonshot", &self.moonshot),
            ("zhipu", &self.zhipu),
        ]
        .into_iter()
    }

    /// Get a reference to a provider config by name
    pub fn get(&self, name: &str) -> Option<&ProviderConfig> {
        match name {
            "anthropic" => Some(&self.anthropic),
            "openai" => Some(&self.openai),
            "openrouter" => Some(&self.openrouter),
            "gemini" => Some(&self.gemini),
            "ollama" => Some(&self.ollama),
            "ollama_cloud" => Some(&self.ollama_cloud),
            "vllm" => Some(&self.vllm),
            "custom" => Some(&self.custom),
            "deepseek" => Some(&self.deepseek),
            "groq" => Some(&self.groq),
            "mistral" => Some(&self.mistral),
            "xai" | "grok" => Some(&self.xai),
            "together" => Some(&self.together),
            "fireworks" => Some(&self.fireworks),
            "perplexity" => Some(&self.perplexity),
            "cohere" => Some(&self.cohere),
            "venice" => Some(&self.venice),
            "aihubmix" => Some(&self.aihubmix),
            "vercel" => Some(&self.vercel),
            "cloudflare" => Some(&self.cloudflare),
            "copilot" => Some(&self.copilot),
            "bedrock" => Some(&self.bedrock),
            "minimax" => Some(&self.minimax),
            "dashscope" => Some(&self.dashscope),
            "moonshot" => Some(&self.moonshot),
            "zhipu" => Some(&self.zhipu),
            _ => None,
        }
    }

    /// Get a mutable reference to a provider config by name
    pub fn get_mut(&mut self, name: &str) -> Option<&mut ProviderConfig> {
        match name {
            "anthropic" => Some(&mut self.anthropic),
            "openai" => Some(&mut self.openai),
            "openrouter" => Some(&mut self.openrouter),
            "gemini" => Some(&mut self.gemini),
            "ollama" => Some(&mut self.ollama),
            "ollama_cloud" => Some(&mut self.ollama_cloud),
            "vllm" => Some(&mut self.vllm),
            "custom" => Some(&mut self.custom),
            "deepseek" => Some(&mut self.deepseek),
            "groq" => Some(&mut self.groq),
            "mistral" => Some(&mut self.mistral),
            "xai" | "grok" => Some(&mut self.xai),
            "together" => Some(&mut self.together),
            "fireworks" => Some(&mut self.fireworks),
            "perplexity" => Some(&mut self.perplexity),
            "cohere" => Some(&mut self.cohere),
            "venice" => Some(&mut self.venice),
            "aihubmix" => Some(&mut self.aihubmix),
            "vercel" => Some(&mut self.vercel),
            "cloudflare" => Some(&mut self.cloudflare),
            "copilot" => Some(&mut self.copilot),
            "bedrock" => Some(&mut self.bedrock),
            "minimax" => Some(&mut self.minimax),
            "dashscope" => Some(&mut self.dashscope),
            "moonshot" => Some(&mut self.moonshot),
            "zhipu" => Some(&mut self.zhipu),
            _ => None,
        }
    }

    /// List of all known provider names
    pub fn known_names() -> &'static [&'static str] {
        &[
            // Primary
            "anthropic",
            "openai",
            "openrouter",
            "gemini",
            // Local
            "ollama",
            "ollama_cloud",
            "vllm",
            "custom",
            // Cloud
            "deepseek",
            "groq",
            "mistral",
            "xai",
            "together",
            "fireworks",
            "perplexity",
            "cohere",
            "venice",
            // Gateways
            "aihubmix",
            "vercel",
            "cloudflare",
            "copilot",
            "bedrock",
            // Chinese
            "minimax",
            "dashscope",
            "moonshot",
            "zhipu",
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
    /// Require OTP pairing for unknown senders (default: false).
    /// When true, senders not in `allow_from` receive a 6-digit code to verify.
    #[serde(default)]
    pub pairing_required: bool,
    /// In groups, only respond when @mentioned or replied to (default: true).
    #[serde(default = "default_true")]
    pub mention_required: bool,
    /// Default response mode for contacts on this channel.
    /// Empty or "automatic" = respond immediately. Options: automatic, assisted, on_demand, silent.
    #[serde(default)]
    pub response_mode: String,
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
    /// Require OTP pairing for unknown senders (default: false).
    #[serde(default)]
    pub pairing_required: bool,
    /// Bot display name used for @mention detection in groups (e.g. "homun").
    /// In groups, the bot only responds when mentioned by this name.
    #[serde(default = "default_bot_name")]
    pub bot_name: String,
    /// Default response mode for contacts on this channel.
    #[serde(default)]
    pub response_mode: String,
}

fn default_bot_name() -> String {
    "homun".to_string()
}

impl Default for WhatsAppConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            phone_number: String::new(),
            db_path: "~/.homun/whatsapp.db".to_string(),
            skip_history_sync: true,
            allow_from: Vec::new(),
            pairing_required: false,
            bot_name: default_bot_name(),
            response_mode: String::new(),
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
    /// Require OTP pairing for unknown senders (default: false).
    #[serde(default)]
    pub pairing_required: bool,
    /// In guilds, only respond when @mentioned (default: true).
    #[serde(default = "default_true")]
    pub mention_required: bool,
    /// Default response mode for contacts on this channel.
    #[serde(default)]
    pub response_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WebConfig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    /// Custom domain for the web UI (default: "localhost").
    /// Used in self-signed cert SANs and CORS origin matching.
    pub domain: String,
    /// Optional auth token for remote access. Empty = no auth (localhost only).
    pub auth_token: String,
    /// API rate limit: max requests per minute per IP (default: 60).
    pub rate_limit_per_minute: u32,
    /// Auth rate limit: max login attempts per minute per IP (default: 5).
    pub auth_rate_limit_per_minute: u32,
    /// Path to TLS certificate PEM file. Empty = no TLS.
    pub tls_cert: String,
    /// Path to TLS private key PEM file. Empty = no TLS.
    pub tls_key: String,
    /// Auto-generate self-signed cert if no cert/key provided (default: true).
    pub auto_tls: bool,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            host: "127.0.0.1".to_string(),
            port: 18443,
            domain: "localhost".to_string(),
            auth_token: String::new(),
            rate_limit_per_minute: 60,
            auth_rate_limit_per_minute: 5,
            tls_cert: String::new(),
            tls_key: String::new(),
            auto_tls: true,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SlackConfig {
    pub enabled: bool,
    pub token: String,
    /// App-level token (xapp-*) for Socket Mode. If empty, falls back to HTTP polling.
    #[serde(default)]
    pub app_token: String,
    /// Channel ID to monitor (e.g., "C1234567890"). Empty or "*" = auto-discover all accessible channels.
    #[serde(default)]
    pub channel_id: String,
    /// Default channel ID for proactive/cross-channel messaging.
    /// Falls back to `channel_id` if empty.
    #[serde(default)]
    pub default_channel_id: String,
    /// List of user IDs allowed to interact (e.g., "U1234567890"). "*" = allow all.
    #[serde(default)]
    pub allow_from: Vec<String>,
    /// Require OTP pairing for unknown senders (default: false).
    #[serde(default)]
    pub pairing_required: bool,
    /// In channels, only respond when @mentioned (default: true).
    #[serde(default = "default_true")]
    pub mention_required: bool,
    /// Default response mode for contacts on this channel.
    #[serde(default)]
    pub response_mode: String,
}

/// Email response mode for an account.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EmailMode {
    /// Summarize incoming email and ask for approval on the notify channel (default).
    #[default]
    Assisted,
    /// Respond automatically. Escalates to assisted if it lacks info or would leak vault secrets.
    Automatic,
    /// Only process when a trigger word (or `@homun`) is found in subject/body.
    /// When triggered, behaves like assisted.
    OnDemand,
}

/// Per-account email configuration (IMAP + SMTP + mode).
///
/// Stored under `[channels.emails.<name>]` in config.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EmailAccountConfig {
    pub enabled: bool,
    /// IMAP server hostname
    pub imap_host: String,
    /// IMAP server port (default: 993 for TLS)
    pub imap_port: u16,
    /// IMAP folder to poll (default: INBOX)
    pub imap_folder: String,
    /// SMTP server hostname
    pub smtp_host: String,
    /// SMTP server port (default: 465 for TLS)
    pub smtp_port: u16,
    /// Use TLS for SMTP (default: true)
    pub smtp_tls: bool,
    /// Email username for authentication
    pub username: String,
    /// Email password (use ***ENCRYPTED*** to store in vault)
    pub password: String,
    /// From address for outgoing emails
    pub from_address: String,
    /// IDLE timeout in seconds before re-establishing connection (default: 1740 = 29 minutes)
    /// RFC 2177 recommends clients restart IDLE every 29 minutes
    pub idle_timeout_secs: u64,
    /// Allowed sender addresses/domains (empty = deny all, ["*"] = allow all)
    pub allow_from: Vec<String>,
    /// Require OTP pairing for unknown senders (default: false).
    #[serde(default)]
    pub pairing_required: bool,

    // --- New fields ---
    /// Response mode: assisted (default), automatic, on_demand.
    #[serde(default)]
    pub mode: EmailMode,
    /// Channel to send notifications/approvals to (e.g. "telegram", "whatsapp", "slack").
    #[serde(default)]
    pub notify_channel: Option<String>,
    /// Chat ID on the notify channel (e.g. Telegram user ID).
    #[serde(default)]
    pub notify_chat_id: Option<String>,
    /// Trigger word for on_demand mode. Auto-generated if absent.
    #[serde(default)]
    pub trigger_word: Option<String>,

    /// Batching: items before emitting a digest (default: 3).
    #[serde(default = "default_batch_threshold")]
    pub batch_threshold: u32,
    /// Batching: accumulation window in seconds (default: 120).
    #[serde(default = "default_batch_window_secs")]
    pub batch_window_secs: u64,
    /// Delay in seconds between sending successive responses (default: 30).
    #[serde(default = "default_send_delay_secs")]
    pub send_delay_secs: u64,
}

fn default_batch_threshold() -> u32 {
    3
}
fn default_batch_window_secs() -> u64 {
    120
}
fn default_send_delay_secs() -> u64 {
    30
}

impl Default for EmailAccountConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            imap_host: String::new(),
            imap_port: 993,
            imap_folder: "INBOX".to_string(),
            smtp_host: String::new(),
            smtp_port: 465,
            smtp_tls: true,
            username: String::new(),
            password: String::new(),
            from_address: String::new(),
            idle_timeout_secs: 1740,
            allow_from: Vec::new(),
            pairing_required: false,
            mode: EmailMode::Assisted,
            notify_channel: None,
            notify_chat_id: None,
            trigger_word: None,
            batch_threshold: default_batch_threshold(),
            batch_window_secs: default_batch_window_secs(),
            send_delay_secs: default_send_delay_secs(),
        }
    }
}

impl EmailAccountConfig {
    /// Check if this account is properly configured.
    pub fn is_configured(&self) -> bool {
        !self.imap_host.is_empty()
            && !self.smtp_host.is_empty()
            && !self.username.is_empty()
            && !self.password.is_empty()
    }

    /// Build a `QueueConfig` from the batching fields.
    pub fn queue_config(&self) -> crate::queue::QueueConfig {
        crate::queue::QueueConfig {
            batch_threshold: self.batch_threshold,
            batch_window_secs: self.batch_window_secs,
            process_delay_secs: self.send_delay_secs,
        }
    }
}

/// Legacy single-account email config.
///
/// Kept for backward compatibility: if `[channels.email]` is present in the
/// config file, it is automatically migrated into `channels.emails` as a
/// "default" account.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EmailConfig {
    pub enabled: bool,
    pub imap_host: String,
    pub imap_port: u16,
    pub imap_folder: String,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_tls: bool,
    pub username: String,
    pub password: String,
    pub from_address: String,
    pub idle_timeout_secs: u64,
    pub allow_from: Vec<String>,
    #[serde(default)]
    pub pairing_required: bool,
}

impl Default for EmailConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            imap_host: String::new(),
            imap_port: 993,
            imap_folder: "INBOX".to_string(),
            smtp_host: String::new(),
            smtp_port: 465,
            smtp_tls: true,
            username: String::new(),
            password: String::new(),
            from_address: String::new(),
            idle_timeout_secs: 1740,
            allow_from: Vec::new(),
            pairing_required: false,
        }
    }
}

impl EmailConfig {
    /// Check if email channel is properly configured
    pub fn is_configured(&self) -> bool {
        !self.imap_host.is_empty()
            && !self.smtp_host.is_empty()
            && !self.username.is_empty()
            && !self.password.is_empty()
    }

    /// Convert legacy config into a new `EmailAccountConfig` (for migration).
    pub fn into_account(self) -> EmailAccountConfig {
        EmailAccountConfig {
            enabled: self.enabled,
            imap_host: self.imap_host,
            imap_port: self.imap_port,
            imap_folder: self.imap_folder,
            smtp_host: self.smtp_host,
            smtp_port: self.smtp_port,
            smtp_tls: self.smtp_tls,
            username: self.username,
            password: self.password,
            from_address: self.from_address,
            idle_timeout_secs: self.idle_timeout_secs,
            allow_from: self.allow_from,
            pairing_required: self.pairing_required,
            mode: EmailMode::Assisted,
            notify_channel: None,
            notify_chat_id: None,
            trigger_word: None,
            batch_threshold: default_batch_threshold(),
            batch_window_secs: default_batch_window_secs(),
            send_delay_secs: default_send_delay_secs(),
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
    pub slack: SlackConfig,
    /// Legacy single-account email config. Migrated into `emails` at startup.
    pub email: EmailConfig,
    /// Multi-account email configuration. Keys are account names.
    pub emails: HashMap<String, EmailAccountConfig>,
}

impl ChannelsConfig {
    /// Migrate legacy `[channels.email]` into `[channels.emails.default]` if needed.
    ///
    /// Call this once after loading config. If the legacy `email` field is
    /// configured and `emails` is empty, the legacy config is promoted.
    pub fn migrate_legacy_email(&mut self) {
        if self.emails.is_empty() && self.email.enabled && self.email.is_configured() {
            let account = self.email.clone().into_account();
            self.emails.insert("default".to_string(), account);
            tracing::info!("Migrated legacy [channels.email] → [channels.emails.default]");
        }
    }

    /// Get all enabled and configured email accounts.
    pub fn active_email_accounts(&self) -> Vec<(&String, &EmailAccountConfig)> {
        self.emails
            .iter()
            .filter(|(_, acc)| acc.enabled && acc.is_configured())
            .collect()
    }

    /// Return a list of enabled channels with their default chat IDs.
    /// Used to inject cross-channel routing info into the agent's system prompt.
    pub fn active_channels_with_chat_ids(&self) -> Vec<(String, String)> {
        let mut channels = Vec::new();

        if self.telegram.enabled && !self.telegram.token.is_empty() {
            if let Some(user_id) = self.telegram.allow_from.first() {
                channels.push(("telegram".to_string(), user_id.clone()));
            }
        }

        if self.whatsapp.enabled && !self.whatsapp.phone_number.is_empty() {
            let jid = format!("{}@s.whatsapp.net", self.whatsapp.phone_number);
            channels.push(("whatsapp".to_string(), jid));
        }

        if self.discord.enabled
            && !self.discord.token.is_empty()
            && !self.discord.default_channel_id.is_empty()
        {
            channels.push((
                "discord".to_string(),
                self.discord.default_channel_id.clone(),
            ));
        }

        if self.slack.enabled && !self.slack.token.is_empty() {
            let proactive_target = if !self.slack.default_channel_id.is_empty() {
                &self.slack.default_channel_id
            } else {
                &self.slack.channel_id
            };
            if !proactive_target.is_empty() {
                channels.push(("slack".to_string(), proactive_target.clone()));
            }
        }

        // Multi-account emails
        for (name, acc) in &self.emails {
            if acc.enabled && acc.is_configured() {
                channels.push((format!("email:{name}"), acc.from_address.clone()));
            }
        }

        // Legacy single-account fallback (only if no multi-account)
        if self.emails.is_empty() && self.email.enabled && self.email.is_configured() {
            channels.push(("email".to_string(), self.email.from_address.clone()));
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ToolsConfig {
    pub web_search: WebSearchConfig,
    pub exec: ExecConfig,
    /// Default timeout for tool execution in seconds. 0 = no timeout.
    #[serde(default = "default_tool_timeout")]
    pub default_timeout_secs: u64,
    /// Per-tool timeout overrides: tool_name → seconds.
    #[serde(default)]
    pub timeouts: std::collections::HashMap<String, u64>,
}

fn default_tool_timeout() -> u64 {
    120
}

impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            web_search: WebSearchConfig::default(),
            exec: ExecConfig::default(),
            default_timeout_secs: default_tool_timeout(),
            timeouts: std::collections::HashMap::new(),
        }
    }
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
    /// Explicit attachment-analysis capabilities exposed by this server.
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Whether this server is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Connection Recipe that created this server (for multi-instance support).
    /// `None` for servers configured manually or via MCP presets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recipe_id: Option<String>,
    /// For HTTP transport: env key whose resolved value is used as Bearer token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_env_key: Option<String>,
    /// Cached tool count from last successful connection test or gateway startup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discovered_tool_count: Option<usize>,
}

impl Default for McpServerConfig {
    fn default() -> Self {
        Self {
            transport: "stdio".to_string(),
            command: None,
            args: Vec::new(),
            url: None,
            env: HashMap::new(),
            capabilities: Vec::new(),
            enabled: true,
            recipe_id: None,
            auth_env_key: None,
            discovered_tool_count: None,
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

// --- Memory Config ---

/// Memory retention and indexing configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MemoryConfig {
    /// Days to keep conversation messages before pruning (0 = never prune)
    pub conversation_retention_days: u32,
    /// Days to keep history entries (0 = never prune)
    pub history_retention_days: u32,
    /// Months after which daily files are archived (0 = never archive)
    pub daily_archive_months: u32,
    /// Enable automatic memory cleanup on startup
    pub auto_cleanup: bool,
    /// Embedding provider: "ollama" (default, free), "openai", "mistral", or any
    /// OpenAI-compatible provider name. The factory auto-detects API key from the
    /// matching LLM provider config if `embedding_api_key` is empty.
    pub embedding_provider: String,
    /// Embedding model name. Empty = provider default.
    /// Ollama: nomic-embed-text, OpenAI: text-embedding-3-small, Mistral: mistral-embed.
    pub embedding_model: String,
    /// Embedding API base URL. Empty = provider default.
    /// E.g., "http://ollama:11434/v1" for Ollama in Docker.
    pub embedding_api_base: String,
    /// Dedicated API key for embedding provider. Empty = auto-detect from
    /// the matching LLM provider config (e.g., providers.openai.api_key).
    pub embedding_api_key: String,
    /// Embedding vector dimensions. Must match HNSW index.
    /// Default 384. Change requires re-indexing all vectors.
    pub embedding_dimensions: usize,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            conversation_retention_days: 30, // Keep messages for 30 days
            history_retention_days: 365,     // Keep history for 1 year
            daily_archive_months: 3,         // Archive daily files after 3 months
            auto_cleanup: false,             // Don't auto-cleanup by default
            embedding_provider: "ollama".to_string(),
            embedding_model: String::new(),
            embedding_api_base: String::new(),
            embedding_api_key: String::new(),
            embedding_dimensions: 384,
        }
    }
}

// --- Knowledge (RAG) Config ---

/// Knowledge base (RAG) configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct KnowledgeConfig {
    /// Enable RAG knowledge base
    pub enabled: bool,
    /// Maximum tokens per chunk
    pub chunk_max_tokens: usize,
    /// Overlap tokens between chunks
    pub chunk_overlap_tokens: usize,
    /// Number of RAG results to inject per query
    pub results_per_query: usize,
    /// Directories to watch for auto-ingestion (e.g., ["~/Documents/notes"])
    #[serde(default)]
    pub watch_dirs: Vec<String>,
    /// MCP server names to sync resources from (references keys in [mcp.servers])
    #[serde(default)]
    pub cloud_sources: Vec<String>,
}

impl Default for KnowledgeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            chunk_max_tokens: 512,
            chunk_overlap_tokens: 50,
            results_per_query: 3,
            watch_dirs: Vec::new(),
            cloud_sources: Vec::new(),
        }
    }
}

// --- Permissions Config ---

/// Permission mode for file access control
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PermissionMode {
    /// No restrictions (except hardcoded blocks)
    Open,
    /// Only workspace + brain + memory directories
    #[default]
    Workspace,
    /// Full ACL-based control
    Acl,
}

/// Permission value - can be boolean or require confirmation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PermissionValue {
    Bool(bool),
    Confirm,
}

impl Default for PermissionValue {
    fn default() -> Self {
        Self::Bool(false)
    }
}

impl PermissionValue {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Bool(true))
    }

    pub fn needs_confirmation(&self) -> bool {
        matches!(self, Self::Confirm)
    }
}

/// Permissions for a path (read/write/delete)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PathPermissions {
    pub read: PermissionValue,
    pub write: PermissionValue,
    pub delete: PermissionValue,
}

impl Default for PathPermissions {
    fn default() -> Self {
        Self {
            read: PermissionValue::Bool(true),
            write: PermissionValue::Bool(false),
            delete: PermissionValue::Bool(false),
        }
    }
}

/// Default permissions for unmatched paths
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DefaultPermissions {
    pub read: bool,
    pub write: bool,
    pub delete: bool,
}

impl Default for DefaultPermissions {
    fn default() -> Self {
        Self {
            read: true,
            write: false,
            delete: false,
        }
    }
}

/// ACL entry - matches a path pattern and defines permissions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AclEntry {
    /// Glob pattern (supports **, *, ?)
    pub path: String,
    /// Permissions for matching paths
    pub permissions: PathPermissions,
    /// "allow" or "deny" - deny takes precedence
    #[serde(default = "default_acl_type")]
    pub entry_type: String,
}

fn default_acl_type() -> String {
    "allow".to_string()
}

/// OS-specific shell profile
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OsShellProfile {
    /// Allow risky commands (package removal, process killing, etc.)
    pub allow_risky: bool,
    /// Additional blocked commands beyond built-in deny lists
    pub blocked_commands: Vec<String>,
    /// If non-empty, only these commands are allowed (whitelist mode)
    pub allowed_commands: Vec<String>,
    /// Shell to use: "bash", "zsh", "sh", "powershell", "cmd"
    pub shell: Option<String>,
}

impl OsShellProfile {
    pub fn default_for(os: &str) -> Self {
        Self {
            allow_risky: false,
            blocked_commands: match os {
                "macos" => vec!["launchctl load".to_string(), "defaults delete".to_string()],
                "linux" => vec!["systemctl --now disable".to_string()],
                "windows" => vec![
                    "reg delete".to_string(),
                    "powershell -encodedcommand".to_string(),
                ],
                _ => vec![],
            },
            allowed_commands: vec![],
            shell: if os == "windows" {
                Some("powershell".to_string())
            } else {
                None
            },
        }
    }
}

impl Default for OsShellProfile {
    fn default() -> Self {
        Self::default_for("")
    }
}

/// Shell permissions for all platforms
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ShellPermissions {
    pub macos: OsShellProfile,
    pub linux: OsShellProfile,
    pub windows: OsShellProfile,
}

impl Default for ShellPermissions {
    fn default() -> Self {
        Self {
            macos: OsShellProfile::default_for("macos"),
            linux: OsShellProfile::default_for("linux"),
            windows: OsShellProfile::default_for("windows"),
        }
    }
}

impl ShellPermissions {
    /// Get the profile for the current OS
    pub fn current(&self) -> &OsShellProfile {
        #[cfg(target_os = "macos")]
        {
            &self.macos
        }
        #[cfg(target_os = "linux")]
        {
            &self.linux
        }
        #[cfg(target_os = "windows")]
        {
            &self.windows
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            &self.linux
        }
    }
}

// --- Approval Config (P0-4: Command Allowlist) ---

/// Autonomy level for tool execution
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AutonomyLevel {
    /// Full autonomy - all tools execute without prompts
    #[default]
    Full,
    /// Supervised - prompts for non-whitelisted tools
    Supervised,
    /// ReadOnly - only read-only tools allowed
    ReadOnly,
}

/// Approval configuration for shell commands
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ApprovalConfig {
    /// Autonomy level
    pub level: AutonomyLevel,
    /// Tools/commands that never need approval
    pub auto_approve: Vec<String>,
    /// Tools/commands that always require approval (even after "Always")
    pub always_ask: Vec<String>,
    /// Enable audit logging of all executed commands
    pub audit_enabled: bool,
    /// Path to audit log file (empty = use default ~/.homun/shell-audit.log)
    pub audit_path: String,
}

impl Default for ApprovalConfig {
    fn default() -> Self {
        Self {
            level: AutonomyLevel::Full, // Default to full autonomy - approval workflow is opt-in
            // Safe commands that don't need approval
            auto_approve: vec![
                "ls".to_string(),
                "cat".to_string(),
                "head".to_string(),
                "tail".to_string(),
                "wc".to_string(),
                "echo".to_string(),
                "pwd".to_string(),
                "which".to_string(),
                "grep".to_string(),
                "find".to_string(),
                "git status".to_string(),
                "git log".to_string(),
                "git diff".to_string(),
                "git branch".to_string(),
            ],
            // Commands that always require approval
            always_ask: vec![
                "rm".to_string(),
                "mv".to_string(),
                "cp".to_string(),
                "chmod".to_string(),
                "chown".to_string(),
                "kill".to_string(),
                "pkill".to_string(),
                "docker".to_string(),
                "npm install".to_string(),
                "npm uninstall".to_string(),
                "pip install".to_string(),
                "pip uninstall".to_string(),
                "cargo install".to_string(),
                "brew install".to_string(),
                "brew uninstall".to_string(),
            ],
            audit_enabled: true,
            audit_path: String::new(),
        }
    }
}

/// Single audit log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellAuditEntry {
    pub timestamp: String,
    pub command: String,
    pub args: String,
    pub result: String,
    pub output_preview: String,
    pub channel: String,
    pub approved: bool,
    pub approval_type: Option<String>,
}

/// Root permissions configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PermissionsConfig {
    /// Permission mode: "open", "workspace", or "acl"
    pub mode: PermissionMode,
    /// Default permissions for paths not matching any ACL
    pub default: DefaultPermissions,
    /// ACL entries (evaluated in order, first match wins)
    pub acl: Vec<AclEntry>,
    /// OS-specific shell permissions
    pub shell: ShellPermissions,
    /// Approval workflow for shell commands
    pub approval: ApprovalConfig,
}

impl Default for PermissionsConfig {
    fn default() -> Self {
        Self {
            mode: PermissionMode::Workspace,
            default: DefaultPermissions::default(),
            acl: vec![
                // Built-in protections (sensitive directories)
                AclEntry {
                    path: "~/.ssh/**".to_string(),
                    permissions: PathPermissions {
                        read: PermissionValue::Bool(false),
                        write: PermissionValue::Bool(false),
                        delete: PermissionValue::Bool(false),
                    },
                    entry_type: "deny".to_string(),
                },
                AclEntry {
                    path: "~/.aws/**".to_string(),
                    permissions: PathPermissions {
                        read: PermissionValue::Bool(false),
                        write: PermissionValue::Bool(false),
                        delete: PermissionValue::Bool(false),
                    },
                    entry_type: "deny".to_string(),
                },
                AclEntry {
                    path: "~/.gnupg/**".to_string(),
                    permissions: PathPermissions {
                        read: PermissionValue::Bool(false),
                        write: PermissionValue::Bool(false),
                        delete: PermissionValue::Bool(false),
                    },
                    entry_type: "deny".to_string(),
                },
                AclEntry {
                    path: "~/.config/gcloud/**".to_string(),
                    permissions: PathPermissions {
                        read: PermissionValue::Bool(false),
                        write: PermissionValue::Bool(false),
                        delete: PermissionValue::Bool(false),
                    },
                    entry_type: "deny".to_string(),
                },
                // Always-allowed directories for agent operation
                AclEntry {
                    path: "~/.homun/brain/**".to_string(),
                    permissions: PathPermissions {
                        read: PermissionValue::Bool(true),
                        write: PermissionValue::Bool(true),
                        delete: PermissionValue::Bool(false),
                    },
                    entry_type: "allow".to_string(),
                },
                AclEntry {
                    path: "~/.homun/memory/**".to_string(),
                    permissions: PathPermissions {
                        read: PermissionValue::Bool(true),
                        write: PermissionValue::Bool(true),
                        delete: PermissionValue::Bool(false),
                    },
                    entry_type: "allow".to_string(),
                },
                AclEntry {
                    path: "~/.homun/workspace/**".to_string(),
                    permissions: PathPermissions {
                        read: PermissionValue::Bool(true),
                        write: PermissionValue::Bool(true),
                        delete: PermissionValue::Bool(true),
                    },
                    entry_type: "allow".to_string(),
                },
            ],
            shell: ShellPermissions::default(),
            approval: ApprovalConfig::default(),
        }
    }
}

// --- Security Config ---

/// Exfiltration prevention configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ExfiltrationConfig {
    /// Enable exfiltration detection for LLM output
    pub enabled: bool,
    /// Block output on detection (true) or just redact (false)
    pub block_on_detection: bool,
    /// Log detection attempts
    pub log_attempts: bool,
    /// Custom patterns to detect (regex strings)
    pub custom_patterns: Vec<String>,
}

impl Default for ExfiltrationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            block_on_detection: false, // Redact by default, don't block
            log_attempts: true,
            custom_patterns: Vec::new(),
        }
    }
}

/// Root security configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct SecurityConfig {
    pub exfiltration: ExfiltrationConfig,
    /// Process sandbox configuration for shell/MCP/skills execution.
    pub execution_sandbox: ExecutionSandboxConfig,
}

/// Sandbox configuration for process execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ExecutionSandboxConfig {
    /// Enable sandbox wrapping for process execution paths.
    pub enabled: bool,
    /// Backend selection: "none", "auto", or "docker".
    pub backend: String,
    /// When true, fail execution if requested backend is unavailable.
    pub strict: bool,
    /// Configured runtime image reference used by Docker and the runtime image lifecycle checks.
    pub docker_image: String,
    /// Runtime image policy: infer, pinned, versioned_tag, or floating.
    pub runtime_image_policy: String,
    /// Expected image version override used when runtime_image_policy is explicit.
    pub runtime_image_expected_version: String,
    /// Docker network mode (recommended: "none").
    pub docker_network: String,
    /// Memory limit (MB) for docker sandbox.
    pub docker_memory_mb: u64,
    /// CPU limit for docker sandbox.
    pub docker_cpus: f32,
    /// Mount root filesystem read-only inside docker container.
    pub docker_read_only_rootfs: bool,
    /// Mount Homun workspace into the container at `/workspace`.
    pub docker_mount_workspace: bool,
}

impl ExecutionSandboxConfig {
    /// Return a config with sandbox explicitly disabled (native execution).
    /// Used for processes that must run natively (e.g. browser MCP).
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Self::default()
        }
    }
}

impl Default for ExecutionSandboxConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            backend: "auto".to_string(),
            strict: false,
            docker_image: "node:22-alpine".to_string(),
            runtime_image_policy: "infer".to_string(),
            runtime_image_expected_version: String::new(),
            docker_network: "none".to_string(),
            docker_memory_mb: 512,
            docker_cpus: 1.0,
            docker_read_only_rootfs: true,
            docker_mount_workspace: true,
        }
    }
}

// --- UI Config ---

/// UI configuration for web dashboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    /// Theme: "light", "dark", or "system"
    pub theme: String,
    /// Preferred UI/assistant language: "system", "en", "it"
    pub language: String,
    /// Accent color: "moss", "terracotta", "plum", "stone"
    pub accent: String,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: "system".to_string(),
            language: "system".to_string(),
            accent: "moss".to_string(),
        }
    }
}

/// Browser automation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BrowserConfig {
    /// Enable browser automation
    pub enabled: bool,
    /// Internal one-shot migration marker for legacy browser defaults.
    pub migration_version: u8,
    /// Legacy field kept for TOML compat — always "playwright" now (MCP-based).
    #[serde(default = "default_backend")]
    pub backend: String,
    /// Run browser in headless mode
    pub headless: bool,
    /// Browser type: "chromium", "firefox", "webkit"
    pub browser_type: String,
    /// Path to browser executable (optional, uses system default if not set)
    pub executable_path: String,
    /// Default profile to use (if not specified in action)
    pub default_profile: String,
    /// Named browser profiles for isolation
    pub profiles: HashMap<String, BrowserProfile>,
    /// Inject anti-detection (stealth) scripts that mask automation signals
    /// like `navigator.webdriver`. Default: false.
    /// Modern bot detectors can detect these patches, making you MORE visible.
    /// Only enable if a site specifically blocks `navigator.webdriver = true`.
    #[serde(default)]
    pub stealth: bool,
    /// Action policy — allow/deny categories and URL patterns
    #[serde(default)]
    pub policy: BrowserPolicyConfig,
}

/// Browser action policy — category-based allow/deny rules.
///
/// When `enabled = false` (default), all actions are allowed.
/// When enabled, actions are matched to categories (navigate, click,
/// fill, observe, interact, eval, tabs, network) and checked against
/// `deny` / `allow` lists. Navigate actions also check URL patterns.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BrowserPolicyConfig {
    /// Enable policy enforcement (default: false).
    pub enabled: bool,
    /// Default stance: "allow" or "deny" (default: "allow").
    pub default: String,
    /// Categories to allow (meaningful when default = "deny").
    pub allow: Vec<String>,
    /// Categories to deny (meaningful when default = "allow").
    pub deny: Vec<String>,
    /// URL glob patterns to block for navigate (e.g., "*.evil.com").
    pub blocked_urls: Vec<String>,
    /// URL glob patterns to allow for navigate (when default = "deny").
    pub allowed_urls: Vec<String>,
}

impl Default for BrowserPolicyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            default: "allow".to_string(),
            allow: Vec::new(),
            deny: Vec::new(),
            blocked_urls: Vec::new(),
            allowed_urls: Vec::new(),
        }
    }
}

fn default_backend() -> String {
    "playwright".to_string()
}

#[derive(Debug, Clone, Serialize)]
pub struct BrowserRuntimeStatus {
    pub enabled: bool,
    pub available: bool,
    pub executable_path: Option<String>,
    pub reason: Option<String>,
}

/// A browser profile with isolated cookies, storage, and cache.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BrowserProfile {
    /// Profile name (for display)
    pub name: String,
    /// Browser type override: "chromium", "firefox", "webkit" (optional)
    pub browser_type: Option<String>,
    /// Custom user data directory (optional, auto-generated if not set)
    pub user_data_dir: Option<String>,
    /// Run this profile in headless mode (overrides global setting)
    pub headless: Option<bool>,
    /// Proxy server URL (e.g., "http://proxy:8080")
    pub proxy: Option<String>,
    /// Custom user agent string
    pub user_agent: Option<String>,
    /// Additional browser arguments
    pub args: Vec<String>,
    /// Description of what this profile is for
    pub description: Option<String>,
}

impl Default for BrowserProfile {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            browser_type: None,
            user_data_dir: None,
            headless: None,
            proxy: None,
            user_agent: None,
            args: Vec::new(),
            description: None,
        }
    }
}

impl Default for BrowserConfig {
    fn default() -> Self {
        let mut profiles = HashMap::new();
        profiles.insert(
            "default".to_string(),
            BrowserProfile {
                name: "Default".to_string(),
                description: Some("Default browser profile for general automation".to_string()),
                ..Default::default()
            },
        );

        Self {
            enabled: false,
            migration_version: 0,
            backend: "playwright".to_string(),
            headless: true,
            browser_type: "chromium".to_string(),
            executable_path: String::new(),
            default_profile: "default".to_string(),
            profiles,
            stealth: false,
            policy: BrowserPolicyConfig::default(),
        }
    }
}

impl BrowserConfig {
    pub fn maybe_auto_enable_for_legacy_config(&mut self) -> bool {
        if self.migration_version >= 1 || self.enabled {
            return false;
        }

        if !self.looks_like_legacy_default() || !self.mcp_prerequisites_available() {
            return false;
        }

        self.enabled = true;
        self.backend = "playwright".to_string();
        self.migration_version = 1;
        true
    }

    fn looks_like_legacy_default(&self) -> bool {
        self.executable_path.trim().is_empty()
            && self.browser_type.eq_ignore_ascii_case("chromium")
            && self.default_profile == "default"
            && self.profiles.len() <= 1
    }

    pub fn runtime_status(&self) -> BrowserRuntimeStatus {
        let executable_path = self
            .resolved_executable()
            .map(|path| path.display().to_string());

        if !self.enabled {
            return BrowserRuntimeStatus {
                enabled: false,
                available: false,
                executable_path,
                reason: Some("Browser automation is disabled in configuration.".to_string()),
            };
        }

        if !self.mcp_prerequisites_available() {
            return BrowserRuntimeStatus {
                enabled: true,
                available: false,
                executable_path,
                reason: Some(
                    "npx is not available on this machine. Install Node.js to use browser automation via @playwright/mcp."
                        .to_string(),
                ),
            };
        }

        BrowserRuntimeStatus {
            enabled: true,
            available: true,
            executable_path,
            reason: None,
        }
    }

    /// Get the browser user data directory path (for login persistence)
    pub fn user_data_path(&self) -> std::path::PathBuf {
        Config::data_dir().join("browser-profile")
    }

    /// Get user data path for a specific profile
    pub fn profile_user_data_path(&self, profile_name: &str) -> std::path::PathBuf {
        if let Some(profile) = self.profiles.get(profile_name) {
            if let Some(ref custom_dir) = profile.user_data_dir {
                return std::path::PathBuf::from(custom_dir);
            }
        }
        // Default: use profile name as subdirectory
        Config::data_dir()
            .join("browser-profiles")
            .join(profile_name)
    }

    /// Get a profile by name, or the default profile
    pub fn get_profile(&self, name: Option<&str>) -> (&String, &BrowserProfile) {
        let profile_name = name.unwrap_or(&self.default_profile);
        self.profiles
            .get_key_value(profile_name)
            .unwrap_or_else(|| {
                self.profiles
                    .get_key_value(&self.default_profile)
                    .expect("Default profile must exist")
            })
    }

    /// List all available profile names
    pub fn profile_names(&self) -> Vec<&String> {
        self.profiles.keys().collect()
    }

    /// Check if a profile exists
    pub fn has_profile(&self, name: &str) -> bool {
        self.profiles.contains_key(name)
    }

    /// Get browser type for a profile (profile override or global default)
    pub fn browser_type_for_profile(&self, profile_name: &str) -> &str {
        self.profiles
            .get(profile_name)
            .and_then(|p| p.browser_type.as_deref())
            .unwrap_or(&self.browser_type)
    }

    /// Get headless setting for a profile (profile override or global default)
    pub fn headless_for_profile(&self, profile_name: &str) -> bool {
        self.profiles
            .get(profile_name)
            .and_then(|p| p.headless)
            .unwrap_or(self.headless)
    }

    /// Get the resolved executable path (or try to find Chrome)
    pub fn resolved_executable(&self) -> Option<std::path::PathBuf> {
        if !self.executable_path.is_empty() {
            let custom = std::path::PathBuf::from(&self.executable_path);
            if custom.exists() {
                Some(custom)
            } else {
                None
            }
        } else {
            // Try common Chrome/Chromium paths
            let candidates = if cfg!(target_os = "macos") {
                vec![
                    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
                    "/Applications/Chromium.app/Contents/MacOS/Chromium",
                ]
            } else if cfg!(target_os = "linux") {
                vec![
                    "/usr/bin/google-chrome",
                    "/usr/bin/chromium",
                    "/usr/bin/chromium-browser",
                ]
            } else {
                vec![]
            };

            candidates
                .iter()
                .find(|p| std::path::Path::new(p).exists())
                .map(std::path::PathBuf::from)
        }
    }

    /// Check if `npx` is available (needed to run `npx @playwright/mcp`).
    pub fn mcp_prerequisites_available(&self) -> bool {
        std::env::var_os("PATH").is_some_and(|paths| {
            std::env::split_paths(&paths).any(|dir| {
                let candidate = if cfg!(windows) {
                    dir.join("npx.cmd")
                } else {
                    dir.join("npx")
                };
                candidate.exists()
            })
        })
    }
}

/// Business Autopilot configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BusinessConfig {
    /// Enable the business tool
    pub enabled: bool,
    /// Default autonomy level: "semi", "budget", "full"
    pub default_autonomy: String,
    /// Default currency
    pub default_currency: String,
    /// Default OODA review interval (cron-style schedule)
    pub default_ooda_interval: String,
    /// Fiscal country (ISO 3166-1 alpha-2)
    pub fiscal_country: String,
    /// VAT number (P.IVA for IT)
    pub vat_number: Option<String>,
    /// Fiscal regime: "standard", "forfettario", "exempt"
    pub fiscal_regime: String,
}

impl Default for BusinessConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            default_autonomy: "semi".to_string(),
            default_currency: "EUR".to_string(),
            default_ooda_interval: "every:86400".to_string(),
            fiscal_country: "IT".to_string(),
            vat_number: None,
            fiscal_regime: "standard".to_string(),
        }
    }
}

// ── Skills Configuration ───────────────────────────────────────────

/// Per-skill configuration for env injection and overrides.
///
/// Example TOML:
/// ```toml
/// [skills.entries.my-skill]
/// env = { GITHUB_ORG = "myorg", API_URL = "https://api.example.com" }
/// api_key = "vault://my-skill-api-key"
/// enabled = true
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SkillsConfig {
    /// Per-skill configuration entries, keyed by skill name
    #[serde(default)]
    pub entries: HashMap<String, SkillEntryConfig>,
}

/// Configuration for a single skill.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SkillEntryConfig {
    /// Environment variables to inject into skill script execution
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// API key (plain or vault:// reference) — injected as API_KEY env var
    #[serde(default)]
    pub api_key: Option<String>,
    /// Override to disable a skill regardless of eligibility
    #[serde(default)]
    pub enabled: Option<bool>,
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
        assert_eq!(config.browser.backend, "playwright");
    }

    #[test]
    fn test_browser_mcp_prerequisites() {
        let config = BrowserConfig::default();
        // Just check the method doesn't panic; actual availability depends on env
        let _ = config.mcp_prerequisites_available();
    }

    #[test]
    fn test_legacy_browser_migration_is_one_shot() {
        let mut browser = BrowserConfig::default();
        browser.enabled = false;
        browser.migration_version = 0;

        if browser.mcp_prerequisites_available() {
            assert!(browser.maybe_auto_enable_for_legacy_config());
            assert!(browser.enabled);
            assert_eq!(browser.migration_version, 1);

            browser.enabled = false;
            assert!(!browser.maybe_auto_enable_for_legacy_config());
            assert!(!browser.enabled);
        }
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
        let (name, _) = config
            .resolve_provider("anthropic/claude-sonnet-4-20250514")
            .unwrap();
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
        // Use canonical "ollama/model" prefix — bare model names fall through
        // to gateways (Step 3), which is correct when OpenRouter is configured.
        // Accept both "ollama" and "ollama_cloud" since global_secrets() may find
        // keyring entries that influence resolution ordering.
        let (name, _) = config.resolve_provider("ollama/llama3").unwrap();
        assert!(
            name.starts_with("ollama"),
            "Expected an Ollama provider, got '{name}'"
        );
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

    #[test]
    fn test_effective_model_capabilities_apply_overrides() {
        let mut config = Config::default();
        config.agent.model_overrides.insert(
            "ollama/custom-vision".to_string(),
            ModelOverrides {
                multimodal: Some(true),
                image_input: Some(true),
                tool_calls: Some(false),
                ..Default::default()
            },
        );

        let caps = config
            .agent
            .effective_model_capabilities("ollama", "ollama/custom-vision");
        assert!(caps.multimodal);
        assert!(caps.image_input);
        assert!(!caps.tool_calls);
    }

    #[test]
    fn test_xml_dispatch_uses_model_tool_call_override() {
        let mut config = Config::default();
        config.agent.model_overrides.insert(
            "ollama/custom-vision".to_string(),
            ModelOverrides {
                tool_calls: Some(false),
                ..Default::default()
            },
        );
        assert!(config.should_use_xml_dispatch("ollama", "ollama/custom-vision"));

        config.agent.model_overrides.insert(
            "ollama/cloud-native-tools:cloud".to_string(),
            ModelOverrides {
                tool_calls: Some(true),
                ..Default::default()
            },
        );
        assert!(!config.should_use_xml_dispatch("ollama", "ollama/cloud-native-tools:cloud"));
    }

    #[test]
    fn test_slack_proactive_uses_default_channel_id() {
        let mut ch = ChannelsConfig::default();
        ch.slack.enabled = true;
        ch.slack.token = "xoxb-test".to_string();
        ch.slack.channel_id = "C_LISTEN".to_string();
        ch.slack.default_channel_id = "C_PROACTIVE".to_string();

        let targets = ch.active_channels_with_chat_ids();
        let slack = targets.iter().find(|(n, _)| n == "slack");
        assert_eq!(slack.unwrap().1, "C_PROACTIVE");
    }

    #[test]
    fn test_slack_proactive_fallback_to_channel_id() {
        let mut ch = ChannelsConfig::default();
        ch.slack.enabled = true;
        ch.slack.token = "xoxb-test".to_string();
        ch.slack.channel_id = "C_LISTEN".to_string();
        // default_channel_id left empty

        let targets = ch.active_channels_with_chat_ids();
        let slack = targets.iter().find(|(n, _)| n == "slack");
        assert_eq!(slack.unwrap().1, "C_LISTEN");
    }
}
