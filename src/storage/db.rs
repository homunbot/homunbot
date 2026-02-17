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
