use std::path::PathBuf;

use anyhow::{Context as _, Result};
use chrono::Datelike;
use serde::Deserialize;

use std::sync::Arc;

use crate::config::Config;
use crate::provider::{ChatMessage, ChatRequest, Provider, RequestPriority};
use crate::security::redact_vault_values;
use crate::storage::MemoryBackend;

/// Memory consolidation system — LLM-powered summarization.
///
/// Follows nanobot's pattern with two-tier storage:
/// 1. **MEMORY.md** (long-term facts): user preferences, context, personal info
/// 2. **HISTORY.md** (event log): timestamped summaries of past conversations
///
/// Consolidation is triggered when message count exceeds threshold.
/// Runs as a background task (non-blocking).
pub struct MemoryConsolidator {
    store: Arc<dyn MemoryBackend>,
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
    /// Chunk IDs pruned during budget enforcement — need HNSW removal.
    pub pruned_chunk_ids: Vec<i64>,
}

/// Result of a session compaction run
#[derive(Debug)]
pub struct CompactionResult {
    pub messages_removed: u64,
    pub summary_inserted: bool,
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
    instructions: Vec<ScoredInstruction>,
    #[serde(default)]
    vault_entries: Vec<VaultEntry>,
}

/// An instruction with an importance score (1-5).
///
/// Accepts both `{"text": "...", "importance": N}` and plain `"string"` (importance defaults to 3).
#[derive(Debug)]
struct ScoredInstruction {
    text: String,
    /// Importance 1-5: 1=trivial, 3=normal, 5=critical behavioral rule.
    importance: i32,
}

impl<'de> serde::Deserialize<'de> for ScoredInstruction {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de;

        struct ScoredInstructionVisitor;

        impl<'de> de::Visitor<'de> for ScoredInstructionVisitor {
            type Value = ScoredInstruction;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a string or {\"text\": \"...\", \"importance\": N}")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> std::result::Result<Self::Value, E> {
                Ok(ScoredInstruction {
                    text: v.to_string(),
                    importance: 3,
                })
            }

            fn visit_map<M: de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> std::result::Result<Self::Value, M::Error> {
                let mut text = None;
                let mut importance = 3i32;
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "text" => text = Some(map.next_value()?),
                        "importance" => importance = map.next_value()?,
                        _ => {
                            let _ = map.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ScoredInstruction {
                    text: text.unwrap_or_default(),
                    importance,
                })
            }
        }

        deserializer.deserialize_any(ScoredInstructionVisitor)
    }
}

/// A secret detected in conversation that should be stored encrypted.
#[derive(Debug, Deserialize)]
struct VaultEntry {
    key: String,
    value: String,
}

impl MemoryConsolidator {
    /// Create from a concrete Database (backwards-compatible convenience).
    pub fn new(db: crate::storage::Database) -> Self {
        Self {
            store: Arc::new(db),
            data_dir: Config::data_dir(),
        }
    }

    /// Create from any MemoryBackend implementation.
    pub fn from_store(store: Arc<dyn MemoryBackend>) -> Self {
        Self {
            store,
            data_dir: Config::data_dir(),
        }
    }

