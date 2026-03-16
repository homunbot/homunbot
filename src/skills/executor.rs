use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};

use crate::config::ExecutionSandboxConfig;
use crate::tools::sandbox::build_process_command;

/// Execute a skill script from the skill's `scripts/` directory.
///
/// Supports:
/// - Python scripts (.py) — executed with `python3`
/// - Bash scripts (.sh) — executed with `bash`
/// - JavaScript scripts (.js) — executed with `node`
///
/// Scripts inherit the skill's directory as working directory.
/// Output is captured (stdout + stderr) and returned.
pub async fn execute_skill_script(
    skill_dir: &Path,
    script_name: &str,
    args: &[&str],
    timeout_secs: u64,
) -> Result<ScriptOutput> {
    execute_skill_script_inner(
        skill_dir,
        script_name,
        args,
        timeout_secs,
        &ExecutionSandboxConfig::disabled(),
        false,
    )
    .await
}

/// Execute a skill script with explicit sandbox configuration.
///
/// This path uses the shared process runner and env sanitization.
pub async fn execute_skill_script_with_sandbox(
    skill_dir: &Path,
    script_name: &str,
    args: &[&str],
    timeout_secs: u64,
    sandbox: &ExecutionSandboxConfig,
) -> Result<ScriptOutput> {
    execute_skill_script_inner(skill_dir, script_name, args, timeout_secs, sandbox, true).await
}

async fn execute_skill_script_inner(
    skill_dir: &Path,
    script_name: &str,
    args: &[&str],
    timeout_secs: u64,
    sandbox: &ExecutionSandboxConfig,
    sanitize_env: bool,
) -> Result<ScriptOutput> {
    execute_skill_script_with_env(
        skill_dir,
        script_name,
        args,
        timeout_secs,
        sandbox,
        sanitize_env,
        &HashMap::new(),
    )
    .await
}

/// Execute a skill script with explicit env vars and sandbox configuration.
///
/// SKL-5: extra_env allows injecting skill-specific env vars resolved from config.
pub async fn execute_skill_script_with_env(
    skill_dir: &Path,
    script_name: &str,
    args: &[&str],
    timeout_secs: u64,
    sandbox: &ExecutionSandboxConfig,
    sanitize_env: bool,
    extra_env: &HashMap<String, String>,
) -> Result<ScriptOutput> {
    let scripts_dir = skill_dir.join("scripts");
    let script_path = scripts_dir.join(script_name);

    if !script_path.exists() {
        anyhow::bail!(
            "Script '{}' not found in {}",
            script_name,
            scripts_dir.display()
        );
    }

    // Determine interpreter from extension
    let interpreter = match script_path.extension().and_then(|e| e.to_str()) {
        Some("py") => "python3",
        Some("sh") => "bash",
        Some("js") => "node",
        Some("ts") => "npx",
        Some(ext) => anyhow::bail!("Unsupported script type: .{ext}"),
        None => "bash", // Default to bash for extensionless scripts
    };

    // For ts files, run via `npx tsx <script>`.
    let mut command_args: Vec<String> = Vec::new();
    if interpreter == "npx" {
        command_args.push("tsx".to_string());
    }
    command_args.push(script_path.display().to_string());
    command_args.extend(args.iter().map(|a| a.to_string()));

    let mut cmd = build_process_command(
        "skill",
        interpreter,
        &command_args,
        skill_dir,
        extra_env,
        sanitize_env,
        sandbox,
    )
    .with_context(|| format!("Failed to prepare command for script '{}'", script_name))?;

    // Capture output
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    // Spawn child, apply Windows Job Object limits if applicable, then wait
    let child = cmd
        .spawn()
        .with_context(|| format!("Failed to execute script '{}'", script_name))?;

    #[cfg(target_os = "windows")]
    let _job_guard = {
        use crate::tools::sandbox::{resolve_sandbox_backend, ResolvedSandboxBackend};
        match resolve_sandbox_backend(sandbox) {
            Ok(ResolvedSandboxBackend::WindowsNative) => child.id().and_then(|pid| {
                crate::tools::sandbox::enforce_job_limits(pid, sandbox)
                    .map_err(|e| tracing::warn!("Job Object enforcement failed: {e}"))
                    .ok()
            }),
            _ => None,
        }
    };

    let output = tokio::time::timeout(Duration::from_secs(timeout_secs), child.wait_with_output())
        .await
        .with_context(|| format!("Script '{}' timed out after {}s", script_name, timeout_secs))?
        .with_context(|| format!("Failed to execute script '{}'", script_name))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code().unwrap_or(-1);

    tracing::debug!(
        script = %script_name,
        exit_code,
        stdout_len = stdout.len(),
        stderr_len = stderr.len(),
        "Script execution completed"
    );

    Ok(ScriptOutput {
        stdout,
        stderr,
        exit_code,
        success: output.status.success(),
    })
}

