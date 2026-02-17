use std::path::PathBuf;

use anyhow::{Context as _, Result};

use crate::config::Config;
use crate::provider::{ChatMessage, ChatRequest, Provider};
use crate::storage::Database;

/// Memory consolidation system — LLM-powered summarization.
///
/// Follows nanobot's pattern with two-tier storage:
/// 1. **MEMORY.md** (long-term facts): user preferences, context, personal info
/// 2. **HISTORY.md** (event log): timestamped summaries of past conversations
///
/// Consolidation is triggered when message count exceeds threshold.
/// Runs as a background task (non-blocking).
pub struct MemoryConsolidator {
    db: Database,
    data_dir: PathBuf,
}

/// Result of a consolidation run
#[derive(Debug)]
pub struct ConsolidationResult {
    pub history_entry: String,
    pub memory_updated: bool,
    pub messages_processed: usize,
}

impl MemoryConsolidator {
    pub fn new(db: Database) -> Self {
        Self {
            db,
            data_dir: Config::data_dir(),
        }
    }

    /// Check if consolidation is needed for a session
    pub async fn should_consolidate(
        &self,
        session_key: &str,
        memory_window: u32,
    ) -> Result<bool> {
        let count = self.db.count_messages(session_key).await?;
        Ok(count > memory_window as i64)
    }

    /// Run memory consolidation for a session.
    ///
    /// 1. Load messages since last consolidation
    /// 2. Load current long-term memory
    /// 3. Ask LLM to produce: history_entry + memory_update
    /// 4. Append to HISTORY.md, update MEMORY.md + DB
    /// 5. Update last_consolidated pointer
    pub async fn consolidate(
        &self,
        session_key: &str,
        memory_window: u32,
        provider: &dyn Provider,
        model: &str,
    ) -> Result<ConsolidationResult> {
        // How many to keep in active session
        let keep_count = (memory_window / 2) as i64;

        // Load all messages for the session
        let all_messages = self.db.load_messages(session_key, 10000).await?;
        let total = all_messages.len() as i64;

        // Get last_consolidated pointer
        let session = self.db.load_session(session_key).await?;
        let last_consolidated = session.map(|s| s.last_consolidated).unwrap_or(0);

        // Messages to process: from last_consolidated to (total - keep_count)
        let process_end = (total - keep_count).max(0) as usize;
        let process_start = last_consolidated as usize;

        if process_start >= process_end {
            return Ok(ConsolidationResult {
                history_entry: String::new(),
                memory_updated: false,
                messages_processed: 0,
            });
        }

        let messages_to_process = &all_messages[process_start..process_end];

        tracing::info!(
            session = %session_key,
            total_messages = total,
            processing = messages_to_process.len(),
            from = process_start,
            to = process_end,
            "Starting memory consolidation"
        );

        // Format messages for the consolidation prompt
        let conversation_text = messages_to_process
            .iter()
            .map(|m| {
                let tools: String = if m.tools_used != "[]" {
                    format!(" [tools: {}]", m.tools_used.trim_matches(|c| c == '[' || c == ']' || c == '"'))
                } else {
                    String::new()
                };
                format!("[{}] {}{}: {}", m.timestamp, m.role.to_uppercase(), tools, m.content)
            })
            .collect::<Vec<_>>()
            .join("\n");

        // Load current long-term memory
        let current_memory = self.load_memory_md().unwrap_or_default();

        // Build consolidation prompt
        let system_prompt = build_consolidation_prompt(&current_memory);

        let request = ChatRequest {
            messages: vec![
                ChatMessage::system(&system_prompt),
                ChatMessage::user(&conversation_text),
            ],
            tools: vec![], // No tools for consolidation
            model: model.to_string(),
            max_tokens: 2048,
            temperature: 0.3, // Low temperature for factual summarization
        };

        let response = provider
            .chat(request)
            .await
            .context("Memory consolidation LLM call failed")?;

        let response_text = response.content.unwrap_or_default();

        // Parse the JSON response
        let (history_entry, memory_update) = parse_consolidation_response(&response_text);

        // Append to HISTORY.md
        if !history_entry.is_empty() {
            self.append_history_md(&history_entry)?;
        }

        // Update MEMORY.md + DB if memory changed
        let memory_updated = !memory_update.is_empty() && memory_update != current_memory;
        if memory_updated {
            self.save_memory_md(&memory_update)?;
            self.db.upsert_long_term_memory(&memory_update).await?;
        }

        // Append history entry to DB as well
        if !history_entry.is_empty() {
            self.db
                .insert_memory(Some(session_key), &history_entry, "history")
                .await?;
        }

        // Update last_consolidated pointer
        let new_consolidated = process_end as i64;
        self.db
            .upsert_session(session_key, new_consolidated)
            .await?;

        let messages_processed = messages_to_process.len();

        tracing::info!(
            session = %session_key,
            messages_processed,
            memory_updated,
            "Memory consolidation complete"
        );

        Ok(ConsolidationResult {
            history_entry,
            memory_updated,
            messages_processed,
        })
    }

