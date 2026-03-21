use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::config::Config;

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
    /// Whether the skill can be invoked via `/skill-name` slash commands.
    /// Default: true. Set to false to hide from slash command dispatch.
    #[serde(rename = "user-invocable", default = "default_true")]
    pub user_invocable: bool,
    /// Whether to exclude this skill from the LLM's system prompt and tool list.
    /// Default: false. Set to true for skills only invocable by the user via slash commands.
    #[serde(rename = "disable-model-invocation", default)]
    pub disable_model_invocation: bool,
}

fn default_true() -> bool {
    true
}

/// Runtime requirements parsed from skill metadata (OpenClaw format).
///
/// Extracted from `metadata.openclaw.requires` (or `metadata.clawdbot.requires`).
/// Skills failing these checks are excluded from the LLM prompt.
#[derive(Debug, Clone, Default)]
pub struct SkillRequirements {
    /// All these binaries must be present (AND logic)
    pub bins: Vec<String>,
    /// At least one of these binaries must be present (OR logic)
    pub any_bins: Vec<String>,
    /// All these environment variables must be set
    pub env: Vec<String>,
    /// OS must match one of these (e.g. "macos", "linux", "windows")
    pub os: Vec<String>,
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
    /// Whether this skill passes runtime eligibility checks
    pub eligible: bool,
    /// Profile this skill belongs to. None = global (available to all profiles).
    pub profile_slug: Option<String>,
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
    /// 1. ~/.homun/skills/ (user-installed, global)
    /// 2. ./skills/ (project-local, global)
    /// 3. ~/.homun/brain/profiles/{slug}/skills/ (per-profile)
    pub async fn scan_and_load(&mut self) -> Result<()> {
        let scan_dirs = vec![
            // User-installed skills
            Config::skills_dir(),
            // Project-local skills
            PathBuf::from("skills"),
        ];

        for dir in scan_dirs {
            if dir.exists() && dir.is_dir() {
                self.scan_directory_with_profile(&dir, None).await?;
            }
        }

        // Scan per-profile skill directories
        let data_dir = Config::data_dir();
        self.scan_profile_skills(&data_dir).await?;

        // Check eligibility for all loaded skills
        self.check_all_eligibility();

        if !self.skills.is_empty() {
            let eligible_count = self.skills.values().filter(|s| s.eligible).count();
            tracing::info!(
                total = self.skills.len(),
                eligible = eligible_count,
                names = ?self.skills.values().filter(|s| s.eligible).map(|s| &s.meta.name).collect::<Vec<_>>(),
                "Skills loaded"
            );
        }

        Ok(())
    }

    /// Check runtime eligibility for all loaded skills.
    ///
    /// Skills failing eligibility checks (missing bins, env vars, wrong OS)
    /// are marked as ineligible and excluded from the LLM prompt.
    pub fn check_all_eligibility(&mut self) {
        for skill in self.skills.values_mut() {
            let reqs = extract_requirements(&skill.meta.metadata);
            match check_eligibility(&reqs) {
                Ok(()) => {
                    skill.eligible = true;
                }
                Err(reason) => {
                    skill.eligible = false;
                    tracing::warn!(
                        skill = %skill.meta.name,
                        reason = %reason,
                        "Skill ineligible — excluded from prompt"
                    );
                }
            }
        }
    }

    /// Scan profile-specific skill directories.
    ///
    /// Scans `~/.homun/brain/profiles/{slug}/skills/` for each profile.
    /// Skills found are tagged with the profile slug.
    pub async fn scan_profile_skills(&mut self, data_dir: &Path) -> Result<()> {
        let profiles_dir = data_dir.join("brain").join("profiles");
        if !profiles_dir.exists() {
            return Ok(());
        }
        let mut entries = match tokio::fs::read_dir(&profiles_dir).await {
            Ok(e) => e,
            Err(_) => return Ok(()),
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let profile_path = entry.path();
            if !profile_path.is_dir() {
                continue;
            }
            let skills_dir = profile_path.join("skills");
            if skills_dir.exists() && skills_dir.is_dir() {
                let slug = profile_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                self.scan_directory_with_profile(&skills_dir, Some(&slug))
                    .await?;
            }
        }
        Ok(())
    }

