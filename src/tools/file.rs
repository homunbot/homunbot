use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

use super::registry::{get_string_param, Tool, ToolContext, ToolResult};
use crate::config::{
    AclEntry, DefaultPermissions, PathPermissions, PermissionMode, PermissionValue,
    PermissionsConfig,
};

/// Maximum file size to read (chars)
const MAX_READ_SIZE: usize = 50_000;

// =============================================================================
// ACL-based Permission Checking
// =============================================================================

/// File operation types for permission checking
#[derive(Debug, Clone, Copy)]
pub enum FileOp {
    Read,
    Write,
    Delete,
}

impl FileOp {
    pub fn as_str(&self) -> &'static str {
        match self {
            FileOp::Read => "read",
            FileOp::Write => "write",
            FileOp::Delete => "delete",
        }
    }
}

/// Result of permission check
#[derive(Debug, Clone)]
pub enum PermissionResult {
    Allowed,
    Denied(String),
    NeedsConfirmation(String),
}

/// Check if a path matches a glob pattern (supports **, *, ?)
fn glob_matches(pattern: &str, path: &Path) -> bool {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));

    // Expand ~ in pattern
    let expanded_pattern = if let Some(stripped) = pattern.strip_prefix("~/") {
        home.join(stripped).to_string_lossy().to_string()
    } else {
        pattern.to_string()
    };

    // Convert path to string
    let path_str = path.to_string_lossy();

    // Convert glob pattern to regex-like matching
    // Simple implementation: support ** (any path), * (any non-separator), ? (single char)
    let pattern_parts: Vec<&str> = expanded_pattern.split(std::path::is_separator).collect();
    let path_parts: Vec<&str> = path_str.split(std::path::is_separator).collect();

    fn match_parts(pat_parts: &[&str], path_parts: &[&str]) -> bool {
        match (pat_parts.first(), path_parts.first()) {
            (None, None) => true,
            (None, Some(_)) => false,
            (Some(pat), None) => pat == &"**",
            (Some(pat), Some(p)) => {
                if *pat == "**" {
                    // ** matches zero or more path segments
                    match_parts(&pat_parts[1..], path_parts)
                        || match_parts(pat_parts, &path_parts[1..])
                } else if glob_segment_matches(pat, p) {
                    match_parts(&pat_parts[1..], &path_parts[1..])
                } else {
                    false
                }
            }
        }
    }

    fn glob_segment_matches(pattern: &str, segment: &str) -> bool {
        let pat_chars: Vec<char> = pattern.chars().collect();
        let seg_chars: Vec<char> = segment.chars().collect();

        fn match_chars(pat: &[char], seg: &[char]) -> bool {
            match (pat.first(), seg.first()) {
                (None, None) => true,
                (None, Some(_)) => false,
                (Some('*'), None) => match_chars(&pat[1..], seg),
                (Some('?'), None) => false,
                (Some('*'), Some(_)) => match_chars(&pat[1..], seg) || match_chars(pat, &seg[1..]),
                (Some('?'), Some(_)) => match_chars(&pat[1..], &seg[1..]),
                (Some(p), Some(s)) if *p == *s => match_chars(&pat[1..], &seg[1..]),
                _ => false,
            }
        }

        match_chars(&pat_chars, &seg_chars)
    }

    match_parts(&pattern_parts, &path_parts)
}

/// Get permission value for an operation from PathPermissions
fn get_permission_value(perms: &PathPermissions, op: FileOp) -> &PermissionValue {
    match op {
        FileOp::Read => &perms.read,
        FileOp::Write => &perms.write,
        FileOp::Delete => &perms.delete,
    }
}

