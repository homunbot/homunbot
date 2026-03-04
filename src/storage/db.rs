use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Pool, Sqlite};

/// Database connection pool and initialization.
///
/// All persistent data goes through here: sessions, messages, memories, cron jobs.
/// Uses sqlx with SQLite. Migrations are applied automatically on init.
#[derive(Clone)]
pub struct Database {
    pool: Pool<Sqlite>,
}

impl Database {
    /// Open (or create) the database at the given path and run migrations.
    pub async fn open(path: &Path) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create database directory {}", parent.display())
            })?;
        }

        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .foreign_keys(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .with_context(|| format!("Failed to open database at {}", path.display()))?;

        // Run migrations
        Self::run_migrations(&pool).await?;

        tracing::info!(path = %path.display(), "Database initialized");

        Ok(Self { pool })
    }

    /// Run SQL migrations from the migrations/ directory.
    async fn run_migrations(pool: &Pool<Sqlite>) -> Result<()> {
        // Create migrations tracking table
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS _migrations (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                applied_at TEXT NOT NULL DEFAULT (datetime('now'))
            )",
        )
        .execute(pool)
        .await
        .context("Failed to create migrations table")?;

        // Migration 001
        let migration_name = "001_initial";
        let already_applied: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM _migrations WHERE name = ?)")
                .bind(migration_name)
                .fetch_one(pool)
                .await
                .unwrap_or(false);

        if !already_applied {
            let sql = include_str!("../../migrations/001_initial.sql");

            // Strip SQL comments, then split and execute each statement
            let clean_sql: String = sql
                .lines()
                .map(|line| {
                    // Remove inline comments (but keep content before --)
                    if let Some(pos) = line.find("--") {
                        &line[..pos]
                    } else {
                        line
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");

            for statement in clean_sql.split(';') {
                let statement = statement.trim();
                if statement.is_empty() {
                    continue;
                }
                sqlx::query(statement)
                    .execute(pool)
                    .await
                    .with_context(|| {
                        format!(
                            "Migration failed: {}",
                            &statement[..statement.len().min(80)]
                        )
                    })?;
            }

            sqlx::query("INSERT INTO _migrations (name) VALUES (?)")
                .bind(migration_name)
                .execute(pool)
                .await
                .context("Failed to record migration")?;

            tracing::info!(migration = migration_name, "Applied database migration");
        }

        // Migration 002 — memory_chunks + FTS5
        Self::apply_migration(
            pool,
            "002_memory_chunks",
            include_str!("../../migrations/002_memory_chunks.sql"),
        )
        .await?;

        // Migration 003 — users + identities + webhook_tokens
        Self::apply_migration(
            pool,
            "003_users",
            include_str!("../../migrations/003_users.sql"),
        )
        .await?;

        // Migration 004 — token usage tracking
        Self::apply_migration(
            pool,
            "004_token_usage",
            include_str!("../../migrations/004_token_usage.sql"),
        )
        .await?;

        // Migration 005 — email pending queue for assisted approval flow
        Self::apply_migration(
            pool,
            "005_email_pending",
            include_str!("../../migrations/005_email_pending.sql"),
        )
        .await?;

        // Migration 006 — automations + runs
        Self::apply_migration(
            pool,
            "006_automations",
            include_str!("../../migrations/006_automations.sql"),
        )
        .await?;

        // Migration 007 — automation trigger fields
        Self::apply_migration(
            pool,
            "007_automation_triggers",
            include_str!("../../migrations/007_automation_triggers.sql"),
        )
        .await?;

        // Migration 008 — automation plan/dependencies metadata
        Self::apply_migration(
            pool,
            "008_automation_plan",
            include_str!("../../migrations/008_automation_plan.sql"),
        )
        .await?;

        Ok(())
    }

    /// Apply a named migration if not already applied.
    async fn apply_migration(pool: &Pool<Sqlite>, name: &str, sql: &str) -> Result<()> {
        let already_applied: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM _migrations WHERE name = ?)")
                .bind(name)
                .fetch_one(pool)
                .await
                .unwrap_or(false);

        if already_applied {
            return Ok(());
        }

        // Strip SQL comments, then split into statements.
        // Handles BEGIN...END blocks (e.g. triggers) where inner semicolons
        // are part of the block, not statement separators.
        let clean_sql: String = sql
            .lines()
            .map(|line| {
                if let Some(pos) = line.find("--") {
                    &line[..pos]
                } else {
                    line
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        let statements = split_sql_statements(&clean_sql);

        for statement in &statements {
            let statement = statement.trim();
            if statement.is_empty() {
                continue;
            }
            sqlx::query(statement)
                .execute(pool)
                .await
                .with_context(|| {
                    format!(
                        "Migration {name} failed: {}",
                        &statement[..statement.len().min(80)]
                    )
                })?;
        }

        sqlx::query("INSERT INTO _migrations (name) VALUES (?)")
            .bind(name)
            .execute(pool)
            .await
            .context("Failed to record migration")?;

        tracing::info!(migration = name, "Applied database migration");
        Ok(())
    }

    /// Get a reference to the connection pool
    pub fn pool(&self) -> &Pool<Sqlite> {
        &self.pool
    }

    // --- Session operations ---

    /// Create or update a session record
    pub async fn upsert_session(&self, key: &str, last_consolidated: i64) -> Result<()> {
        sqlx::query(
            "INSERT INTO sessions (key, last_consolidated)
             VALUES (?, ?)
             ON CONFLICT(key) DO UPDATE SET
                updated_at = datetime('now'),
                last_consolidated = excluded.last_consolidated",
        )
        .bind(key)
        .bind(last_consolidated)
        .execute(&self.pool)
        .await
        .context("Failed to upsert session")?;

        Ok(())
    }

    /// Load session metadata
    pub async fn load_session(&self, key: &str) -> Result<Option<SessionRow>> {
        let row = sqlx::query_as::<_, SessionRow>(
            "SELECT key, created_at, updated_at, last_consolidated, metadata
             FROM sessions WHERE key = ?",
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to load session")?;

        Ok(row)
    }

    // --- Message operations ---

    /// Append a message to a session
    pub async fn insert_message(
        &self,
        session_key: &str,
        role: &str,
        content: &str,
        tools_used: &[String],
    ) -> Result<()> {
        let tools_json = serde_json::to_string(tools_used).unwrap_or_else(|_| "[]".to_string());

        sqlx::query(
            "INSERT INTO messages (session_key, role, content, tools_used)
             VALUES (?, ?, ?, ?)",
        )
        .bind(session_key)
        .bind(role)
        .bind(content)
        .bind(tools_json)
        .execute(&self.pool)
        .await
        .context("Failed to insert message")?;

        Ok(())
    }

    /// Load the last N messages for a session, ordered oldest-first
    pub async fn load_messages(&self, session_key: &str, limit: u32) -> Result<Vec<MessageRow>> {
        let rows = sqlx::query_as::<_, MessageRow>(
            "SELECT id, session_key, role, content, tools_used, timestamp
             FROM (
                 SELECT * FROM messages
                 WHERE session_key = ?
                 ORDER BY id DESC
                 LIMIT ?
             ) sub
             ORDER BY id ASC",
        )
        .bind(session_key)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .context("Failed to load messages")?;

        Ok(rows)
    }

    /// Count messages in a session
    pub async fn count_messages(&self, session_key: &str) -> Result<i64> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE session_key = ?")
            .bind(session_key)
            .fetch_one(&self.pool)
            .await
            .context("Failed to count messages")?;

        Ok(count)
    }

    /// Delete all messages for a session (for /new command)
    pub async fn clear_messages(&self, session_key: &str) -> Result<()> {
        sqlx::query("DELETE FROM messages WHERE session_key = ?")
            .bind(session_key)
            .execute(&self.pool)
            .await
            .context("Failed to clear messages")?;

        sqlx::query(
            "UPDATE sessions SET last_consolidated = 0, updated_at = datetime('now')
             WHERE key = ?",
        )
        .bind(session_key)
        .execute(&self.pool)
        .await
        .context("Failed to reset session")?;

        Ok(())
    }

    /// Load the oldest messages that would be pruned during compaction
    /// (all except the newest `keep_count`).
    pub async fn load_old_messages(
        &self,
        session_key: &str,
        keep_count: u32,
    ) -> Result<Vec<MessageRow>> {
        sqlx::query_as::<_, MessageRow>(
            "SELECT id, session_key, role, content, tools_used, timestamp
             FROM messages
             WHERE session_key = ? AND id NOT IN (
                 SELECT id FROM messages WHERE session_key = ?
                 ORDER BY id DESC LIMIT ?
             )
             ORDER BY id ASC",
        )
        .bind(session_key)
        .bind(session_key)
        .bind(keep_count)
        .fetch_all(&self.pool)
        .await
        .context("Failed to load old messages")
    }

    /// Delete old messages, keeping only the newest `keep_count`.
    /// Returns the number of messages deleted.
    pub async fn delete_old_messages(&self, session_key: &str, keep_count: u32) -> Result<u64> {
        let result = sqlx::query(
            "DELETE FROM messages WHERE session_key = ? AND id NOT IN (
                SELECT id FROM messages WHERE session_key = ?
                ORDER BY id DESC LIMIT ?
            )",
        )
        .bind(session_key)
        .bind(session_key)
        .bind(keep_count)
        .execute(&self.pool)
        .await
        .context("Failed to delete old messages")?;

        Ok(result.rows_affected())
    }

    // --- Memory operations ---

    /// Insert a consolidated memory
    pub async fn insert_memory(
        &self,
        session_key: Option<&str>,
        content: &str,
        memory_type: &str,
    ) -> Result<()> {
        sqlx::query("INSERT INTO memories (session_key, content, memory_type) VALUES (?, ?, ?)")
            .bind(session_key)
            .bind(content)
            .bind(memory_type)
            .execute(&self.pool)
            .await
            .context("Failed to insert memory")?;

        Ok(())
    }

    /// Load all memories for a session (plus global memories)
    pub async fn load_memories(&self, session_key: &str) -> Result<Vec<MemoryRow>> {
        let rows = sqlx::query_as::<_, MemoryRow>(
            "SELECT id, session_key, content, memory_type, created_at
             FROM memories
             WHERE session_key IS NULL OR session_key = ?
             ORDER BY created_at ASC",
        )
        .bind(session_key)
        .fetch_all(&self.pool)
        .await
        .context("Failed to load memories")?;

        Ok(rows)
    }

    /// Load the latest long-term memory content (type = 'long_term')
    pub async fn load_long_term_memory(&self) -> Result<Option<String>> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT content FROM memories
             WHERE memory_type = 'long_term' AND session_key IS NULL
             ORDER BY created_at DESC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await
        .context("Failed to load long-term memory")?;

        Ok(row.map(|(c,)| c))
    }

    /// Replace the global long-term memory (upsert pattern)
    pub async fn upsert_long_term_memory(&self, content: &str) -> Result<()> {
        // Delete old global long-term memory, then insert fresh
        sqlx::query("DELETE FROM memories WHERE memory_type = 'long_term' AND session_key IS NULL")
            .execute(&self.pool)
            .await
            .context("Failed to clear old long-term memory")?;

        self.insert_memory(None, content, "long_term").await
    }

    // --- Cron job operations ---

    /// Insert a new cron job
    pub async fn insert_cron_job(
        &self,
        id: &str,
        name: &str,
        message: &str,
        schedule: &str,
        deliver_to: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO cron_jobs (id, name, message, schedule, deliver_to)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(name)
        .bind(message)
        .bind(schedule)
        .bind(deliver_to)
        .execute(&self.pool)
        .await
        .context("Failed to insert cron job")?;

        Ok(())
    }

    /// Load all enabled cron jobs
    pub async fn load_cron_jobs(&self) -> Result<Vec<CronJobRow>> {
        let rows = sqlx::query_as::<_, CronJobRow>(
            "SELECT id, name, message, schedule, enabled, deliver_to, last_run, created_at
             FROM cron_jobs
             ORDER BY created_at ASC",
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to load cron jobs")?;

        Ok(rows)
    }

    /// Update last_run timestamp for a cron job
    pub async fn update_cron_last_run(&self, id: &str) -> Result<()> {
        sqlx::query("UPDATE cron_jobs SET last_run = datetime('now') WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .context("Failed to update cron job last_run")?;

        Ok(())
    }

    /// Delete a cron job
    pub async fn delete_cron_job(&self, id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM cron_jobs WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .context("Failed to delete cron job")?;

        Ok(result.rows_affected() > 0)
    }

    // --- Memory chunk operations (for vector + FTS5 search) ---

    /// Insert a memory chunk and return its row ID (for vector indexing).
    pub async fn insert_memory_chunk(
        &self,
        date: &str,
        source: &str,
        heading: &str,
        content: &str,
        memory_type: &str,
    ) -> Result<i64> {
        let result = sqlx::query(
            "INSERT INTO memory_chunks (date, source, heading, content, memory_type)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(date)
        .bind(source)
        .bind(heading)
        .bind(content)
        .bind(memory_type)
        .execute(&self.pool)
        .await
        .context("Failed to insert memory chunk")?;

        Ok(result.last_insert_rowid())
    }

    /// Load memory chunks by their IDs (after vector search returns matching IDs).
    pub async fn load_chunks_by_ids(&self, ids: &[i64]) -> Result<Vec<MemoryChunkRow>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        // Build a parameterized IN clause
        let placeholders: Vec<String> = ids.iter().map(|_| "?".to_string()).collect();
        let query = format!(
            "SELECT id, date, source, heading, content, memory_type, created_at
             FROM memory_chunks WHERE id IN ({})
             ORDER BY created_at DESC",
            placeholders.join(",")
        );

        let mut q = sqlx::query_as::<_, MemoryChunkRow>(&query);
        for id in ids {
            q = q.bind(id);
        }

        let rows = q
            .fetch_all(&self.pool)
            .await
            .context("Failed to load memory chunks by IDs")?;

        Ok(rows)
    }

    /// Full-text search on memory chunks using FTS5 BM25 ranking.
    /// Returns `(chunk_id, bm25_score)` pairs, best matches first.
    pub async fn fts5_search(&self, query: &str, limit: usize) -> Result<Vec<(i64, f64)>> {
        let rows: Vec<(i64, f64)> = sqlx::query_as(
            "SELECT rowid, rank
             FROM memory_fts
             WHERE memory_fts MATCH ?
             ORDER BY rank
             LIMIT ?",
        )
        .bind(query)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .context("FTS5 search failed")?;

        Ok(rows)
    }

    /// Count total memory chunks.
    pub async fn count_memory_chunks(&self) -> Result<i64> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memory_chunks")
            .fetch_one(&self.pool)
            .await
            .context("Failed to count memory chunks")?;
        Ok(count)
    }

    /// Delete all memory data from the database (memory_chunks, memories, messages).
    pub async fn reset_all_memory(&self) -> Result<()> {
        sqlx::query("DELETE FROM memory_chunks")
            .execute(&self.pool)
            .await
            .context("Failed to clear memory_chunks")?;
        sqlx::query("DELETE FROM memories")
            .execute(&self.pool)
            .await
            .context("Failed to clear memories")?;
        sqlx::query("DELETE FROM messages")
            .execute(&self.pool)
            .await
            .context("Failed to clear messages")?;
        Ok(())
    }

    /// Toggle a cron job's enabled state
    pub async fn toggle_cron_job(&self, id: &str, enabled: bool) -> Result<bool> {
        let result = sqlx::query("UPDATE cron_jobs SET enabled = ? WHERE id = ?")
            .bind(enabled)
            .bind(id)
            .execute(&self.pool)
            .await
            .context("Failed to toggle cron job")?;

        Ok(result.rows_affected() > 0)
    }

    // --- Automation operations ---

    /// Insert a new automation definition.
    pub async fn insert_automation(
        &self,
        id: &str,
        name: &str,
        prompt: &str,
        schedule: &str,
        enabled: bool,
        status: &str,
        deliver_to: Option<&str>,
        trigger_kind: &str,
        trigger_value: Option<&str>,
    ) -> Result<()> {
        self.insert_automation_with_plan(
            id,
            name,
            prompt,
            schedule,
            enabled,
            status,
            deliver_to,
            trigger_kind,
            trigger_value,
            None,
            "[]",
            1,
            None,
        )
        .await
    }

    /// Insert a new automation definition with compiled plan metadata.
    pub async fn insert_automation_with_plan(
        &self,
        id: &str,
        name: &str,
        prompt: &str,
        schedule: &str,
        enabled: bool,
        status: &str,
        deliver_to: Option<&str>,
        trigger_kind: &str,
        trigger_value: Option<&str>,
        plan_json: Option<&str>,
        dependencies_json: &str,
        plan_version: i64,
        validation_errors: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO automations
                 (id, name, prompt, schedule, enabled, status, deliver_to, trigger_kind, trigger_value,
                  plan_json, dependencies_json, plan_version, validation_errors)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(name)
        .bind(prompt)
        .bind(schedule)
        .bind(enabled)
        .bind(status)
        .bind(deliver_to)
        .bind(trigger_kind)
        .bind(trigger_value)
        .bind(plan_json)
        .bind(dependencies_json)
        .bind(plan_version)
        .bind(validation_errors)
        .execute(&self.pool)
        .await
        .context("Failed to insert automation")?;

        Ok(())
    }

    /// Load all automations.
    pub async fn load_automations(&self) -> Result<Vec<AutomationRow>> {
        let rows = sqlx::query_as::<_, AutomationRow>(
            "SELECT id, name, prompt, schedule, enabled, status, deliver_to,
                    trigger_kind, trigger_value,
                    last_run, last_result, created_at, updated_at,
                    plan_json, dependencies_json, plan_version, validation_errors
             FROM automations
             ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to load automations")?;

        Ok(rows)
    }

    /// Load one automation by ID.
    pub async fn load_automation(&self, id: &str) -> Result<Option<AutomationRow>> {
        let row = sqlx::query_as::<_, AutomationRow>(
            "SELECT id, name, prompt, schedule, enabled, status, deliver_to,
                    trigger_kind, trigger_value,
                    last_run, last_result, created_at, updated_at,
                    plan_json, dependencies_json, plan_version, validation_errors
             FROM automations
             WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to load automation")?;

        Ok(row)
    }

    /// Apply a partial update to an automation.
    pub async fn update_automation(&self, id: &str, update: AutomationUpdate) -> Result<bool> {
        let Some(current) = self.load_automation(id).await? else {
            return Ok(false);
        };

        let name = update.name.unwrap_or(current.name);
        let prompt = update.prompt.unwrap_or(current.prompt);
        let schedule = update.schedule.unwrap_or(current.schedule);
        let enabled = update.enabled.unwrap_or(current.enabled);
        let status = update.status.unwrap_or(current.status);
        let deliver_to = update.deliver_to.unwrap_or(current.deliver_to);
        let trigger_kind = update.trigger_kind.unwrap_or(current.trigger_kind);
        let trigger_value = update.trigger_value.unwrap_or(current.trigger_value);
        let last_result = update.last_result.unwrap_or(current.last_result);
        let plan_json = match update.plan_json {
            Some(v) => v,
            None => current.plan_json,
        };
        let dependencies_json = match update.dependencies_json {
            Some(Some(v)) => v,
            Some(None) => "[]".to_string(),
            None => current.dependencies_json,
        };
        let plan_version = update.plan_version.unwrap_or(current.plan_version);
        let validation_errors = match update.validation_errors {
            Some(v) => v,
            None => current.validation_errors,
        };

        let result = sqlx::query(
            "UPDATE automations
             SET name = ?, prompt = ?, schedule = ?, enabled = ?, status = ?,
                 deliver_to = ?, trigger_kind = ?, trigger_value = ?, last_result = ?,
                 plan_json = ?, dependencies_json = ?, plan_version = ?, validation_errors = ?,
                 last_run = CASE WHEN ? THEN datetime('now') ELSE last_run END,
                 updated_at = datetime('now')
             WHERE id = ?",
        )
        .bind(name)
        .bind(prompt)
        .bind(schedule)
        .bind(enabled)
        .bind(status)
        .bind(deliver_to)
        .bind(trigger_kind)
        .bind(trigger_value)
        .bind(last_result)
        .bind(plan_json)
        .bind(dependencies_json)
        .bind(plan_version)
        .bind(validation_errors)
        .bind(update.touch_last_run)
        .bind(id)
        .execute(&self.pool)
        .await
        .context("Failed to update automation")?;

        Ok(result.rows_affected() > 0)
    }

    /// Delete an automation.
    pub async fn delete_automation(&self, id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM automations WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .context("Failed to delete automation")?;

        Ok(result.rows_affected() > 0)
    }

    /// Mark automations as invalid when a dependency is removed.
    ///
    /// Returns number of automations updated.
    pub async fn invalidate_automations_by_dependency(
        &self,
        dependency_kind: &str,
        dependency_name: &str,
        reason: &str,
    ) -> Result<u64> {
        let rows = self.load_automations().await?;
        let mut affected = 0_u64;

        for row in rows {
            if !crate::scheduler::automations::dependencies_include(
                &row.dependencies_json,
                dependency_kind,
                dependency_name,
            ) {
                continue;
            }

            let mut errors = crate::scheduler::automations::parse_validation_errors_json(
                row.validation_errors.as_deref(),
            );
            if !errors.iter().any(|e| e == reason) {
                errors.push(reason.to_string());
            }
            let errors_json = serde_json::to_string(&errors).unwrap_or_else(|_| "[]".to_string());

            let changed = self
                .update_automation(
                    &row.id,
                    AutomationUpdate {
                        status: Some("invalid_config".to_string()),
                        validation_errors: Some(Some(errors_json)),
                        last_result: Some(Some(format!("Automation invalidated: {reason}"))),
                        ..Default::default()
                    },
                )
                .await?;
            if changed {
                affected += 1;
            }
        }

        Ok(affected)
    }

    /// Insert a new automation run.
    pub async fn insert_automation_run(
        &self,
        id: &str,
        automation_id: &str,
        status: &str,
        result: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO automation_runs (id, automation_id, status, result)
             VALUES (?, ?, ?, ?)",
        )
        .bind(id)
        .bind(automation_id)
        .bind(status)
        .bind(result)
        .execute(&self.pool)
        .await
        .context("Failed to insert automation run")?;
        Ok(())
    }

    /// Complete an automation run with final status/result.
    pub async fn complete_automation_run(
        &self,
        run_id: &str,
        status: &str,
        result: Option<&str>,
    ) -> Result<bool> {
        let changed = sqlx::query(
            "UPDATE automation_runs
             SET status = ?, result = ?, finished_at = datetime('now')
             WHERE id = ?",
        )
        .bind(status)
        .bind(result)
        .bind(run_id)
        .execute(&self.pool)
        .await
        .context("Failed to complete automation run")?;
        Ok(changed.rows_affected() > 0)
    }

    /// Load run history for an automation (latest first).
    pub async fn load_automation_runs(
        &self,
        automation_id: &str,
        limit: u32,
    ) -> Result<Vec<AutomationRunRow>> {
        let rows = sqlx::query_as::<_, AutomationRunRow>(
            "SELECT id, automation_id, started_at, finished_at, status, result
             FROM automation_runs
             WHERE automation_id = ?
             ORDER BY started_at DESC
             LIMIT ?",
        )
        .bind(automation_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .context("Failed to load automation runs")?;
        Ok(rows)
    }

    /// Load latest successful run result for an automation.
    /// Optionally excludes a run ID (useful when finalizing that same run).
    pub async fn load_last_successful_automation_result(
        &self,
        automation_id: &str,
        exclude_run_id: Option<&str>,
    ) -> Result<Option<String>> {
        let row = sqlx::query_scalar::<_, String>(
            "SELECT result
             FROM automation_runs
             WHERE automation_id = ?
               AND status = 'success'
               AND result IS NOT NULL
               AND (? IS NULL OR id <> ?)
             ORDER BY started_at DESC
             LIMIT 1",
        )
        .bind(automation_id)
        .bind(exclude_run_id)
        .bind(exclude_run_id)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to load last successful automation result")?;

        Ok(row)
    }

    // ═══════════════════════════════════════════════════════════════
    // MEMORY RETENTION - Prune old data based on retention policies
    // ═══════════════════════════════════════════════════════════════

    /// Prune old conversation messages based on retention policy.
    /// Returns the number of messages deleted.
    pub async fn prune_old_messages(&self, retention_days: u32) -> Result<u64> {
        if retention_days == 0 {
            return Ok(0); // Never prune
        }

        let cutoff = chrono::Utc::now() - chrono::Duration::days(retention_days as i64);
        let cutoff_str = cutoff.to_rfc3339();

        let result = sqlx::query("DELETE FROM messages WHERE timestamp < ?")
            .bind(&cutoff_str)
            .execute(&self.pool)
            .await
            .context("Failed to prune old messages")?;

        let deleted = result.rows_affected();
        if deleted > 0 {
            tracing::info!(
                deleted,
                retention_days,
                cutoff = %cutoff_str,
                "Pruned old conversation messages"
            );
        }
        Ok(deleted)
    }

    /// Prune old memory chunks (history entries) based on retention policy.
    /// Returns the number of chunks deleted.
    pub async fn prune_old_memory_chunks(&self, retention_days: u32) -> Result<u64> {
        if retention_days == 0 {
            return Ok(0); // Never prune
        }

        let cutoff = chrono::Utc::now() - chrono::Duration::days(retention_days as i64);
        let cutoff_date = cutoff.format("%Y-%m-%d").to_string();

        // Only prune history-type chunks, keep facts and instructions
        let result =
            sqlx::query("DELETE FROM memory_chunks WHERE date < ? AND memory_type = 'history'")
                .bind(&cutoff_date)
                .execute(&self.pool)
                .await
                .context("Failed to prune old memory chunks")?;

        let deleted = result.rows_affected();
        if deleted > 0 {
            tracing::info!(
                deleted,
                retention_days,
                cutoff = %cutoff_date,
                "Pruned old history chunks"
            );
        }
        Ok(deleted)
    }

    /// Get list of daily memory files that are older than the archive threshold.
    /// Returns list of dates (YYYY-MM-DD format) that can be archived.
    pub fn get_old_daily_files(&self, archive_months: u32) -> Result<Vec<String>> {
        if archive_months == 0 {
            return Ok(Vec::new()); // Never archive
        }

        let cutoff = chrono::Utc::now() - chrono::Duration::days(archive_months as i64 * 30);
        let cutoff_date = cutoff.format("%Y-%m-%d").to_string();

        let data_dir = crate::config::Config::data_dir();
        let memory_dir = data_dir.join("memory");

        if !memory_dir.exists() {
            return Ok(Vec::new());
        }

        let mut old_files = Vec::new();
        for entry in std::fs::read_dir(&memory_dir)
            .with_context(|| format!("Failed to read memory directory {}", memory_dir.display()))?
        {
            let entry = entry.context("Failed to read directory entry")?;
            let filename = entry.file_name();
            let name = filename.to_string_lossy();

            // Check if it's a daily file (YYYY-MM-DD.md)
            if name.len() == 14 && name.ends_with(".md") {
                let date = &name[..10]; // YYYY-MM-DD
                if date < cutoff_date.as_str() {
                    old_files.push(date.to_string());
                }
            }
        }

        old_files.sort();
        Ok(old_files)
    }

    /// Run full memory cleanup based on retention policies.
    /// Returns summary of what was cleaned up.
    pub async fn run_memory_cleanup(
        &self,
        conversation_retention_days: u32,
        history_retention_days: u32,
    ) -> Result<MemoryCleanupResult> {
        let messages_deleted = self.prune_old_messages(conversation_retention_days).await?;
        let chunks_deleted = self.prune_old_memory_chunks(history_retention_days).await?;

        Ok(MemoryCleanupResult {
            messages_deleted,
            chunks_deleted,
        })
    }

    // ═══════════════════════════════════════════════════════════════
    // USER SYSTEM OPERATIONS
    // ═══════════════════════════════════════════════════════════════

    /// Create a new user with the given ID, username, and roles.
    pub async fn create_user(&self, id: &str, username: &str, roles: &[&str]) -> Result<()> {
        let roles_json = serde_json::to_string(roles).unwrap_or_else(|_| "[]".to_string());

        sqlx::query("INSERT INTO users (id, username, roles) VALUES (?, ?, ?)")
            .bind(id)
            .bind(username)
            .bind(roles_json)
            .execute(&self.pool)
            .await
            .context("Failed to create user")?;

        Ok(())
    }

    /// Load a user by their internal ID.
    pub async fn load_user(&self, id: &str) -> Result<Option<UserRow>> {
        let row = sqlx::query_as::<_, UserRow>(
            "SELECT id, username, roles, created_at, updated_at, metadata
             FROM users WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to load user")?;

        Ok(row)
    }

    /// Load a user by their username.
    pub async fn load_user_by_username(&self, username: &str) -> Result<Option<UserRow>> {
        let row = sqlx::query_as::<_, UserRow>(
            "SELECT id, username, roles, created_at, updated_at, metadata
             FROM users WHERE username = ?",
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to load user by username")?;

        Ok(row)
    }

    /// Load all users.
    pub async fn load_all_users(&self) -> Result<Vec<UserRow>> {
        let rows = sqlx::query_as::<_, UserRow>(
            "SELECT id, username, roles, created_at, updated_at, metadata
             FROM users ORDER BY created_at ASC",
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to load users")?;

        Ok(rows)
    }

    /// Update a user's roles.
    pub async fn update_user_roles(&self, id: &str, roles: &[&str]) -> Result<bool> {
        let roles_json = serde_json::to_string(roles).unwrap_or_else(|_| "[]".to_string());

        let result =
            sqlx::query("UPDATE users SET roles = ?, updated_at = datetime('now') WHERE id = ?")
                .bind(roles_json)
                .bind(id)
                .execute(&self.pool)
                .await
                .context("Failed to update user roles")?;

        Ok(result.rows_affected() > 0)
    }

    /// Delete a user (cascades to identities and webhook tokens).
    pub async fn delete_user(&self, id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM users WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .context("Failed to delete user")?;

        Ok(result.rows_affected() > 0)
    }

    // --- User identities ---

    /// Add a channel identity to a user.
    pub async fn add_user_identity(
        &self,
        user_id: &str,
        channel: &str,
        platform_id: &str,
        display_name: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO user_identities (user_id, channel, platform_id, display_name)
             VALUES (?, ?, ?, ?)",
        )
        .bind(user_id)
        .bind(channel)
        .bind(platform_id)
        .bind(display_name)
        .execute(&self.pool)
        .await
        .context("Failed to add user identity")?;

        Ok(())
    }

    /// Look up a user by their channel identity.
    pub async fn lookup_user_by_identity(
        &self,
        channel: &str,
        platform_id: &str,
    ) -> Result<Option<UserRow>> {
        let row = sqlx::query_as::<_, UserRow>(
            "SELECT u.id, u.username, u.roles, u.created_at, u.updated_at, u.metadata
             FROM users u
             JOIN user_identities i ON u.id = i.user_id
             WHERE i.channel = ? AND i.platform_id = ?",
        )
        .bind(channel)
        .bind(platform_id)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to lookup user by identity")?;

        Ok(row)
    }

    /// Load all identities for a user.
    pub async fn load_user_identities(&self, user_id: &str) -> Result<Vec<UserIdentityRow>> {
        let rows = sqlx::query_as::<_, UserIdentityRow>(
            "SELECT id, user_id, channel, platform_id, display_name, created_at
             FROM user_identities WHERE user_id = ?
             ORDER BY created_at ASC",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .context("Failed to load user identities")?;

        Ok(rows)
    }

    /// Remove a user identity.
    pub async fn remove_user_identity(
        &self,
        user_id: &str,
        channel: &str,
        platform_id: &str,
    ) -> Result<bool> {
        let result = sqlx::query(
            "DELETE FROM user_identities
             WHERE user_id = ? AND channel = ? AND platform_id = ?",
        )
        .bind(user_id)
        .bind(channel)
        .bind(platform_id)
        .execute(&self.pool)
        .await
        .context("Failed to remove user identity")?;

        Ok(result.rows_affected() > 0)
    }

    // --- Webhook tokens ---

    /// Create a new webhook token for a user.
    pub async fn create_webhook_token(&self, token: &str, user_id: &str, name: &str) -> Result<()> {
        sqlx::query("INSERT INTO webhook_tokens (token, user_id, name) VALUES (?, ?, ?)")
            .bind(token)
            .bind(user_id)
            .bind(name)
            .execute(&self.pool)
            .await
            .context("Failed to create webhook token")?;

        Ok(())
    }

    /// Look up a webhook token and return the associated user.
    pub async fn lookup_user_by_webhook_token(&self, token: &str) -> Result<Option<UserRow>> {
        let row = sqlx::query_as::<_, UserRow>(
            "SELECT u.id, u.username, u.roles, u.created_at, u.updated_at, u.metadata
             FROM users u
             JOIN webhook_tokens wt ON u.id = wt.user_id
             WHERE wt.token = ? AND wt.enabled = 1",
        )
        .bind(token)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to lookup user by webhook token")?;

        Ok(row)
    }

    /// Update the last_used timestamp for a webhook token.
    pub async fn touch_webhook_token(&self, token: &str) -> Result<()> {
        sqlx::query("UPDATE webhook_tokens SET last_used = datetime('now') WHERE token = ?")
            .bind(token)
            .execute(&self.pool)
            .await
            .context("Failed to update webhook token last_used")?;

        Ok(())
    }

    /// Load all webhook tokens for a user.
    pub async fn load_webhook_tokens(&self, user_id: &str) -> Result<Vec<WebhookTokenRow>> {
        let rows = sqlx::query_as::<_, WebhookTokenRow>(
            "SELECT token, user_id, name, enabled, last_used, created_at
             FROM webhook_tokens WHERE user_id = ?
             ORDER BY created_at DESC",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .context("Failed to load webhook tokens")?;

        Ok(rows)
    }

    /// Delete a webhook token.
    pub async fn delete_webhook_token(&self, token: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM webhook_tokens WHERE token = ?")
            .bind(token)
            .execute(&self.pool)
            .await
            .context("Failed to delete webhook token")?;

        Ok(result.rows_affected() > 0)
    }

    /// Toggle a webhook token's enabled state.
    pub async fn toggle_webhook_token(&self, token: &str, enabled: bool) -> Result<bool> {
        let result = sqlx::query("UPDATE webhook_tokens SET enabled = ? WHERE token = ?")
            .bind(enabled)
            .bind(token)
            .execute(&self.pool)
            .await
            .context("Failed to toggle webhook token")?;

        Ok(result.rows_affected() > 0)
    }

    // --- Token usage ---

    pub async fn insert_token_usage(
        &self,
        session_key: &str,
        model: &str,
        provider: &str,
        prompt_tokens: u32,
        completion_tokens: u32,
        total_tokens: u32,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO token_usage (session_key, model, provider, prompt_tokens, completion_tokens, total_tokens)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(session_key)
        .bind(model)
        .bind(provider)
        .bind(prompt_tokens as i64)
        .bind(completion_tokens as i64)
        .bind(total_tokens as i64)
        .execute(&self.pool)
        .await
        .context("Failed to insert token usage")?;
        Ok(())
    }

    pub async fn query_token_usage(
        &self,
        session_key: Option<&str>,
        since: Option<&str>,
        until: Option<&str>,
    ) -> Result<Vec<TokenUsageAggRow>> {
        let mut sql = String::from(
            "SELECT model, provider,
                    SUM(prompt_tokens) as prompt_tokens,
                    SUM(completion_tokens) as completion_tokens,
                    SUM(total_tokens) as total_tokens,
                    COUNT(*) as call_count
             FROM token_usage WHERE 1=1",
        );
        let mut binds: Vec<String> = Vec::new();

        if let Some(s) = session_key {
            sql.push_str(" AND session_key = ?");
            binds.push(s.to_string());
        }
        if let Some(s) = since {
            sql.push_str(" AND created_at >= ?");
            binds.push(s.to_string());
        }
        if let Some(u) = until {
            sql.push_str(" AND created_at <= ?");
            binds.push(u.to_string());
        }

        sql.push_str(" GROUP BY model, provider ORDER BY total_tokens DESC");

        let mut q = sqlx::query_as::<_, TokenUsageAggRow>(&sql);
        for b in &binds {
            q = q.bind(b);
        }

        q.fetch_all(&self.pool)
            .await
            .context("Failed to query token usage")
    }

    // ═══════════════════════════════════════════════════════════════
    // EMAIL PENDING — assisted approval flow
    // ═══════════════════════════════════════════════════════════════

    /// Insert a new email pending record (draft awaiting approval).
    pub async fn insert_email_pending(&self, row: &EmailPendingRow) -> Result<()> {
        sqlx::query(
            "INSERT INTO email_pending (id, account_name, from_address, subject, body_preview,
             message_id, draft_response, status, notify_session_key, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))",
        )
        .bind(&row.id)
        .bind(&row.account_name)
        .bind(&row.from_address)
        .bind(&row.subject)
        .bind(&row.body_preview)
        .bind(&row.message_id)
        .bind(&row.draft_response)
        .bind(&row.status)
        .bind(&row.notify_session_key)
        .execute(&self.pool)
        .await
        .context("Failed to insert email_pending")?;
        Ok(())
    }

    /// Update the draft response for an existing pending email.
    pub async fn update_email_pending_draft(&self, id: &str, draft: &str) -> Result<bool> {
        let result = sqlx::query(
            "UPDATE email_pending SET draft_response = ?, updated_at = datetime('now')
             WHERE id = ? AND status = 'pending'",
        )
        .bind(draft)
        .bind(id)
        .execute(&self.pool)
        .await
        .context("Failed to update email_pending draft")?;
        Ok(result.rows_affected() > 0)
    }

    /// Change status of a pending email (e.g. pending → sent, pending → rejected).
    /// Only updates rows that are currently 'pending' for atomicity.
    pub async fn update_email_pending_status(&self, id: &str, status: &str) -> Result<bool> {
        let result = sqlx::query(
            "UPDATE email_pending SET status = ?, updated_at = datetime('now')
             WHERE id = ? AND status = 'pending'",
        )
        .bind(status)
        .bind(id)
        .execute(&self.pool)
        .await
        .context("Failed to update email_pending status")?;
        Ok(result.rows_affected() > 0)
    }

    /// Load all pending emails for a given notify session key, ordered FIFO.
    pub async fn load_pending_for_notify(&self, notify_key: &str) -> Result<Vec<EmailPendingRow>> {
        let rows = sqlx::query_as::<_, EmailPendingRow>(
            "SELECT id, account_name, from_address, subject, body_preview,
                    message_id, draft_response, status, notify_session_key,
                    created_at, updated_at
             FROM email_pending
             WHERE notify_session_key = ? AND status = 'pending'
             ORDER BY created_at ASC",
        )
        .bind(notify_key)
        .fetch_all(&self.pool)
        .await
        .context("Failed to load pending emails for notify")?;
        Ok(rows)
    }

    /// Load a single email_pending record by ID.
    pub async fn load_email_pending_by_id(&self, id: &str) -> Result<Option<EmailPendingRow>> {
        let row = sqlx::query_as::<_, EmailPendingRow>(
            "SELECT id, account_name, from_address, subject, body_preview,
                    message_id, draft_response, status, notify_session_key,
                    created_at, updated_at
             FROM email_pending WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to load email_pending by id")?;
        Ok(row)
    }
}

/// Result of memory cleanup operation.
#[derive(Debug, Default)]
pub struct MemoryCleanupResult {
    pub messages_deleted: u64,
    pub chunks_deleted: u64,
}

// --- Row types for sqlx ---

#[derive(Debug, sqlx::FromRow)]
pub struct SessionRow {
    pub key: String,
    pub created_at: String,
    pub updated_at: String,
    pub last_consolidated: i64,
    pub metadata: String,
}

#[derive(Debug, sqlx::FromRow)]
pub struct MessageRow {
    pub id: i64,
    pub session_key: String,
    pub role: String,
    pub content: String,
    pub tools_used: String,
    pub timestamp: String,
}

#[derive(Debug, sqlx::FromRow)]
pub struct MemoryRow {
    pub id: i64,
    pub session_key: Option<String>,
    pub content: String,
    pub memory_type: String,
    pub created_at: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MemoryChunkRow {
    pub id: i64,
    pub date: String,
    pub source: String,
    pub heading: String,
    pub content: String,
    pub memory_type: String,
    pub created_at: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CronJobRow {
    pub id: String,
    pub name: String,
    pub message: String,
    pub schedule: String,
    pub enabled: bool,
    pub deliver_to: Option<String>,
    pub last_run: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct AutomationRow {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub schedule: String,
    pub enabled: bool,
    pub status: String,
    pub deliver_to: Option<String>,
    pub trigger_kind: String,
    pub trigger_value: Option<String>,
    pub last_run: Option<String>,
    pub last_result: Option<String>,
    pub created_at: String,
    pub updated_at: Option<String>,
    pub plan_json: Option<String>,
    pub dependencies_json: String,
    pub plan_version: i64,
    pub validation_errors: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AutomationUpdate {
    pub name: Option<String>,
    pub prompt: Option<String>,
    pub schedule: Option<String>,
    pub enabled: Option<bool>,
    pub status: Option<String>,
    /// Use `Some(None)` to clear `deliver_to`.
    pub deliver_to: Option<Option<String>>,
    pub trigger_kind: Option<String>,
    /// Use `Some(None)` to clear trigger value.
    pub trigger_value: Option<Option<String>>,
    pub last_result: Option<Option<String>>,
    /// Use `Some(None)` to clear plan JSON.
    pub plan_json: Option<Option<String>>,
    /// Use `Some(None)` to reset dependencies to an empty list.
    pub dependencies_json: Option<Option<String>>,
    pub plan_version: Option<i64>,
    /// Use `Some(None)` to clear validation errors.
    pub validation_errors: Option<Option<String>>,
    pub touch_last_run: bool,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct AutomationRunRow {
    pub id: String,
    pub automation_id: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub status: String,
    pub result: Option<String>,
}

// ═══════════════════════════════════════════════════════════════
// USER SYSTEM ROW TYPES
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UserRow {
    pub id: String,
    pub username: String,
    pub roles: String, // JSON array
    pub created_at: String,
    pub updated_at: String,
    pub metadata: String, // JSON object
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UserIdentityRow {
    pub id: i64,
    pub user_id: String,
    pub channel: String,
    pub platform_id: String,
    pub display_name: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WebhookTokenRow {
    pub token: String,
    pub user_id: String,
    pub name: String,
    pub enabled: bool,
    pub last_used: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct TokenUsageAggRow {
    pub model: String,
    pub provider: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub call_count: i64,
}

// ═══════════════════════════════════════════════════════════════
// EMAIL PENDING (assisted approval flow)
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct EmailPendingRow {
    pub id: String,
    pub account_name: String,
    pub from_address: String,
    pub subject: Option<String>,
    pub body_preview: Option<String>,
    pub message_id: Option<String>,
    pub draft_response: Option<String>,
    pub status: String, // pending | sent | rejected
    pub notify_session_key: Option<String>,
    pub created_at: String,
    pub updated_at: Option<String>,
}

/// Split SQL into individual statements, respecting BEGIN...END blocks.
///
/// Standard `split(';')` breaks triggers and other compound statements
/// that contain semicolons inside `BEGIN...END` blocks. This parser
/// tracks nesting depth to correctly handle `CREATE TRIGGER ... BEGIN ... END;`.
fn split_sql_statements(sql: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut depth = 0_usize; // BEGIN/END nesting depth

    for line in sql.lines() {
        let upper = line.trim().to_uppercase();

        // Track BEGIN/END nesting
        if upper.ends_with("BEGIN") || upper == "BEGIN" {
            depth += 1;
        }

        current.push_str(line);
        current.push('\n');

        if depth > 0 {
            // Inside a BEGIN block — check for END
            if upper.starts_with("END;") || upper == "END;" {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    let stmt = current.trim().trim_end_matches(';').to_string();
                    if !stmt.is_empty() {
                        statements.push(stmt);
                    }
                    current.clear();
                }
            }
        } else if line.contains(';') {
            // Outside BEGIN block and line has semicolons — split
            let accumulated = std::mem::take(&mut current);
            let parts: Vec<&str> = accumulated.split(';').collect();
            for (i, part) in parts.iter().enumerate() {
                let trimmed = part.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if i < parts.len() - 1 {
                    // Complete statement (before a ';')
                    statements.push(trimmed.to_string());
                } else {
                    // Last fragment — carry over to next line
                    current = format!("{trimmed}\n");
                }
            }
        }
    }

    // Any remaining content
    let remaining = current.trim().to_string();
    if !remaining.is_empty() {
        statements.push(remaining);
    }

    statements
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn test_db() -> (Database, TempDir) {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let db = Database::open(&db_path).await.unwrap();
        (db, dir)
    }

    #[test]
    fn test_split_sql_with_triggers() {
        let sql = r#"
CREATE TABLE IF NOT EXISTS foo (id INTEGER PRIMARY KEY);
CREATE INDEX IF NOT EXISTS idx_foo ON foo(id);

CREATE TRIGGER IF NOT EXISTS foo_ai AFTER INSERT ON foo BEGIN
    INSERT INTO bar(rowid, content) VALUES (new.id, new.content);
END;

CREATE TRIGGER IF NOT EXISTS foo_ad AFTER DELETE ON foo BEGIN
    INSERT INTO bar(bar, rowid, content) VALUES ('delete', old.id, old.content);
END;
"#;
        let stmts = split_sql_statements(sql);
        assert_eq!(stmts.len(), 4, "Expected 4 statements, got: {stmts:#?}");
        assert!(stmts[0].contains("CREATE TABLE"));
        assert!(stmts[1].contains("CREATE INDEX"));
        assert!(stmts[2].contains("CREATE TRIGGER") && stmts[2].contains("AFTER INSERT"));
        assert!(stmts[3].contains("CREATE TRIGGER") && stmts[3].contains("AFTER DELETE"));
    }

    #[tokio::test]
    async fn test_open_and_migrate() {
        let (db, _dir) = test_db().await;
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_idempotent_migrations() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let _db1 = Database::open(&db_path).await.unwrap();
        let _db2 = Database::open(&db_path).await.unwrap();
    }

    #[tokio::test]
    async fn test_session_crud() {
        let (db, _dir) = test_db().await;

        db.upsert_session("cli:default", 0).await.unwrap();

        let session = db.load_session("cli:default").await.unwrap().unwrap();
        assert_eq!(session.key, "cli:default");
        assert_eq!(session.last_consolidated, 0);

        db.upsert_session("cli:default", 5).await.unwrap();
        let session = db.load_session("cli:default").await.unwrap().unwrap();
        assert_eq!(session.last_consolidated, 5);
    }

    #[tokio::test]
    async fn test_messages() {
        let (db, _dir) = test_db().await;
        db.upsert_session("cli:test", 0).await.unwrap();

        db.insert_message("cli:test", "user", "Hello", &[])
            .await
            .unwrap();
        db.insert_message("cli:test", "assistant", "Hi!", &[])
            .await
            .unwrap();
        db.insert_message("cli:test", "user", "How are you?", &[])
            .await
            .unwrap();

        assert_eq!(db.count_messages("cli:test").await.unwrap(), 3);

        let msgs = db.load_messages("cli:test", 100).await.unwrap();
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].role, "user");
        assert_eq!(msgs[0].content, "Hello");
        assert_eq!(msgs[2].content, "How are you?");

        // Load with limit (last 2)
        let msgs = db.load_messages("cli:test", 2).await.unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].content, "Hi!");
        assert_eq!(msgs[1].content, "How are you?");
    }

    #[tokio::test]
    async fn test_clear_messages() {
        let (db, _dir) = test_db().await;
        db.upsert_session("cli:test", 3).await.unwrap();
        db.insert_message("cli:test", "user", "msg1", &[])
            .await
            .unwrap();
        db.insert_message("cli:test", "assistant", "msg2", &[])
            .await
            .unwrap();

        db.clear_messages("cli:test").await.unwrap();

        assert_eq!(db.count_messages("cli:test").await.unwrap(), 0);
        let session = db.load_session("cli:test").await.unwrap().unwrap();
        assert_eq!(session.last_consolidated, 0);
    }

    #[tokio::test]
    async fn test_delete_old_messages() {
        let (db, _dir) = test_db().await;
        db.upsert_session("cli:test", 0).await.unwrap();

        for i in 0..10 {
            db.insert_message("cli:test", "user", &format!("msg{}", i), &[])
                .await
                .unwrap();
        }
        assert_eq!(db.count_messages("cli:test").await.unwrap(), 10);

        let deleted = db.delete_old_messages("cli:test", 3).await.unwrap();
        assert_eq!(deleted, 7);
        assert_eq!(db.count_messages("cli:test").await.unwrap(), 3);

        let msgs = db.load_messages("cli:test", 100).await.unwrap();
        assert_eq!(msgs[0].content, "msg7");
        assert_eq!(msgs[2].content, "msg9");
    }

    #[tokio::test]
    async fn test_load_old_messages() {
        let (db, _dir) = test_db().await;
        db.upsert_session("cli:test", 0).await.unwrap();

        for i in 0..10 {
            db.insert_message("cli:test", "user", &format!("msg{}", i), &[])
                .await
                .unwrap();
        }

        let old = db.load_old_messages("cli:test", 3).await.unwrap();
        assert_eq!(old.len(), 7);
        assert_eq!(old[0].content, "msg0");
        assert_eq!(old[6].content, "msg6");
    }

    #[tokio::test]
    async fn test_message_tools_used() {
        let (db, _dir) = test_db().await;
        db.upsert_session("cli:test", 0).await.unwrap();

        let tools = vec!["shell".to_string(), "file".to_string()];
        db.insert_message("cli:test", "assistant", "Done", &tools)
            .await
            .unwrap();

        let msgs = db.load_messages("cli:test", 1).await.unwrap();
        assert_eq!(msgs[0].tools_used, r#"["shell","file"]"#);
    }

    #[tokio::test]
    async fn test_automation_crud_and_runs() {
        let (db, _dir) = test_db().await;

        db.insert_automation(
            "auto-1",
            "Daily brief",
            "Send me a summary",
            "cron:0 9 * * *",
            true,
            "active",
            Some("cli:default"),
            "always",
            None,
        )
        .await
        .unwrap();

        let rows = db.load_automations().await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "auto-1");
        assert_eq!(rows[0].name, "Daily brief");
        assert_eq!(rows[0].trigger_kind, "always");
        assert!(rows[0].trigger_value.is_none());

        let changed = db
            .update_automation(
                "auto-1",
                AutomationUpdate {
                    enabled: Some(false),
                    status: Some("paused".to_string()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert!(changed);

        let row = db.load_automation("auto-1").await.unwrap().unwrap();
        assert!(!row.enabled);
        assert_eq!(row.status, "paused");

        db.insert_automation_run("run-1", "auto-1", "queued", Some("queued"))
            .await
            .unwrap();
        db.complete_automation_run("run-1", "success", Some("ok"))
            .await
            .unwrap();
        db.insert_automation_run("run-2", "auto-1", "queued", Some("queued"))
            .await
            .unwrap();
        db.complete_automation_run("run-2", "error", Some("boom"))
            .await
            .unwrap();

        let last_success = db
            .load_last_successful_automation_result("auto-1", None)
            .await
            .unwrap();
        assert_eq!(last_success.as_deref(), Some("ok"));

        let runs = db.load_automation_runs("auto-1", 10).await.unwrap();
        assert_eq!(runs.len(), 2);
        let statuses = runs.iter().map(|r| r.status.as_str()).collect::<Vec<_>>();
        assert!(statuses.contains(&"success"));
        assert!(statuses.contains(&"error"));
        assert!(runs.iter().any(|r| r.result.as_deref() == Some("ok")));

        let deleted = db.delete_automation("auto-1").await.unwrap();
        assert!(deleted);
    }
}