    // --- File operations for MEMORY.md / HISTORY.md ---

    /// Load MEMORY.md content
    pub fn load_memory_md(&self) -> Option<String> {
        let path = self.data_dir.join("MEMORY.md");
        std::fs::read_to_string(&path).ok().filter(|s| !s.trim().is_empty())
    }

    /// Save MEMORY.md (overwrite)
    fn save_memory_md(&self, content: &str) -> Result<()> {
        let path = self.data_dir.join("MEMORY.md");
        std::fs::create_dir_all(&self.data_dir)
            .context("Failed to create data directory")?;
        std::fs::write(&path, content)
            .with_context(|| format!("Failed to write {}", path.display()))?;
        tracing::info!(path = %path.display(), "Updated MEMORY.md");
        Ok(())
    }

    /// Append an entry to HISTORY.md
    fn append_history_md(&self, entry: &str) -> Result<()> {
        let path = self.data_dir.join("HISTORY.md");
        std::fs::create_dir_all(&self.data_dir)
            .context("Failed to create data directory")?;

        use std::fs::OpenOptions;
        use std::io::Write;

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("Failed to open {}", path.display()))?;

        writeln!(file, "\n{entry}")
            .with_context(|| format!("Failed to append to {}", path.display()))?;

        tracing::debug!(path = %path.display(), "Appended to HISTORY.md");
        Ok(())
    }
}

/// Build the consolidation prompt for the LLM.
fn build_consolidation_prompt(current_memory: &str) -> String {
    let memory_section = if current_memory.is_empty() {
        "(empty)".to_string()
    } else {
        current_memory.to_string()
    };

    format!(
        r#"You are a memory consolidation agent. Process this conversation and return a JSON object with exactly two keys:

1. "history_entry": A paragraph (2-5 sentences) summarizing the key events, decisions, and topics discussed. Start with a timestamp like [YYYY-MM-DD HH:MM]. Include enough detail to be useful when found by search later.

2. "memory_update": The updated long-term memory content. Add any new facts discovered: user location, preferences, personal info, habits, project context, technical decisions, tools/services used. If nothing new, return the existing content unchanged.

## Current Long-term Memory
{memory_section}

Respond with ONLY valid JSON, no markdown fences."#
    )
}

/// Parse the consolidation response JSON.
/// Handles common LLM JSON issues gracefully.
fn parse_consolidation_response(response: &str) -> (String, String) {
    // Try to parse as JSON
    let text = response.trim();

    // Strip markdown fences if present
    let text = text
        .strip_prefix("```json")
        .or_else(|| text.strip_prefix("```"))
        .unwrap_or(text);
    let text = text.strip_suffix("```").unwrap_or(text).trim();

    if let Ok(val) = serde_json::from_str::<serde_json::Value>(text) {
        let history = val
            .get("history_entry")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let memory = val
            .get("memory_update")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        return (history, memory);
    }

    // Fallback: use entire response as history entry, keep existing memory
    tracing::warn!("Failed to parse consolidation JSON, using raw text as history");
    (text.to_string(), String::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_consolidation_response_valid() {
        let json = r#"{"history_entry": "[2025-02-16 14:30] User asked about Rust.", "memory_update": "User is a Rust developer."}"#;
        let (history, memory) = parse_consolidation_response(json);
        assert!(history.contains("Rust"));
        assert!(memory.contains("Rust developer"));
    }

    #[test]
    fn test_parse_consolidation_response_with_fences() {
        let json = "```json\n{\"history_entry\": \"summary\", \"memory_update\": \"facts\"}\n```";
        let (history, memory) = parse_consolidation_response(json);
        assert_eq!(history, "summary");
        assert_eq!(memory, "facts");
    }

    #[test]
    fn test_parse_consolidation_response_invalid() {
        let text = "This is not JSON but a summary of what happened.";
        let (history, memory) = parse_consolidation_response(text);
        assert_eq!(history, text);
        assert!(memory.is_empty());
    }

    #[test]
    fn test_build_consolidation_prompt_empty_memory() {
        let prompt = build_consolidation_prompt("");
        assert!(prompt.contains("(empty)"));
        assert!(prompt.contains("history_entry"));
        assert!(prompt.contains("memory_update"));
    }

    #[test]
    fn test_build_consolidation_prompt_with_memory() {
        let prompt = build_consolidation_prompt("User lives in Rome.");
        assert!(prompt.contains("User lives in Rome."));
        assert!(!prompt.contains("(empty)"));
    }
}
