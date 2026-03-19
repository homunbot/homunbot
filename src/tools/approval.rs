//! Approval workflow for shell commands.
//!
//! Provides pre-execution approval with session-scoped allowlists and audit logging.
//! Inspired by ZeroClaw's implementation (https://github.com/zeroclaw-labs/zeroclaw)

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};
use uuid::Uuid;

use crate::config::{ApprovalConfig, AutonomyLevel};
use crate::utils::text::truncate_str;

/// Unique ID for a pending approval request
pub type ApprovalId = String;

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

/// A pending approval request waiting for user decision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingApproval {
    pub id: ApprovalId,
    pub tool_name: String,
    pub command: String,
    pub arguments: serde_json::Value,
    pub channel: String,
    pub chat_id: String,
    pub created_at: String,
}

/// Response to an approval request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalResponse {
    pub approved: bool,
    pub decision: Option<ApprovalDecision>,
    pub message: String,
    pub pending_id: Option<ApprovalId>,
}

/// Manages the interactive approval workflow.
#[derive(Debug)]
pub struct ApprovalManager {
    auto_approve: HashSet<String>,
    always_ask: HashSet<String>,
    autonomy_level: AutonomyLevel,
    session_allowlist: Mutex<HashSet<String>>,
    audit_log: Mutex<Vec<ApprovalLogEntry>>,
    pending_approvals: Mutex<HashMap<ApprovalId, PendingApproval>>,
}

impl ApprovalManager {
    pub fn from_config(config: &ApprovalConfig) -> Self {
        Self {
            auto_approve: config.auto_approve.iter().cloned().collect(),
            always_ask: config.always_ask.iter().cloned().collect(),
            autonomy_level: config.level,
            session_allowlist: Mutex::new(HashSet::new()),
            audit_log: Mutex::new(Vec::new()),
            pending_approvals: Mutex::new(HashMap::new()),
        }
    }

    pub fn new() -> Self {
        Self::from_config(&ApprovalConfig::default())
    }

    /// Get the current autonomy level
    pub fn autonomy_level(&self) -> AutonomyLevel {
        self.autonomy_level
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

    /// Create a pending approval request and return its ID
    pub fn create_pending(
        &self,
        tool_name: &str,
        command: &str,
        args: &serde_json::Value,
        channel: &str,
        chat_id: &str,
    ) -> ApprovalId {
        let id = Uuid::new_v4().to_string();
        let pending = PendingApproval {
            id: id.clone(),
            tool_name: tool_name.to_string(),
            command: command.to_string(),
            arguments: args.clone(),
            channel: channel.to_string(),
            chat_id: chat_id.to_string(),
            created_at: Utc::now().to_rfc3339(),
        };
        self.pending_approvals
            .lock()
            .unwrap()
            .insert(id.clone(), pending);
        id
    }

    /// Get all pending approvals
    pub fn get_pending(&self) -> Vec<PendingApproval> {
        self.pending_approvals
            .lock()
            .unwrap()
            .values()
            .cloned()
            .collect()
    }

    /// Get a specific pending approval by ID
    pub fn get_pending_by_id(&self, id: &str) -> Option<PendingApproval> {
        self.pending_approvals.lock().unwrap().get(id).cloned()
    }

    /// Approve a pending request
    pub fn approve(&self, id: &str, always: bool) -> Result<(), String> {
        let mut pending = self.pending_approvals.lock().unwrap();
        if let Some(req) = pending.remove(id) {
            let decision = if always {
                ApprovalDecision::Always
            } else {
                ApprovalDecision::Yes
            };
            self.record_decision(&req.tool_name, &req.arguments, decision, &req.channel);
            Ok(())
        } else {
            Err(format!("Pending approval not found: {}", id))
        }
    }

    /// Deny a pending request
    pub fn deny(&self, id: &str) -> Result<(), String> {
        let mut pending = self.pending_approvals.lock().unwrap();
        if let Some(req) = pending.remove(id) {
            self.record_decision(
                &req.tool_name,
                &req.arguments,
                ApprovalDecision::No,
                &req.channel,
            );
            Ok(())
        } else {
            Err(format!("Pending approval not found: {}", id))
        }
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
            self.session_allowlist
                .lock()
                .unwrap()
                .insert(tool_name.to_string());
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
    /// If not approved, creates a pending approval request.
    pub fn check_command(&self, command: &str, channel: &str, chat_id: &str) -> ApprovalResponse {
        let base_cmd = command.split_whitespace().next().unwrap_or("");

        if !self.needs_approval(base_cmd) {
            return ApprovalResponse {
                approved: true,
                decision: None,
                message: format!("Auto-approved: {}", base_cmd),
                pending_id: None,
            };
        }

        // Create pending approval
        let args = serde_json::json!({"command": command});
        let pending_id = self.create_pending("shell", command, &args, channel, chat_id);

        ApprovalResponse {
            approved: false,
            decision: None,
            message: format!(
                "Approval required for '{}' (channel: {}). Use Web UI /approvals to approve.",
                base_cmd, channel
            ),
            pending_id: Some(pending_id),
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
        serde_json::Value::Object(map) => map
            .iter()
            .map(|(k, v)| {
                let val = match v {
                    serde_json::Value::String(s) => truncate_str(s, 80, "..."),
                    other => truncate_str(&other.to_string(), 80, "..."),
                };
                format!("{}: {}", k, val)
            })
            .collect::<Vec<_>>()
            .join(", "),
        other => truncate_str(&other.to_string(), 120, "..."),
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
        let manager = ApprovalManager::from_config(&supervised_config());
        assert!(!manager.needs_approval("ls"));
        assert!(!manager.needs_approval("cat"));
    }

    #[test]
    fn always_ask_always_prompts() {
        let manager = ApprovalManager::from_config(&supervised_config());
        assert!(manager.needs_approval("rm"));
    }

    #[test]
    fn unknown_tool_needs_approval() {
        let manager = ApprovalManager::from_config(&supervised_config());
        assert!(manager.needs_approval("npm"));
    }

    #[test]
    fn full_autonomy_never_prompts() {
        let config = ApprovalConfig {
            level: AutonomyLevel::Full,
            ..Default::default()
        };
        let manager = ApprovalManager::from_config(&config);
        assert!(!manager.needs_approval("rm"));
        assert!(!manager.needs_approval("npm"));
    }

    #[test]
    fn always_response_adds_to_session_allowlist() {
        let manager = ApprovalManager::from_config(&supervised_config());
        assert!(manager.needs_approval("npm"));

        manager.record_decision(
            "npm",
            &serde_json::json!({"command": "npm install"}),
            ApprovalDecision::Always,
            "cli",
        );

        assert!(!manager.needs_approval("npm"));
    }

    #[test]
    fn always_ask_overrides_session_allowlist() {
        let manager = ApprovalManager::from_config(&supervised_config());
        manager.record_decision(
            "rm",
            &serde_json::json!({"command": "rm test"}),
            ApprovalDecision::Always,
            "cli",
        );
        assert!(manager.needs_approval("rm"));
    }
}