/// Check path against ACL rules
fn check_acl_permission(
    resolved: &Path,
    operation: FileOp,
    permissions: &PermissionsConfig,
) -> PermissionResult {
    // Evaluate ACL entries in order (first match wins)
    for entry in &permissions.acl {
        if glob_matches(&entry.path, resolved) {
            let perm_value = get_permission_value(&entry.permissions, operation);

            // Check if it's a deny rule
            if entry.entry_type == "deny" {
                if !perm_value.is_allowed() {
                    return PermissionResult::Denied(format!(
                        "Access denied by ACL rule: {} not allowed on '{}'",
                        operation.as_str(),
                        resolved.display()
                    ));
                }
                // If deny rule allows, continue to next rule
                continue;
            }

            // Allow rule
            return match perm_value {
                PermissionValue::Bool(true) => PermissionResult::Allowed,
                PermissionValue::Bool(false) => PermissionResult::Denied(format!(
                    "Access denied by ACL: {} not allowed on '{}'",
                    operation.as_str(),
                    resolved.display()
                )),
                PermissionValue::Confirm => PermissionResult::NeedsConfirmation(format!(
                    "Confirmation required to {} '{}'",
                    operation.as_str(),
                    resolved.display()
                )),
            };
        }
    }

    // No ACL match - use default permissions
    let default_allowed = match operation {
        FileOp::Read => permissions.default.read,
        FileOp::Write => permissions.default.write,
        FileOp::Delete => permissions.default.delete,
    };

    if default_allowed {
        PermissionResult::Allowed
    } else {
        PermissionResult::Denied(format!(
            "Access denied by default policy: {} not allowed",
            operation.as_str()
        ))
    }
}

/// Check if a resolved path points to a sensitive location.
///
/// This is a hardcoded, unconditional blocklist that protects critical files
/// regardless of the `allowed_dir` setting. Even if `restrict_to_workspace`
/// is disabled, these paths can never be accessed by the agent.
fn check_sensitive_path(resolved: &Path) -> Result<(), String> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/nonexistent"));

    // Blocked directory prefixes — any path under these is denied
    let blocked_dirs: &[PathBuf] = &[
        home.join(".ssh"),
        home.join(".aws"),
        home.join(".gnupg"),
        home.join(".config/gcloud"),
    ];

    // Blocked files inside ~/.homun/ (allow workspace/, brain/, memory/ subtrees)
    let homun_dir = home.join(".homun");
    let allowed_subtrees: &[PathBuf] = &[
        homun_dir.join("workspace"),
        homun_dir.join("brain"),
        homun_dir.join("memory"),
    ];

    // If path is under ~/.homun/ but NOT under an allowed subtree, block it
    if resolved.starts_with(&homun_dir) {
        let in_allowed = allowed_subtrees.iter().any(|s| resolved.starts_with(s));
        if !in_allowed {
            return Err(format!(
                "Access denied: '{}' is in the protected Homun config directory",
                resolved.display()
            ));
        }
    }

    // Check blocked directory prefixes
    for blocked in blocked_dirs {
        if resolved.starts_with(blocked) {
            return Err(format!(
                "Access denied: '{}' is in a sensitive directory",
                resolved.display()
            ));
        }
    }

    // Check blocked filenames regardless of location
    if let Some(filename) = resolved.file_name().and_then(|f| f.to_str()) {
        let blocked_names = [".env", "secrets.enc"];
        for blocked in &blocked_names {
            if filename == *blocked {
                return Err(format!("Access denied: '{}' is a sensitive file", filename));
            }
        }
    }

    Ok(())
}