    /// Check if consolidation is needed for a session
    pub async fn should_consolidate(&self, session_key: &str, memory_window: u32) -> Result<bool> {
        let count = self.store.count_messages(session_key).await?;
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
        contact_id: Option<i64>,
        agent_id: Option<&str>,
        profile_brain_dir: Option<std::path::PathBuf>,
        profile_id: Option<i64>,
    ) -> Result<ConsolidationResult> {
        // How many to keep in active session
        let keep_count = (memory_window / 2) as i64;

        // Load all messages for the session
        let all_messages = self.store.load_messages(session_key, 10000).await?;
        let total = all_messages.len() as i64;

        // Get last_consolidated pointer
        let session = self.store.load_session(session_key).await?;
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
                pruned_chunk_ids: Vec::new(),
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
                    format!(
                        " [tools: {}]",
                        m.tools_used
                            .trim_matches(|c| c == '[' || c == ']' || c == '"')
                    )
                } else {
                    String::new()
                };
                format!(
                    "[{}] {}{}: {}",
                    m.timestamp,
                    m.role.to_uppercase(),
                    tools,
                    m.content
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        // Load current long-term memory
        let current_memory = self.load_memory_md().unwrap_or_default();

        // Load existing instructions for deduplication (profile-scoped if available)
        let instructions_path = profile_brain_dir
            .as_ref()
            .map(|d| d.join("INSTRUCTIONS.md"))
            .unwrap_or_else(|| self.data_dir.join("brain/INSTRUCTIONS.md"));
        let existing_instructions = std::fs::read_to_string(&instructions_path).unwrap_or_default();

        // Load existing vault keys for deduplication (not values, for security)
        let existing_vault_keys: Vec<String> = match crate::storage::global_secrets() {
            Ok(secrets) => {
                // Get all keys with vault. prefix
                secrets
                    .list_keys()
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
            think: None,
            priority: RequestPriority::Low,
        };

        let response = provider
            .chat(request)
            .await
            .context("Memory consolidation LLM call failed")?;

        let response_text = response.content.unwrap_or_default();

        // Debug: log raw response to understand parsing failures
        tracing::debug!(
            response_len = response_text.len(),
            response_preview = %crate::utils::text::truncate_str(&response_text, 500, ""),
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
                        let key =
                            crate::storage::SecretKey::custom(&format!("vault.{}", entry.key));

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
        let instruction_texts: Vec<String> =
            parsed.instructions.iter().map(|i| i.text.clone()).collect();
        let new_instruction_texts =
            deduplicate_instructions(&instruction_texts, &existing_instructions);
        // Keep the ScoredInstructions that survived deduplication
        let new_instructions: Vec<&ScoredInstruction> = parsed
            .instructions
            .iter()
            .filter(|i| new_instruction_texts.contains(&i.text))
            .collect();
        let instructions_learned = new_instructions.len();
        if !new_instructions.is_empty() {
            tracing::info!(
                total = parsed.instructions.len(),
                deduplicated = instructions_learned,
                "Saving deduplicated instructions"
            );
            let texts: Vec<String> = new_instructions.iter().map(|i| i.text.clone()).collect();
            // Write to profile-scoped brain dir if available, else global
            let global_brain = self.data_dir.join("brain");
            let target_brain_dir = profile_brain_dir
                .as_deref()
                .unwrap_or(&global_brain);
            self.append_instructions_to(target_brain_dir, &texts)?;
        }

        // --- 3. Append to HISTORY.md and daily memory file ---
        if !history_entry.is_empty() {
            self.append_history_md(&history_entry)?;
            self.save_daily_md(&history_entry)?;
        }

        // --- 4. Update MEMORY.md + DB if memory changed ---
        let memory_updated = !memory_update.is_empty() && memory_update != current_memory;
        if memory_updated {
            // Write to profile-scoped MEMORY.md if available, else global
            if let Some(ref pdir) = profile_brain_dir {
                let path = pdir.join("MEMORY.md");
                std::fs::create_dir_all(pdir).ok();
                std::fs::write(&path, &memory_update)
                    .with_context(|| format!("Failed to write {}", path.display()))?;
                tracing::info!(path = %path.display(), "Updated profile MEMORY.md");
            } else {
                self.save_memory_md(&memory_update)?;
            }
            self.store.upsert_long_term_memory(&memory_update).await?;
        }

        // --- 5. Store history entry in DB (legacy) + memory_chunks (for vector/FTS5 search) ---
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let mut new_chunk_ids: Vec<(i64, String)> = Vec::new();

        if !history_entry.is_empty() {
            self.store
                .insert_memory(Some(session_key), &history_entry, "history")
                .await?;

            // Also insert into memory_chunks for hybrid search
            // History entries get default importance 2 (routine summaries)
            let chunk_id = self
                .store
                .insert_memory_chunk(
                    &today,
                    session_key,
                    "consolidation",
                    &history_entry,
                    "history",
                    contact_id,
                    agent_id,
                    2,
                    profile_id,
                )
                .await?;
            new_chunk_ids.push((chunk_id, history_entry.clone()));
        }

        // Insert memory facts as a separate chunk (so they're independently searchable)
        // Memory facts get default importance 3 (user profile data)
        if memory_updated {
            let chunk_id = self
                .store
                .insert_memory_chunk(
                    &today,
                    session_key,
                    "memory",
                    &memory_update,
                    "fact",
                    contact_id,
                    agent_id,
                    3,
                    profile_id,
                )
                .await?;
            new_chunk_ids.push((chunk_id, memory_update.clone()));
        }

        // Insert each learned instruction as its own chunk (with LLM-assigned importance)
        for instruction in &new_instructions {
            let importance = instruction.importance.clamp(1, 5);
            let chunk_id = self
                .store
                .insert_memory_chunk(
                    &today,
                    session_key,
                    "instruction",
                    &instruction.text,
                    "instruction",
                    contact_id,
                    agent_id,
                    importance,
                    profile_id,
                )
                .await?;
            new_chunk_ids.push((chunk_id, instruction.text.clone()));
        }

        // --- 6. Update last_consolidated pointer ---
        let new_consolidated = process_end as i64;
        self.store
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
            pruned_chunk_ids: Vec::new(), // Pruning happens in the caller
        })
    }

