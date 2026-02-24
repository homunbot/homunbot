use std::path::PathBuf;

use anyhow::{Context as _, Result};
use serde::Deserialize;

use crate::config::Config;
use crate::provider::{ChatMessage, ChatRequest, Provider};
use crate::security::redact_vault_values;
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
    pub instructions_learned: usize,
    pub secrets_stored: usize,
    /// New chunk IDs + text from memory_chunks — need HNSW indexing after consolidation.
    pub new_chunks: Vec<(i64, String)>,
}

/// Parsed response from the v2 consolidation prompt.
/// Each field is optional — the LLM may not produce all of them.
#[derive(Debug, Default, Deserialize)]
struct ConsolidationResponseV2 {
    #[serde(default)]
    history_entry: String,
    #[serde(default)]
    memory_update: String,
    #[serde(default)]
    instructions: Vec<String>,
    #[serde(default)]
    vault_entries: Vec<VaultEntry>,
}

/// A secret detected in conversation that should be stored encrypted.
#[derive(Debug, Deserialize)]
struct VaultEntry {
    key: String,
    value: String,
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

    /// Run memory consolidation for a session (v2 — with classification).
    ///
    /// 1. Load messages since last consolidation
    /// 2. Load current long-term memory
    /// 3. Ask LLM to classify: history, facts, instructions, secrets
    /// 4. Store secrets in vault (encrypted), save instructions to INSTRUCTIONS.md
    /// 5. Append to HISTORY.md + daily file, update MEMORY.md + DB
    /// 6. Update last_consolidated pointer
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
                instructions_learned: 0,
                secrets_stored: 0,
                new_chunks: Vec::new(),
            });
        }

        let messages_to_process = &all_messages[process_start..process_end];

        tracing::info!(
            session = %session_key,
            total_messages = total,
            processing = messages_to_process.len(),
            from = process_start,
            to = process_end,
            "Starting memory consolidation (v2)"
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

        // Load existing instructions for deduplication
        let instructions_path = self.data_dir.join("brain/INSTRUCTIONS.md");
        let existing_instructions = std::fs::read_to_string(&instructions_path)
            .unwrap_or_default();

        // Load existing vault keys for deduplication (not values, for security)
        let existing_vault_keys: Vec<String> = match crate::storage::global_secrets() {
            Ok(secrets) => {
                // Get all keys with vault. prefix
                secrets.list_keys()
                    .into_iter()
                    .filter(|k| k.starts_with("vault."))
                    .collect()
            }
            Err(_) => Vec::new(),
        };

        tracing::debug!(
            existing_instructions_len = existing_instructions.len(),
            existing_vault_keys = existing_vault_keys.len(),
            "Loaded existing context for deduplication"
        );

        // Build v2 consolidation prompt with deduplication context
        let system_prompt = build_consolidation_prompt_v2(
            &current_memory,
            &existing_instructions,
            &existing_vault_keys,
        );

        let request = ChatRequest {
            messages: vec![
                ChatMessage::system(&system_prompt),
                ChatMessage::user(&conversation_text),
            ],
            tools: vec![],
            model: model.to_string(),
            max_tokens: 4096,
            temperature: 0.3,
        };

        let response = provider
            .chat(request)
            .await
            .context("Memory consolidation LLM call failed")?;

        let response_text = response.content.unwrap_or_default();

        // Debug: log raw response to understand parsing failures
        tracing::debug!(
            response_len = response_text.len(),
            response_preview = %response_text.chars().take(500).collect::<String>(),
            "Consolidation LLM raw response"
        );

        // Parse v2 response (falls back to v1 parsing if JSON structure differs)
        let parsed = parse_consolidation_response_v2(&response_text);

        // --- 1. Store secrets in vault (encrypted, FIRST — before anything touches memory) ---
        // DEDUPLICATION: Skip if value already exists with same key
        let mut secrets_stored = 0;
        let mut all_vault_entries: Vec<(String, String)> = Vec::new();
        if !parsed.vault_entries.is_empty() {
            match crate::storage::global_secrets() {
                Ok(secrets) => {
                    for entry in &parsed.vault_entries {
                        let key = crate::storage::SecretKey::custom(
                            &format!("vault.{}", entry.key),
                        );

                        // Check if value already exists (deduplication)
                        match secrets.get(&key) {
                            Ok(Some(existing_value)) if existing_value == entry.value => {
                                tracing::debug!(
                                    key = %entry.key,
                                    "Vault entry already exists with same value, skipping"
                                );
                                // Still add to redaction list (existing value should be redacted)
                                all_vault_entries.push((entry.key.clone(), entry.value.clone()));
                                continue; // Skip duplicate
                            }
                            Ok(Some(_)) => {
                                // Value changed, update it
                                tracing::info!(
                                    key = %entry.key,
                                    "Vault entry value changed, updating"
                                );
                            }
                            Ok(None) => {
                                // New entry
                            }
                            Err(e) => {
                                tracing::warn!(
                                    key = %entry.key,
                                    error = %e,
                                    "Failed to check existing vault value, storing anyway"
                                );
                            }
                        }

                        if let Err(e) = secrets.set(&key, &entry.value) {
                            tracing::warn!(
                                key = %entry.key,
                                error = %e,
                                "Failed to store secret in vault"
                            );
                        } else {
                            secrets_stored += 1;
                            tracing::info!(
                                key = %entry.key,
                                "Stored secret in vault during consolidation"
                            );
                        }

                        // Add to redaction list
                        all_vault_entries.push((entry.key.clone(), entry.value.clone()));
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Vault unavailable, skipping secret storage");
                }
            }
        }

        // --- 1b. Redact vault values from history and memory (vault leak prevention) ---
        // Load ALL existing vault values for redaction (not just new ones)
        if let Ok(secrets) = crate::storage::global_secrets() {
            for key in secrets.list_keys() {
                if key.starts_with("vault.") {
                    if let Ok(Some(value)) = secrets.get(&crate::storage::SecretKey::custom(&key)) {
                        let short_key = key.strip_prefix("vault.").unwrap_or(&key);
                        // Only add if not already in list
                        if !all_vault_entries.iter().any(|(k, _)| k == short_key) {
                            all_vault_entries.push((short_key.to_string(), value));
                        }
                    }
                }
            }
        }

        // Redact vault values from parsed content
        let history_entry = redact_vault_values(&parsed.history_entry, &all_vault_entries);
        let memory_update = redact_vault_values(&parsed.memory_update, &all_vault_entries);

        // Log if any redaction occurred
        if history_entry != parsed.history_entry || memory_update != parsed.memory_update {
            tracing::info!(
                history_redacted = history_entry != parsed.history_entry,
                memory_redacted = memory_update != parsed.memory_update,
                vault_entries_count = all_vault_entries.len(),
                "Redacted vault values from consolidation output"
            );
        }

        // --- 2. Save learned instructions to INSTRUCTIONS.md ---
        // DEDUPLICATION: Skip instructions similar to existing ones
        let new_instructions = deduplicate_instructions(&parsed.instructions, &existing_instructions);
        let instructions_learned = new_instructions.len();
        if !new_instructions.is_empty() {
            tracing::info!(
                total = parsed.instructions.len(),
                deduplicated = instructions_learned,
                "Saving deduplicated instructions"
            );
            self.append_instructions_md(&new_instructions)?;
        }

        // --- 3. Append to HISTORY.md and daily memory file ---
        if !history_entry.is_empty() {
            self.append_history_md(&history_entry)?;
            self.save_daily_md(&history_entry)?;
        }

        // --- 4. Update MEMORY.md + DB if memory changed ---
        let memory_updated = !memory_update.is_empty()
            && memory_update != current_memory;
        if memory_updated {
            self.save_memory_md(&memory_update)?;
            self.db.upsert_long_term_memory(&memory_update).await?;
        }

        // --- 5. Store history entry in DB (legacy) + memory_chunks (for vector/FTS5 search) ---
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let mut new_chunk_ids: Vec<(i64, String)> = Vec::new();

        if !history_entry.is_empty() {
            self.db
                .insert_memory(Some(session_key), &history_entry, "history")
                .await?;

            // Also insert into memory_chunks for hybrid search
            let chunk_id = self
                .db
                .insert_memory_chunk(
                    &today,
                    session_key,
                    "consolidation",
                    &history_entry,
                    "history",
                )
                .await?;
            new_chunk_ids.push((chunk_id, history_entry.clone()));
        }

        // Insert memory facts as a separate chunk (so they're independently searchable)
        if memory_updated {
            let chunk_id = self
                .db
                .insert_memory_chunk(
                    &today,
                    session_key,
                    "memory",
                    &memory_update,
                    "fact",
                )
                .await?;
            new_chunk_ids.push((chunk_id, memory_update.clone()));
        }

        // Insert each learned instruction as its own chunk
        for instruction in &parsed.instructions {
            let chunk_id = self
                .db
                .insert_memory_chunk(&today, session_key, "instruction", instruction, "instruction")
                .await?;
            new_chunk_ids.push((chunk_id, instruction.clone()));
        }

        // --- 6. Update last_consolidated pointer ---
        let new_consolidated = process_end as i64;
        self.db
            .upsert_session(session_key, new_consolidated)
            .await?;

        let messages_processed = messages_to_process.len();

        tracing::info!(
            session = %session_key,
            messages_processed,
            memory_updated,
            instructions_learned,
            secrets_stored,
            "Memory consolidation (v2) complete"
        );

        Ok(ConsolidationResult {
            history_entry: parsed.history_entry,
            memory_updated,
            messages_processed,
            instructions_learned,
            secrets_stored,
            new_chunks: new_chunk_ids,
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

    /// Save a daily memory file at ~/.homun/memory/YYYY-MM-DD.md
    /// Each consolidation appends to the day's file, creating human-readable logs.
    pub fn save_daily_md(&self, entry: &str) -> Result<()> {
        let memory_dir = self.data_dir.join("memory");
        std::fs::create_dir_all(&memory_dir)
            .context("Failed to create memory/ directory")?;

        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let path = memory_dir.join(format!("{today}.md"));

        use std::fs::OpenOptions;
        use std::io::Write;

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("Failed to open {}", path.display()))?;

        writeln!(file, "\n{entry}")
            .with_context(|| format!("Failed to append to {}", path.display()))?;

        tracing::debug!(path = %path.display(), "Saved daily memory");
        Ok(())
    }

    /// Append learned instructions to brain/INSTRUCTIONS.md
    /// These are directives the user taught in chat ("remember to always do X").
    pub fn append_instructions_md(&self, instructions: &[String]) -> Result<()> {
        if instructions.is_empty() {
            return Ok(());
        }

        let brain_dir = self.data_dir.join("brain");
        let path = brain_dir.join("INSTRUCTIONS.md");
        std::fs::create_dir_all(&brain_dir)
            .context("Failed to create brain directory")?;

        use std::fs::OpenOptions;
        use std::io::Write;

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .with_context(|| format!("Failed to open {}", path.display()))?;

        for instruction in instructions {
            writeln!(file, "- {instruction}")
                .with_context(|| format!("Failed to append to {}", path.display()))?;
        }

        tracing::info!(
            count = instructions.len(),
            path = %path.display(),
            "Appended learned instructions"
        );
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

/// Build the v2 consolidation prompt with classification, secret redaction,
/// instruction extraction, and deduplication context.
fn build_consolidation_prompt_v2(
    current_memory: &str,
    existing_instructions: &str,
    existing_vault_keys: &[String],
) -> String {
    let memory_section = if current_memory.is_empty() {
        "(empty)".to_string()
    } else {
        current_memory.to_string()
    };

    let instructions_section = if existing_instructions.is_empty() {
        "(none)".to_string()
    } else {
        existing_instructions.to_string()
    };

    let vault_keys_section = if existing_vault_keys.is_empty() {
        "(none)".to_string()
    } else {
        existing_vault_keys.join(", ")
    };

    format!(
        r#"You are a memory consolidation agent. Analyze this conversation and return a JSON object.

## CRITICAL RULES

- You are an ANALYZER, not an actor. Do NOT call any tools or functions.
- Do NOT write `[TOOL_CALL]`, `<tool_call`, or any command blocks.
- Do NOT try to write files or execute commands.
- Your ONLY output must be a single JSON object — nothing else.

## DEDUPLICATION RULES (VERY IMPORTANT)

- DO NOT add instructions that are semantically equivalent to existing ones
- DO NOT add vault entries for keys that already exist with the same value
- Only add NEW or CHANGED items
- If an instruction is similar to an existing one, skip it entirely

## Existing Instructions (DO NOT DUPLICATE)
{instructions_section}

## Existing Vault Keys (DO NOT DUPLICATE unless value changed)
{vault_keys_section}

## Tasks

1. **Summarize**: Write a `history_entry` paragraph (2-5 sentences) summarizing events, decisions, and topics. Start with a timestamp [YYYY-MM-DD HH:MM].

2. **Update memory**: Update `memory_update` with any new FACTS about the user:
   - Personal info: name, birth date, location, profession, family members
   - Preferences: likes, dislikes, hobbies, interests
   - Context: projects, tools used, technical decisions
   - Contact info: email, phone, addresses (use vault:// references for sensitive data)

   IMPORTANT: never include passwords, tokens, or secrets in this field — use vault_entries instead.

3. **Extract instructions**: ONLY behavioral directives the user explicitly taught you.

   ✅ CORRECT instructions (add to `instructions`):
   - "Always run clippy before committing"
   - "Use Telegram for urgent messages"
   - "Never commit without tests"
   - "From now on, respond in Italian"
   - "Remember to check the vault before asking for passwords"

   ❌ NOT instructions (these are FACTS — add to `memory_update`):
   - "Figli: Claudio (2008), Gaia (2010)" ← This is a FACT about family
   - "User lives in Milan" ← This is a FACT about location
   - "User likes pizza" ← This is a FACT about preferences
   - "Project uses Rust" ← This is a FACT about context

   Ask yourself: "Is this a rule about HOW I should behave?" If NO, it goes in memory_update, NOT instructions.

4. **Detect secrets**: If the user shared NEW passwords, tokens, API keys, or other sensitive data, add them to `vault_entries`. SKIP if the key already exists above unless the value changed.

## Current Long-term Memory
{memory_section}

## Response Format
Return ONLY valid JSON (no markdown fences, no tool calls, no code blocks):
{{
  "history_entry": "...",
  "memory_update": "...",
  "instructions": ["Always do X before Y", ...],
  "vault_entries": [{{"key": "aws_password", "value": "actual_secret"}}]
}}

If there are no NEW instructions or secrets, use empty arrays: `"instructions": [], "vault_entries": []`"#
    )
}

/// Parse the v2 consolidation response JSON.
/// Falls back gracefully: tries v2 structure, then v1, then raw text.
fn parse_consolidation_response_v2(response: &str) -> ConsolidationResponseV2 {
    let text = response.trim();

    // Strip markdown fences if present
    let text = text
        .strip_prefix("```json")
        .or_else(|| text.strip_prefix("```"))
        .unwrap_or(text);
    let text = text.strip_suffix("```").unwrap_or(text).trim();

    // Filter out tool call attempts (some LLMs try to call tools despite instructions)
    let text = filter_tool_calls(text);

    // Try full v2 parse
    if let Ok(parsed) = serde_json::from_str::<ConsolidationResponseV2>(text) {
        return parsed;
    }

    // Fallback: try as generic JSON (v1 compat)
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
        return ConsolidationResponseV2 {
            history_entry: history,
            memory_update: memory,
            ..Default::default()
        };
    }

    // Try to extract JSON from mixed content (text before/after JSON)
    if let Some(json) = extract_json_from_mixed(text) {
        if let Ok(parsed) = serde_json::from_str::<ConsolidationResponseV2>(&json) {
            tracing::debug!("Parsed consolidation JSON from mixed content");
            return parsed;
        }
    }

    // Last resort: use entire response as history entry
    let preview: String = text.chars().take(200).collect();
    tracing::warn!(
        preview = %preview,
        len = text.len(),
        "Failed to parse consolidation JSON, using raw text as history. \
         This means memory_update, instructions, and vault_entries will be empty!"
    );
    ConsolidationResponseV2 {
        history_entry: text.to_string(),
        ..Default::default()
    }
}

/// Filter out tool call blocks from text.
/// Some LLMs try to call tools despite being told not to.
fn filter_tool_calls<'a>(text: &'a str) -> &'a str {
    // Check if this looks like a tool call attempt
    if text.contains("[TOOL_CALL]") || text.contains("<tool_call") {
        // Try to find a valid JSON object in the text
        // Look for the pattern {" which is the start of our expected format
        if let Some(start) = text.find("{\"") {
            if let Some(end) = text.rfind('}') {
                if end > start {
                    let candidate = &text[start..=end];
                    // Verify it's valid JSON
                    if serde_json::from_str::<serde_json::Value>(candidate).is_ok() {
                        return candidate;
                    }
                }
            }
        }
    }
    text
}

/// Try to extract a JSON object from mixed content (text before/after JSON).
fn extract_json_from_mixed(text: &str) -> Option<String> {
    // Find the first { and last }
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end > start {
        Some(text[start..=end].to_string())
    } else {
        None
    }
}

/// Deduplicate instructions by filtering out those similar to existing ones.
/// Uses simple word overlap heuristic: if >70% of words overlap, consider it a duplicate.
fn deduplicate_instructions(new_instructions: &[String], existing_instructions: &str) -> Vec<String> {
    use std::collections::HashSet;
    
    // Parse existing instructions into word sets for comparison
    let existing_word_sets: Vec<HashSet<String>> = existing_instructions
        .lines()
        .filter(|l| !l.trim().is_empty() && l.trim().starts_with('-'))
        .map(|l| {
            l.to_lowercase()
                .split_whitespace()
                .map(|w| w.trim_matches(|c| "!.,;:".contains(c)).to_string())
                .collect()
        })
        .collect();

    new_instructions
        .iter()
        .filter(|instr| {
            let instr_words: HashSet<String> = instr
                .to_lowercase()
                .split_whitespace()
                .map(|w| w.trim_matches(|c| "!.,;:".contains(c)).to_string())
                .collect();
            
            // Check against all existing instructions
            for existing in &existing_word_sets {
                let intersection = instr_words.intersection(existing).count();
                let union = instr_words.union(existing).count();
                let similarity = if union == 0 { 0.0 } else { intersection as f64 / union as f64 };
                
                if similarity > 0.7 {
                    tracing::debug!(
                        instruction = %instr,
                        similarity = %similarity,
                        "Skipping similar instruction"
                    );
                    return false; // Too similar, skip
                }
            }
            true // Keep this instruction
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_v2_full_response() {
        let json = r#"{
            "history_entry": "[2025-02-16 14:30] User discussed Rust project.",
            "memory_update": "User is a Rust developer. AWS password: vault://aws_password",
            "instructions": ["Always run clippy before committing"],
            "vault_entries": [{"key": "aws_password", "value": "s3cret123"}]
        }"#;
        let parsed = parse_consolidation_response_v2(json);
        assert!(parsed.history_entry.contains("Rust"));
        assert!(parsed.memory_update.contains("vault://aws_password"));
        assert_eq!(parsed.instructions.len(), 1);
        assert_eq!(parsed.instructions[0], "Always run clippy before committing");
        assert_eq!(parsed.vault_entries.len(), 1);
        assert_eq!(parsed.vault_entries[0].key, "aws_password");
        assert_eq!(parsed.vault_entries[0].value, "s3cret123");
    }

    #[test]
    fn test_parse_v2_v1_compat() {
        // v1-format response (only history_entry + memory_update)
        let json = r#"{"history_entry": "summary", "memory_update": "facts"}"#;
        let parsed = parse_consolidation_response_v2(json);
        assert_eq!(parsed.history_entry, "summary");
        assert_eq!(parsed.memory_update, "facts");
        assert!(parsed.instructions.is_empty());
        assert!(parsed.vault_entries.is_empty());
    }

    #[test]
    fn test_parse_v2_with_fences() {
        let json = "```json\n{\"history_entry\": \"summary\", \"memory_update\": \"facts\", \"instructions\": [], \"vault_entries\": []}\n```";
        let parsed = parse_consolidation_response_v2(json);
        assert_eq!(parsed.history_entry, "summary");
        assert_eq!(parsed.memory_update, "facts");
    }

    #[test]
    fn test_parse_v2_invalid_fallback() {
        let text = "This is not JSON but a summary of what happened.";
        let parsed = parse_consolidation_response_v2(text);
        assert_eq!(parsed.history_entry, text);
        assert!(parsed.memory_update.is_empty());
        assert!(parsed.instructions.is_empty());
    }

    #[test]
    fn test_build_prompt_v2_empty_memory() {
        let prompt = build_consolidation_prompt_v2("", "", &[]);
        assert!(prompt.contains("(empty)"));
        assert!(prompt.contains("history_entry"));
        assert!(prompt.contains("instructions"));
        assert!(prompt.contains("vault_entries"));
        assert!(prompt.contains("vault://")); // Reference to vault format
        assert!(prompt.contains("DEDUPLICATION"));
    }

    #[test]
    fn test_build_prompt_v2_with_memory() {
        let prompt = build_consolidation_prompt_v2("User lives in Rome.", "", &[]);
        assert!(prompt.contains("User lives in Rome."));
        assert!(!prompt.contains("(empty)"));
        assert!(prompt.contains("instructions"));
        assert!(prompt.contains("vault_entries"));
    }

    #[test]
    fn test_build_prompt_v2_with_deduplication_context() {
        let prompt = build_consolidation_prompt_v2(
            "User data",
            "- Use Telegram\n",
            &["vault.password".to_string()],
        );
        assert!(prompt.contains("Use Telegram"));
        assert!(prompt.contains("vault.password"));
        assert!(prompt.contains("DEDUPLICATION"));
    }

    #[test]
    fn test_parse_v2_with_tool_call_prefix() {
        // Some LLMs try to call tools despite instructions not to
        let response = r#"[TOOL_CALL]
{tool => "write_file", args => {--path "test.md", --content "hello"}}
{"history_entry": "User discussed their preferences", "memory_update": "User likes pizza", "instructions": [], "vault_entries": []}"#;
        let parsed = parse_consolidation_response_v2(response);
        // Should extract the JSON after the tool call attempt
        assert!(parsed.history_entry.contains("preferences"));
        assert!(parsed.memory_update.contains("pizza"));
    }

    #[test]
    fn test_parse_v2_with_embedded_json() {
        // JSON embedded in text
        let response = r#"Here's the analysis:
{"history_entry": "Summary", "memory_update": "Facts", "instructions": ["Do X"], "vault_entries": []}
That's my analysis."#;
        let parsed = parse_consolidation_response_v2(response);
        assert_eq!(parsed.history_entry, "Summary");
        assert_eq!(parsed.memory_update, "Facts");
        assert_eq!(parsed.instructions.len(), 1);
    }

    #[test]
    fn test_deduplicate_instructions_exact_duplicate() {
        let existing = "- Usa Telegram per comunicare\n";
        let new = vec!["Usa Telegram per comunicare".to_string()];
        let result = deduplicate_instructions(&new, existing);
        assert!(result.is_empty(), "Exact duplicate should be filtered");
    }

    #[test]
    fn test_deduplicate_instructions_similar() {
        let existing = "- Usa Telegram per comunicare con l'utente\n";
        let new = vec![
            "Usa Telegram per comunicare con l'utente".to_string(),
            "Usa Telegram come canale preferito".to_string(), // Similar but different
            "Salva i dati nel vault".to_string(), // Completely different
        ];
        let result = deduplicate_instructions(&new, existing);
        assert_eq!(result.len(), 2, "Should keep 2 instructions (1 similar but different, 1 new)");
        assert!(result.iter().any(|i| i.contains("vault")));
    }

    #[test]
    fn test_deduplicate_instructions_new_only() {
        let existing = "- Instruction A\n";
        let new = vec![
            "Instruction B".to_string(), // Different
            "Instruction C".to_string(), // Different
        ];
        let result = deduplicate_instructions(&new, existing);
        assert_eq!(result.len(), 2, "Should keep all new instructions");
    }

    #[test]
    fn test_deduplicate_instructions_empty_existing() {
        let existing = "";
        let new = vec!["New instruction".to_string()];
        let result = deduplicate_instructions(&new, existing);
        assert_eq!(result.len(), 1, "Should keep instruction when no existing");
    }
}