/// List available scripts in a skill directory
pub fn list_skill_scripts(skill_dir: &Path) -> Vec<String> {
    let scripts_dir = skill_dir.join("scripts");
    if !scripts_dir.exists() {
        return Vec::new();
    }

    let mut scripts = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&scripts_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let is_script = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|ext| matches!(ext, "py" | "sh" | "js" | "ts"))
                .unwrap_or(false);

            if is_script {
                if let Some(name) = entry.file_name().to_str() {
                    scripts.push(name.to_string());
                }
            }
        }
    }

    scripts.sort();
    scripts
}

/// Output from a skill script execution
pub struct ScriptOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub success: bool,
}

impl ScriptOutput {
    /// Format as a single string for tool output
    pub fn to_output_string(&self) -> String {
        let mut output = String::new();

        if !self.stdout.is_empty() {
            output.push_str(&self.stdout);
        }

        if !self.stderr.is_empty() {
            if !output.is_empty() {
                output.push('\n');
            }
            output.push_str("[stderr] ");
            output.push_str(&self.stderr);
        }

        if !self.success {
            if !output.is_empty() {
                output.push('\n');
            }
            output.push_str(&format!("[exit code: {}]", self.exit_code));
        }

        if output.is_empty() {
            output = "(no output)".to_string();
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_skill_scripts_no_scripts_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        let scripts = list_skill_scripts(dir.path());
        assert!(scripts.is_empty());
    }

    #[tokio::test]
    async fn test_list_skill_scripts_with_scripts() {
        let dir = tempfile::TempDir::new().unwrap();
        let scripts_dir = dir.path().join("scripts");
        std::fs::create_dir(&scripts_dir).unwrap();

        std::fs::write(scripts_dir.join("fetch.py"), "print('hello')").unwrap();
        std::fs::write(scripts_dir.join("setup.sh"), "echo hello").unwrap();
        std::fs::write(scripts_dir.join("readme.txt"), "not a script").unwrap();

        let scripts = list_skill_scripts(dir.path());
        assert_eq!(scripts.len(), 2);
        assert!(scripts.contains(&"fetch.py".to_string()));
        assert!(scripts.contains(&"setup.sh".to_string()));
    }

    #[tokio::test]
    async fn test_execute_bash_script() {
        let dir = tempfile::TempDir::new().unwrap();
        let scripts_dir = dir.path().join("scripts");
        std::fs::create_dir(&scripts_dir).unwrap();
        std::fs::write(scripts_dir.join("test.sh"), "echo 'hello from skill'").unwrap();

        let result = execute_skill_script(dir.path(), "test.sh", &[], 10)
            .await
            .unwrap();
        assert!(result.success);
        assert!(result.stdout.contains("hello from skill"));
    }

    #[tokio::test]
    async fn test_execute_nonexistent_script() {
        let dir = tempfile::TempDir::new().unwrap();
        let result = execute_skill_script(dir.path(), "nope.sh", &[], 10).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_script_output_formatting() {
        let output = ScriptOutput {
            stdout: "result data".to_string(),
            stderr: String::new(),
            exit_code: 0,
            success: true,
        };
        assert_eq!(output.to_output_string(), "result data");

        let output_err = ScriptOutput {
            stdout: String::new(),
            stderr: "something went wrong".to_string(),
            exit_code: 1,
            success: false,
        };
        let formatted = output_err.to_output_string();
        assert!(formatted.contains("[stderr]"));
        assert!(formatted.contains("exit code: 1"));
    }
}
