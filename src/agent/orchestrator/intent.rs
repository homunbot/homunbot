//! LLM-based intent analyzer.
//!
//! Replaces keyword matching with a fast LLM call to classify user intent.
//! Falls back to `Simple` on any error (fail-open design).

use crate::config::Config;
use crate::provider::one_shot::{llm_one_shot, OneShotRequest};

use super::types::{IntentAnalysis, TaskComplexity};

/// Minimum word count to consider LLM classification.
/// Shorter messages are almost always simple (greetings, short questions).
const MIN_WORDS_FOR_LLM: usize = 8;

/// Maximum tokens for the classification response.
const CLASSIFY_MAX_TOKENS: u32 = 192;

/// Timeout for classification call (seconds).
const CLASSIFY_TIMEOUT_SECS: u64 = 8;

const SYSTEM_PROMPT: &str = r#"Classify this request as "simple" or "orchestrated".

Reply with ONLY a JSON object. Example:
{"complexity":"simple","intent":"web_search","needs_browser":false,"multi_source":false,"entities":[],"reasoning":"single lookup"}

Rules:
- "simple": greetings, questions, single searches, code tasks, single-site actions — anything one agent can do in one pass.
- "orchestrated": finding/comparing things across MULTIPLE websites, research requiring data from several independent sources, tasks where subtasks can run in parallel.

Most messages are "simple". Use "orchestrated" only when the user needs data gathered from multiple independent sources and combined."#;

/// Classify user intent using a fast LLM call.
///
/// Uses the routing classifier model (e.g. Haiku) for speed.
/// Falls back to `Simple` on any error — fail-open design ensures
/// the existing ReAct loop always works as before.
pub async fn classify(config: &Config, user_prompt: &str) -> IntentAnalysis {
    // Fast-path: short messages are almost always simple.
    let word_count = user_prompt.split_whitespace().count();
    if word_count < MIN_WORDS_FOR_LLM {
        tracing::debug!(
            word_count,
            "Intent classifier: fast-path simple (short message)"
        );
        return IntentAnalysis::simple();
    }

    // Use the routing classifier model if configured, otherwise primary model.
    let model = classifier_model(config);

    let req = OneShotRequest {
        system_prompt: SYSTEM_PROMPT.to_string(),
        user_message: user_prompt.to_string(),
        max_tokens: CLASSIFY_MAX_TOKENS,
        temperature: 0.0,
        timeout_secs: CLASSIFY_TIMEOUT_SECS,
        model: Some(model.clone()),
        ..Default::default()
    };

    match llm_one_shot(config, req).await {
        Ok(response) => match parse_response(&response.content) {
            Some(analysis) => {
                tracing::info!(
                    complexity = %analysis.complexity,
                    intent = %analysis.intent,
                    needs_browser = analysis.needs_browser,
                    multi_source = analysis.multi_source,
                    model = %response.model,
                    latency_ms = response.latency.as_millis() as u64,
                    "Intent classified"
                );
                analysis
            }
            None => {
                tracing::warn!(
                    raw = %response.content,
                    "Intent classifier: failed to parse LLM response, falling back to simple"
                );
                IntentAnalysis::simple()
            }
        },
        Err(e) => {
            tracing::warn!(
                error = %e,
                "Intent classifier: LLM call failed, falling back to simple"
            );
            IntentAnalysis::simple()
        }
    }
}

/// Determine which model to use for classification.
fn classifier_model(config: &Config) -> String {
    // Prefer the routing classifier model (fast/cheap, e.g. Haiku).
    let routing_model = &config.routing.classifier_model;
    if !routing_model.is_empty() {
        return routing_model.clone();
    }
    // Fall back to primary agent model.
    config.agent.model.trim().to_string()
}

/// Parse the LLM response into an IntentAnalysis.
///
/// Multi-strategy parser: tries JSON first, then extracts from free text.
/// This ensures classification works even with models that can't produce
/// valid JSON (e.g. smaller/weaker models).
fn parse_response(raw: &str) -> Option<IntentAnalysis> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Strategy 1: Try direct JSON parse.
    if let Some(analysis) = try_json_parse(trimmed) {
        return Some(analysis);
    }

    // Strategy 2: Extract from free text — look for "orchestrated" or "simple".
    parse_from_text(trimmed)
}

/// Try parsing as JSON, handling markdown code fences.
fn try_json_parse(raw: &str) -> Option<IntentAnalysis> {
    let json_str = if raw.starts_with("```") {
        raw.strip_prefix("```json")
            .or_else(|| raw.strip_prefix("```"))
            .and_then(|s| s.strip_suffix("```"))
            .unwrap_or(raw)
            .trim()
    } else {
        raw
    };

    // Try full JSON parse.
    if let Ok(analysis) = serde_json::from_str::<IntentAnalysis>(json_str) {
        return Some(analysis);
    }

    // Try extracting JSON from a larger text (model may wrap it in explanation).
    if let Some(start) = json_str.find('{') {
        if let Some(end) = json_str.rfind('}') {
            let candidate = &json_str[start..=end];
            if let Ok(analysis) = serde_json::from_str::<IntentAnalysis>(candidate) {
                return Some(analysis);
            }
        }
    }

    None
}