    /// Scan a directory for skills, optionally tagging with a profile slug.
    async fn scan_directory_with_profile(
        &mut self,
        dir: &Path,
        profile_slug: Option<&str>,
    ) -> Result<()> {
        let mut entries = tokio::fs::read_dir(dir)
            .await
            .with_context(|| format!("Failed to read skills directory {}", dir.display()))?;

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.is_dir() {
                let skill_md = path.join("SKILL.md");
                if skill_md.exists() {
                    match self.load_skill_metadata(&path).await {
                        Ok(mut skill) => {
                            skill.profile_slug = profile_slug.map(|s| s.to_string());
                            tracing::debug!(
                                skill = %skill.meta.name,
                                path = %path.display(),
                                profile = ?profile_slug,
                                "Loaded skill metadata"
                            );
                            self.skills.insert(skill.meta.name.clone(), skill);
                        }
                        Err(e) => {
                            tracing::warn!(
                                path = %path.display(),
                                error = ?e,
                                "Failed to load skill"
                            );
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Scan a single directory for skill subdirectories (legacy, delegates to profile-aware version)
    #[allow(dead_code)]
    async fn scan_directory(&mut self, dir: &Path) -> Result<()> {
        self.scan_directory_with_profile(dir, None).await
    }

    /// Load only the metadata (frontmatter) from a skill directory
    async fn load_skill_metadata(&self, skill_dir: &Path) -> Result<Skill> {
        let skill_md_path = skill_dir.join("SKILL.md");
        let content = tokio::fs::read_to_string(&skill_md_path)
            .await
            .with_context(|| format!("Failed to read {}", skill_md_path.display()))?;

        let (meta, _) = parse_skill_md(&content)?;

        // Validate name matches directory
        let dir_name = skill_dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
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
            body: None,     // Loaded on demand (progressive disclosure)
            eligible: true, // Checked later by check_all_eligibility()
            profile_slug: None, // Set by scan_directory_with_profile
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

    /// List all loaded skills (name + description only), including ineligible.
    pub fn list(&self) -> Vec<(&str, &str)> {
        self.skills
            .values()
            .map(|s| (s.meta.name.as_str(), s.meta.description.as_str()))
            .collect()
    }

    /// List eligible skills only (name + description).
    ///
    /// Skills failing runtime eligibility checks are excluded.
    pub fn list_eligible(&self) -> Vec<(&str, &str)> {
        self.skills
            .values()
            .filter(|s| s.eligible)
            .map(|s| (s.meta.name.as_str(), s.meta.description.as_str()))
            .collect()
    }

    /// List skills available for LLM model invocation (name + description).
    ///
    /// Filters: eligible AND NOT disable-model-invocation.
    /// Used for tool registration and system prompt summary.
    pub fn list_for_model(&self) -> Vec<(&str, &str)> {
        self.skills
            .values()
            .filter(|s| s.eligible && !s.meta.disable_model_invocation)
            .map(|s| (s.meta.name.as_str(), s.meta.description.as_str()))
            .collect()
    }

    /// Map skill name → profile slug for per-profile skills only.
    ///
    /// Global skills (profile_slug=None) are not included in the map.
    /// Used by cognition discovery to filter skills by active profile.
    pub fn list_profile_scopes(&self) -> std::collections::HashMap<&str, &str> {
        self.skills
            .values()
            .filter_map(|s| {
                s.profile_slug
                    .as_deref()
                    .map(|slug| (s.meta.name.as_str(), slug))
            })
            .collect()
    }

    /// Build the skills summary for the system prompt.
    /// Lists only model-invocable skills with `/slash-command` hints and descriptions.
    pub fn build_prompt_summary(&self) -> String {
        let model_skills: Vec<_> = self
            .skills
            .values()
            .filter(|s| s.eligible && !s.meta.disable_model_invocation)
            .collect();
        if model_skills.is_empty() {
            return String::new();
        }

        let mut summary = String::from("\n\nAvailable Skills:\n");
        for skill in &model_skills {
            summary.push_str(&format!(
                "- {} (/{0}): {}\n",
                skill.meta.name, skill.meta.description
            ));
        }
        summary.push_str(
            "\nTo use a skill, call it as a tool with a `query` parameter, \
             or the user can invoke it with `/skill-name query`.\n",
        );
        summary
    }

    /// Public version of scan_directory for use by the skill watcher.
    pub async fn scan_directory_public(&mut self, dir: &Path) -> Result<()> {
        self.scan_directory(dir).await
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
///
/// Uses `gray_matter` as primary parser with a manual fallback for long lines
/// (gray_matter's YAML engine silently returns null on very long values).
fn parse_skill_md(content: &str) -> Result<(SkillMetadata, String)> {
    let matter = gray_matter::Matter::<gray_matter::engine::YAML>::new();
    let parsed = matter.parse(content);

    let json_value: serde_json::Value = match parsed.data {
        Some(data) => {
            let v: serde_json::Value = data.into();
            if v.is_null() {
                // gray_matter returned null Pod — fallback to manual extraction
                extract_frontmatter_manual(content)?
            } else {
                v
            }
        }
        None => anyhow::bail!("SKILL.md has no YAML frontmatter"),
    };

    let meta: SkillMetadata = serde_json::from_value(json_value.clone()).with_context(|| {
        format!(
            "Failed to parse SKILL.md frontmatter: {}",
            serde_json::to_string(&json_value).unwrap_or_default()
        )
    })?;

    // Validate required fields
    if meta.name.is_empty() {
        anyhow::bail!("SKILL.md frontmatter: 'name' is required");
    }
    if meta.description.is_empty() {
        anyhow::bail!("SKILL.md frontmatter: 'description' is required");
    }

    Ok((meta, parsed.content))
}

/// Manual YAML frontmatter extraction when gray_matter fails.
///
/// Extracts the raw YAML between `---` delimiters and parses key-value pairs
/// into a JSON object. Handles nested `metadata:` blocks and quoted values.
fn extract_frontmatter_manual(content: &str) -> Result<serde_json::Value> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        anyhow::bail!("No frontmatter delimiter found");
    }

    let after_first = &trimmed[3..].trim_start_matches('\r');
    let after_first = after_first.strip_prefix('\n').unwrap_or(after_first);

    let end = after_first
        .find("\n---")
        .context("No closing frontmatter delimiter")?;
    let yaml_str = &after_first[..end];

    // Parse YAML manually: top-level key: value pairs + one level of nesting
    let mut map = serde_json::Map::new();
    let mut current_nested_key: Option<String> = None;
    let mut nested_map = serde_json::Map::new();

    for line in yaml_str.lines() {
        if line.trim().is_empty() {
            continue;
        }

        let indent = line.len() - line.trim_start().len();

        if indent >= 2 {
            // Nested value under current_nested_key
            if let Some(ref _parent) = current_nested_key {
                if let Some((k, v)) = line.trim().split_once(':') {
                    let k = k.trim();
                    let v = v.trim();
                    let v = strip_yaml_quotes(v);
                    nested_map.insert(k.to_string(), serde_json::Value::String(v.to_string()));
                }
            }
        } else {
            // Flush previous nested block
            if let Some(parent) = current_nested_key.take() {
                if !nested_map.is_empty() {
                    map.insert(parent, serde_json::Value::Object(nested_map.clone()));
                    nested_map.clear();
                }
            }

            if let Some((k, v)) = line.split_once(':') {
                let k = k.trim();
                let v = v.trim();
                if v.is_empty() {
                    // Start of nested block (e.g., `metadata:`)
                    current_nested_key = Some(k.to_string());
                } else {
                    let v = strip_yaml_quotes(v);
                    map.insert(k.to_string(), serde_json::Value::String(v.to_string()));
                }
            }
        }
    }

    // Flush final nested block
    if let Some(parent) = current_nested_key {
        if !nested_map.is_empty() {
            map.insert(parent, serde_json::Value::Object(nested_map));
        }
    }

    Ok(serde_json::Value::Object(map))
}

/// Strip surrounding YAML quotes (single or double).
fn strip_yaml_quotes(s: &str) -> &str {
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

// ── Skill Activation Helpers ─────────────────────────────────────────────
//
// These functions support ClawHub/OpenClaw skill compatibility by enriching
// the skill body with runtime context when the LLM activates a skill.

/// Substitute ClawHub/OpenClaw/Claude-Code compatible variables in a skill body.
///
/// Supported variables:
/// - `$ARGUMENTS` — replaced with the user's query text (Claude Code pattern)
/// - `${SKILL_DIR}` / `${CLAUDE_SKILL_DIR}` — absolute path to the skill directory
/// - `$USER_NAME` — user's name
///
/// Backward-compatible: if no variables are present, body is returned unchanged.
pub fn substitute_skill_variables(
    body: &str,
    arguments: &str,
    skill_dir: &Path,
    user_name: Option<&str>,
) -> String {
    let skill_dir_str = skill_dir.to_string_lossy();
    let name = user_name.unwrap_or("User");

    body.replace("$ARGUMENTS", arguments)
        .replace("${SKILL_DIR}", &skill_dir_str)
        .replace("${CLAUDE_SKILL_DIR}", &skill_dir_str)
        .replace("$USER_NAME", name)
}

/// List reference files in a skill's `references/` directory.
pub fn list_skill_references(skill_dir: &Path) -> Vec<String> {
    let refs_dir = skill_dir.join("references");
    if !refs_dir.exists() {
        return Vec::new();
    }

    let mut refs = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&refs_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(name) = entry.file_name().to_str() {
                    refs.push(name.to_string());
                }
            }
        }
    }

    refs.sort();
    refs
}

/// Build the context header prepended to a skill body when activated.
///
/// Gives the LLM the information it needs to execute the skill:
/// - Skill directory path (to run scripts, read references)
/// - Available scripts and references with full paths
/// - Allowed-tools restriction (prompt-based enforcement, like OpenClaw contextModifier)
/// - The user's query/arguments
pub fn build_skill_activation_header(
    skill_name: &str,
    skill_dir: &Path,
    scripts: &[String],
    references: &[String],
    allowed_tools: Option<&str>,
    arguments: &str,
) -> String {
    let mut header = String::new();
    let dir = skill_dir.display();

    header.push_str(&format!("Skill: {skill_name}\n"));
    header.push_str(&format!("Skill directory: {dir}\n"));

    if !arguments.is_empty() {
        header.push_str(&format!("User request: {arguments}\n"));
    }

    if !scripts.is_empty() {
        header.push_str("\nAvailable scripts:\n");
        for script in scripts {
            header.push_str(&format!("  {dir}/scripts/{script}\n"));
        }
        header.push_str(&format!(
            "\nTo run a script: cd {dir} && python3 scripts/SCRIPT_NAME.py [args]\n\
             (use bash/node for .sh/.js scripts)\n"
        ));
    }

    if !references.is_empty() {
        header.push_str("\nAvailable references (read when needed):\n");
        for reference in references {
            header.push_str(&format!("  {dir}/references/{reference}\n"));
        }
    }

    if let Some(tools) = allowed_tools {
        header.push_str(&format!(
            "\nIMPORTANT: While executing this skill, you may ONLY use these tools: {tools}.\n\
             Do NOT use any other tools unless explicitly listed above.\n"
        ));
    }

    header
}

/// Extract full runtime requirements from skill metadata.
///
/// Supports OpenClaw/ClawHub format: `metadata.openclaw.requires.*`
/// (or `metadata.clawdbot.requires.*`).
/// Fields: `bins` (all required), `anyBins` (at least one), `env`, `os`.
pub fn extract_requirements(metadata: &Option<serde_json::Value>) -> SkillRequirements {
    let Some(meta) = metadata else {
        return SkillRequirements::default();
    };

    let requires = meta
        .get("openclaw")
        .or_else(|| meta.get("clawdbot"))
        .and_then(|oc| oc.get("requires"));

    let Some(requires) = requires else {
        return SkillRequirements::default();
    };

    let extract_strings = |key: &str| -> Vec<String> {
        requires
            .get(key)
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| item.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default()
    };

    SkillRequirements {
        bins: extract_strings("bins"),
        any_bins: extract_strings("anyBins"),
        env: extract_strings("env"),
        os: extract_strings("os"),
    }
}

/// Extract required binary dependencies from skill metadata.
///
/// Convenience wrapper over `extract_requirements()` for backward compatibility.
pub fn extract_required_bins(metadata: &Option<serde_json::Value>) -> Vec<String> {
    let reqs = extract_requirements(metadata);
    let mut bins = reqs.bins;
    bins.extend(reqs.any_bins);
    bins
}

/// Check if a skill's runtime requirements are satisfied.
///
/// Returns `Ok(())` if all requirements are met, `Err(reason)` if not.
/// Checks: bins (all required), any_bins (at least one), env vars, OS platform.
pub fn check_eligibility(reqs: &SkillRequirements) -> Result<(), String> {
    // Check required binaries (AND logic — all must be present)
    for bin in &reqs.bins {
        if std::process::Command::new("which")
            .arg(bin)
            .output()
            .map(|o| !o.status.success())
            .unwrap_or(true)
        {
            return Err(format!("Required binary '{}' not found", bin));
        }
    }

    // Check any_bins (OR logic — at least one must be present)
    if !reqs.any_bins.is_empty() {
        let any_found = reqs.any_bins.iter().any(|bin| {
            std::process::Command::new("which")
                .arg(bin)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        });
        if !any_found {
            return Err(format!(
                "At least one of these binaries is required: {}",
                reqs.any_bins.join(", ")
            ));
        }
    }

    // Check environment variables (all must be set)
    for var in &reqs.env {
        if std::env::var(var).is_err() {
            return Err(format!(
                "Required environment variable '{}' is not set",
                var
            ));
        }
    }

    // Check OS platform
    if !reqs.os.is_empty() {
        let current_os = std::env::consts::OS;
        // Also match "darwin" as alias for "macos"
        let matches = reqs.os.iter().any(|os| {
            let os_lower = os.to_lowercase();
            os_lower == current_os || (os_lower == "darwin" && current_os == "macos")
        });
        if !matches {
            return Err(format!(
                "Skill requires OS {:?}, current OS is '{}'",
                reqs.os, current_os
            ));
        }
    }

    Ok(())
}

/// Parse the `allowed-tools` string from a skill into a set of Homun tool names.
///
/// Supports OpenClaw-style names (mapped to Homun equivalents) and raw Homun tool names.
/// Patterns in parentheses like `Bash(curl:*)` are stripped — only the base name is matched.
///
/// # Examples
/// - `"Web Bash(curl:*)"` → `{"web_search", "web_fetch", "shell"}`
/// - `"shell read_file"` → `{"shell", "read_file"}`
pub fn parse_allowed_tools(spec: &str) -> std::collections::HashSet<String> {
    use std::collections::HashSet;

    let mut tools = HashSet::new();
    if spec.is_empty() {
        return tools;
    }

    for token in spec.split_whitespace() {
        // Strip parenthesized patterns: "Bash(curl:*)" → "Bash"
        let base = token.split('(').next().unwrap_or(token);

        // Map OpenClaw aliases to Homun tool names
        match base {
            "Web" | "web" => {
                tools.insert("web_search".to_string());
                tools.insert("web_fetch".to_string());
            }
            "Bash" | "bash" | "Shell" | "shell" => {
                tools.insert("shell".to_string());
            }
            "Read" | "read" => {
                tools.insert("read_file".to_string());
            }
            "Write" | "write" => {
                tools.insert("write_file".to_string());
            }
            "Edit" | "edit" => {
                tools.insert("edit_file".to_string());
            }
            "Browser" | "browser" => {
                tools.insert("browser".to_string());
            }
            // Raw Homun tool name — pass through as-is
            other => {
                tools.insert(other.to_string());
            }
        }
    }

    tools
}

/// Helper: resolve a vault:// reference to its secret value.
fn resolve_vault_value(
    skill_name: &str,
    key: &str,
    vault_key: &str,
    fallback: &str,
    secrets: Option<&crate::storage::EncryptedSecrets>,
) -> String {
    use crate::storage::SecretKey;
    secrets
        .and_then(|s| s.get(&SecretKey::custom(vault_key)).ok().flatten())
        .unwrap_or_else(|| {
            tracing::warn!(
                skill = %skill_name,
                key = %key,
                vault_key = %vault_key,
                "Failed to resolve vault:// reference for skill env"
            );
            fallback.to_string()
        })
}

/// Resolve environment variables for a skill from config.
///
/// Looks up `[skills.entries.<skill_name>]` in config and builds a HashMap
/// of env vars to inject into skill script execution:
/// - All entries from `env` table are included as-is
/// - If `api_key` is set, it's added as `API_KEY`
/// - Values with `vault://` prefix are resolved via the secrets vault
///
/// Returns an empty HashMap if no config exists for the skill.
pub fn resolve_skill_env(
    skill_name: &str,
    config: &crate::config::SkillsConfig,
    secrets: Option<&crate::storage::EncryptedSecrets>,
) -> HashMap<String, String> {
    let entry = match config.entries.get(skill_name) {
        Some(e) => e,
        None => return HashMap::new(),
    };

    let mut env = HashMap::new();

    // Add explicit env vars, resolving vault:// references
    for (key, value) in &entry.env {
        let resolved = if let Some(vault_key) = value.strip_prefix("vault://") {
            resolve_vault_value(skill_name, key, vault_key, value, secrets)
        } else {
            value.clone()
        };
        env.insert(key.clone(), resolved);
    }

    // Add api_key as API_KEY env var
    if let Some(ref api_key) = entry.api_key {
        let resolved = if let Some(vault_key) = api_key.strip_prefix("vault://") {
            resolve_vault_value(skill_name, "api_key", vault_key, api_key, secrets)
        } else {
            api_key.clone()
        };
        env.insert("API_KEY".to_string(), resolved);
    }

    env
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
  author: homun
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
                    user_invocable: true,
                    disable_model_invocation: false,
                },
                path: PathBuf::from("/tmp/test"),
                body: None,
                eligible: true,
                profile_slug: None,
            },
        );

        let summary = registry.build_prompt_summary();
        assert!(summary.contains("test (/test): A test skill"));
        assert!(summary.contains("Available Skills"));
        assert!(summary.contains("/skill-name"));
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

    #[test]
    fn test_parse_skill_md_inline_json_metadata() {
        // This is how ClawHub skills store metadata — inline JSON in YAML frontmatter
        let content = r#"---
name: gog
description: Google Workspace CLI for Gmail, Calendar, Drive, Contacts, Sheets, and Docs.
homepage: https://gogcli.sh
metadata: {"clawdbot":{"emoji":"🎮","requires":{"bins":["gog"]},"install":[{"id":"brew"}]}}
---

# gog
"#;
        let (meta, _body) = parse_skill_md(content).unwrap();
        assert_eq!(meta.name, "gog");

        // Check that metadata is parsed as an object, not a string
        let metadata = meta.metadata.expect("metadata should be Some");
        println!(
            "metadata: {}",
            serde_json::to_string_pretty(&metadata).unwrap()
        );
        println!("metadata type is_object: {}", metadata.is_object());
        println!("metadata type is_string: {}", metadata.is_string());

        let clawdbot = metadata.get("clawdbot").expect("clawdbot should exist");
        let requires = clawdbot.get("requires").expect("requires should exist");
        let bins = requires.get("bins").expect("bins should exist");
        assert!(bins.is_array());
        assert_eq!(bins.as_array().unwrap()[0], "gog");
    }

    #[test]
    fn test_parse_skill_md_ui_critic_long_desc() {
        // gray_matter fails with null Pod on very long single-line values;
        // our manual fallback handles it.
        let content = "---\nname: ui-critic\ndescription: Review UI/UX quality with strict premium criteria: alignment, spacing, hierarchy, usability, and visual consistency. Use when the user asks for UI review or shares screenshots.\nlicense: MIT\nallowed-tools: \"read_file list_dir Bash(rg:*) Bash(sed:*)\"\nmetadata:\n  author: homun\n  version: \"1.0\"\n  category: design\n---\n\n# UI Critic\n";
        let (meta, _body) = parse_skill_md(content).unwrap();
        assert_eq!(meta.name, "ui-critic");
        assert_eq!(meta.license.as_deref(), Some("MIT"));
        assert_eq!(
            meta.allowed_tools.as_deref(),
            Some("read_file list_dir Bash(rg:*) Bash(sed:*)")
        );
        let metadata = meta.metadata.expect("metadata should be parsed");
        assert_eq!(
            metadata.get("author").and_then(|v| v.as_str()),
            Some("homun")
        );
        assert_eq!(
            metadata.get("version").and_then(|v| v.as_str()),
            Some("1.0")
        );
        assert_eq!(
            metadata.get("category").and_then(|v| v.as_str()),
            Some("design")
        );
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

    // ── Activation helper tests ──────────────────────────────────────

    #[test]
    fn test_substitute_variables_all() {
        let body =
            "Run $ARGUMENTS in ${SKILL_DIR}/scripts/ for $USER_NAME. Also ${CLAUDE_SKILL_DIR}.";
        let result = substitute_skill_variables(
            body,
            "my query",
            Path::new("/home/skills/test"),
            Some("Fabio"),
        );
        assert_eq!(
            result,
            "Run my query in /home/skills/test/scripts/ for Fabio. Also /home/skills/test."
        );
    }

    #[test]
    fn test_substitute_variables_none() {
        let body = "No variables here, just plain instructions.";
        let result = substitute_skill_variables(body, "query", Path::new("/tmp/skill"), None);
        assert_eq!(result, body);
    }

    #[test]
    fn test_substitute_variables_partial() {
        let body = "Path is ${SKILL_DIR} but no arguments used.";
        let result = substitute_skill_variables(body, "ignored", Path::new("/opt/skills/x"), None);
        assert_eq!(result, "Path is /opt/skills/x but no arguments used.");
    }

    #[test]
    fn test_substitute_variables_default_user() {
        let body = "Hello $USER_NAME";
        let result = substitute_skill_variables(body, "", Path::new("/tmp"), None);
        assert_eq!(result, "Hello User");
    }

    #[test]
    fn test_build_activation_header_full() {
        let header = build_skill_activation_header(
            "code-review",
            Path::new("/home/skills/code-review"),
            &["analyze.py".into(), "lint.sh".into()],
            &["guidelines.md".into(), "patterns.md".into()],
            Some("Bash(git:*) read_file"),
            "review src/main.rs",
        );
        assert!(header.contains("Skill: code-review"));
        assert!(header.contains("Skill directory: /home/skills/code-review"));
        assert!(header.contains("User request: review src/main.rs"));
        assert!(header.contains("scripts/analyze.py"));
        assert!(header.contains("scripts/lint.sh"));
        assert!(header.contains("references/guidelines.md"));
        assert!(header.contains("references/patterns.md"));
        assert!(header.contains("ONLY use these tools: Bash(git:*) read_file"));
    }

    #[test]
    fn test_build_activation_header_minimal() {
        let header =
            build_skill_activation_header("simple", Path::new("/tmp/simple"), &[], &[], None, "");
        assert!(header.contains("Skill: simple"));
        assert!(header.contains("Skill directory: /tmp/simple"));
        assert!(!header.contains("scripts"));
        assert!(!header.contains("references"));
        assert!(!header.contains("ONLY use these tools"));
        assert!(!header.contains("User request"));
    }

    #[test]
    fn test_list_skill_references() {
        let dir = tempfile::TempDir::new().unwrap();
        // No references/ dir → empty
        assert!(list_skill_references(dir.path()).is_empty());

        // Create references/ with files
        let refs_dir = dir.path().join("references");
        std::fs::create_dir(&refs_dir).unwrap();
        std::fs::write(refs_dir.join("guide.md"), "content").unwrap();
        std::fs::write(refs_dir.join("api.md"), "content").unwrap();
        std::fs::write(refs_dir.join("data.json"), "{}").unwrap();

        let refs = list_skill_references(dir.path());
        assert_eq!(refs.len(), 3);
        assert_eq!(refs[0], "api.md"); // sorted
        assert_eq!(refs[1], "data.json");
        assert_eq!(refs[2], "guide.md");
    }

    #[test]
    fn test_extract_required_bins_openclaw() {
        let meta = Some(serde_json::json!({
            "openclaw": {
                "emoji": "🐙",
                "requires": {
                    "bins": ["gh", "git"]
                }
            }
        }));
        let bins = extract_required_bins(&meta);
        assert_eq!(bins, vec!["gh", "git"]);
    }

    #[test]
    fn test_extract_required_bins_clawdbot() {
        let meta = Some(serde_json::json!({
            "clawdbot": {
                "requires": { "bins": ["gog"] }
            }
        }));
        let bins = extract_required_bins(&meta);
        assert_eq!(bins, vec!["gog"]);
    }

    #[test]
    fn test_extract_required_bins_any_bins() {
        let meta = Some(serde_json::json!({
            "openclaw": {
                "requires": { "anyBins": ["claude", "codex", "pi"] }
            }
        }));
        let bins = extract_required_bins(&meta);
        assert_eq!(bins, vec!["claude", "codex", "pi"]);
    }

    #[test]
    fn test_extract_required_bins_none() {
        assert!(extract_required_bins(&None).is_empty());
        assert!(extract_required_bins(&Some(serde_json::json!({}))).is_empty());
        assert!(extract_required_bins(&Some(serde_json::json!({"openclaw": {}}))).is_empty());
    }

    // ── SKL-2: Eligibility gating tests ──────────────────────────────

    #[test]
    fn test_extract_requirements_full() {
        let meta = Some(serde_json::json!({
            "openclaw": {
                "requires": {
                    "bins": ["gh", "git"],
                    "anyBins": ["npm", "yarn"],
                    "env": ["GITHUB_TOKEN"],
                    "os": ["macos", "linux"]
                }
            }
        }));
        let reqs = extract_requirements(&meta);
        assert_eq!(reqs.bins, vec!["gh", "git"]);
        assert_eq!(reqs.any_bins, vec!["npm", "yarn"]);
        assert_eq!(reqs.env, vec!["GITHUB_TOKEN"]);
        assert_eq!(reqs.os, vec!["macos", "linux"]);
    }

    #[test]
    fn test_check_eligibility_missing_bin() {
        let reqs = SkillRequirements {
            bins: vec!["__nonexistent_binary_xyz_homun_test__".to_string()],
            ..Default::default()
        };
        let result = check_eligibility(&reqs);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_check_eligibility_missing_env() {
        let reqs = SkillRequirements {
            env: vec!["__HOMUN_TEST_NONEXISTENT_VAR__".to_string()],
            ..Default::default()
        };
        let result = check_eligibility(&reqs);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not set"));
    }

    #[test]
    fn test_check_eligibility_os_mismatch() {
        // Use an OS that definitely isn't the current one
        let wrong_os = if cfg!(target_os = "macos") {
            "windows"
        } else {
            "macos"
        };
        let reqs = SkillRequirements {
            os: vec![wrong_os.to_string()],
            ..Default::default()
        };
        let result = check_eligibility(&reqs);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("requires OS"));
    }

    #[test]
    fn test_check_eligibility_all_ok() {
        // Empty requirements → always eligible
        let reqs = SkillRequirements::default();
        assert!(check_eligibility(&reqs).is_ok());

        // Bins that should exist on any system
        let reqs_with_bins = SkillRequirements {
            bins: vec!["sh".to_string()],
            ..Default::default()
        };
        assert!(check_eligibility(&reqs_with_bins).is_ok());
    }

    #[test]
    fn test_list_eligible_filters_ineligible() {
        let mut registry = SkillRegistry::new();

        registry.skills.insert(
            "ok-skill".to_string(),
            Skill {
                meta: SkillMetadata {
                    name: "ok-skill".to_string(),
                    description: "An eligible skill".to_string(),
                    license: None,
                    compatibility: None,
                    metadata: None,
                    allowed_tools: None,
                    user_invocable: true,
                    disable_model_invocation: false,
                },
                path: PathBuf::from("/tmp/ok"),
                body: None,
                eligible: true,
                profile_slug: None,
            },
        );

        registry.skills.insert(
            "bad-skill".to_string(),
            Skill {
                meta: SkillMetadata {
                    name: "bad-skill".to_string(),
                    description: "An ineligible skill".to_string(),
                    license: None,
                    compatibility: None,
                    metadata: None,
                    allowed_tools: None,
                    user_invocable: true,
                    disable_model_invocation: false,
                },
                path: PathBuf::from("/tmp/bad"),
                body: None,
                eligible: false,
                profile_slug: None,
            },
        );

        // list() returns all
        assert_eq!(registry.list().len(), 2);

        // list_eligible() returns only eligible
        let eligible = registry.list_eligible();
        assert_eq!(eligible.len(), 1);
        assert_eq!(eligible[0].0, "ok-skill");

        // build_prompt_summary() uses eligible only
        let summary = registry.build_prompt_summary();
        assert!(summary.contains("ok-skill"));
        assert!(!summary.contains("bad-skill"));
    }

    // ── SKL-3: Invocation policy tests ───────────────────────────────

    #[test]
    fn test_invocation_policy_defaults() {
        let content = r#"---
name: default-policy
description: Test defaults
---
Body.
"#;
        let (meta, _) = parse_skill_md(content).unwrap();
        assert!(meta.user_invocable); // default true
        assert!(!meta.disable_model_invocation); // default false
    }

    #[test]
    fn test_invocation_policy_parsed() {
        let content = r#"---
name: hidden-skill
description: Not for model
user-invocable: false
disable-model-invocation: true
---
Body.
"#;
        let (meta, _) = parse_skill_md(content).unwrap();
        assert!(!meta.user_invocable);
        assert!(meta.disable_model_invocation);
    }

    #[test]
    fn test_list_for_model_excludes_disabled() {
        let mut registry = SkillRegistry::new();

        // Normal skill — visible to model
        registry.skills.insert(
            "normal".to_string(),
            Skill {
                meta: SkillMetadata {
                    name: "normal".to_string(),
                    description: "A normal skill".to_string(),
                    license: None,
                    compatibility: None,
                    metadata: None,
                    allowed_tools: None,
                    user_invocable: true,
                    disable_model_invocation: false,
                },
                path: PathBuf::from("/tmp/normal"),
                body: None,
                eligible: true,
                profile_slug: None,
            },
        );

        // Model-disabled skill — hidden from model, but slash-invocable
        registry.skills.insert(
            "user-only".to_string(),
            Skill {
                meta: SkillMetadata {
                    name: "user-only".to_string(),
                    description: "Only via slash command".to_string(),
                    license: None,
                    compatibility: None,
                    metadata: None,
                    allowed_tools: None,
                    user_invocable: true,
                    disable_model_invocation: true,
                },
                path: PathBuf::from("/tmp/user-only"),
                body: None,
                eligible: true,
                profile_slug: None,
            },
        );

        // list_for_model() excludes disable-model-invocation
        let for_model = registry.list_for_model();
        assert_eq!(for_model.len(), 1);
        assert_eq!(for_model[0].0, "normal");

        // list_eligible() includes both (both are eligible)
        assert_eq!(registry.list_eligible().len(), 2);

        // build_prompt_summary() uses list_for_model
        let summary = registry.build_prompt_summary();
        assert!(summary.contains("normal"));
        assert!(!summary.contains("user-only"));
    }

    // ── SKL-4: Tool policy tests ────────────────────────────────────

    #[test]
    fn test_parse_allowed_tools_web_bash() {
        let result = parse_allowed_tools("Web Bash(curl:*)");
        assert!(result.contains("web_search"));
        assert!(result.contains("web_fetch"));
        assert!(result.contains("shell"));
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_parse_allowed_tools_raw_names() {
        let result = parse_allowed_tools("shell read_file write_file");
        assert!(result.contains("shell"));
        assert!(result.contains("read_file"));
        assert!(result.contains("write_file"));
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_parse_allowed_tools_empty() {
        let result = parse_allowed_tools("");
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_allowed_tools_aliases() {
        let result = parse_allowed_tools("Read Write Edit Browser");
        assert!(result.contains("read_file"));
        assert!(result.contains("write_file"));
        assert!(result.contains("edit_file"));
        assert!(result.contains("browser"));
        assert_eq!(result.len(), 4);
    }

    #[test]
    fn test_parse_allowed_tools_case_insensitive() {
        let result = parse_allowed_tools("web bash read");
        assert!(result.contains("web_search"));
        assert!(result.contains("web_fetch"));
        assert!(result.contains("shell"));
        assert!(result.contains("read_file"));
    }

    // ── SKL-5: Skill env injection tests ────────────────────────────

    #[test]
    fn test_resolve_skill_env_plain() {
        use crate::config::{SkillEntryConfig, SkillsConfig};

        let mut entries = HashMap::new();
        entries.insert(
            "my-skill".to_string(),
            SkillEntryConfig {
                env: {
                    let mut env = HashMap::new();
                    env.insert("GITHUB_ORG".to_string(), "myorg".to_string());
                    env.insert("API_URL".to_string(), "https://api.example.com".to_string());
                    env
                },
                api_key: Some("plain-key-123".to_string()),
                enabled: None,
            },
        );
        let config = SkillsConfig { entries };

        let env = resolve_skill_env("my-skill", &config, None);
        assert_eq!(env.get("GITHUB_ORG").unwrap(), "myorg");
        assert_eq!(env.get("API_URL").unwrap(), "https://api.example.com");
        assert_eq!(env.get("API_KEY").unwrap(), "plain-key-123");
    }

    #[test]
    fn test_resolve_skill_env_empty() {
        use crate::config::SkillsConfig;

        let config = SkillsConfig::default();
        let env = resolve_skill_env("nonexistent-skill", &config, None);
        assert!(env.is_empty());
    }

    #[test]
    fn test_skills_config_deserialize() {
        let toml_str = r#"
[entries.my-skill]
env = { GITHUB_ORG = "myorg" }
api_key = "vault://my-key"
enabled = true

[entries.another-skill]
env = {}
"#;
        let config: crate::config::SkillsConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.entries.len(), 2);

        let my_skill = config.entries.get("my-skill").unwrap();
        assert_eq!(my_skill.env.get("GITHUB_ORG").unwrap(), "myorg");
        assert_eq!(my_skill.api_key.as_deref(), Some("vault://my-key"));
        assert_eq!(my_skill.enabled, Some(true));

        let another = config.entries.get("another-skill").unwrap();
        assert!(another.env.is_empty());
        assert!(another.api_key.is_none());
    }

    // ── SKL-7: E2E integration tests ───────────────────────────────

    #[test]
    fn test_backward_compatibility_no_new_fields() {
        // A SKILL.md with only name and description (pre-SKL-2 format)
        // should work with all defaults: eligible, user-invocable, no policy.
        let content = r#"---
name: legacy-skill
description: Old-style skill without new fields
---
Do something useful.
"#;
        let (meta, body) = parse_skill_md(content).unwrap();
        assert_eq!(meta.name, "legacy-skill");
        assert!(meta.user_invocable); // default true
        assert!(!meta.disable_model_invocation); // default false
        assert!(meta.allowed_tools.is_none());
        assert!(meta.metadata.is_none());
        assert_eq!(body.trim(), "Do something useful.");

        // Should pass eligibility with no requirements
        let reqs = extract_requirements(&meta.metadata);
        assert!(reqs.bins.is_empty());
        assert!(reqs.any_bins.is_empty());
        assert!(reqs.env.is_empty());
        assert!(reqs.os.is_empty());
        assert!(check_eligibility(&reqs).is_ok());
    }

    #[test]
    fn test_full_lifecycle_eligibility_and_invocation() {
        // Test the full flow: parse → extract requirements → eligibility → registry filtering
        let mut registry = SkillRegistry::new();

        // 1. Normal skill — visible everywhere
        registry.skills.insert(
            "normal".to_string(),
            Skill {
                meta: SkillMetadata {
                    name: "normal".to_string(),
                    description: "Normal skill".to_string(),
                    license: None,
                    compatibility: None,
                    metadata: None,
                    allowed_tools: None,
                    user_invocable: true,
                    disable_model_invocation: false,
                },
                path: PathBuf::from("/tmp/normal"),
                body: None,
                eligible: true,
                profile_slug: None,
            },
        );

        // 2. Ineligible skill — filtered from everything
        registry.skills.insert(
            "missing-deps".to_string(),
            Skill {
                meta: SkillMetadata {
                    name: "missing-deps".to_string(),
                    description: "Needs unavailable binary".to_string(),
                    license: None,
                    compatibility: None,
                    metadata: None,
                    allowed_tools: None,
                    user_invocable: true,
                    disable_model_invocation: false,
                },
                path: PathBuf::from("/tmp/missing"),
                body: None,
                eligible: false,
                profile_slug: None,
            },
        );

        // 3. Model-hidden skill — only via slash commands
        registry.skills.insert(
            "slash-only".to_string(),
            Skill {
                meta: SkillMetadata {
                    name: "slash-only".to_string(),
                    description: "Only callable via /slash-only".to_string(),
                    license: None,
                    compatibility: None,
                    metadata: None,
                    allowed_tools: Some("Web".to_string()),
                    user_invocable: true,
                    disable_model_invocation: true,
                },
                path: PathBuf::from("/tmp/slash"),
                body: None,
                eligible: true,
                profile_slug: None,
            },
        );

        // 4. Not user-invocable — only model can call it
        registry.skills.insert(
            "auto-only".to_string(),
            Skill {
                meta: SkillMetadata {
                    name: "auto-only".to_string(),
                    description: "Auto-triggered only".to_string(),
                    license: None,
                    compatibility: None,
                    metadata: None,
                    allowed_tools: None,
                    user_invocable: false,
                    disable_model_invocation: false,
                },
                path: PathBuf::from("/tmp/auto"),
                body: None,
                eligible: true,
                profile_slug: None,
            },
        );

        // Verify list()
        assert_eq!(registry.list().len(), 4);

        // Verify list_eligible() — excludes ineligible
        let eligible = registry.list_eligible();
        assert_eq!(eligible.len(), 3);
        let eligible_names: Vec<&str> = eligible.iter().map(|(n, _)| *n).collect();
        assert!(!eligible_names.contains(&"missing-deps"));

        // Verify list_for_model() — excludes ineligible AND model-disabled
        let for_model = registry.list_for_model();
        assert_eq!(for_model.len(), 2);
        let model_names: Vec<&str> = for_model.iter().map(|(n, _)| *n).collect();
        assert!(model_names.contains(&"normal"));
        assert!(model_names.contains(&"auto-only"));
        assert!(!model_names.contains(&"slash-only"));
        assert!(!model_names.contains(&"missing-deps"));

        // Verify prompt summary uses list_for_model
        let summary = registry.build_prompt_summary();
        assert!(summary.contains("normal"));
        assert!(summary.contains("auto-only"));
        assert!(!summary.contains("slash-only"));
        assert!(!summary.contains("missing-deps"));
    }

    #[test]
    fn test_tool_policy_parsing_complex() {
        // Complex allowed-tools string with aliases, raw names, and parenthesized patterns.
        // Note: parenthesized patterns like Bash(curl:*) are stripped at '(' so only base name matters.
        // Spaces inside parens would create additional tokens (e.g., "wget:*)") which are pass-through.
        let result = parse_allowed_tools("Web Bash(curl:*) read_file Edit Browser");
        assert!(result.contains("web_search"));
        assert!(result.contains("web_fetch"));
        assert!(result.contains("shell"));
        assert!(result.contains("read_file"));
        assert!(result.contains("edit_file"));
        assert!(result.contains("browser"));
        assert_eq!(result.len(), 6);
    }

    #[tokio::test]
    async fn test_scan_with_eligibility() {
        // Create a temp dir with a skill that has requirements for a nonexistent binary
        // Uses the OpenClaw format: metadata.openclaw.requires.bins
        let dir = tempfile::TempDir::new().unwrap();
        let skill_dir = dir.path().join("needs-docker");
        tokio::fs::create_dir(&skill_dir).await.unwrap();
        tokio::fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: needs-docker
description: Requires docker
metadata:
  openclaw:
    requires:
      bins:
        - docker_nonexistent_test_binary_xyz
---
Use docker to do things.
"#,
        )
        .await
        .unwrap();

        let mut registry = SkillRegistry::new();
        registry.scan_directory(dir.path()).await.unwrap();
        registry.check_all_eligibility();

        // Skill should be loaded but marked ineligible
        assert_eq!(registry.list().len(), 1);
        let skill = registry.get("needs-docker").unwrap();
        assert!(!skill.eligible);

        // Should not appear in eligible or model lists
        assert_eq!(registry.list_eligible().len(), 0);
        assert_eq!(registry.list_for_model().len(), 0);
    }
}
