//! Task orchestrator types.
//!
//! Defines the data model for intent analysis, task planning, and execution.

use serde::{Deserialize, Serialize};

/// How complex the user's request is — determines orchestration path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskComplexity {
    /// Direct passthrough to ReAct loop. No decomposition needed.
    Simple,
    /// Needs task decomposition and possibly parallel execution.
    Orchestrated,
}

/// Output of LLM-based intent analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentAnalysis {
    /// Whether this request needs orchestration or can go directly to the ReAct loop.
    #[serde(default = "default_simple")]
    pub complexity: String,
    /// High-level intent category (e.g. "product_research", "booking", "conversation").
    #[serde(default)]
    pub intent: String,
    /// Whether browser automation is likely needed.
    #[serde(default)]
    pub needs_browser: bool,
    /// Whether multiple independent sources/sites are involved.
    #[serde(default)]
    pub multi_source: bool,
    /// Named entities extracted (sites, brands, locations).
    #[serde(default)]
    pub entities: Vec<String>,
    /// Brief reasoning from the LLM (for debugging/logging).
    #[serde(default)]
    pub reasoning: String,
}

fn default_simple() -> String {
    "simple".to_string()
}

impl IntentAnalysis {
    /// Parse the `complexity` string field into a typed enum.
    pub fn task_complexity(&self) -> TaskComplexity {
        if self.complexity.eq_ignore_ascii_case("orchestrated") {
            TaskComplexity::Orchestrated
        } else {
            TaskComplexity::Simple
        }
    }

    /// Default analysis for simple passthrough.
    pub fn simple() -> Self {
        Self {
            complexity: "simple".to_string(),
            intent: String::new(),
            needs_browser: false,
            multi_source: false,
            entities: Vec::new(),
            reasoning: "fast-path or fallback".to_string(),
        }
    }
}

/// A single atomic task in the execution DAG.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subtask {
    /// Unique identifier within the plan (e.g. "t0", "t1").
    pub id: String,
    /// Human-readable description of what this subtask does.
    pub description: String,
    /// Full instructions for the subagent executing this task.
    pub prompt: String,
    /// IDs of subtasks that must complete before this one starts.
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// Which agent definition to use. Empty = "default".
    #[serde(default)]
    pub agent_id: String,
    /// Current execution status.
    #[serde(skip)]
    pub status: SubtaskStatus,
    /// Result after execution completes.
    #[serde(skip)]
    pub result: Option<SubtaskResult>,
}

/// Execution status of a subtask.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum SubtaskStatus {
    #[default]
    Pending,
    Running,
    Completed,
    Failed {
        reason: String,
    },
}

/// Result of a completed subtask.
#[derive(Debug, Clone)]
pub struct SubtaskResult {
    /// The text output from the subagent.
    pub output: String,
    /// Whether the subtask succeeded.
    pub success: bool,
}

/// The full task plan — a DAG of subtasks produced by the planner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskPlan {
    /// High-level objective (from user prompt).
    pub objective: String,
    /// Ordered list of subtasks. Max 6.
    pub subtasks: Vec<Subtask>,
    /// Optional verification criterion for the final result.
    #[serde(default)]
    pub verification: Option<String>,
}

/// Maximum number of subtasks in a plan.
pub const MAX_SUBTASKS: usize = 6;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intent_analysis_defaults_to_simple() {
        let analysis = IntentAnalysis::simple();
        assert_eq!(analysis.task_complexity(), TaskComplexity::Simple);
    }

    #[test]
    fn parses_orchestrated_complexity() {
        let json = r#"{"complexity":"orchestrated","intent":"product_research","needs_browser":true,"multi_source":true,"entities":["moto guzzi"],"reasoning":"multi-site search"}"#;
        let analysis: IntentAnalysis = serde_json::from_str(json).unwrap();
        assert_eq!(analysis.task_complexity(), TaskComplexity::Orchestrated);
        assert!(analysis.needs_browser);
        assert!(analysis.multi_source);
        assert_eq!(analysis.entities, vec!["moto guzzi"]);
    }

    #[test]
    fn parses_task_plan_json() {
        let json = r#"{
            "objective": "Find Moto Guzzi V7 Special listings",
            "subtasks": [
                {"id": "t0", "description": "Search Google", "prompt": "Search for...", "depends_on": []},
                {"id": "t1", "description": "Visit site A", "prompt": "Navigate to...", "depends_on": ["t0"]},
                {"id": "t2", "description": "Visit site B", "prompt": "Navigate to...", "depends_on": ["t0"]}
            ],
            "verification": "At least 3 sources with price data"
        }"#;
        let plan: TaskPlan = serde_json::from_str(json).unwrap();
        assert_eq!(plan.subtasks.len(), 3);
        assert_eq!(plan.subtasks[1].depends_on, vec!["t0"]);
        assert!(plan.verification.is_some());
    }

    #[test]
    fn subtask_status_defaults_to_pending() {
        let task = Subtask {
            id: "t0".into(),
            description: "test".into(),
            prompt: "do something".into(),
            depends_on: vec![],
            agent_id: String::new(),
            status: SubtaskStatus::default(),
            result: None,
        };
        assert_eq!(task.status, SubtaskStatus::Pending);
    }
}
