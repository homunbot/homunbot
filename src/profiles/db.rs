//! Profile system — database operations.
//!
//! CRUD for profiles + contact persona migration.

use anyhow::{bail, Context, Result};
use sqlx::{Pool, Sqlite};

use super::Profile;

// ── CRUD ────────────────────────────────────────────────────────────

/// Insert a new profile and return its id.
pub async fn insert_profile(
    pool: &Pool<Sqlite>,
    slug: &str,
    display_name: &str,
    avatar_emoji: &str,
    profile_json: &str,
) -> Result<i64> {
    let id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO profiles (slug, display_name, avatar_emoji, profile_json)
         VALUES (?, ?, ?, ?)
         RETURNING id",
    )
    .bind(slug)
    .bind(display_name)
    .bind(avatar_emoji)
    .bind(profile_json)
    .fetch_one(pool)
    .await
    .with_context(|| format!("Failed to insert profile '{slug}'"))?;

    Ok(id)
}

/// Load a profile by id.
pub async fn load_profile_by_id(pool: &Pool<Sqlite>, id: i64) -> Result<Option<Profile>> {
    let row = sqlx::query_as::<_, Profile>("SELECT * FROM profiles WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .context("Failed to load profile by id")?;
    Ok(row)
}

/// Load a profile by slug.
pub async fn load_profile_by_slug(pool: &Pool<Sqlite>, slug: &str) -> Result<Option<Profile>> {
    let row = sqlx::query_as::<_, Profile>("SELECT * FROM profiles WHERE slug = ?")
        .bind(slug)
        .fetch_optional(pool)
        .await
        .context("Failed to load profile by slug")?;
    Ok(row)
}

/// Load all profiles, ordered by is_default DESC then slug ASC.
pub async fn load_all_profiles(pool: &Pool<Sqlite>) -> Result<Vec<Profile>> {
    let rows = sqlx::query_as::<_, Profile>(
        "SELECT * FROM profiles ORDER BY is_default DESC, slug ASC",
    )
    .fetch_all(pool)
    .await
    .context("Failed to load profiles")?;
    Ok(rows)
}

/// Get the default profile (exactly one row with is_default = 1).
pub async fn get_default_profile(pool: &Pool<Sqlite>) -> Result<Profile> {
    sqlx::query_as::<_, Profile>("SELECT * FROM profiles WHERE is_default = 1")
        .fetch_one(pool)
        .await
        .context("Default profile not found — database may be corrupt")
}

/// Update a profile's mutable fields.
pub async fn update_profile(
    pool: &Pool<Sqlite>,
    id: i64,
    display_name: &str,
    avatar_emoji: &str,
    profile_json: &str,
) -> Result<()> {
    sqlx::query(
        "UPDATE profiles SET display_name = ?, avatar_emoji = ?, profile_json = ?,
                updated_at = datetime('now')
         WHERE id = ?",
    )
    .bind(display_name)
    .bind(avatar_emoji)
    .bind(profile_json)
    .bind(id)
    .execute(pool)
    .await
    .context("Failed to update profile")?;
    Ok(())
}

/// Delete a profile. Refuses to delete the default profile.
pub async fn delete_profile(pool: &Pool<Sqlite>, id: i64) -> Result<()> {
    let is_default: i64 =
        sqlx::query_scalar("SELECT is_default FROM profiles WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await
            .context("Failed to check profile")?
            .unwrap_or(0);

    if is_default != 0 {
        bail!("Cannot delete the default profile");
    }

    sqlx::query("DELETE FROM profiles WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .context("Failed to delete profile")?;

    Ok(())
}

// ── Contact persona → profile migration ─────────────────────────────

/// Migrate existing contact persona_override values into profiles.
///
/// For each distinct persona_override ("owner", "company", "custom"):
/// - Creates a profile if it doesn't already exist
/// - Updates the contact's `profile_id` to point to the new profile
/// - For "custom" personas, creates per-contact profiles with persona_instructions as SOUL.md
///
/// This function is idempotent — safe to call on every startup.
pub async fn migrate_contact_personas(
    pool: &Pool<Sqlite>,
    data_dir: &std::path::Path,
) -> Result<()> {
    // "owner" persona → owner profile
    migrate_simple_persona(pool, "owner", "Owner", "👑").await?;

    // "company" persona → company profile
    migrate_simple_persona(pool, "company", "Company", "🏢").await?;

    // "custom" personas → per-contact profiles
    migrate_custom_personas(pool, data_dir).await?;

    Ok(())
}

/// Migrate a simple persona type (owner/company) to a profile.
async fn migrate_simple_persona(
    pool: &Pool<Sqlite>,
    persona: &str,
    display_name: &str,
    emoji: &str,
) -> Result<()> {
    // Check if any contacts have this persona
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM contacts
         WHERE persona_override = ? AND (profile_id IS NULL OR profile_id = 1)",
    )
    .bind(persona)
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    if count == 0 {
        return Ok(());
    }

    // Create profile if it doesn't exist
    let existing = load_profile_by_slug(pool, persona).await?;
    let profile_id = match existing {
        Some(p) => p.id,
        None => {
            let id = insert_profile(pool, persona, display_name, emoji, "{}").await?;
            tracing::info!(profile = persona, "Created profile from persona migration");
            id
        }
    };

    // Update contacts
    let updated = sqlx::query(
        "UPDATE contacts SET profile_id = ?
         WHERE persona_override = ? AND (profile_id IS NULL OR profile_id = 1)",
    )
    .bind(profile_id)
    .bind(persona)
    .execute(pool)
    .await
    .context("Failed to update contacts for persona migration")?;

    if updated.rows_affected() > 0 {
        tracing::info!(
            persona,
            count = updated.rows_affected(),
            "Migrated contacts from persona to profile"
        );
    }

    Ok(())
}

/// Migrate "custom" persona contacts into per-contact profiles.
async fn migrate_custom_personas(
    pool: &Pool<Sqlite>,
    data_dir: &std::path::Path,
) -> Result<()> {
    // Find contacts with custom persona that haven't been migrated yet
    let rows: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT id, name, persona_instructions FROM contacts
         WHERE persona_override = 'custom' AND (profile_id IS NULL OR profile_id = 1)",
    )
    .fetch_all(pool)
    .await
    .context("Failed to query custom persona contacts")?;

    for (contact_id, contact_name, instructions) in rows {
        let slug = format!("custom-{contact_id}");

        // Skip if profile already exists
        if load_profile_by_slug(pool, &slug).await?.is_some() {
            continue;
        }

        let display_name = format!("Custom ({contact_name})");
        let profile_id =
            insert_profile(pool, &slug, &display_name, "✨", "{}").await?;

        // Write persona_instructions as SOUL.md for the new profile
        if !instructions.is_empty() {
            let profile_dir = data_dir
                .join("brain")
                .join("profiles")
                .join(&slug);
            std::fs::create_dir_all(&profile_dir).ok();
            let soul_path = profile_dir.join("SOUL.md");
            if !soul_path.exists() {
                std::fs::write(&soul_path, &instructions).with_context(|| {
                    format!("Failed to write SOUL.md for profile '{slug}'")
                })?;
            }
        }

        // Update the contact
        sqlx::query("UPDATE contacts SET profile_id = ? WHERE id = ?")
            .bind(profile_id)
            .bind(contact_id)
            .execute(pool)
            .await
            .context("Failed to update contact profile_id")?;

        tracing::info!(
            slug,
            contact = contact_name,
            "Created custom profile from persona migration"
        );
    }

    Ok(())
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn test_pool() -> Pool<Sqlite> {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .expect("in-memory SQLite");

        // Create profiles table
        sqlx::query(
            "CREATE TABLE profiles (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                slug TEXT UNIQUE NOT NULL,
                display_name TEXT NOT NULL,
                avatar_emoji TEXT NOT NULL DEFAULT '👤',
                profile_json TEXT NOT NULL DEFAULT '{}',
                is_default INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            )",
        )
        .execute(&pool)
        .await
        .expect("create table");

        // Seed default
        sqlx::query(
            "INSERT INTO profiles (slug, display_name, is_default) VALUES ('default', 'Default', 1)",
        )
        .execute(&pool)
        .await
        .expect("seed default");

        pool
    }

    #[tokio::test]
    async fn crud_lifecycle() {
        let pool = test_pool().await;

        // Insert
        let id = insert_profile(&pool, "acme", "Acme Corp", "🏢", "{}")
            .await
            .expect("insert");
        assert!(id > 1); // default is id=1

        // Load by id
        let p = load_profile_by_id(&pool, id).await.expect("load").expect("found");
        assert_eq!(p.slug, "acme");
        assert_eq!(p.avatar_emoji, "🏢");

        // Load by slug
        let p2 = load_profile_by_slug(&pool, "acme").await.expect("load").expect("found");
        assert_eq!(p2.id, id);

        // List
        let all = load_all_profiles(&pool).await.expect("list");
        assert_eq!(all.len(), 2); // default + acme
        assert_eq!(all[0].is_default, 1); // default comes first

        // Update
        update_profile(&pool, id, "Acme Inc", "🏭", r#"{"version":"1.0"}"#)
            .await
            .expect("update");
        let updated = load_profile_by_id(&pool, id).await.expect("load").expect("found");
        assert_eq!(updated.display_name, "Acme Inc");
        assert_eq!(updated.avatar_emoji, "🏭");

        // Delete
        delete_profile(&pool, id).await.expect("delete");
        assert!(load_profile_by_id(&pool, id).await.expect("load").is_none());
    }

    #[tokio::test]
    async fn cannot_delete_default() {
        let pool = test_pool().await;
        let default = get_default_profile(&pool).await.expect("default");
        let result = delete_profile(&pool, default.id).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("default"));
    }

    #[tokio::test]
    async fn duplicate_slug_rejected() {
        let pool = test_pool().await;
        insert_profile(&pool, "test", "Test", "🧪", "{}")
            .await
            .expect("first insert");
        let result = insert_profile(&pool, "test", "Test 2", "🧪", "{}").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn get_default() {
        let pool = test_pool().await;
        let default = get_default_profile(&pool).await.expect("default");
        assert_eq!(default.slug, "default");
        assert_eq!(default.is_default, 1);
    }
}
