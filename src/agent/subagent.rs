use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::{mpsc, Mutex};

use super::AgentLoop;

/// Result of a completed subagent task.
#[derive(Debug, Clone)]
pub struct SubagentResult {
    pub task_id: String,
    pub task_description: String,
    pub result: String,
    pub success: bool,
}

/// Subagent manager — spawns isolated background agent loops for async tasks.
///
/// Following nanobot's subagent pattern:
/// - A tool ("spawn_subagent") lets the LLM create background tasks
/// - Each subagent gets its own session key (isolated conversation)
/// - Results are sent back through a channel for delivery
/// - The main agent can check on running tasks
pub struct SubagentManager {
    agent: Arc<AgentLoop>,
    /// Currently running tasks: task_id → description
    running: Arc<Mutex<HashMap<String, String>>>,
    /// Channel for completed task results
    result_tx: mpsc::Sender<SubagentResult>,
}

impl SubagentManager {
    pub fn new(agent: Arc<AgentLoop>, result_tx: mpsc::Sender<SubagentResult>) -> Self {
        Self {
            agent,
            running: Arc::new(Mutex::new(HashMap::new())),
            result_tx,
        }
    }

    /// Spawn a new background task.
    /// Returns the task_id for tracking.
    pub async fn spawn(
        &self,
        task_description: &str,
        message: &str,
        channel: &str,
        chat_id: &str,
    ) -> Result<String> {
        let task_id = uuid::Uuid::new_v4().to_string()[..8].to_string();
        let session_key = format!("subagent:{}", task_id);

        // Register as running
        {
            let mut running = self.running.lock().await;
            running.insert(task_id.clone(), task_description.to_string());
        }

        tracing::info!(
            task_id = %task_id,
            description = %task_description,
            "Spawning subagent"
        );

        // Clone what we need for the spawned task
        let agent = self.agent.clone();
        let result_tx = self.result_tx.clone();
        let running = self.running.clone();
        let task_id_clone = task_id.clone();
        let description = task_description.to_string();
        let msg = message.to_string();
        let ch = channel.to_string();
        let cid = chat_id.to_string();

        tokio::spawn(async move {
            let result = match agent.process_message(&msg, &session_key, &ch, &cid).await {
                Ok(text) => SubagentResult {
                    task_id: task_id_clone.clone(),
                    task_description: description.clone(),
                    result: text,
                    success: true,
                },
                Err(e) => SubagentResult {
                    task_id: task_id_clone.clone(),
                    task_description: description.clone(),
                    result: format!("Task failed: {e}"),
                    success: false,
                },
            };

            // Remove from running
            {
                let mut running = running.lock().await;
                running.remove(&task_id_clone);
            }

            tracing::info!(
                task_id = %task_id_clone,
                success = result.success,
                "Subagent completed"
            );

            // Send result
            if let Err(e) = result_tx.send(result).await {
                tracing::error!(error = %e, "Failed to send subagent result");
            }
        });

        Ok(task_id)
    }

    /// List currently running tasks
    pub async fn list_running(&self) -> Vec<(String, String)> {
        let running = self.running.lock().await;
        running
            .iter()
            .map(|(id, desc)| (id.clone(), desc.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subagent_result_struct() {
        let result = SubagentResult {
            task_id: "abc123".to_string(),
            task_description: "Test task".to_string(),
            result: "Done".to_string(),
            success: true,
        };
        assert!(result.success);
        assert_eq!(result.task_id, "abc123");
    }
}
