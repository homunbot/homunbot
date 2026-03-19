//! Multi-agent registry with config-based and LLM-based routing.
//!
//! Holds one `AgentLoop` per `AgentDefinition` and routes incoming
//! messages to the right agent.  Routing priority:
//!
//! 1. `contact.agent_override`
//! 2. `channel.default_agent`
//! 3. Session cache (previous LLM decision for this session)
//! 4. LLM classifier (optional, requires `routing.classifier_model`)
//! 5. Fallback → "default"

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::sync::RwLock;

use crate::config::Config;
use crate::contacts::Contact;
use crate::provider::Provider;
use crate::session::SessionManager;
use crate::storage::Database;
use crate::tools::ToolRegistry;

use super::definition::AgentDefinition;
use super::AgentLoop;

/// Pool of named agent loops with config + LLM routing.
pub struct AgentRegistry {
    agents: HashMap<String, Arc<AgentLoop>>,
    /// Agent definitions (kept for building the classifier prompt).
    definitions: HashMap<String, AgentDefinition>,
    /// In-memory cache: session_key → agent_id (from LLM classification).
    /// Resets on restart.  No persistence needed.
    session_cache: RwLock<HashMap<String, String>>,
}

impl AgentRegistry {
    /// Build agent loops for every definition.
    ///
    /// Creates one `AgentLoop` per entry, each with its own provider
    /// (via `AgentDefinition::create_provider`).  Shared state is
    /// passed by `Arc`/`Clone` so all agents use the same tools,
    /// sessions, config, and database.
    pub async fn build(
        definitions: HashMap<String, AgentDefinition>,
        config: Arc<RwLock<Config>>,
        session_manager: SessionManager,
        tool_registry: Arc<RwLock<ToolRegistry>>,
        db: Database,
    ) -> Result<Self> {
        let cfg = config.read().await;
        let mut agents = HashMap::new();

        for (id, def) in &definitions {
            let provider: Arc<dyn Provider> = def
                .create_provider(&cfg)
                .with_context(|| format!("Failed to create provider for agent '{id}'"))?;

            let agent = AgentLoop::new(
                provider,
                config.clone(),
                session_manager.clone(),
                tool_registry.clone(),
                db.clone(),
            )
            .await
            .with_agent_definition(def);

            tracing::info!(
                agent_id = %id,
                model = %def.effective_model(&cfg.agent),
                tools = ?def.allowed_tools,
                "Agent loop created"
            );

            agents.insert(id.clone(), Arc::new(agent));
        }

        drop(cfg);

        Ok(Self {
            agents,
            definitions,
            session_cache: RwLock::new(HashMap::new()),
        })
    }

    /// Look up an agent by ID.
    pub fn get(&self, agent_id: &str) -> Option<&Arc<AgentLoop>> {
        self.agents.get(agent_id)
    }

    /// The "default" agent — always present after `build()`.
    pub fn default_agent(&self) -> &Arc<AgentLoop> {
        self.agents
            .get("default")
            .expect("AgentRegistry must always contain a 'default' agent")
    }

    /// List all agent IDs (for web API / introspection).
    pub fn agent_ids(&self) -> Vec<&str> {
        self.agents.keys().map(|s| s.as_str()).collect()
    }

    /// Number of registered agents.
    pub fn len(&self) -> usize {
        self.agents.len()
    }

    /// Iterate mutably over agents before they are wrapped in `Arc`.
    ///
    /// **Must be called before any `Arc::clone`** — panics if the
    /// `Arc` has been shared.  Used by `main.rs` to apply setters
    /// (`set_message_tx`, `set_skill_registry`, etc.) to every agent.
    pub fn for_each_mut(&mut self, mut f: impl FnMut(&mut AgentLoop)) {
        for agent in self.agents.values_mut() {
            let agent =
                Arc::get_mut(agent).expect("for_each_mut must be called before the Arc is shared");
            f(agent);
        }
    }

    /// Get all agent Arc references for async setter application.
    ///
    /// Used by `main.rs` to apply async setters before the registry
    /// is wrapped in its own Arc.
    pub fn agents(&self) -> impl Iterator<Item = &Arc<AgentLoop>> {
        self.agents.values()
    }

