//! Multi-agent definitions.
//!
//! Each `AgentDefinition` describes a named agent with its own model,
//! instructions, and tool/skill scope.  Parsed from `[agents.<id>]`
//! sections in `config.toml`.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;

use crate::config::{AgentConfig, AgentDefinitionConfig, Config};
use crate::provider::Provider;

/// A fully-resolved agent definition ready for use at runtime.
///
/// Fields that are empty/`None` inherit from the global `[agent]` section
/// via the `effective_*` helpers.
#[derive(Debug, Clone)]
pub struct AgentDefinition {
    /// Unique identifier (e.g. "default", "coder", "researcher").
    pub id: String,
    /// LLM model string.  Empty = inherit from `[agent].model`.
    pub model: String,
    /// Task-oriented instructions injected into the system prompt.
    pub instructions: String,
    /// Allowed tool names.  Empty = all tools visible.
    pub allowed_tools: Vec<String>,
    /// Allowed skill names.  Empty = all skills visible.
    pub allowed_skills: Vec<String>,
    /// Per-agent concurrency cap for `QueuedProvider`.  0 = use global.
    pub max_concurrency: usize,
    /// Temperature override.  `None` = use global.
    pub temperature: Option<f32>,
    /// Max-tokens override.  `None` = use global.
    pub max_tokens: Option<u32>,
    /// Fallback models.  Empty = use global fallbacks.
    pub fallback_models: Vec<String>,
}

// ── Constructors ──────────────────────────────────────────────────────

impl AgentDefinition {
    /// Synthesize the implicit "default" agent from the global `[agent]` config.
    ///
    /// Used when no `[agents.*]` sections exist — backward-compatible path.
    pub fn from_global(global: &AgentConfig) -> Self {
        Self {
            id: "default".to_string(),
            model: global.model.clone(),
            instructions: String::new(),
            allowed_tools: Vec::new(),
            allowed_skills: Vec::new(),
            max_concurrency: global.llm_max_concurrent,
            temperature: None,
            max_tokens: None,
            fallback_models: global.fallback_models.clone(),
        }
    }

    /// Build from a parsed `[agents.<id>]` config section.
    fn from_config(id: &str, cfg: &AgentDefinitionConfig) -> Self {
        Self {
            id: id.to_lowercase(),
            model: cfg.model.clone(),
            instructions: cfg.instructions.clone(),
            allowed_tools: cfg.tools.clone(),
            allowed_skills: cfg.skills.clone(),
            max_concurrency: cfg.max_concurrency,
            temperature: cfg.temperature,
            max_tokens: cfg.max_tokens,
            fallback_models: cfg.fallback_models.clone(),
        }
    }

    /// Resolve all agent definitions from config.
    ///
    /// If `config.agents` is empty, returns a single "default" entry
    /// synthesized from `[agent]`.  Otherwise maps every
    /// `[agents.<id>]` section and ensures "default" always exists.
    pub fn resolve_all(config: &Config) -> HashMap<String, Self> {
        if config.agents.is_empty() {
            let mut map = HashMap::new();
            map.insert("default".to_string(), Self::from_global(&config.agent));
            return map;
        }

        let mut map: HashMap<String, Self> = config
            .agents
            .iter()
            .map(|(id, cfg)| (id.to_lowercase(), Self::from_config(id, cfg)))
            .collect();

        // Ensure "default" always exists.
        map.entry("default".to_string())
            .or_insert_with(|| Self::from_global(&config.agent));

        map
    }
}

// ── Effective-value helpers ───────────────────────────────────────────

impl AgentDefinition {
    /// Resolved model: agent-specific if set, otherwise global.
    pub fn effective_model<'a>(&'a self, global: &'a AgentConfig) -> &'a str {
        if self.model.is_empty() {
            &global.model
        } else {
            &self.model
        }
    }

    /// Resolved concurrency limit.
    pub fn effective_max_concurrency(&self, global: &AgentConfig) -> usize {
        if self.max_concurrency > 0 {
            self.max_concurrency
        } else {
            global.llm_max_concurrent
        }
    }

    /// Resolved temperature.
    pub fn effective_temperature(&self, global: &AgentConfig) -> f32 {
        self.temperature.unwrap_or(global.temperature)
    }

    /// Resolved max tokens.
    pub fn effective_max_tokens(&self, global: &AgentConfig) -> u32 {
        self.max_tokens.unwrap_or(global.max_tokens)
    }

    /// Whether a tool is allowed by this agent's filter.
    ///
    /// Empty `allowed_tools` means all tools are allowed.
    pub fn is_tool_allowed(&self, tool_name: &str) -> bool {
        self.allowed_tools.is_empty() || self.allowed_tools.iter().any(|t| t == tool_name)
    }

