//! Workflow engine — persistent multi-step autonomous tasks.
//!
//! Evolves the fire-and-forget subagent pattern into a durable orchestrator:
//! - Steps persist to SQLite, surviving restarts
//! - Inter-step context passing via shared JSON
//! - Approval gates pause execution until human confirmation
//! - Retry logic per step with configurable max retries

pub mod db;
pub mod engine;

use serde::{Deserialize, Serialize};
use crate::utils::text::truncate_str;

// ── Status enums ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStatus {
    Pending,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

impl WorkflowStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "running" => Self::Running,
            "paused" => Self::Paused,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            _ => Self::Pending,
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
}

impl StepStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "running" => Self::Running,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            "skipped" => Self::Skipped,
            _ => Self::Pending,
        }
    }
}

// ── Core structs ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub id: String,
    pub name: String,
    pub objective: String,
    pub status: WorkflowStatus,
    pub steps: Vec<WorkflowStep>,
    pub context: serde_json::Value,
    pub created_by: Option<String>,
    pub deliver_to: Option<String>,
    /// If this workflow was spawned by an automation, the automation's ID.
    pub automation_id: Option<String>,
    /// If this workflow was spawned by an automation run, the run's ID.
    pub automation_run_id: Option<String>,
    pub current_step_idx: usize,
    pub created_at: String,
    pub updated_at: Option<String>,
    pub completed_at: Option<String>,
    pub error: Option<String>,
    /// Profile this workflow belongs to. None = global/unscoped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    pub id: String,
    pub workflow_id: String,
    pub idx: usize,
    pub name: String,
    pub instruction: String,
    pub status: StepStatus,
    pub approval_required: bool,
    pub result: Option<String>,
    pub error: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub retry_count: u32,
    pub max_retries: u32,
    /// Agent to execute this step (MAG-4). Defaults to "default".
    pub agent_id: String,
}

// ── Creation request (from LLM tool) ────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct WorkflowCreateRequest {
    pub name: String,
    pub objective: String,
    pub steps: Vec<StepDefinition>,
    pub deliver_to: Option<String>,
    /// Link to the parent automation (set by scheduler, None for user-created).
    #[serde(default)]
    pub automation_id: Option<String>,
    /// Link to the specific automation run that triggered this workflow.
    #[serde(default)]
    pub automation_run_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StepDefinition {
    pub name: String,
    pub instruction: String,
    #[serde(default)]
    pub approval_required: bool,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// Agent to execute this step (MAG-4).  None = "default".
    #[serde(default)]
    pub agent_id: Option<String>,
}

fn default_max_retries() -> u32 {
    1
}

// ── Workflow event (engine → gateway notification) ───────────────────

#[derive(Debug, Clone)]
pub enum WorkflowEvent {
    StepStarted {
        workflow_id: String,
        workflow_name: String,
        step_idx: usize,
        total_steps: usize,
        step_name: String,
        deliver_to: Option<String>,
    },
    StepCompleted {
        workflow_id: String,
        workflow_name: String,
        step_idx: usize,
        total_steps: usize,
        step_name: String,
        result_summary: String,
        deliver_to: Option<String>,
    },
    ApprovalNeeded {
        workflow_id: String,
        workflow_name: String,
        step_idx: usize,
        total_steps: usize,
        step_name: String,
        step_instruction: String,
        deliver_to: Option<String>,
    },
    WorkflowCompleted {
        workflow_id: String,
        workflow_name: String,
        total_steps: usize,
        summary: String,
        deliver_to: Option<String>,
    },
    WorkflowFailed {
        workflow_id: String,
        workflow_name: String,
        step_idx: usize,
        total_steps: usize,
        error: String,
        deliver_to: Option<String>,
    },
}

impl WorkflowEvent {
    pub fn workflow_id(&self) -> &str {
        match self {
            Self::StepStarted { workflow_id, .. }
            | Self::StepCompleted { workflow_id, .. }
            | Self::ApprovalNeeded { workflow_id, .. }
            | Self::WorkflowCompleted { workflow_id, .. }
            | Self::WorkflowFailed { workflow_id, .. } => workflow_id,
        }
    }

    pub fn workflow_name(&self) -> &str {
        match self {
            Self::StepStarted { workflow_name, .. }
            | Self::StepCompleted { workflow_name, .. }
            | Self::ApprovalNeeded { workflow_name, .. }
            | Self::WorkflowCompleted { workflow_name, .. }
            | Self::WorkflowFailed { workflow_name, .. } => workflow_name,
        }
    }

    pub fn deliver_to(&self) -> Option<&str> {
        match self {
            Self::StepStarted { deliver_to, .. }
            | Self::StepCompleted { deliver_to, .. }
            | Self::ApprovalNeeded { deliver_to, .. }
            | Self::WorkflowCompleted { deliver_to, .. }
            | Self::WorkflowFailed { deliver_to, .. } => deliver_to.as_deref(),
        }
    }