    /// Prune memory chunks if total count exceeds the budget.
    ///
    /// Returns IDs of deleted chunks (for HNSW index cleanup).
    /// A budget of 0 means no limit.
    pub async fn prune_if_over_budget(&self, max_chunks: u32) -> Result<Vec<i64>> {
        if max_chunks == 0 {
            return Ok(Vec::new());
        }
        let count = self.store.count_memory_chunks().await?;
        if count <= max_chunks as i64 {
            return Ok(Vec::new());
        }
        self.store.prune_memory_chunks_to_budget(max_chunks).await
    }

    /// Create a hierarchical summary for a completed time period.
    ///
    /// Checks if a week/month boundary was crossed since the last consolidation,
    /// and if so, summarizes all chunks in that period into a digest.
    /// Summaries are stored in `memory_summaries` for search augmentation.
    pub async fn maybe_summarize_period(
        &self,
        provider: &dyn Provider,
        model: &str,
        contact_id: Option<i64>,
        agent_id: Option<&str>,
    ) -> Result<()> {
        let today = chrono::Local::now().date_naive();

        // Check if last week needs summarization (only on Monday or later)
        if today.weekday() == chrono::Weekday::Mon || today.weekday() == chrono::Weekday::Tue {
            let last_monday = today
                - chrono::Duration::days(today.weekday().num_days_from_monday() as i64)
                - chrono::Duration::weeks(1);
            let last_sunday = last_monday + chrono::Duration::days(6);
            let start = last_monday.format("%Y-%m-%d").to_string();
            let end = last_sunday.format("%Y-%m-%d").to_string();

            if !self.store.has_memory_summary("week", &start).await? {
                self.summarize_range("week", &start, &end, provider, model, contact_id, agent_id)
                    .await?;
            }
        }

        // Check if last month needs summarization (only in first 3 days of new month)
        if today.day() <= 3 {
            let last_month = if today.month() == 1 {
                chrono::NaiveDate::from_ymd_opt(today.year() - 1, 12, 1)
            } else {
                chrono::NaiveDate::from_ymd_opt(today.year(), today.month() - 1, 1)
            };
            if let Some(month_start) = last_month {
                let month_end = if today.month() == 1 {
                    chrono::NaiveDate::from_ymd_opt(today.year() - 1, 12, 31)
                } else {
                    chrono::NaiveDate::from_ymd_opt(today.year(), today.month(), 1)
                        .and_then(|d| d.checked_sub_days(chrono::Days::new(1)))
                };
                if let Some(month_end) = month_end {
                    let start = month_start.format("%Y-%m-%d").to_string();
                    let end = month_end.format("%Y-%m-%d").to_string();
                    if !self.store.has_memory_summary("month", &start).await? {
                        self.summarize_range(
                            "month", &start, &end, provider, model, contact_id, agent_id,
                        )
                        .await?;
                    }
                }
            }
        }

        Ok(())
    }

