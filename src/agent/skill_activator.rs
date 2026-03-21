//! Skill activation and slash command resolution.
//!
//! Handles loading SKILL.md bodies on demand, variable substitution,
//! script/reference discovery, and slash command parsing.

use std::path::PathBuf;

use tokio::sync::RwLock;

use crate::skills::loader::SkillRegistry;
use crate::tools::ToolRegistry;

/// Information returned when a skill is activated via tool call.
///
/// Contains the enriched skill body with variable substitution,
/// directory info, available scripts/references, and metadata.
pub(crate) struct ActivatedSkill {
    /// Skill body with variables substituted ($ARGUMENTS, ${SKILL_DIR})
    pub body: String,
    /// Absolute path to the skill directory
    pub skill_dir: PathBuf,
    /// Available scripts in the skill's scripts/ directory
    pub scripts: Vec<String>,
    /// Available reference files in references/
    pub references: Vec<String>,
    /// Allowed tools restriction from frontmatter (if set)
    pub allowed_tools: Option<String>,
    /// Required binary dependencies from metadata.openclaw.requires.bins
    pub required_bins: Vec<String>,
}

/// Check required binaries synchronously and return warning text.
pub(crate) fn check_required_bins_sync(bins: &[String]) -> String {
    if bins.is_empty() {
        return String::new();
    }

    let mut warnings = String::new();
    for bin in bins {
        let found = std::process::Command::new("which")
            .arg(bin)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !found {
            warnings.push_str(&format!(
                "⚠ Required binary '{bin}' not found. Install it before using this skill.\n"
            ));
        }
    }
    warnings
}

/// Try to activate a skill by name.
///
/// Loads the SKILL.md body, substitutes variables, lists scripts/references,
/// and returns the enriched context. Returns `None` if the name matches a
/// built-in tool or no skill is found.
pub(crate) async fn try_activate_skill(
    name: &str,
    arguments: &serde_json::Value,
    tool_registry: &RwLock<ToolRegistry>,
    skill_registry: Option<&RwLock<SkillRegistry>>,
) -> Option<ActivatedSkill> {
    // Skip skill lookup entirely for built-in tools (avoids costly disk rescan)
    if tool_registry.read().await.get(name).is_some() {
        return None;
    }

    let registry = skill_registry?;
    let mut guard = registry.write().await;

    // If skill not found, rescan from disk (may have been created at runtime)
    if guard.get(name).is_none() {
        tracing::debug!(skill = %name, "Skill not in registry, rescanning from disk");
        if let Err(e) = guard.scan_and_load().await {
            tracing::warn!(error = %e, "Failed to rescan skills");
        }
    }

    let skill = guard.get_mut(name)?;

    let body = match skill.load_body().await {
        Ok(body) => body.to_string(),
        Err(e) => {
            tracing::warn!(skill = %name, error = %e, "Failed to load skill body");
            return None;
        }
    };

    let skill_dir = skill.path.clone();
    let allowed_tools = skill.meta.allowed_tools.clone();
    let required_bins = crate::skills::extract_required_bins(&skill.meta.metadata);

    // List available scripts and references
    let scripts = crate::skills::list_skill_scripts(&skill_dir);
    let references = crate::skills::list_skill_references(&skill_dir);

    // Substitute variables for Claude Code / ClawHub compatibility
    let query = arguments
        .get("query")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let substituted_body =
        crate::skills::substitute_skill_variables(&body, query, &skill_dir, None);

    Some(ActivatedSkill {
        body: substituted_body,
        skill_dir,
        scripts,
        references,
        allowed_tools,
        required_bins,
    })
}

/// Try to resolve a `/skill-name args` slash command.
///
/// Returns `Some((enriched_body, allowed_tools))` if the message matches an installed skill,
/// `None` otherwise (message is not a slash command or skill not found).
/// The `allowed_tools` is the raw string from the skill's frontmatter for tool policy enforcement.
pub(crate) async fn try_resolve_slash_command(
    message: &str,
    skill_registry: Option<&RwLock<SkillRegistry>>,
) -> Option<(String, Option<String>)> {
    let trimmed = message.trim();
    if !trimmed.starts_with('/') {
        return None;
    }

    // Parse: /skill-name rest of message
    let without_slash = &trimmed[1..];
    let (skill_name, arguments) = match without_slash.split_once(char::is_whitespace) {
        Some((name, args)) => (name, args.trim()),
        None => (without_slash, ""),
    };

    let registry = skill_registry?;
    let mut guard = registry.write().await;

    // Check if this matches an installed skill
    guard.get(skill_name)?;

    let skill = guard.get_mut(skill_name)?;

    // Check invocation policy: user-invocable: false → block slash commands
    if !skill.meta.user_invocable {
        tracing::debug!(skill = %skill_name, "Skill not user-invocable, ignoring slash command");
        return None;
    }
    let body = match skill.load_body().await {
        Ok(b) => b.to_string(),
        Err(e) => {
            tracing::warn!(skill = %skill_name, error = %e, "Failed to load skill for slash command");
            return None;
        }
    };

    let skill_dir = skill.path.clone();
    let allowed_tools = skill.meta.allowed_tools.clone();
    let required_bins = crate::skills::extract_required_bins(&skill.meta.metadata);

    let scripts = crate::skills::list_skill_scripts(&skill_dir);
    let references = crate::skills::list_skill_references(&skill_dir);

    let substituted =
        crate::skills::substitute_skill_variables(&body, arguments, &skill_dir, None);

    let header = crate::skills::build_skill_activation_header(
        skill_name,
        &skill_dir,
        &scripts,
        &references,
        allowed_tools.as_deref(),
        arguments,
    );

    // Check required binaries and add warnings
    let bin_warnings = check_required_bins_sync(&required_bins);

    tracing::info!(
        skill = %skill_name,
        arguments = %arguments,
        "Slash command activated skill"
    );

    let enriched = format!(
        "[SKILL ACTIVATED: {skill_name}]\n\n\
         {header}{bin_warnings}\n\
         {substituted}\n\n\
         [END SKILL INSTRUCTIONS]"
    );
    Some((enriched, allowed_tools))
}