    /// Route a message to the appropriate agent.
    ///
    /// Priority:
    /// 1. `contact.agent_override` — per-contact override
    /// 2. `channel.default_agent` — per-channel default
    /// 3. Session cache — previous LLM decision for this session
    /// 4. LLM classifier — fast model picks the agent (if configured)
    /// 5. Fallback → "default"
    pub async fn route(
        &self,
        contact: Option<&Contact>,
        channel_name: &str,
        config: &Config,
        session_key: &str,
        message: &str,
    ) -> &Arc<AgentLoop> {
        // 1. Contact override
        if let Some(contact) = contact {
            if let Some(ref override_id) = contact.agent_override {
                if !override_id.is_empty() {
                    if let Some(agent) = self.agents.get(override_id) {
                        tracing::debug!(
                            agent = %override_id,
                            contact = %contact.name,
                            "Routing via contact agent_override"
                        );
                        return agent;
                    }
                    tracing::warn!(
                        agent = %override_id,
                        contact = %contact.name,
                        "Contact agent_override references unknown agent, falling through"
                    );
                }
            }
        }

        // 2. Channel default_agent
        if let Some(behavior) = config.channels.behavior_for(channel_name) {
            let ch_agent = behavior.default_agent();
            if !ch_agent.is_empty() {
                if let Some(agent) = self.agents.get(ch_agent) {
                    tracing::debug!(
                        agent = %ch_agent,
                        channel = %channel_name,
                        "Routing via channel default_agent"
                    );
                    return agent;
                }
                tracing::warn!(
                    agent = %ch_agent,
                    channel = %channel_name,
                    "Channel default_agent references unknown agent, falling through"
                );
            }
        }

        // 3. Session cache (previous LLM decision)
        {
            let cache = self.session_cache.read().await;
            if let Some(cached_id) = cache.get(session_key) {
                if let Some(agent) = self.agents.get(cached_id) {
                    tracing::debug!(
                        agent = %cached_id,
                        session = %session_key,
                        "Routing via session cache"
                    );
                    return agent;
                }
            }
        }

        // 4. LLM classifier (only if configured and 2+ agents)
        if self.agents.len() > 1 && !config.routing.classifier_model.is_empty() {
            if let Some(agent_id) = self.classify_message(message, config).await {
                if let Some(agent) = self.agents.get(&agent_id) {
                    // Cache the decision for this session
                    self.session_cache
                        .write()
                        .await
                        .insert(session_key.to_string(), agent_id.clone());

                    tracing::info!(
                        agent = %agent_id,
                        session = %session_key,
                        "Routing via LLM classifier"
                    );
                    return agent;
                }
            }
        }

        // 5. Fallback
        self.default_agent()
    }

    /// Use a fast LLM to classify which agent should handle the message.
    ///
    /// Returns `None` on error or timeout (caller falls back to default).
    async fn classify_message(&self, message: &str, config: &Config) -> Option<String> {
        // Build the classification prompt
        let mut agent_list = String::new();
        for (id, def) in &self.definitions {
            let desc = if def.instructions.is_empty() {
                "General-purpose assistant".to_string()
            } else {
                // Truncate long instructions to keep the prompt small
                let instr = &def.instructions;
                if instr.len() > 200 {
                    format!("{}...", &instr[..200])
                } else {
                    instr.to_string()
                }
            };
            agent_list.push_str(&format!("- {id}: {desc}\n"));
        }

        let system_prompt = format!(
            "You are a message router. Given a user message, pick the most appropriate agent.\n\n\
             Available agents:\n{agent_list}\n\
             Reply with ONLY the agent id (e.g. \"default\" or \"coder\"). No explanation."
        );

        // Truncate very long messages to save classifier tokens
        let user_msg = if message.len() > 500 {
            format!("{}...", &message[..500])
        } else {
            message.to_string()
        };

        let req = crate::provider::OneShotRequest {
            system_prompt,
            user_message: user_msg,
            max_tokens: 32,
            temperature: 0.0,
            timeout_secs: 5,
            model: Some(config.routing.classifier_model.clone()),
            images: Vec::new(),
        };

        match crate::provider::llm_one_shot(config, req).await {
            Ok(resp) => {
                let agent_id = resp.content.trim().to_lowercase();
                // Validate: must be a known agent id
                if self.agents.contains_key(&agent_id) {
                    tracing::debug!(
                        agent = %agent_id,
                        latency_ms = resp.latency.as_millis(),
                        "LLM classifier chose agent"
                    );
                    Some(agent_id)
                } else {
                    tracing::warn!(
                        raw_response = %resp.content.trim(),
                        "LLM classifier returned unknown agent id, ignoring"
                    );
                    None
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "LLM classifier failed, falling back to default");
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AgentDefinitionConfig;

    #[test]
    fn route_fallback_to_default() {
        // We can't easily create a real AgentLoop in tests, so we test
        // routing logic indirectly via the config-based helpers.
        let config = Config::default();
        let behavior = config.channels.behavior_for("telegram");
        // With default config, default_agent is empty → falls through
        if let Some(b) = behavior {
            assert!(b.default_agent().is_empty());
        }
    }

    #[test]
    fn channel_default_agent_config_parsing() {
        let toml = r#"
[channels.telegram]
enabled = true
default_agent = "coder"
"#;
        let config: Config = toml::from_str(toml).unwrap();
        let behavior = config.channels.behavior_for("telegram").unwrap();
        assert_eq!(behavior.default_agent(), "coder");
    }

    #[test]
    fn agents_config_roundtrip() {
        let toml = r#"
[agents.coder]
model = "openai/gpt-4o"
instructions = "Write tests."
tools = ["shell"]
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert!(config.agents.contains_key("coder"));
        let coder = &config.agents["coder"];
        assert_eq!(coder.model, "openai/gpt-4o");
        assert_eq!(coder.instructions, "Write tests.");
        assert_eq!(coder.tools, vec!["shell"]);
    }

    #[test]
    fn routing_config_parsing() {
        let toml = r#"
[routing]
classifier_model = "anthropic/claude-haiku"
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.routing.classifier_model, "anthropic/claude-haiku");
    }

    #[test]
    fn routing_config_empty_by_default() {
        let config = Config::default();
        assert!(config.routing.classifier_model.is_empty());
    }
}