    /// Format the event as a user-facing notification message.
    pub fn format_notification(&self) -> String {
        match self {
            Self::StepStarted {
                step_idx,
                step_name,
                total_steps,
                ..
            } => {
                format!("[Workflow] Starting step {step_idx}/{total_steps}: \"{step_name}\"")
            }
            Self::StepCompleted {
                step_idx,
                step_name,
                result_summary,
                ..
            } => {
                let summary = truncate_str(result_summary, 200, "\u{2026}");
                format!("[Workflow] Step {step_idx} \"{step_name}\" completed: {summary}")
            }
            Self::ApprovalNeeded {
                workflow_name,
                step_idx,
                step_name,
                step_instruction,
                ..
            } => {
                let instruction = truncate_str(step_instruction, 300, "\u{2026}");
                format!(
                    "[Workflow] \"{workflow_name}\" paused — approval needed for step {step_idx} \"{step_name}\":\n{instruction}\n\nReply \"approve {workflow_name}\" to continue or \"cancel {workflow_name}\" to abort."
                )
            }
            Self::WorkflowCompleted {
                workflow_name,
                summary,
                ..
            } => {
                format!("[Workflow] \"{workflow_name}\" completed.\n\n{summary}")
            }
            Self::WorkflowFailed {
                workflow_name,
                error,
                ..
            } => {
                let err = truncate_str(error, 300, "\u{2026}");
                format!("[Workflow] \"{workflow_name}\" failed: {err}")
            }
        }
    }

    /// Structured progress data for the web UI donut chart.
    pub fn to_progress_json(&self) -> serde_json::Value {
        match self {
            Self::StepStarted {
                workflow_id,
                workflow_name,
                step_idx,
                total_steps,
                step_name,
                ..
            } => serde_json::json!({
                "workflow_id": workflow_id,
                "workflow_name": workflow_name,
                "status": "step_started",
                "completed_steps": *step_idx,
                "total_steps": total_steps,
                "current_step": step_name,
            }),
            Self::StepCompleted {
                workflow_id,
                workflow_name,
                step_idx,
                total_steps,
                step_name,
                ..
            } => serde_json::json!({
                "workflow_id": workflow_id,
                "workflow_name": workflow_name,
                "status": "running",
                "completed_steps": step_idx + 1,
                "total_steps": total_steps,
                "current_step": step_name,
            }),
            Self::ApprovalNeeded {
                workflow_id,
                workflow_name,
                step_idx,
                total_steps,
                step_name,
                ..
            } => serde_json::json!({
                "workflow_id": workflow_id,
                "workflow_name": workflow_name,
                "status": "paused",
                "completed_steps": *step_idx,
                "total_steps": total_steps,
                "current_step": step_name,
            }),
            Self::WorkflowCompleted {
                workflow_id,
                workflow_name,
                total_steps,
                ..
            } => serde_json::json!({
                "workflow_id": workflow_id,
                "workflow_name": workflow_name,
                "status": "completed",
                "completed_steps": total_steps,
                "total_steps": total_steps,
            }),
            Self::WorkflowFailed {
                workflow_id,
                workflow_name,
                step_idx,
                total_steps,
                error,
                ..
            } => serde_json::json!({
                "workflow_id": workflow_id,
                "workflow_name": workflow_name,
                "status": "failed",
                "completed_steps": *step_idx,
                "total_steps": total_steps,
                "error": truncate_str(error, 200, "\u{2026}"),
            }),
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workflow_status_roundtrip() {
        for status in [
            WorkflowStatus::Pending,
            WorkflowStatus::Running,
            WorkflowStatus::Paused,
            WorkflowStatus::Completed,
            WorkflowStatus::Failed,
            WorkflowStatus::Cancelled,
        ] {
            assert_eq!(WorkflowStatus::from_str(status.as_str()), status);
        }
    }

    #[test]
    fn step_status_roundtrip() {
        for status in [
            StepStatus::Pending,
            StepStatus::Running,
            StepStatus::Completed,
            StepStatus::Failed,
            StepStatus::Skipped,
        ] {
            assert_eq!(StepStatus::from_str(status.as_str()), status);
        }
    }

    #[test]
    fn terminal_statuses() {
        assert!(!WorkflowStatus::Pending.is_terminal());
        assert!(!WorkflowStatus::Running.is_terminal());
        assert!(!WorkflowStatus::Paused.is_terminal());
        assert!(WorkflowStatus::Completed.is_terminal());
        assert!(WorkflowStatus::Failed.is_terminal());
        assert!(WorkflowStatus::Cancelled.is_terminal());
    }
}