/// Check path permission with optional ACL-based permissions.
///
/// This combines:
/// 1. Hardcoded sensitive path checks (always enforced)
/// 2. Mode-based permission checking (open/workspace/acl)
pub fn check_path_permission(
    resolved: &Path,
    operation: FileOp,
    permissions: Option<&PermissionsConfig>,
    allowed_dir: Option<&Path>,
) -> PermissionResult {
    // Layer 1: Always check hardcoded sensitive paths
    if let Err(reason) = check_sensitive_path(resolved) {
        return PermissionResult::Denied(reason);
    }

    // If no permissions config, use legacy workspace mode
    let perms = match permissions {
        Some(p) => p,
        None => {
            // Legacy mode: use allowed_dir logic
            if let Some(allowed) = allowed_dir {
                let allowed_resolved = allowed
                    .canonicalize()
                    .unwrap_or_else(|_| allowed.to_path_buf());
                let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
                let always_allowed: Vec<PathBuf> = vec![
                    home.join(".homun").join("brain"),
                    home.join(".homun").join("memory"),
                ];

                let in_allowed = resolved.starts_with(&allowed_resolved)
                    || always_allowed.iter().any(|a| resolved.starts_with(a));

                if !in_allowed {
                    return PermissionResult::Denied(format!(
                        "Path '{}' is outside allowed directories",
                        resolved.display()
                    ));
                }
            }
            return PermissionResult::Allowed;
        }
    };

    // Layer 2: Mode-based permission checking
    match perms.mode {
        PermissionMode::Open => {
            // Open mode: only hardcoded checks apply (already done above)
            PermissionResult::Allowed
        }
        PermissionMode::Workspace => {
            // Workspace mode: use allowed_dir logic
            let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
            let workspace = crate::config::Config::workspace_dir();
            let always_allowed: Vec<PathBuf> = vec![
                home.join(".homun").join("brain"),
                home.join(".homun").join("memory"),
            ];

            let in_allowed = resolved.starts_with(&workspace)
                || always_allowed.iter().any(|a| resolved.starts_with(a));

            if in_allowed {
                PermissionResult::Allowed
            } else {
                PermissionResult::Denied(format!(
                    "Path '{}' is outside allowed directories (workspace mode)",
                    resolved.display()
                ))
            }
        }
        PermissionMode::Acl => {
            // ACL mode: full ACL evaluation
            check_acl_permission(resolved, operation, perms)
        }
    }
}

/// Resolve and validate a path, optionally restricting to allowed directories.
///
/// When `restrict_to_workspace` is enabled, the agent can access:
/// - `~/workspace/` (or configured workspace)
/// - `~/.homun/brain/` (agent memory files)
/// - `~/.homun/memory/` (daily memory files)
fn resolve_path(path: &str, allowed_dir: Option<&Path>) -> Result<PathBuf, String> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let expanded = if let Some(stripped) = path.strip_prefix("~/") {
        home.join(stripped)
    } else {
        PathBuf::from(path)
    };

    let resolved = expanded
        .canonicalize()
        .or_else(|_| -> Result<PathBuf, std::io::Error> {
            // If file doesn't exist yet (write), resolve the parent
            if let Some(parent) = expanded.parent() {
                if parent.exists() {
                    Ok(parent
                        .canonicalize()
                        .unwrap_or_else(|_| parent.to_path_buf())
                        .join(expanded.file_name().unwrap_or_default()))
                } else {
                    Ok(expanded.clone())
                }
            } else {
                Ok(expanded.clone())
            }
        })
        .map_err(|e| format!("Invalid path '{path}': {e}"))?;

    // If restrict_to_workspace is enabled, check allowed directories
    if let Some(allowed) = allowed_dir {
        let allowed_resolved = allowed
            .canonicalize()
            .unwrap_or_else(|_| allowed.to_path_buf());

        // Always allow brain/ and memory/ directories for agent memory access
        let homun_dir = home.join(".homun");
        let always_allowed: Vec<PathBuf> = vec![homun_dir.join("brain"), homun_dir.join("memory")];

        let in_allowed = resolved.starts_with(&allowed_resolved)
            || always_allowed.iter().any(|a| resolved.starts_with(a));

        if !in_allowed {
            return Err(format!(
                "Path '{}' is outside allowed directories. Allowed: workspace, ~/.homun/brain/, ~/.homun/memory/",
                path
            ));
        }
    }

    Ok(resolved)
}

// =============================================================================
// ReadFileTool
// =============================================================================

/// Read the contents of a file.
pub struct ReadFileTool {
    allowed_dir: Option<PathBuf>,
    permissions: Option<Arc<PermissionsConfig>>,
}

impl ReadFileTool {
    pub fn new(allowed_dir: Option<PathBuf>) -> Self {
        Self {
            allowed_dir,
            permissions: None,
        }
    }