    /// Summarize all memory chunks in a date range into a single digest.
    #[allow(clippy::too_many_arguments)]
    async fn summarize_range(
        &self,
        period: &str,
        start_date: &str,
        end_date: &str,
        provider: &dyn Provider,
        model: &str,
        contact_id: Option<i64>,
        agent_id: Option<&str>,
    ) -> Result<()> {
        let chunks = self.store.load_chunks_in_range(start_date, end_date).await?;
        if chunks.is_empty() {
            return Ok(());
        }

        // Build text from chunks
        let chunks_text = chunks
            .iter()
            .map(|c| format!("[{}] {}: {}", c.date, c.memory_type, c.content))
            .collect::<Vec<_>>()
            .join("\n");

        let system_prompt = format!(
            "You are a memory summarizer. Summarize the following {period} of memory entries \
             ({start_date} to {end_date}) into a concise digest (3-10 sentences).\n\n\
             Focus on:\n\
             - Key events and decisions\n\
             - Important facts learned about the user\n\
             - Patterns or recurring themes\n\
             - Pending items or unresolved topics\n\n\
             Write in the same language as the entries. Output ONLY the summary."
        );

        let request = ChatRequest {
            messages: vec![
                ChatMessage::system(&system_prompt),
                ChatMessage::user(&chunks_text),
            ],
            tools: vec![],
            model: model.to_string(),
            max_tokens: 1024,
            temperature: 0.3,
            think: None,
            priority: RequestPriority::Low,
        };

        let response = provider
            .chat(request)
            .await
            .context("Period summarization LLM call failed")?;

        if let Some(summary) = response.content.filter(|s| !s.trim().is_empty()) {
            self.store
                .insert_memory_summary(period, start_date, end_date, &summary, contact_id, agent_id)
                .await?;
            tracing::info!(
                period,
                start_date,
                end_date,
                chunks = chunks.len(),
                summary_len = summary.len(),
                "Created hierarchical memory summary"
            );
        }

        Ok(())
    }

    // --- File operations for MEMORY.md / HISTORY.md ---

    /// Load MEMORY.md content
    pub fn load_memory_md(&self) -> Option<String> {
        let path = self.data_dir.join("MEMORY.md");
        std::fs::read_to_string(&path)
            .ok()
            .filter(|s| !s.trim().is_empty())
    }

    /// Save MEMORY.md (overwrite)
    fn save_memory_md(&self, content: &str) -> Result<()> {
        let path = self.data_dir.join("MEMORY.md");
        std::fs::create_dir_all(&self.data_dir).context("Failed to create data directory")?;
        std::fs::write(&path, content)
            .with_context(|| format!("Failed to write {}", path.display()))?;
        tracing::info!(path = %path.display(), "Updated MEMORY.md");
        Ok(())
    }

