use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

/// Metadata parsed from SKILL.md YAML frontmatter.
///
/// Follows the open Agent Skills specification (https://github.com/agentskills/agentskills):
/// - name: unique skill identifier (lowercase, hyphens only)
/// - description: what the skill does AND when to use it
/// - Optional: license, compatibility, metadata, allowed-tools
#[derive(Debug, Clone, Deserialize)]
pub struct SkillMetadata {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub compatibility: Option<String>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
    #[serde(rename = "allowed-tools", default)]
    pub allowed_tools: Option<String>,
}

/// A loaded skill — metadata + optional full content.
///
/// Progressive disclosure: at startup only metadata is loaded (~100 tokens).
/// The full body is loaded on demand when the LLM activates the skill.
#[derive(Debug, Clone)]
pub struct Skill {
    /// Parsed frontmatter metadata
    pub meta: SkillMetadata,
    /// Path to the skill directory
    pub path: PathBuf,
    /// Full SKILL.md body (markdown), loaded on demand
    pub body: Option<String>,
}

impl Skill {
    /// Get the full body, loading it from disk if needed
    pub async fn load_body(&mut self) -> Result<&str> {
        if self.body.is_none() {
            let skill_md_path = self.path.join("SKILL.md");
            let content = tokio::fs::read_to_string(&skill_md_path)
                .await
                .with_context(|| {
                    format!("Failed to read SKILL.md from {}", skill_md_path.display())
                })?;
            let (_, body) = parse_skill_md(&content)?;
            self.body = Some(body);
        }
        Ok(self.body.as_deref().unwrap_or(""))
    }
}