    pub fn with_permissions(
        allowed_dir: Option<PathBuf>,
        permissions: Arc<PermissionsConfig>,
    ) -> Self {
        Self {
            allowed_dir,
            permissions: Some(permissions),
        }
    }
}

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read the contents of a file at the given path. Returns the file content as text."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to read"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<ToolResult> {
        let path_str = get_string_param(&args, "path")?;
        let path = match resolve_path(&path_str, self.allowed_dir.as_deref()) {
            Ok(p) => p,
            Err(e) => return Ok(ToolResult::error(e)),
        };

        // Check permissions (combines sensitive path + ACL)
        match check_path_permission(
            &path,
            FileOp::Read,
            self.permissions.as_deref(),
            self.allowed_dir.as_deref(),
        ) {
            PermissionResult::Denied(reason) => return Ok(ToolResult::error(reason)),
            PermissionResult::NeedsConfirmation(reason) => {
                return Ok(ToolResult::error(format!(
                    "{} (confirmation required)",
                    reason
                )))
            }
            PermissionResult::Allowed => {}
        }

        if !path.exists() {
            return Ok(ToolResult::error(format!("File not found: {path_str}")));
        }

        if !path.is_file() {
            return Ok(ToolResult::error(format!("Not a file: {path_str}")));
        }

        match tokio::fs::read_to_string(&path).await {
            Ok(content) => {
                if content.len() > MAX_READ_SIZE {
                    let truncated = &content[..MAX_READ_SIZE];
                    Ok(ToolResult::success(format!(
                        "{truncated}\n\n... [truncated at {MAX_READ_SIZE} chars, total: {} chars]",
                        content.len()
                    )))
                } else {
                    Ok(ToolResult::success(content))
                }
            }
            Err(e) => Ok(ToolResult::error(format!("Failed to read file: {e}"))),
        }
    }
}

// =============================================================================
// WriteFileTool
// =============================================================================

/// Write content to a file, creating it and parent directories if needed.
pub struct WriteFileTool {
    allowed_dir: Option<PathBuf>,
    permissions: Option<Arc<PermissionsConfig>>,
}

impl WriteFileTool {
    pub fn new(allowed_dir: Option<PathBuf>) -> Self {
        Self {
            allowed_dir,
            permissions: None,
        }
    }

    pub fn with_permissions(
        allowed_dir: Option<PathBuf>,
        permissions: Arc<PermissionsConfig>,
    ) -> Self {
        Self {
            allowed_dir,
            permissions: Some(permissions),
        }
    }
}

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Write content to a file at the given path. Creates the file and parent directories if they don't exist. Overwrites existing content."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to write"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write to the file"
                }
            },
            "required": ["path", "content"]
        })
    }

    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<ToolResult> {
        let path_str = get_string_param(&args, "path")?;
        let content = get_string_param(&args, "content")?;
        let path = match resolve_path(&path_str, self.allowed_dir.as_deref()) {
            Ok(p) => p,
            Err(e) => return Ok(ToolResult::error(e)),
        };

        // Check permissions (combines sensitive path + ACL)
        match check_path_permission(
            &path,
            FileOp::Write,
            self.permissions.as_deref(),
            self.allowed_dir.as_deref(),
        ) {
            PermissionResult::Denied(reason) => return Ok(ToolResult::error(reason)),
            PermissionResult::NeedsConfirmation(reason) => {
                return Ok(ToolResult::error(format!(
                    "{} (confirmation required)",
                    reason
                )))
            }
            PermissionResult::Allowed => {}
        }

        // Create parent directories
        if let Some(parent) = path.parent() {
            if let Err(e) = tokio::fs::create_dir_all(parent).await {
                return Ok(ToolResult::error(format!(
                    "Failed to create directories: {e}"
                )));
            }
        }

        match tokio::fs::write(&path, &content).await {
            Ok(()) => Ok(ToolResult::success(format!(
                "Wrote {} bytes to {}",
                content.len(),
                path_str
            ))),
            Err(e) => Ok(ToolResult::error(format!("Failed to write file: {e}"))),
        }
    }
}

