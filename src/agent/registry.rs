//! Multi-agent registry and config-based router.
//!
//! Holds one `AgentLoop` per `AgentDefinition` and routes incoming
//! messages to the right agent based on contact override, channel
//! default, or fallback to "default".

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

/// Pool of named agent loops with config-based routing.
///
/// Each agent has its own provider (potentially a different LLM model)
/// but shares the same `ToolRegistry`, `SessionManager`, `Config`, and `Database`.
pub struct AgentRegistry {
    agents: HashMap<String, Arc<AgentLoop>>,
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

        Ok(Self { agents })
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
            let agent = Arc::get_mut(agent)
                .expect("for_each_mut must be called before the Arc is shared");
            f(agent);
        }
    }

    /// Route a message to the appropriate agent.
    ///
    /// Priority:
    /// 1. `contact.agent_override` — per-contact override
    /// 2. `channel.default_agent` — per-channel default
    /// 3. Fallback → "default"
    pub fn route(
        &self,
        contact: Option<&Contact>,
        channel_name: &str,
        config: &Config,
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

        // 3. Fallback
        self.default_agent()
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
}
