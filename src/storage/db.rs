use std::path::Path;

use anyhow::{Context, Result};
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
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create database directory {}", parent.display()))?;
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
            )"
        )
        .execute(pool)
        .await
        .context("Failed to create migrations table")?;

        // Migration 001
        let migration_name = "001_initial";
        let already_applied: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM _migrations WHERE name = ?)"
        )
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
                        format!("Migration failed: {}", &statement[..statement.len().min(80)])
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

        Ok(())
    }

    /// Apply a named migration if not already applied.
    async fn apply_migration(pool: &Pool<Sqlite>, name: &str, sql: &str) -> Result<()> {
        let already_applied: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM _migrations WHERE name = ?)",
        )
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
    pub async fn upsert_session(
        &self,
        key: &str,
        last_consolidated: i64,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO sessions (key, last_consolidated)
             VALUES (?, ?)
             ON CONFLICT(key) DO UPDATE SET
                updated_at = datetime('now'),
                last_consolidated = excluded.last_consolidated"
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
             FROM sessions WHERE key = ?"
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
        let tools_json = serde_json::to_string(tools_used)
            .unwrap_or_else(|_| "[]".to_string());

        sqlx::query(
            "INSERT INTO messages (session_key, role, content, tools_used)
             VALUES (?, ?, ?, ?)"
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
    pub async fn load_messages(
        &self,
        session_key: &str,
        limit: u32,
    ) -> Result<Vec<MessageRow>> {
        let rows = sqlx::query_as::<_, MessageRow>(
            "SELECT id, session_key, role, content, tools_used, timestamp
             FROM (
                 SELECT * FROM messages
                 WHERE session_key = ?
                 ORDER BY id DESC
                 LIMIT ?
             ) sub
             ORDER BY id ASC"
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
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM messages WHERE session_key = ?"
        )
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
             WHERE key = ?"
        )
        .bind(session_key)
        .execute(&self.pool)
        .await
        .context("Failed to reset session")?;

        Ok(())
    }

    // --- Memory operations ---

    /// Insert a consolidated memory
    pub async fn insert_memory(
        &self,
        session_key: Option<&str>,
        content: &str,
        memory_type: &str,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO memories (session_key, content, memory_type) VALUES (?, ?, ?)"
        )
        .bind(session_key)
        .bind(content)
        .bind(memory_type)
        .execute(&self.pool)
        .await
        .context("Failed to insert memory")?;

        Ok(())
    }

    /// Load all memories for a session (plus global memories)
    pub async fn load_memories(
        &self,
        session_key: &str,
    ) -> Result<Vec<MemoryRow>> {
        let rows = sqlx::query_as::<_, MemoryRow>(
            "SELECT id, session_key, content, memory_type, created_at
             FROM memories
             WHERE session_key IS NULL OR session_key = ?
             ORDER BY created_at ASC"
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
             ORDER BY created_at DESC LIMIT 1"
        )
        .fetch_optional(&self.pool)
        .await
        .context("Failed to load long-term memory")?;

        Ok(row.map(|(c,)| c))
    }

    /// Replace the global long-term memory (upsert pattern)
    pub async fn upsert_long_term_memory(&self, content: &str) -> Result<()> {
        // Delete old global long-term memory, then insert fresh
        sqlx::query(
            "DELETE FROM memories WHERE memory_type = 'long_term' AND session_key IS NULL"
        )
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
             VALUES (?, ?, ?, ?, ?)"
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
             ORDER BY created_at ASC"
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to load cron jobs")?;

        Ok(rows)
    }

    /// Update last_run timestamp for a cron job
    pub async fn update_cron_last_run(&self, id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE cron_jobs SET last_run = datetime('now') WHERE id = ?"
        )
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
    pub async fn fts5_search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<(i64, f64)>> {
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
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM memory_chunks")
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
        let result = sqlx::query(
            "UPDATE cron_jobs SET enabled = ? WHERE id = ?"
        )
        .bind(enabled)
        .bind(id)
        .execute(&self.pool)
        .await
        .context("Failed to toggle cron job")?;

        Ok(result.rows_affected() > 0)
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

        let result = sqlx::query(
            "DELETE FROM messages WHERE timestamp < ?"
        )
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
        let result = sqlx::query(
            "DELETE FROM memory_chunks WHERE date < ? AND memory_type = 'history'"
        )
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
    pub async fn create_user(
        &self,
        id: &str,
        username: &str,
        roles: &[&str],
    ) -> Result<()> {
        let roles_json = serde_json::to_string(roles)
            .unwrap_or_else(|_| "[]".to_string());

        sqlx::query(
            "INSERT INTO users (id, username, roles) VALUES (?, ?, ?)"
        )
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
             FROM users WHERE id = ?"
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
             FROM users WHERE username = ?"
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
             FROM users ORDER BY created_at ASC"
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to load users")?;

        Ok(rows)
    }

    /// Update a user's roles.
    pub async fn update_user_roles(&self, id: &str, roles: &[&str]) -> Result<bool> {
        let roles_json = serde_json::to_string(roles)
            .unwrap_or_else(|_| "[]".to_string());

        let result = sqlx::query(
            "UPDATE users SET roles = ?, updated_at = datetime('now') WHERE id = ?"
        )
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
             VALUES (?, ?, ?, ?)"
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
             WHERE i.channel = ? AND i.platform_id = ?"
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
             ORDER BY created_at ASC"
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
             WHERE user_id = ? AND channel = ? AND platform_id = ?"
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
    pub async fn create_webhook_token(
        &self,
        token: &str,
        user_id: &str,
        name: &str,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO webhook_tokens (token, user_id, name) VALUES (?, ?, ?)"
        )
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
             WHERE wt.token = ? AND wt.enabled = 1"
        )
        .bind(token)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to lookup user by webhook token")?;

        Ok(row)
    }

    /// Update the last_used timestamp for a webhook token.
    pub async fn touch_webhook_token(&self, token: &str) -> Result<()> {
        sqlx::query(
            "UPDATE webhook_tokens SET last_used = datetime('now') WHERE token = ?"
        )
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
             ORDER BY created_at DESC"
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
        let result = sqlx::query(
            "UPDATE webhook_tokens SET enabled = ? WHERE token = ?"
        )
        .bind(enabled)
        .bind(token)
        .execute(&self.pool)
        .await
        .context("Failed to toggle webhook token")?;

        Ok(result.rows_affected() > 0)
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

// ═══════════════════════════════════════════════════════════════
// USER SYSTEM ROW TYPES
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UserRow {
    pub id: String,
    pub username: String,
    pub roles: String,        // JSON array
    pub created_at: String,
    pub updated_at: String,
    pub metadata: String,     // JSON object
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

        db.insert_message("cli:test", "user", "Hello", &[]).await.unwrap();
        db.insert_message("cli:test", "assistant", "Hi!", &[]).await.unwrap();
        db.insert_message("cli:test", "user", "How are you?", &[]).await.unwrap();

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
        db.insert_message("cli:test", "user", "msg1", &[]).await.unwrap();
        db.insert_message("cli:test", "assistant", "msg2", &[]).await.unwrap();

        db.clear_messages("cli:test").await.unwrap();

        assert_eq!(db.count_messages("cli:test").await.unwrap(), 0);
        let session = db.load_session("cli:test").await.unwrap().unwrap();
        assert_eq!(session.last_consolidated, 0);
    }

    #[tokio::test]
    async fn test_message_tools_used() {
        let (db, _dir) = test_db().await;
        db.upsert_session("cli:test", 0).await.unwrap();

        let tools = vec!["shell".to_string(), "file".to_string()];
        db.insert_message("cli:test", "assistant", "Done", &tools).await.unwrap();

        let msgs = db.load_messages("cli:test", 1).await.unwrap();
        assert_eq!(msgs[0].tools_used, r#"["shell","file"]"#);
    }
}