// =============================================================================
// EditFileTool
// =============================================================================

/// Edit a file by replacing an exact text match.
pub struct EditFileTool {
    allowed_dir: Option<PathBuf>,
    permissions: Option<Arc<PermissionsConfig>>,
}

impl EditFileTool {
    pub fn new(allowed_dir: Option<PathBuf>) -> Self {
        Self {
            allowed_dir,
            permissions: None,
        }
    }

    pub fn with_permissions(
        allowed_dir: Option<PathBuf>,
        permissions: Arc<PermissionsConfig>,
    ) -> Self {
        Self {
            allowed_dir,
            permissions: Some(permissions),
        }
    }
}

#[async_trait]
impl Tool for EditFileTool {
    fn name(&self) -> &str {
        "edit_file"
    }

    fn description(&self) -> &str {
        "Edit a file by replacing an exact text match with new text. \
         The old_text must match exactly (including whitespace). \
         Only the first occurrence is replaced."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to edit"
                },
                "old_text": {
                    "type": "string",
                    "description": "Exact text to find and replace"
                },
                "new_text": {
                    "type": "string",
                    "description": "Text to replace old_text with"
                }
            },
            "required": ["path", "old_text", "new_text"]
        })
    }

    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<ToolResult> {
        let path_str = get_string_param(&args, "path")?;
        let old_text = get_string_param(&args, "old_text")?;
        let new_text = get_string_param(&args, "new_text")?;
        let path = match resolve_path(&path_str, self.allowed_dir.as_deref()) {
            Ok(p) => p,
            Err(e) => return Ok(ToolResult::error(e)),
        };

        // Check permissions (edit requires write)
        match check_path_permission(
            &path,
            FileOp::Write,
            self.permissions.as_deref(),
            self.allowed_dir.as_deref(),
        ) {
            PermissionResult::Denied(reason) => return Ok(ToolResult::error(reason)),
            PermissionResult::NeedsConfirmation(reason) => {
                return Ok(ToolResult::error(format!(
                    "{} (confirmation required)",
                    reason
                )))
            }
            PermissionResult::Allowed => {}
        }

        if !path.exists() {
            return Ok(ToolResult::error(format!("File not found: {path_str}")));
        }

        let content = match tokio::fs::read_to_string(&path).await {
            Ok(c) => c,
            Err(e) => return Ok(ToolResult::error(format!("Failed to read file: {e}"))),
        };

        // Count occurrences
        let count = content.matches(&old_text).count();

        if count == 0 {
            return Ok(ToolResult::error(
                "old_text not found in file. Make sure it matches exactly (including whitespace)."
                    .to_string(),
            ));
        }

        // Replace first occurrence only (safety)
        let new_content = content.replacen(&old_text, &new_text, 1);

        match tokio::fs::write(&path, &new_content).await {
            Ok(()) => {
                let mut msg = format!("Replaced text in {path_str}");
                if count > 1 {
                    msg.push_str(&format!(
                        " (warning: {count} occurrences found, only first replaced)"
                    ));
                }
                Ok(ToolResult::success(msg))
            }
            Err(e) => Ok(ToolResult::error(format!("Failed to write file: {e}"))),
        }
    }
}

// =============================================================================
// ListDirTool
// =============================================================================

/// List the contents of a directory.
pub struct ListDirTool {
    allowed_dir: Option<PathBuf>,
    permissions: Option<Arc<PermissionsConfig>>,
}

impl ListDirTool {
    pub fn new(allowed_dir: Option<PathBuf>) -> Self {
        Self {
            allowed_dir,
            permissions: None,
        }
    }

    pub fn with_permissions(
        allowed_dir: Option<PathBuf>,
        permissions: Arc<PermissionsConfig>,
    ) -> Self {
        Self {
            allowed_dir,
            permissions: Some(permissions),
        }
    }
}

#[async_trait]
impl Tool for ListDirTool {
    fn name(&self) -> &str {
        "list_dir"
    }

