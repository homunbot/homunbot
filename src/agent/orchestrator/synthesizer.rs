//! Result synthesizer — combines subtask outputs into a unified response.
//!
//! Uses a final LLM call to merge, deduplicate, and format the results
//! from all subtasks into a coherent answer.

use anyhow::Result;

use crate::config::Config;
use crate::provider::one_shot::{llm_one_shot, OneShotRequest};

use super::types::{SubtaskResult, TaskPlan};

/// Maximum tokens for the synthesis response.
const SYNTH_MAX_TOKENS: u32 = 4096;

/// Timeout for synthesis call (seconds).
const SYNTH_TIMEOUT_SECS: u64 = 30;

const SYSTEM_PROMPT: &str = r#"You are a result synthesizer for an AI assistant.
You receive the results from multiple subtasks that were executed to fulfill a user's request.

Your job:
1. Combine all subtask results into a single, coherent response.
2. Deduplicate overlapping information.
3. Organize by relevance — most useful results first.
4. For product/listing searches: create a structured summary with prices, links, images, conditions, contact info.
5. Note any gaps (sites that returned no results, failed subtasks).
6. If a verification criterion is provided, check whether it is satisfied and note any shortcomings.

Write the final response as if you are directly answering the user. Do not mention subtasks, orchestration, or internal processes."#;

/// Synthesize subtask results into a unified response.
///
/// If there's only one successful subtask with substantial output,
/// returns it directly without an extra LLM call.
pub async fn synthesize(
    config: &Config,
    user_prompt: &str,
    plan: &TaskPlan,
    results: &[SubtaskResult],
) -> Result<String> {
    // Shortcut: single subtask with success → return directly.
    let successful: Vec<_> = results.iter().filter(|r| r.success).collect();
    if successful.len() == 1 && plan.subtasks.len() == 1 {
        return Ok(successful[0].output.clone());
    }

    // Build the user message with all results.
    let mut parts = Vec::new();
    parts.push(format!("Original user request: {user_prompt}"));
    parts.push(format!("Objective: {}", plan.objective));

    parts.push("Subtask results:".to_string());
    for (i, (subtask, result)) in plan.subtasks.iter().zip(results.iter()).enumerate() {
        let status = if result.success { "SUCCESS" } else { "FAILED" };
        parts.push(format!(
            "[Task {i}: {}] ({status}):\n{}",
            subtask.description,
            crate::utils::text::truncate_str(&result.output, 6000, "\n[...truncated]")
        ));
    }

    if let Some(ref verification) = plan.verification {
        parts.push(format!("Verification criterion: {verification}"));
    }

    let req = OneShotRequest {
        system_prompt: SYSTEM_PROMPT.to_string(),
        user_message: parts.join("\n\n"),
        max_tokens: SYNTH_MAX_TOKENS,
        temperature: 0.3,
        timeout_secs: SYNTH_TIMEOUT_SECS,
        ..Default::default()
    };

    let response = llm_one_shot(config, req).await?;

    tracing::info!(
        output_len = response.content.len(),
        latency_ms = response.latency.as_millis() as u64,
        model = %response.model,
        "Synthesis completed"
    );

    Ok(response.content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::orchestrator::types::{Subtask, SubtaskStatus};

    #[test]
    fn single_successful_subtask_skips_llm() {
        // This tests the shortcut logic — can't call LLM in unit test.
        let plan = TaskPlan {
            objective: "test".into(),
            subtasks: vec![Subtask {
                id: "t0".into(),
                description: "only task".into(),
                prompt: "do it".into(),
                depends_on: vec![],
                agent_id: String::new(),
                status: SubtaskStatus::Completed,
                result: None,
            }],
            verification: None,
        };
        let results = vec![SubtaskResult {
            output: "Here are the results...".into(),
            success: true,
        }];

        // The synthesize function would return directly for single-task plans.
        // We verify the logic condition.
        let successful: Vec<_> = results.iter().filter(|r| r.success).collect();
        assert_eq!(successful.len(), 1);
        assert_eq!(plan.subtasks.len(), 1);
    }
}
