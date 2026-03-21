//! Profile system — the fundamental identity unit.
//!
//! A profile scopes memory, knowledge, contacts, sessions, and brain files.
//! The "default" profile always exists and cannot be deleted.

pub mod db;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::storage::Database;

// ── Domain types ────────────────────────────────────────────────────

/// A profile row from the database.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Profile {
    pub id: i64,
    pub slug: String,
    pub display_name: String,
    pub avatar_emoji: String,
    pub profile_json: String,
    /// 1 = default profile, 0 = non-default. Exactly one row should be 1.
    pub is_default: i64,
    pub created_at: String,
    pub updated_at: String,
}

impl Profile {
    /// Parse the cached `profile_json` into structured form.
    pub fn parsed_json(&self) -> Result<ProfileJson> {
        serde_json::from_str(&self.profile_json)
            .context("Failed to parse profile_json")
    }

    /// Directory path for this profile's brain files.
    pub fn brain_dir(&self, data_dir: &Path) -> PathBuf {
        data_dir.join("brain").join("profiles").join(&self.slug)
    }
}

// ── PROFILE.json schema (AIEOS-inspired) ────────────────────────────

/// Structured identity stored as JSON inside the `profile_json` column.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProfileJson {
    pub version: String,
    pub identity: ProfileIdentity,
    pub linguistics: ProfileLinguistics,
    pub personality: ProfilePersonality,
    pub capabilities: ProfileCapabilities,
    pub visibility: ProfileVisibility,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProfileIdentity {
    pub name: String,
    pub display_name: String,
    pub bio: String,
    pub role: String,
    pub avatar_emoji: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProfileLinguistics {
    pub language: String,
    pub formality: String,
    pub style: String,
    pub forbidden_words: Vec<String>,
    pub catchphrases: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProfilePersonality {
    pub traits: Vec<String>,
    pub tone: String,
    pub humor: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProfileCapabilities {
    pub tools_emphasis: Vec<String>,
    pub domains: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProfileVisibility {
    /// Profile slugs this profile can read from. `["default"]` = read default too.
    pub readable_from: Vec<String>,
}

/// Resolve the set of profile IDs visible to a given profile.
///
/// Returns the profile's own ID + IDs from `visibility.readable_from`.
/// Used by memory_search and RAG to filter results.
pub async fn resolve_visible_profile_ids(
    profile: &Profile,
    pool: &sqlx::Pool<sqlx::Sqlite>,
) -> Vec<i64> {
    let mut ids = vec![profile.id];

    if let Ok(pj) = profile.parsed_json() {
        for slug in &pj.visibility.readable_from {
            if let Ok(Some(p)) = db::load_profile_by_slug(pool, slug).await {
                if !ids.contains(&p.id) {
                    ids.push(p.id);
                }
            }
        }
    }

    ids
}

// ── Profile context for system prompt ────────────────────────────────

/// Build a text summary of the profile's PROFILE.json for injection into the system prompt.
///
/// Only includes non-empty sections. Returns empty string if the profile
/// has no structured JSON or if it's all defaults.
pub fn build_profile_context(profile: &Profile) -> String {
    let pj = match profile.parsed_json() {
        Ok(pj) => pj,
        Err(_) => return String::new(),
    };

    let mut parts: Vec<String> = Vec::new();

    // Identity
    if !pj.identity.name.is_empty() || !pj.identity.role.is_empty() {
        let mut identity = format!("Profile: {} ({})", profile.display_name, profile.slug);
        if !pj.identity.role.is_empty() {
            identity.push_str(&format!(" — role: {}", pj.identity.role));
        }
        parts.push(identity);
    }

    // Linguistics
    if !pj.linguistics.language.is_empty()
        || !pj.linguistics.formality.is_empty()
        || !pj.linguistics.style.is_empty()
    {
        let mut ling = String::from("Language: ");
        let mut items = Vec::new();
        if !pj.linguistics.language.is_empty() {
            items.push(pj.linguistics.language.clone());
        }
        if !pj.linguistics.formality.is_empty() {
            items.push(format!("formality: {}", pj.linguistics.formality));
        }
        if !pj.linguistics.style.is_empty() {
            items.push(format!("style: {}", pj.linguistics.style));
        }
        ling.push_str(&items.join(", "));
        if !pj.linguistics.forbidden_words.is_empty() {
            ling.push_str(&format!(
                ". Avoid: {}",
                pj.linguistics.forbidden_words.join(", ")
            ));
        }
        parts.push(ling);
    }

    // Personality
    if !pj.personality.traits.is_empty() || !pj.personality.tone.is_empty() {
        let mut pers = String::from("Personality: ");
        if !pj.personality.traits.is_empty() {
            pers.push_str(&pj.personality.traits.join(", "));
        }
        if !pj.personality.tone.is_empty() {
            pers.push_str(&format!(". Tone: {}", pj.personality.tone));
        }
        if pj.personality.humor {
            pers.push_str(". Humor: yes");
        }
        parts.push(pers);
    }

    // Capabilities
    if !pj.capabilities.domains.is_empty() {
        parts.push(format!("Domains: {}", pj.capabilities.domains.join(", ")));
    }
    if !pj.capabilities.tools_emphasis.is_empty() {
        parts.push(format!(
            "Preferred tools: {}",
            pj.capabilities.tools_emphasis.join(", ")
        ));
    }

    if parts.is_empty() {
        return String::new();
    }

    parts.join("\n")
}

// ── ProfileRegistry ─────────────────────────────────────────────────

/// In-memory cache of all profiles, keyed by slug.
///
/// Loaded from DB at startup, supports hot-reload via [`reload()`].
#[derive(Clone)]
pub struct ProfileRegistry {
    profiles: Arc<RwLock<HashMap<String, Profile>>>,
    data_dir: PathBuf,
}

impl ProfileRegistry {
    /// Load all profiles from the database and create brain directories.
    pub async fn load(db: &Database, data_dir: &Path) -> Result<Self> {
        let all = db::load_all_profiles(db.pool()).await?;
        let mut map = HashMap::with_capacity(all.len());
        for p in all {
            map.insert(p.slug.clone(), p);
        }

        let registry = Self {
            profiles: Arc::new(RwLock::new(map)),
            data_dir: data_dir.to_path_buf(),
        };

        registry.ensure_brain_dirs().await?;

        Ok(registry)
    }

    /// Reload profiles from the database.
    pub async fn reload(&self, db: &Database) -> Result<()> {
        let all = db::load_all_profiles(db.pool()).await?;
        let mut map = HashMap::with_capacity(all.len());
        for p in all {
            map.insert(p.slug.clone(), p);
        }
        *self.profiles.write().await = map;
        self.ensure_brain_dirs().await?;
        Ok(())
    }

    /// Get the default profile.
    pub async fn get_default(&self) -> Option<Profile> {
        self.profiles
            .read()
            .await
            .values()
            .find(|p| p.is_default != 0)
            .cloned()
    }

    /// Get a profile by slug.
    pub async fn get_by_slug(&self, slug: &str) -> Option<Profile> {
        self.profiles.read().await.get(slug).cloned()
    }

    /// Get a profile by database id.
    pub async fn get_by_id(&self, id: i64) -> Option<Profile> {
        self.profiles
            .read()
            .await
            .values()
            .find(|p| p.id == id)
            .cloned()
    }

    /// List all profiles.
    pub async fn list(&self) -> Vec<Profile> {
        self.profiles.read().await.values().cloned().collect()
    }

    /// Create brain directories for all profiles and migrate legacy brain files.
    async fn ensure_brain_dirs(&self) -> Result<()> {
        let profiles = self.profiles.read().await;
        for profile in profiles.values() {
            let dir = profile.brain_dir(&self.data_dir);
            if !dir.exists() {
                std::fs::create_dir_all(&dir).with_context(|| {
                    format!("Failed to create brain dir {}", dir.display())
                })?;
                tracing::info!(profile = %profile.slug, path = %dir.display(), "Created profile brain directory");
            }
        }

        // Migrate legacy brain files into default profile directory
        if let Some(default) = profiles.values().find(|p| p.is_default != 0) {
            self.migrate_legacy_brain_files(default)?;
        }

        Ok(())
    }

    /// Copy legacy global brain files into the default profile directory.
    ///
    /// Files are **copied** (not moved) so the old path still works as fallback
    /// until Sprint 2 changes the agent loop to read from the profile directory.
    fn migrate_legacy_brain_files(&self, default_profile: &Profile) -> Result<()> {
        let brain_root = self.data_dir.join("brain");
        let target_dir = default_profile.brain_dir(&self.data_dir);

        for filename in &["SOUL.md", "USER.md", "INSTRUCTIONS.md"] {
            let legacy = brain_root.join(filename);
            let target = target_dir.join(filename);

            if legacy.exists() && !target.exists() {
                std::fs::copy(&legacy, &target).with_context(|| {
                    format!(
                        "Failed to copy {} → {}",
                        legacy.display(),
                        target.display()
                    )
                })?;
                tracing::info!(
                    file = %filename,
                    from = %legacy.display(),
                    to = %target.display(),
                    "Migrated legacy brain file to default profile"
                );
            }
        }

        Ok(())
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_json_defaults() {
        let pj: ProfileJson = serde_json::from_str("{}").expect("empty JSON parses");
        assert!(pj.version.is_empty());
        assert!(pj.visibility.readable_from.is_empty());
        assert!(!pj.personality.humor);
    }

    #[test]
    fn profile_json_round_trip() {
        let pj = ProfileJson {
            version: "1.0".into(),
            identity: ProfileIdentity {
                name: "Test".into(),
                role: "personal".into(),
                ..Default::default()
            },
            visibility: ProfileVisibility {
                readable_from: vec!["default".into()],
            },
            ..Default::default()
        };
        let json = serde_json::to_string(&pj).expect("serialize");
        let parsed: ProfileJson = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.version, "1.0");
        assert_eq!(parsed.identity.name, "Test");
        assert_eq!(parsed.visibility.readable_from, vec!["default"]);
    }

    #[test]
    fn brain_dir_path() {
        let p = Profile {
            id: 1,
            slug: "acme-corp".into(),
            display_name: "Acme".into(),
            avatar_emoji: "🏢".into(),
            profile_json: "{}".into(),
            is_default: 0,
            created_at: String::new(),
            updated_at: String::new(),
        };
        let dir = p.brain_dir(Path::new("/home/user/.homun"));
        assert_eq!(
            dir,
            PathBuf::from("/home/user/.homun/brain/profiles/acme-corp")
        );
    }
}