    fn description(&self) -> &str {
        "List the contents of a directory. Shows files and subdirectories with their types."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the directory to list"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<ToolResult> {
        let path_str = get_string_param(&args, "path")?;
        let path = match resolve_path(&path_str, self.allowed_dir.as_deref()) {
            Ok(p) => p,
            Err(e) => return Ok(ToolResult::error(e)),
        };

        // Check permissions (list requires read)
        match check_path_permission(
            &path,
            FileOp::Read,
            self.permissions.as_deref(),
            self.allowed_dir.as_deref(),
        ) {
            PermissionResult::Denied(reason) => return Ok(ToolResult::error(reason)),
            PermissionResult::NeedsConfirmation(reason) => {
                return Ok(ToolResult::error(format!(
                    "{} (confirmation required)",
                    reason
                )))
            }
            PermissionResult::Allowed => {}
        }

        if !path.exists() {
            return Ok(ToolResult::error(format!(
                "Directory not found: {path_str}"
            )));
        }

        if !path.is_dir() {
            return Ok(ToolResult::error(format!("Not a directory: {path_str}")));
        }

        let mut entries = Vec::new();

        let mut read_dir = match tokio::fs::read_dir(&path).await {
            Ok(rd) => rd,
            Err(e) => return Ok(ToolResult::error(format!("Failed to read directory: {e}"))),
        };

        while let Ok(Some(entry)) = read_dir.next_entry().await {
            let name = entry.file_name().to_string_lossy().to_string();
            let file_type = entry.file_type().await.ok();
            let prefix = match file_type {
                Some(ft) if ft.is_dir() => "[dir]  ",
                Some(ft) if ft.is_symlink() => "[link] ",
                _ => "[file] ",
            };
            entries.push(format!("{prefix}{name}"));
        }

        entries.sort();

        if entries.is_empty() {
            Ok(ToolResult::success("(empty directory)"))
        } else {
            Ok(ToolResult::success(entries.join("\n")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_ctx() -> ToolContext {
        ToolContext {
            workspace: "/tmp".to_string(),
            channel: "cli".to_string(),
            chat_id: "test".to_string(),
            message_tx: None,
            approval_manager: None,
            skill_env: None,
        }
    }

    #[tokio::test]
    async fn test_write_and_read() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.txt");
        let path_str = file_path.to_str().unwrap();

        let write_tool = WriteFileTool::new(None);
        let args = serde_json::json!({"path": path_str, "content": "hello world"});
        let result = write_tool.execute(args, &test_ctx()).await.unwrap();
        assert!(!result.is_error, "write error: {}", result.output);

        let read_tool = ReadFileTool::new(None);
        let args = serde_json::json!({"path": path_str});
        let result = read_tool.execute(args, &test_ctx()).await.unwrap();
        assert!(!result.is_error);
        assert_eq!(result.output, "hello world");
    }

    #[tokio::test]
    async fn test_read_not_found() {
        let read_tool = ReadFileTool::new(None);
        let args = serde_json::json!({"path": "/tmp/nonexistent_homun_test_file.txt"});
        let result = read_tool.execute(args, &test_ctx()).await.unwrap();
        assert!(result.is_error);
        assert!(result.output.contains("not found") || result.output.contains("Invalid path"));
    }

    #[tokio::test]
    async fn test_edit_file() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("edit_test.txt");
        let path_str = file_path.to_str().unwrap();

        // Write initial content
        tokio::fs::write(&file_path, "hello world\nfoo bar")
            .await
            .unwrap();

        let edit_tool = EditFileTool::new(None);
        let args = serde_json::json!({
            "path": path_str,
            "old_text": "foo bar",
            "new_text": "baz qux"
        });
        let result = edit_tool.execute(args, &test_ctx()).await.unwrap();
        assert!(!result.is_error, "edit error: {}", result.output);

        let content = tokio::fs::read_to_string(&file_path).await.unwrap();
        assert_eq!(content, "hello world\nbaz qux");
    }

    #[tokio::test]
    async fn test_edit_not_found() {
        let edit_tool = EditFileTool::new(None);
        let args = serde_json::json!({
            "path": "/tmp/test_edit.txt",
            "old_text": "this text does not exist anywhere",
            "new_text": "replacement"
        });
        let result = edit_tool.execute(args, &test_ctx()).await.unwrap();
        assert!(result.is_error);
    }

    #[tokio::test]
    async fn test_list_dir() {
        let dir = TempDir::new().unwrap();
        let dir_str = dir.path().to_str().unwrap();

        tokio::fs::write(dir.path().join("a.txt"), "")
            .await
            .unwrap();
        tokio::fs::write(dir.path().join("b.txt"), "")
            .await
            .unwrap();
        tokio::fs::create_dir(dir.path().join("subdir"))
            .await
            .unwrap();

        let list_tool = ListDirTool::new(None);
        let args = serde_json::json!({"path": dir_str});
        let result = list_tool.execute(args, &test_ctx()).await.unwrap();
        assert!(!result.is_error);
        assert!(result.output.contains("a.txt"));
        assert!(result.output.contains("b.txt"));
        assert!(result.output.contains("[dir]"));
        assert!(result.output.contains("subdir"));
    }

    #[tokio::test]
    async fn test_allowed_dir_restriction() {
        let dir = TempDir::new().unwrap();
        let read_tool = ReadFileTool::new(Some(dir.path().to_path_buf()));

        let args = serde_json::json!({"path": "/etc/passwd"});
        let result = read_tool.execute(args, &test_ctx()).await.unwrap();
        assert!(result.is_error);
        assert!(result.output.contains("outside allowed directories"));
    }

    #[tokio::test]
    async fn test_homun_dir_protection() {
        let home = dirs::home_dir().expect("need home dir for test");
        let homun_dir = home.join(".homun");

        // Files directly in ~/.homun/ should be blocked
        for filename in &["config.toml", "homun.db", "MEMORY.md"] {
            let path = homun_dir.join(filename);
            let result = check_sensitive_path(&path);
            assert!(
                result.is_err(),
                "{filename} should be blocked but was allowed"
            );
        }

        // Files under ~/.homun/workspace/ should be allowed
        let ws_file = homun_dir.join("workspace").join("notes.txt");
        let result = check_sensitive_path(&ws_file);
        assert!(result.is_ok(), "workspace file should be allowed");

        // Files under ~/.homun/brain/ should be allowed (agent memory)
        for filename in &["USER.md", "INSTRUCTIONS.md", "SOUL.md"] {
            let path = homun_dir.join("brain").join(filename);
            let result = check_sensitive_path(&path);
            assert!(
                result.is_ok(),
                "brain/{filename} should be allowed but got: {result:?}"
            );
        }

        // Files under ~/.homun/memory/ should be allowed (daily memory)
        let mem_file = homun_dir.join("memory").join("2026-02-21.md");
        let result = check_sensitive_path(&mem_file);
        assert!(
            result.is_ok(),
            "memory/2026-02-21.md should be allowed but got: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_brain_and_memory_always_allowed() {
        // When restrict_to_workspace is true, brain/ and memory/ should still be accessible
        let home = dirs::home_dir().expect("need home dir for test");
        let workspace = TempDir::new().unwrap();

        // Simulate restrict_to_workspace mode
        let read_tool = ReadFileTool::new(Some(workspace.path().to_path_buf()));

        // brain/ should be allowed even with restrict_to_workspace
        let brain_path = home.join(".homun").join("brain").join("USER.md");
        let result = resolve_path(brain_path.to_str().unwrap(), Some(workspace.path()));
        assert!(result.is_ok(), "brain/ should be allowed: {:?}", result);

        // memory/ should be allowed
        let memory_path = home.join(".homun").join("memory").join("2026-02-21.md");
        let result = resolve_path(memory_path.to_str().unwrap(), Some(workspace.path()));
        assert!(result.is_ok(), "memory/ should be allowed: {:?}", result);
    }
}