/// Extract classification from free-text response when JSON fails.
///
/// Handles responses like "orchestrated", "ORCHESTRATED", "This is an
/// orchestrated task", "complexity: orchestrated", etc.
fn parse_from_text(raw: &str) -> Option<IntentAnalysis> {
    let lower = raw.to_ascii_lowercase();

    let is_orchestrated = lower.contains("orchestrated")
        || lower.contains("multi_source")
        || lower.contains("multi-source")
        || lower.contains("product_research")
        || lower.contains("comparison_shopping");

    if is_orchestrated {
        // Try to extract intent and other fields from text.
        let intent = extract_text_field(&lower, "intent");
        let needs_browser = lower.contains("needs_browser")
            || lower.contains("browser")
            || lower.contains("navigate");
        let multi_source = lower.contains("multi_source")
            || lower.contains("multi-source")
            || lower.contains("multiple");

        Some(IntentAnalysis {
            complexity: "orchestrated".to_string(),
            intent: intent.unwrap_or_else(|| "unknown".to_string()),
            needs_browser,
            multi_source,
            entities: Vec::new(),
            reasoning: format!("text-parsed from: {}", crate::utils::text::truncate_str(raw, 100, "...")),
        })
    } else if lower.contains("simple") || lower.len() < 30 {
        // Short or explicitly simple responses.
        Some(IntentAnalysis::simple())
    } else {
        // Can't determine — let caller handle None (will fallback to simple).
        None
    }
}

/// Extract a field value from text like `"intent": "product_research"` or
/// `intent: product_research`.
fn extract_text_field(text: &str, field: &str) -> Option<String> {
    // Try "field": "value" or field: value patterns.
    for pattern in [
        format!("\"{field}\""),
        format!("{field}:"),
        format!("{field} :"),
    ] {
        if let Some(pos) = text.find(&pattern) {
            let after = &text[pos + pattern.len()..];
            let value = after
                .trim()
                .trim_start_matches(':')
                .trim()
                .trim_matches('"')
                .trim_matches(',')
                .split_whitespace()
                .next()
                .map(|s| s.trim_matches('"').trim_matches(',').to_string());
            if let Some(v) = value {
                if !v.is_empty() {
                    return Some(v);
                }
            }
        }
    }
    None
}

/// Check if orchestration is enabled in config.
///
/// Allows disabling the orchestrator for rollback without code changes.
pub fn is_enabled(config: &Config) -> bool {
    config.agent.orchestrator_enabled
}

/// Whether intent analysis should be skipped entirely.
///
/// Skipped when orchestrator is disabled or no model is available.
pub fn should_skip(config: &Config) -> bool {
    !is_enabled(config) || config.agent.model.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_clean_json() {
        let raw = r#"{"complexity":"orchestrated","intent":"product_research","needs_browser":true,"multi_source":true,"entities":["moto guzzi"],"reasoning":"multi-site product search"}"#;
        let analysis = parse_response(raw).expect("should parse");
        assert_eq!(analysis.task_complexity(), TaskComplexity::Orchestrated);
        assert_eq!(analysis.intent, "product_research");
    }

    #[test]
    fn parses_json_with_code_fences() {
        let raw = "```json\n{\"complexity\":\"simple\",\"intent\":\"conversation\",\"needs_browser\":false,\"multi_source\":false,\"entities\":[],\"reasoning\":\"greeting\"}\n```";
        let analysis = parse_response(raw).expect("should parse fenced JSON");
        assert_eq!(analysis.task_complexity(), TaskComplexity::Simple);
    }

    #[test]
    fn empty_returns_none() {
        assert!(parse_response("").is_none());
    }

    #[test]
    fn short_garbage_falls_back_to_simple() {
        // Short non-JSON text falls back to simple (fail-open design).
        let analysis = parse_response("I don't understand").expect("short text → simple");
        assert_eq!(analysis.task_complexity(), TaskComplexity::Simple);
    }

    #[test]
    fn handles_partial_json_with_defaults() {
        // Missing optional fields should use serde defaults.
        let raw = r#"{"complexity":"simple","intent":"qa"}"#;
        let analysis = parse_response(raw).expect("should parse partial JSON");
        assert!(!analysis.needs_browser);
        assert!(analysis.entities.is_empty());
    }

    #[test]
    fn short_messages_are_simple() {
        // Messages under MIN_WORDS_FOR_LLM should not trigger LLM call.
        // We test the word count logic directly.
        let short = "ciao come stai";
        assert!(short.split_whitespace().count() < MIN_WORDS_FOR_LLM);

        let long = "trovami delle moto guzzi v7 special usate in buone condizioni con prezzo ragionevole";
        assert!(long.split_whitespace().count() >= MIN_WORDS_FOR_LLM);
    }
}