    /// Save a daily memory file at ~/.homun/memory/YYYY-MM-DD.md
    /// Each consolidation appends to the day's file, creating human-readable logs.
    pub fn save_daily_md(&self, entry: &str) -> Result<()> {
        let memory_dir = self.data_dir.join("memory");
        std::fs::create_dir_all(&memory_dir).context("Failed to create memory/ directory")?;

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

    /// Append learned instructions to INSTRUCTIONS.md in the given brain directory.
    /// These are directives the user taught in chat ("remember to always do X").
    pub fn append_instructions_md(&self, instructions: &[String]) -> Result<()> {
        self.append_instructions_to(&self.data_dir.join("brain"), instructions)
    }

    /// Append learned instructions to INSTRUCTIONS.md in a specific brain directory.
    pub fn append_instructions_to(
        &self,
        brain_dir: &std::path::Path,
        instructions: &[String],
    ) -> Result<()> {
        if instructions.is_empty() {
            return Ok(());
        }

        let path = brain_dir.join("INSTRUCTIONS.md");
        std::fs::create_dir_all(&brain_dir).context("Failed to create brain directory")?;

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
        std::fs::create_dir_all(&self.data_dir).context("Failed to create data directory")?;

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

    // --- Session compaction ---

    /// Check if session compaction is needed.
    pub async fn should_compact(&self, session_key: &str, memory_window: u32) -> Result<bool> {
        let count = self.store.count_messages(session_key).await?;
        Ok(count > memory_window as i64)
    }

    /// Compact a session: summarize old messages via LLM, prune them, insert summary.
    ///
    /// Must run AFTER memory consolidation so knowledge is extracted first.
    /// If LLM summarization fails, falls back to plain truncation with a note.
    pub async fn compact_session(
        &self,
        session_key: &str,
        memory_window: u32,
        provider: &dyn Provider,
        model: &str,
    ) -> Result<CompactionResult> {
        let keep_count = memory_window / 2;
        let total = self.store.count_messages(session_key).await?;

        if total <= memory_window as i64 {
            return Ok(CompactionResult {
                messages_removed: 0,
                summary_inserted: false,
            });
        }

        let old_messages = self.store.load_old_messages(session_key, keep_count).await?;
        if old_messages.is_empty() {
            return Ok(CompactionResult {
                messages_removed: 0,
                summary_inserted: false,
            });
        }

        let messages_to_remove = old_messages.len();

        tracing::info!(
            session = %session_key,
            total_messages = total,
            removing = messages_to_remove,
            keeping = keep_count,
            "Starting session compaction"
        );

        // Format old messages for summarization
        let conversation_text = old_messages
            .iter()
            .map(|m| format!("[{}] {}: {}", m.timestamp, m.role.to_uppercase(), m.content))
            .collect::<Vec<_>>()
            .join("\n");

        // Try LLM summarization, fallback to truncation note
        let summary = match self
            .summarize_for_compaction(&conversation_text, provider, model)
            .await
        {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    session = %session_key,
                    "LLM summarization failed, falling back to truncation"
                );
                format!(
                    "{} older messages were removed to keep context manageable.",
                    messages_to_remove
                )
            }
        };

        // Delete old messages
        let deleted = self.store.delete_old_messages(session_key, keep_count).await?;

        // Insert summary as system message
        self.store
            .insert_message(
                session_key,
                "system",
                &format!("[Session Summary]\n{}", summary),
                &[],
            )
            .await?;

        // Reset last_consolidated pointer so consolidation knows the new layout
        let new_consolidated = (keep_count + 1) as i64;
        self.store
            .upsert_session(session_key, new_consolidated)
            .await?;

        tracing::info!(
            session = %session_key,
            messages_removed = deleted,
            summary_len = summary.len(),
            "Session compaction complete"
        );

        Ok(CompactionResult {
            messages_removed: deleted,
            summary_inserted: true,
        })
    }

    /// Ask the LLM to produce a concise summary of old conversation messages.
    async fn summarize_for_compaction(
        &self,
        conversation_text: &str,
        provider: &dyn Provider,
        model: &str,
    ) -> Result<String> {
        let request = ChatRequest {
            messages: vec![
                ChatMessage::system(COMPACTION_SUMMARY_PROMPT),
                ChatMessage::user(conversation_text),
            ],
            tools: vec![],
            model: model.to_string(),
            max_tokens: 1024,
            temperature: 0.2,
            think: None,
            priority: RequestPriority::Low,
        };

        let response = provider
            .chat(request)
            .await
            .context("Compaction summary LLM call failed")?;

        response
            .content
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| anyhow::anyhow!("LLM returned empty compaction summary"))
    }
}

const COMPACTION_SUMMARY_PROMPT: &str = "\
You are a conversation summarizer. Given a conversation history, produce a concise summary \
(3-8 sentences) that captures:

1. Key topics discussed and decisions made
2. Important facts or information shared
3. Any pending tasks or unresolved questions
4. The overall context needed to continue the conversation naturally