    /// Whether a skill is allowed by this agent's filter.
    ///
    /// Empty `allowed_skills` means all skills are allowed.
    pub fn is_skill_allowed(&self, skill_name: &str) -> bool {
        self.allowed_skills.is_empty() || self.allowed_skills.iter().any(|s| s == skill_name)
    }
}

// ── Provider creation ─────────────────────────────────────────────────

impl AgentDefinition {
    /// Create an LLM provider for this agent definition.
    ///
    /// Reuses the existing factory — `ReliableProvider` + `QueuedProvider`
    /// wrapping is applied automatically.
    pub fn create_provider(&self, config: &Config) -> Result<Arc<dyn Provider>> {
        let model = self.effective_model(&config.agent);
        crate::provider::factory::create_provider_for_model(config, model)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_global() -> AgentConfig {
        AgentConfig {
            model: "anthropic/claude-sonnet-4-20250514".to_string(),
            temperature: 0.7,
            max_tokens: 8192,
            llm_max_concurrent: 5,
            ..AgentConfig::default()
        }
    }

    #[test]
    fn from_global_defaults() {
        let global = test_global();
        let def = AgentDefinition::from_global(&global);
        assert_eq!(def.id, "default");
        assert_eq!(def.model, global.model);
        assert!(def.instructions.is_empty());
        assert!(def.allowed_tools.is_empty());
    }

    #[test]
    fn effective_model_inherits_when_empty() {
        let global = test_global();
        let mut def = AgentDefinition::from_global(&global);
        def.model = String::new();
        assert_eq!(def.effective_model(&global), "anthropic/claude-sonnet-4-20250514");
    }

    #[test]
    fn effective_model_overrides() {
        let global = test_global();
        let mut def = AgentDefinition::from_global(&global);
        def.model = "openai/gpt-4o".to_string();
        assert_eq!(def.effective_model(&global), "openai/gpt-4o");
    }

    #[test]
    fn effective_temperature_inherits() {
        let global = test_global();
        let def = AgentDefinition::from_global(&global);
        assert!((def.effective_temperature(&global) - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn effective_temperature_overrides() {
        let global = test_global();
        let mut def = AgentDefinition::from_global(&global);
        def.temperature = Some(0.2);
        assert!((def.effective_temperature(&global) - 0.2).abs() < f32::EPSILON);
    }

    #[test]
    fn tool_filter_empty_allows_all() {
        let def = AgentDefinition::from_global(&test_global());
        assert!(def.is_tool_allowed("shell"));
        assert!(def.is_tool_allowed("anything"));
    }

    #[test]
    fn tool_filter_restricts() {
        let mut def = AgentDefinition::from_global(&test_global());
        def.allowed_tools = vec!["shell".to_string(), "file_read".to_string()];
        assert!(def.is_tool_allowed("shell"));
        assert!(def.is_tool_allowed("file_read"));
        assert!(!def.is_tool_allowed("web_search"));
    }

    #[test]
    fn skill_filter_restricts() {
        let mut def = AgentDefinition::from_global(&test_global());
        def.allowed_skills = vec!["coder".to_string()];
        assert!(def.is_skill_allowed("coder"));
        assert!(!def.is_skill_allowed("researcher"));
    }

    #[test]
    fn resolve_all_empty_config_produces_default() {
        let config = Config::default();
        let defs = AgentDefinition::resolve_all(&config);
        assert_eq!(defs.len(), 1);
        assert!(defs.contains_key("default"));
    }

    #[test]
    fn resolve_all_with_entries() {
        let mut config = Config::default();
        config.agents.insert(
            "coder".to_string(),
            AgentDefinitionConfig {
                model: "openai/gpt-4o".to_string(),
                instructions: "Write tests first.".to_string(),
                tools: vec!["shell".to_string()],
                ..AgentDefinitionConfig::default()
            },
        );
        let defs = AgentDefinition::resolve_all(&config);
        // "default" always present + "coder"
        assert_eq!(defs.len(), 2);
        assert!(defs.contains_key("default"));
        let coder = &defs["coder"];
        assert_eq!(coder.model, "openai/gpt-4o");
        assert_eq!(coder.instructions, "Write tests first.");
        assert_eq!(coder.allowed_tools, vec!["shell"]);
    }

    #[test]
    fn resolve_all_explicit_default_overrides() {
        let mut config = Config::default();
        config.agents.insert(
            "default".to_string(),
            AgentDefinitionConfig {
                instructions: "You are the main assistant.".to_string(),
                ..AgentDefinitionConfig::default()
            },
        );
        let defs = AgentDefinition::resolve_all(&config);
        assert_eq!(defs.len(), 1);
        assert_eq!(defs["default"].instructions, "You are the main assistant.");
    }
}
