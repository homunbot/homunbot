pub mod adapter;
pub mod clawhub;
pub mod creator;
pub mod executor;
pub mod installer;
pub mod loader;
pub mod mcp_registry;
pub mod openskills;
pub mod search;
pub mod security;
pub mod watcher;

pub use adapter::{
    adapt_legacy_skill_dir, parse_legacy_manifest, AdaptedSkill, LegacySkillManifest,
};
pub use clawhub::{ClawHubInstaller, ClawHubSearchResult};
pub use creator::{create_skill, SkillCreationRequest, SkillCreationResult};
pub use executor::{
    execute_skill_script, execute_skill_script_with_sandbox, list_skill_scripts, ScriptOutput,
};
pub use installer::{InstallResult, InstalledSkillInfo, SkillInstaller};
pub use loader::{
    build_skill_activation_header, check_eligibility, extract_required_bins,
    extract_requirements, list_skill_references, parse_allowed_tools, parse_skill_md_public,
    resolve_skill_env, substitute_skill_variables, Skill, SkillMetadata, SkillRegistry,
    SkillRequirements,
};
pub use mcp_registry::{
    all_mcp_presets, find_mcp_preset, suggest_mcp_presets, McpEnvVar, McpServerPreset,
};
pub use openskills::OpenSkillsSource;
pub use security::{
    scan_skill_content, scan_skill_package, InstallSecurityOptions, SecurityReport, SecurityWarning,
};
pub use watcher::{SkillWatcher, WatcherHandle as SkillWatcherHandle};
