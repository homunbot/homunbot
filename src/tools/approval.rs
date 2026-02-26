//! Approval workflow for shell commands.
//!
//! Provides pre-execution approval with session-scoped allowlists and audit logging.
//! Inspired by ZeroClaw's implementation (https://github.com/zeroclaw-labs/zeroclaw)

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::{Arc, Mutex, OnceLock};

use crate::config::{ApprovalConfig, AutonomyLevel};

/// A single audit log entry for an approval decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalLogEntry {
    pub timestamp: String,
    pub tool_name: String,
    pub arguments_summary: String,
    pub decision: ApprovalDecision,
    pub channel: String,
}

/// User's response to an approval request
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalDecision {
    Yes,
    No,
    Always,
}

/// A single request to approve a tool call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub channel: String,
}

/// Response to an approval request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalResponse {
    pub approved: bool,
    pub decision: Option<ApprovalDecision>,
    pub message: String,
}

/// Manages the interactive approval workflow.
#[derive(Debug)]
pub struct ApprovalManager {
    auto_approve: HashSet<String>,
    always_ask: HashSet<String>,
    autonomy_level: AutonomyLevel,
    session_allowlist: Mutex<HashSet<String>>,
    audit_log: Mutex<Vec<ApprovalLogEntry>>,
}

impl ApprovalManager {
    pub fn from_config(config: &ApprovalConfig) -> Self {
        Self {
            auto_approve: config.auto_approve.iter().cloned().collect(),
            always_ask: config.always_ask.iter().cloned().collect(),
            autonomy_level: config.level,
            session_allowlist: Mutex::new(HashSet::new()),
            audit_log: Mutex::new(Vec::new()),
        }
    }

    pub fn new() -> Self {
        Self::from_config(&ApprovalConfig::default())
    }

    /// Check whether a tool call requires interactive approval.
    pub fn needs_approval(&self, tool_name: &str) -> bool {
        if self.autonomy_level == AutonomyLevel::Full {
            return false;
        }
        if self.autonomy_level == AutonomyLevel::ReadOnly {
            return true;
        }
        if self.always_ask.contains(tool_name) {
            return true;
        }
        if self.auto_approve.contains(tool_name) {
            return false;
        }
        let allowlist = self.session_allowlist.lock().unwrap();
        !allowlist.contains(tool_name)
    }

    /// Record an approval decision and update session state
    pub fn record_decision(
        &self,
        tool_name: &str,
        args: &serde_json::Value,
        decision: ApprovalDecision,
        channel: &str,
    ) {
        if decision == ApprovalDecision::Always {
            self.session_allowlist.lock().unwrap().insert(tool_name.to_string());
        }
        let entry = ApprovalLogEntry {
            timestamp: Utc::now().to_rfc3339(),
            tool_name: tool_name.to_string(),
            arguments_summary: summarize_args(args),
            decision,
            channel: channel.to_string(),
        };
        self.audit_log.lock().unwrap().push(entry);
    }

    pub fn audit_log(&self) -> Vec<ApprovalLogEntry> {
        self.audit_log.lock().unwrap().clone()
    }

    pub fn session_allowlist(&self) -> HashSet<String> {
        self.session_allowlist.lock().unwrap().clone()
    }

    pub fn clear_session(&self) {
        self.session_allowlist.lock().unwrap().clear();
    }

    /// Process a command and return whether it's approved.
    pub fn check_command(&self, command: &str, channel: &str) -> ApprovalResponse {
        let base_cmd = command.split_whitespace().next().unwrap_or("");
        
        if !self.needs_approval(base_cmd) {
            return ApprovalResponse {
                approved: true,
                decision: None,
                message: format!("Auto-approved: {}", base_cmd),
            };
        }

        ApprovalResponse {
            approved: false,
            decision: None,
            message: format!(
                "Approval required for '{}' (channel: {}). Use Web UI /permissions.",
                base_cmd, channel
            ),
        }
    }
}

impl Default for ApprovalManager {
    fn default() -> Self {
        Self::new()
    }
}

fn summarize_args(args: &serde_json::Value) -> String {
    match args {
        serde_json::Value::Object(map) => {
            map.iter()
                .map(|(k, v)| {
                    let val = match v {
                        serde_json::Value::String(s) => truncate(s, 80),
                        other => truncate(&other.to_string(), 80),
                    };
                    format!("{}: {}", k, val)
                })
                .collect::<Vec<_>>()
                .join(", ")
        }
        other => truncate(&other.to_string(), 120),
    }
}

fn truncate(input: &str, max_chars: usize) -> String {
    let mut chars = input.chars();
    let truncated: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{}...", truncated)
    } else {
        input.to_string()
    }
}

// Global instance
static GLOBAL_APPROVAL_MANAGER: OnceLock<Arc<ApprovalManager>> = OnceLock::new();

pub fn global_approval_manager() -> Option<Arc<ApprovalManager>> {
    GLOBAL_APPROVAL_MANAGER.get().cloned()
}

pub fn init_approval_manager(config: &ApprovalConfig) {
    let manager = Arc::new(ApprovalManager::from_config(config));
    let _ = GLOBAL_APPROVAL_MANAGER.set(manager);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn supervised_config() -> ApprovalConfig {
        ApprovalConfig {
            level: AutonomyLevel::Supervised,
            auto_approve: vec!["ls".into(), "cat".into()],
            always_ask: vec!["rm".into()],
            ..Default::default()
        }
    }

    #[test]
    fn auto_approve_skips_prompt() {
        let mgr = ApprovalManager::from_config(&supervised_config());
        assert!(!mgr.needs_approval("ls"));
        assert!(!mgr.needs_approval("cat"));
    }

    #[test]
    fn always_ask_always_prompts() {
        let mgr = ApprovalManager::from_config(&supervised_config());
        assert!(mgr.needs_approval("rm"));
    }

    #[test]
    fn unknown_tool_needs_approval() {
        let mgr = ApprovalManager::from_config(&supervised_config());
        assert!(mgr.needs_approval("npm"));
    }

    #[test]
    fn full_autonomy_never_prompts() {
        let config = ApprovalConfig {
            level: AutonomyLevel::Full,
            ..Default::default()
        };
        let mgr = ApprovalManager::from_config(&config);
        assert!(!mgr.needs_approval("rm"));
        assert!(!mgr.needs_approval("npm"));
    }

    #[test]
    fn always_response_adds_to_session_allowlist() {
        let mgr = ApprovalManager::from_config(&supervised_config());
        assert!(mgr.needs_approval("npm"));

        mgr.record_decision(
            "npm",
            &serde_json::json!({"command": "npm install"}),
            ApprovalDecision::Always,
            "cli",
        );

        assert!(!mgr.needs_approval("npm"));
    }

    #[test]
    fn always_ask_overrides_session_allowlist() {
        let mgr = ApprovalManager::from_config(&supervised_config());
        mgr.record_decision(
            "rm",
            &serde_json::json!({"command": "rm test"}),
            ApprovalDecision::Always,
            "cli",
        );
        assert!(mgr.needs_approval("rm"));
    }
}
