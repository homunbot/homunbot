use std::path::{Path, PathBuf};

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

use super::registry::{get_string_param, Tool, ToolContext, ToolResult};

/// Maximum file size to read (chars)
const MAX_READ_SIZE: usize = 50_000;

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

    // Blocked files inside ~/.homun/ (allow ~/.homun/workspace/ subtree)
    let homun_dir = home.join(".homun");
    let workspace_dir = homun_dir.join("workspace");

    // If path is under ~/.homun/ but NOT under ~/.homun/workspace/, block it
    if resolved.starts_with(&homun_dir) && !resolved.starts_with(&workspace_dir) {
        return Err(format!(
            "Access denied: '{}' is in the protected Homun config directory",
            resolved.display()
        ));
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
                return Err(format!(
                    "Access denied: '{}' is a sensitive file",
                    filename
                ));
            }
        }
    }

    Ok(())
}

/// Resolve and validate a path, optionally restricting to an allowed directory.
fn resolve_path(path: &str, allowed_dir: Option<&Path>) -> Result<PathBuf, String> {
    let expanded = if let Some(stripped) = path.strip_prefix("~/") {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(stripped)
    } else {
        PathBuf::from(path)
    };

    let resolved = expanded
        .canonicalize()
        .or_else(|_| -> Result<PathBuf, std::io::Error> {
            // If file doesn't exist yet (write), resolve the parent
            if let Some(parent) = expanded.parent() {
                if parent.exists() {
                    Ok(parent.canonicalize().unwrap_or_else(|_| parent.to_path_buf()).join(
                        expanded.file_name().unwrap_or_default(),
                    ))
                } else {
                    Ok(expanded.clone())
                }
            } else {
                Ok(expanded.clone())
            }
        })
        .map_err(|e| format!("Invalid path '{path}': {e}"))?;

    if let Some(allowed) = allowed_dir {
        let allowed_resolved = allowed.canonicalize().unwrap_or_else(|_| allowed.to_path_buf());
        if !resolved.starts_with(&allowed_resolved) {
            return Err(format!(
                "Path '{}' is outside allowed directory '{}'",
                path,
                allowed.display()
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
}

impl ReadFileTool {
    pub fn new(allowed_dir: Option<PathBuf>) -> Self {
        Self { allowed_dir }
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

        // Sensitive path blocklist — unconditional
        if let Err(reason) = check_sensitive_path(&path) {
            return Ok(ToolResult::error(reason));
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
}

impl WriteFileTool {
    pub fn new(allowed_dir: Option<PathBuf>) -> Self {
        Self { allowed_dir }
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

        // Sensitive path blocklist — unconditional
        if let Err(reason) = check_sensitive_path(&path) {
            return Ok(ToolResult::error(reason));
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
}

impl EditFileTool {
    pub fn new(allowed_dir: Option<PathBuf>) -> Self {
        Self { allowed_dir }
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

        // Sensitive path blocklist — unconditional
        if let Err(reason) = check_sensitive_path(&path) {
            return Ok(ToolResult::error(reason));
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
}

impl ListDirTool {
    pub fn new(allowed_dir: Option<PathBuf>) -> Self {
        Self { allowed_dir }
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

        // Sensitive path blocklist — unconditional
        if let Err(reason) = check_sensitive_path(&path) {
            return Ok(ToolResult::error(reason));
        }

        if !path.exists() {
            return Ok(ToolResult::error(format!("Directory not found: {path_str}")));
        }

        if !path.is_dir() {
            return Ok(ToolResult::error(format!("Not a directory: {path_str}")));
        }

        let mut entries = Vec::new();

        let mut read_dir = match tokio::fs::read_dir(&path).await {
            Ok(rd) => rd,
            Err(e) => {
                return Ok(ToolResult::error(format!(
                    "Failed to read directory: {e}"
                )))
            }
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
        tokio::fs::write(&file_path, "hello world\nfoo bar").await.unwrap();

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

        tokio::fs::write(dir.path().join("a.txt"), "").await.unwrap();
        tokio::fs::write(dir.path().join("b.txt"), "").await.unwrap();
        tokio::fs::create_dir(dir.path().join("subdir")).await.unwrap();

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
        assert!(result.output.contains("outside allowed directory"));
    }
}