Rules:
- Write in the same language as the conversation
- Be factual and concise — this summary replaces the original messages
- Do NOT include greetings, pleasantries, or tool call details
- Focus on information the assistant needs to continue helping effectively
- Output ONLY the summary text, nothing else";

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
   Each instruction must include an **importance** score (1-5):
   - 1 = minor preference, rarely relevant
   - 2 = useful habit, situational
   - 3 = standard behavioral rule
   - 4 = important directive, frequently relevant
   - 5 = critical rule, must always follow

   ✅ CORRECT instructions (add to `instructions`):
   - {{"text": "Always run clippy before committing", "importance": 4}}
   - {{"text": "Use Telegram for urgent messages", "importance": 3}}
   - {{"text": "Never commit without tests", "importance": 5}}
   - {{"text": "Respond in Italian", "importance": 4}}

   Also extract **process patterns** — implicit preferences about HOW tasks should be done.
   These emerge from corrections, repeated requests, or explicit preferences:
   - {{"text": "When searching travel options, compare at least 3 alternatives with prices", "importance": 3}}
   - {{"text": "When booking trains, prefer Frecciarossa and 1st class", "importance": 3}}
   - {{"text": "For web research, use multiple sources and cross-reference", "importance": 2}}
   - {{"text": "When the user asks to buy something, always show options before purchasing", "importance": 4}}

   Process patterns are instructions too — they tell you how to approach a CATEGORY of tasks.

   ❌ NOT instructions (these are FACTS — add to `memory_update`):
   - "Figli: Claudio (2008), Gaia (2010)" ← This is a FACT about family
   - "User lives in Milan" ← This is a FACT about location
   - "User likes pizza" ← This is a FACT about preferences
   - "Project uses Rust" ← This is a FACT about context

   Ask yourself: "Is this a rule about HOW I should behave, or HOW I should approach a type of task?" If NO, it goes in memory_update, NOT instructions.

4. **Detect secrets**: If the user shared NEW passwords, tokens, API keys, or other sensitive data, add them to `vault_entries`. SKIP if the key already exists above unless the value changed.

## Current Long-term Memory
{memory_section}

## Response Format
Return ONLY valid JSON (no markdown fences, no tool calls, no code blocks):
{{
  "history_entry": "...",
  "memory_update": "...",
  "instructions": [{{"text": "Always do X before Y", "importance": 4}}, ...],
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
    let preview = crate::utils::text::truncate_str(text, 200, "...");
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
fn filter_tool_calls(text: &str) -> &str {
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
fn deduplicate_instructions(
    new_instructions: &[String],
    existing_instructions: &str,
) -> Vec<String> {
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
                let similarity = if union == 0 {
                    0.0
                } else {
                    intersection as f64 / union as f64
                };

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
        // Test with plain string instructions (backwards compat)
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
        assert_eq!(
            parsed.instructions[0].text,
            "Always run clippy before committing"
        );
        assert_eq!(parsed.instructions[0].importance, 3); // default
        assert_eq!(parsed.vault_entries.len(), 1);
        assert_eq!(parsed.vault_entries[0].key, "aws_password");
        assert_eq!(parsed.vault_entries[0].value, "s3cret123");
    }

    #[test]
    fn test_parse_v2_scored_instructions() {
        let json = r#"{
            "history_entry": "summary",
            "memory_update": "",
            "instructions": [
                {"text": "Never commit without tests", "importance": 5},
                {"text": "Use Italian", "importance": 4},
                "Plain instruction"
            ],
            "vault_entries": []
        }"#;
        let parsed = parse_consolidation_response_v2(json);
        assert_eq!(parsed.instructions.len(), 3);
        assert_eq!(parsed.instructions[0].text, "Never commit without tests");
        assert_eq!(parsed.instructions[0].importance, 5);
        assert_eq!(parsed.instructions[1].importance, 4);
        assert_eq!(parsed.instructions[2].text, "Plain instruction");
        assert_eq!(parsed.instructions[2].importance, 3); // default for plain string
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
            "Salva i dati nel vault".to_string(),             // Completely different
        ];
        let result = deduplicate_instructions(&new, existing);
        assert_eq!(
            result.len(),
            2,
            "Should keep 2 instructions (1 similar but different, 1 new)"
        );
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