/// In-memory skill registry.
///
/// At startup, scans skill directories and loads only metadata (name + description).
/// Full skill content is loaded on demand for progressive disclosure.
pub struct SkillRegistry {
    skills: HashMap<String, Skill>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self {
            skills: HashMap::new(),
        }
    }

    /// Scan directories for skills and load their metadata.
    /// Scans in priority order:
    /// 1. ~/.homunbot/skills/ (user-installed)
    /// 2. ./skills/ (project-local)
    pub async fn scan_and_load(&mut self) -> Result<()> {
        let scan_dirs = vec![
            // User-installed skills
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".homunbot")
                .join("skills"),
            // Project-local skills
            PathBuf::from("skills"),
        ];

        for dir in scan_dirs {
            if dir.exists() && dir.is_dir() {
                self.scan_directory(&dir).await?;
            }
        }

        if !self.skills.is_empty() {
            tracing::info!(
                skills = self.skills.len(),
                names = ?self.skills.keys().collect::<Vec<_>>(),
                "Skills loaded"
            );
        }

        Ok(())
    }

    /// Scan a single directory for skill subdirectories
    async fn scan_directory(&mut self, dir: &Path) -> Result<()> {
        let mut entries = tokio::fs::read_dir(dir)
            .await
            .with_context(|| format!("Failed to read skills directory {}", dir.display()))?;

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.is_dir() {
                let skill_md = path.join("SKILL.md");
                if skill_md.exists() {
                    match self.load_skill_metadata(&path).await {
                        Ok(skill) => {
                            tracing::debug!(
                                skill = %skill.meta.name,
                                path = %path.display(),
                                "Loaded skill metadata"
                            );
                            self.skills.insert(skill.meta.name.clone(), skill);
                        }
                        Err(e) => {
                            tracing::warn!(
                                path = %path.display(),
                                error = %e,
                                "Failed to load skill"
                            );
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Load only the metadata (frontmatter) from a skill directory
    async fn load_skill_metadata(&self, skill_dir: &Path) -> Result<Skill> {
        let skill_md_path = skill_dir.join("SKILL.md");
        let content = tokio::fs::read_to_string(&skill_md_path)
            .await
            .with_context(|| {
                format!("Failed to read {}", skill_md_path.display())
            })?;

        let (meta, _) = parse_skill_md(&content)?;

        // Validate name matches directory
        let dir_name = skill_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        if meta.name != dir_name {
            tracing::warn!(
                skill = %meta.name,
                dir = %dir_name,
                "Skill name doesn't match directory name"
            );
        }

        Ok(Skill {
            meta,
            path: skill_dir.to_path_buf(),
            body: None, // Loaded on demand (progressive disclosure)
        })
    }

    /// Get a skill by name
    pub fn get(&self, name: &str) -> Option<&Skill> {
        self.skills.get(name)
    }

    /// Get a mutable skill by name (for loading body)
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Skill> {
        self.skills.get_mut(name)
    }

    /// List all loaded skills (name + description only)
    pub fn list(&self) -> Vec<(&str, &str)> {
        self.skills
            .values()
            .map(|s| (s.meta.name.as_str(), s.meta.description.as_str()))
            .collect()
    }

    /// Build the skills summary for the system prompt.
    /// Returns a compact XML-style list of skill names + descriptions.
    pub fn build_prompt_summary(&self) -> String {
        if self.skills.is_empty() {
            return String::new();
        }

        let mut summary = String::from("\n\nAvailable Skills:\n");
        for skill in self.skills.values() {
            summary.push_str(&format!(
                "- {}: {}\n",
                skill.meta.name, skill.meta.description
            ));
        }
        summary.push_str("\nTo use a skill, reference it by name. The full instructions will be loaded.");
        summary
    }

    /// Number of loaded skills
    pub fn len(&self) -> usize {
        self.skills.len()
    }

    /// Whether the registry is empty
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse a SKILL.md file: extract YAML frontmatter + markdown body.
///
/// Uses the `gray_matter` crate for frontmatter parsing.
/// Returns (SkillMetadata, body_markdown).
///
/// Public alias for use by the installer module.
pub fn parse_skill_md_public(content: &str) -> Result<(SkillMetadata, String)> {
    parse_skill_md(content)
}

/// Parse a SKILL.md file (internal).
fn parse_skill_md(content: &str) -> Result<(SkillMetadata, String)> {
    let matter = gray_matter::Matter::<gray_matter::engine::YAML>::new();
    let parsed = matter.parse(content);

    let data = parsed
        .data
        .context("SKILL.md has no YAML frontmatter")?;

    // Convert gray_matter's Pod to serde_json::Value, then deserialize
    let json_value: serde_json::Value = data.into();
    let meta: SkillMetadata = serde_json::from_value(json_value)
        .context("Failed to parse SKILL.md frontmatter")?;

    // Validate required fields
    if meta.name.is_empty() {
        anyhow::bail!("SKILL.md frontmatter: 'name' is required");
    }
    if meta.description.is_empty() {
        anyhow::bail!("SKILL.md frontmatter: 'description' is required");
    }

    Ok((meta, parsed.content))
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_skill_md_basic() {
        let content = r#"---
name: test-skill
description: A test skill for unit testing
---

# Test Skill

This is the body of the skill.
"#;
        let (meta, body) = parse_skill_md(content).unwrap();
        assert_eq!(meta.name, "test-skill");
        assert_eq!(meta.description, "A test skill for unit testing");
        assert!(body.contains("# Test Skill"));
        assert!(body.contains("body of the skill"));
    }

    #[test]
    fn test_parse_skill_md_full() {
        let content = r#"---
name: market-monitor
description: Monitor prices and alert on changes
license: MIT
compatibility: Requires internet access
allowed-tools: "Web Bash(curl:*)"
metadata:
  author: homunbot
  version: "1.0"
---

# Market Monitor

Instructions here.
"#;
        let (meta, _body) = parse_skill_md(content).unwrap();
        assert_eq!(meta.name, "market-monitor");
        assert_eq!(meta.license.as_deref(), Some("MIT"));
        assert_eq!(
            meta.compatibility.as_deref(),
            Some("Requires internet access")
        );
        assert_eq!(meta.allowed_tools.as_deref(), Some("Web Bash(curl:*)"));
        assert!(meta.metadata.is_some());
    }

    #[test]
    fn test_parse_skill_md_missing_name() {
        let content = r#"---
description: No name
---

Body.
"#;
        let result = parse_skill_md(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_skill_md_no_frontmatter() {
        let content = "# Just markdown\n\nNo frontmatter here.";
        let result = parse_skill_md(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_registry_build_prompt_summary() {
        let mut registry = SkillRegistry::new();

        // Empty registry
        assert_eq!(registry.build_prompt_summary(), "");

        // Add a skill manually
        registry.skills.insert(
            "test".to_string(),
            Skill {
                meta: SkillMetadata {
                    name: "test".to_string(),
                    description: "A test skill".to_string(),
                    license: None,
                    compatibility: None,
                    metadata: None,
                    allowed_tools: None,
                },
                path: PathBuf::from("/tmp/test"),
                body: None,
            },
        );

        let summary = registry.build_prompt_summary();
        assert!(summary.contains("test: A test skill"));
        assert!(summary.contains("Available Skills"));
    }

    #[tokio::test]
    async fn test_scan_empty_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut registry = SkillRegistry::new();
        registry.scan_directory(dir.path()).await.unwrap();
        assert_eq!(registry.len(), 0);
    }

    #[tokio::test]
    async fn test_scan_with_skill() {
        let dir = tempfile::TempDir::new().unwrap();

        // Create a valid skill directory
        let skill_dir = dir.path().join("my-skill");
        tokio::fs::create_dir(&skill_dir).await.unwrap();
        tokio::fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: my-skill
description: A test skill
---

# My Skill

Instructions here.
"#,
        )
        .await
        .unwrap();

        let mut registry = SkillRegistry::new();
        registry.scan_directory(dir.path()).await.unwrap();
        assert_eq!(registry.len(), 1);
        assert!(registry.get("my-skill").is_some());

        let skill = registry.get("my-skill").unwrap();
        assert_eq!(skill.meta.name, "my-skill");
        assert!(skill.body.is_none()); // Not loaded yet (progressive)
    }

    #[tokio::test]
    async fn test_load_body_on_demand() {
        let dir = tempfile::TempDir::new().unwrap();

        let skill_dir = dir.path().join("lazy-skill");
        tokio::fs::create_dir(&skill_dir).await.unwrap();
        tokio::fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: lazy-skill
description: Test lazy loading
---

# Lazy Body

This should only load on demand.
"#,
        )
        .await
        .unwrap();

        let mut registry = SkillRegistry::new();
        registry.scan_directory(dir.path()).await.unwrap();

        // Body not loaded at startup
        assert!(registry.get("lazy-skill").unwrap().body.is_none());

        // Load body on demand
        let skill = registry.get_mut("lazy-skill").unwrap();
        let body = skill.load_body().await.unwrap();
        assert!(body.contains("Lazy Body"));
        assert!(body.contains("only load on demand"));
    }
}
