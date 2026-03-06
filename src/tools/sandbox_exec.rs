use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::config::ExecutionSandboxConfig;

/// Minimal env allowlist used when command env sanitization is enabled.
pub const SAFE_ENV_KEYS: &[&str] = &[
    "PATH", "HOME", "USER", "SHELL", "LANG", "LC_ALL", "TERM", "TMPDIR",
];

const DOCKER_SAFE_ENV_KEYS: &[&str] = &[
    "DOCKER_HOST",
    "DOCKER_CONTEXT",
    "DOCKER_TLS_VERIFY",
    "DOCKER_CERT_PATH",
    "DOCKER_CONFIG",
];

const SANDBOX_EVENTS_LIMIT: usize = 100;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxEvent {
    pub timestamp: String,
    pub execution_kind: String,
    pub program: String,
    pub args_preview: Vec<String>,
    pub working_dir: String,
    pub requested_backend: String,
    pub resolved_backend: String,
    pub strict: bool,
    pub fallback_to_native: bool,
    pub docker_image: Option<String>,
    pub status: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxImageStatus {
    pub image: String,
    pub docker_available: bool,
    pub present: bool,
    pub image_id: Option<String>,
    pub created_at: Option<String>,
    pub size_bytes: Option<u64>,
    pub checked_at: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxImagePullResult {
    pub status: SandboxImageStatus,
    pub output: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedSandboxBackend {
    None,
    Docker,
}

impl ResolvedSandboxBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Docker => "docker",
        }
    }
}

fn docker_available() -> bool {
    static CACHED: OnceLock<bool> = OnceLock::new();
    *CACHED.get_or_init(|| {
        std::process::Command::new("docker")
            .arg("info")
            .arg("--format")
            .arg("{{.ServerVersion}}")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    })
}

pub fn docker_backend_available() -> bool {
    docker_available()
}

fn normalize_backend(raw: &str) -> String {
    raw.trim().to_ascii_lowercase()
}

fn sandbox_state_dir() -> PathBuf {
    if let Ok(path) = std::env::var("HOMUN_SANDBOX_STATE_DIR") {
        return PathBuf::from(path);
    }

    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".homun")
        .join("logs")
}

fn sandbox_events_path() -> PathBuf {
    sandbox_state_dir().join("sandbox-events.jsonl")
}

fn preview_args(args: &[String]) -> Vec<String> {
    args.iter().take(4).cloned().collect()
}

fn append_sandbox_event(event: &SandboxEvent) {
    let path = sandbox_events_path();
    if let Some(parent) = path.parent() {
        if let Err(err) = fs::create_dir_all(parent) {
            tracing::warn!(error = %err, path = %parent.display(), "Failed to create sandbox event log directory");
            return;
        }
    }

    let mut recent = list_recent_sandbox_events(SANDBOX_EVENTS_LIMIT.saturating_sub(1));
    recent.insert(0, event.clone());
    recent.truncate(SANDBOX_EVENTS_LIMIT);

    let file = match OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&path)
    {
        Ok(file) => file,
        Err(err) => {
            tracing::warn!(error = %err, path = %path.display(), "Failed to open sandbox event log");
            return;
        }
    };

    let mut writer = std::io::BufWriter::new(file);
    for item in recent.iter().rev() {
        let line = match serde_json::to_string(item) {
            Ok(line) => line,
            Err(err) => {
                tracing::warn!(error = %err, "Failed to serialize sandbox event");
                continue;
            }
        };
        if writeln!(writer, "{line}").is_err() {
            tracing::warn!(path = %path.display(), "Failed to write sandbox event log line");
            return;
        }
    }
}

pub fn list_recent_sandbox_events(limit: usize) -> Vec<SandboxEvent> {
    let limit = limit.clamp(1, SANDBOX_EVENTS_LIMIT);
    let path = sandbox_events_path();
    let file = match std::fs::File::open(&path) {
        Ok(file) => file,
        Err(_) => return Vec::new(),
    };

    let mut events = Vec::new();
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(event) = serde_json::from_str::<SandboxEvent>(trimmed) {
            events.push(event);
        }
    }
    events.into_iter().rev().take(limit).collect()
}

fn log_sandbox_event(
    execution_kind: &str,
    program: &str,
    args: &[String],
    working_dir: &Path,
    sandbox: &ExecutionSandboxConfig,
    resolved_backend: &str,
    status: &str,
    reason: String,
) {
    let requested_backend = normalize_backend(&sandbox.backend);
    let fallback_to_native = sandbox.enabled
        && requested_backend != "none"
        && resolved_backend == "none"
        && !sandbox.strict;
    append_sandbox_event(&SandboxEvent {
        timestamp: Utc::now().to_rfc3339(),
        execution_kind: execution_kind.to_string(),
        program: program.to_string(),
        args_preview: preview_args(args),
        working_dir: working_dir.display().to_string(),
        requested_backend,
        resolved_backend: resolved_backend.to_string(),
        strict: sandbox.strict,
        fallback_to_native,
        docker_image: if sandbox.enabled {
            Some(sandbox.docker_image.clone())
        } else {
            None
        },
        status: status.to_string(),
        reason,
    });
}

fn parse_docker_inspect_fields(output: &str) -> (Option<String>, Option<String>, Option<u64>) {
    let mut parts = output.lines();
    let image_id = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let created_at = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let size_bytes = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(|value| value.parse::<u64>().ok());
    (image_id, created_at, size_bytes)
}

pub fn get_docker_image_status(image: &str) -> SandboxImageStatus {
    let checked_at = Utc::now().to_rfc3339();
    let image = image.trim();
    if image.is_empty() {
        return SandboxImageStatus {
            image: "node:22-alpine".to_string(),
            docker_available: docker_available(),
            present: false,
            image_id: None,
            created_at: None,
            size_bytes: None,
            checked_at,
            message: "No runtime image configured; using default node:22-alpine.".to_string(),
        };
    }

    let docker_available = docker_available();
    if !docker_available {
        return SandboxImageStatus {
            image: image.to_string(),
            docker_available,
            present: false,
            image_id: None,
            created_at: None,
            size_bytes: None,
            checked_at,
            message: "Docker is unavailable, so the runtime image cannot be inspected.".to_string(),
        };
    }

    let output = std::process::Command::new("docker")
        .arg("image")
        .arg("inspect")
        .arg(image)
        .arg("--format")
        .arg("{{.Id}}\n{{.Created}}\n{{.Size}}")
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let (image_id, created_at, size_bytes) = parse_docker_inspect_fields(&stdout);
            SandboxImageStatus {
                image: image.to_string(),
                docker_available,
                present: true,
                image_id,
                created_at,
                size_bytes,
                checked_at,
                message: "Runtime image is available locally.".to_string(),
            }
        }
        Ok(_) => SandboxImageStatus {
            image: image.to_string(),
            docker_available,
            present: false,
            image_id: None,
            created_at: None,
            size_bytes: None,
            checked_at,
            message: "Runtime image is not present locally yet.".to_string(),
        },
        Err(err) => SandboxImageStatus {
            image: image.to_string(),
            docker_available,
            present: false,
            image_id: None,
            created_at: None,
            size_bytes: None,
            checked_at,
            message: format!("Failed to inspect runtime image: {err}"),
        },
    }
}

pub async fn pull_docker_image(image: &str) -> Result<SandboxImagePullResult> {
    let image = image.trim();
    if image.is_empty() {
        anyhow::bail!("Runtime image is empty");
    }
    if !docker_available() {
        anyhow::bail!("Docker is unavailable; cannot pull sandbox runtime image");
    }

    let output = Command::new("docker")
        .arg("pull")
        .arg(image)
        .output()
        .await
        .with_context(|| format!("Failed to pull sandbox runtime image '{image}'"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let combined = [stdout, stderr]
        .into_iter()
        .filter(|chunk| !chunk.is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    if !output.status.success() {
        anyhow::bail!(
            "Failed to pull sandbox runtime image '{}': {}",
            image,
            if combined.is_empty() {
                "docker pull returned a non-zero exit status".to_string()
            } else {
                combined
            }
        );
    }

    Ok(SandboxImagePullResult {
        status: get_docker_image_status(image),
        output: combined,
    })
}

pub fn resolve_sandbox_backend(config: &ExecutionSandboxConfig) -> Result<ResolvedSandboxBackend> {
    resolve_sandbox_backend_with_availability(config, docker_available())
}

fn resolve_sandbox_backend_with_availability(
    config: &ExecutionSandboxConfig,
    docker_is_available: bool,
) -> Result<ResolvedSandboxBackend> {
    if !config.enabled {
        return Ok(ResolvedSandboxBackend::None);
    }

    let backend = normalize_backend(&config.backend);
    match backend.as_str() {
        "none" => Ok(ResolvedSandboxBackend::None),
        "docker" => {
            if docker_is_available {
                Ok(ResolvedSandboxBackend::Docker)
            } else if config.strict {
                anyhow::bail!(
                    "Sandbox backend 'docker' requested but Docker is unavailable (strict mode)"
                );
            } else {
                tracing::warn!("Sandbox backend 'docker' unavailable; falling back to native");
                Ok(ResolvedSandboxBackend::None)
            }
        }
        "auto" => {
            if docker_is_available {
                Ok(ResolvedSandboxBackend::Docker)
            } else if config.strict {
                anyhow::bail!(
                    "Sandbox backend 'auto' could not find an available backend (strict mode)"
                );
            } else {
                tracing::warn!("Sandbox backend 'auto' found no backend; falling back to native");
                Ok(ResolvedSandboxBackend::None)
            }
        }
        other => {
            if config.strict {
                anyhow::bail!("Unsupported sandbox backend '{other}' (strict mode)");
            }
            tracing::warn!(
                backend = other,
                "Unsupported sandbox backend in config; falling back to native"
            );
            Ok(ResolvedSandboxBackend::None)
        }
    }
}

fn absolute_working_dir(working_dir: &Path) -> Result<PathBuf> {
    if working_dir.is_absolute() {
        return Ok(working_dir.to_path_buf());
    }
    let cwd = std::env::current_dir().context("Failed to resolve current directory")?;
    Ok(cwd.join(working_dir))
}

fn apply_sanitized_env(
    cmd: &mut Command,
    include_docker_keys: bool,
    extra_env: &HashMap<String, String>,
) {
    cmd.env_clear();

    for key in SAFE_ENV_KEYS {
        if let Ok(val) = std::env::var(key) {
            cmd.env(key, val);
        }
    }
    if include_docker_keys {
        for key in DOCKER_SAFE_ENV_KEYS {
            if let Ok(val) = std::env::var(key) {
                cmd.env(key, val);
            }
        }
    }
    if std::env::var("PATH").is_err() {
        cmd.env("PATH", "/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin");
    }

    for (k, v) in extra_env {
        cmd.env(k, v);
    }
}

/// Build a process command with optional env sanitization and optional sandbox wrapping.
pub fn build_process_command(
    execution_kind: &str,
    program: &str,
    args: &[String],
    working_dir: &Path,
    extra_env: &HashMap<String, String>,
    sanitize_env: bool,
    sandbox: &ExecutionSandboxConfig,
) -> Result<Command> {
    let backend = match resolve_sandbox_backend(sandbox) {
        Ok(backend) => backend,
        Err(err) => {
            log_sandbox_event(
                execution_kind,
                program,
                args,
                working_dir,
                sandbox,
                "none",
                "rejected",
                err.to_string(),
            );
            return Err(err);
        }
    };

    match backend {
        ResolvedSandboxBackend::None => {
            let mut cmd = Command::new(program);
            cmd.args(args).current_dir(working_dir);
            if sanitize_env {
                apply_sanitized_env(&mut cmd, false, extra_env);
            } else {
                for (k, v) in extra_env {
                    cmd.env(k, v);
                }
            }
            let reason = if sandbox.enabled && normalize_backend(&sandbox.backend) != "none" {
                "Sandbox backend unavailable or not requested; using native execution.".to_string()
            } else {
                "Native execution prepared.".to_string()
            };
            log_sandbox_event(
                execution_kind,
                program,
                args,
                working_dir,
                sandbox,
                backend.as_str(),
                "prepared",
                reason,
            );
            Ok(cmd)
        }
        ResolvedSandboxBackend::Docker => {
            let mut cmd = Command::new("docker");
            cmd.arg("run")
                .arg("--rm")
                .arg("--interactive")
                .arg("--network")
                .arg(&sandbox.docker_network);

            if sandbox.docker_memory_mb > 0 {
                cmd.arg("--memory")
                    .arg(format!("{}m", sandbox.docker_memory_mb));
            }
            if sandbox.docker_cpus > 0.0 {
                cmd.arg("--cpus").arg(format!("{}", sandbox.docker_cpus));
            }
            if sandbox.docker_read_only_rootfs {
                cmd.arg("--read-only");
            }

            if sandbox.docker_mount_workspace {
                let host_workspace = absolute_working_dir(working_dir)?;
                cmd.arg("--volume")
                    .arg(format!("{}:/workspace:rw", host_workspace.display()))
                    .arg("--workdir")
                    .arg("/workspace");
            }

            for (k, v) in extra_env {
                cmd.arg("-e").arg(format!("{k}={v}"));
            }

            cmd.arg(&sandbox.docker_image);
            cmd.arg(program);
            cmd.args(args);

            if sanitize_env {
                apply_sanitized_env(&mut cmd, true, &HashMap::new());
            }
            log_sandbox_event(
                execution_kind,
                program,
                args,
                working_dir,
                sandbox,
                backend.as_str(),
                "prepared",
                format!(
                    "Docker sandbox prepared with image '{}'.",
                    sandbox.docker_image
                ),
            );
            Ok(cmd)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn disabled_sandbox_resolves_to_none() {
        let cfg = ExecutionSandboxConfig {
            enabled: false,
            ..ExecutionSandboxConfig::default()
        };
        assert_eq!(
            resolve_sandbox_backend(&cfg).unwrap(),
            ResolvedSandboxBackend::None
        );
    }

    #[test]
    fn unknown_backend_non_strict_falls_back_to_none() {
        let cfg = ExecutionSandboxConfig {
            enabled: true,
            backend: "unknown".to_string(),
            strict: false,
            ..ExecutionSandboxConfig::default()
        };
        assert_eq!(
            resolve_sandbox_backend_with_availability(&cfg, false).unwrap(),
            ResolvedSandboxBackend::None
        );
    }

    #[test]
    fn strict_unknown_backend_fails() {
        let cfg = ExecutionSandboxConfig {
            enabled: true,
            backend: "unknown".to_string(),
            strict: true,
            ..ExecutionSandboxConfig::default()
        };
        assert!(resolve_sandbox_backend_with_availability(&cfg, false).is_err());
    }

    #[test]
    fn strict_auto_without_docker_fails() {
        let cfg = ExecutionSandboxConfig {
            enabled: true,
            backend: "auto".to_string(),
            strict: true,
            ..ExecutionSandboxConfig::default()
        };
        assert!(resolve_sandbox_backend_with_availability(&cfg, false).is_err());
    }

    #[test]
    fn auto_with_docker_resolves_to_docker() {
        let cfg = ExecutionSandboxConfig {
            enabled: true,
            backend: "auto".to_string(),
            strict: false,
            ..ExecutionSandboxConfig::default()
        };
        assert_eq!(
            resolve_sandbox_backend_with_availability(&cfg, true).unwrap(),
            ResolvedSandboxBackend::Docker
        );
    }

    #[test]
    fn docker_requested_without_docker_non_strict_falls_back() {
        let cfg = ExecutionSandboxConfig {
            enabled: true,
            backend: "docker".to_string(),
            strict: false,
            ..ExecutionSandboxConfig::default()
        };
        assert_eq!(
            resolve_sandbox_backend_with_availability(&cfg, false).unwrap(),
            ResolvedSandboxBackend::None
        );
    }

    #[test]
    fn docker_requested_without_docker_strict_fails() {
        let cfg = ExecutionSandboxConfig {
            enabled: true,
            backend: "docker".to_string(),
            strict: true,
            ..ExecutionSandboxConfig::default()
        };
        assert!(resolve_sandbox_backend_with_availability(&cfg, false).is_err());
    }

    #[test]
    fn parse_docker_inspect_fields_reads_expected_lines() {
        let (image_id, created_at, size_bytes) =
            parse_docker_inspect_fields("sha256:abc\n2026-03-06T08:00:00Z\n12345\n");
        assert_eq!(image_id.as_deref(), Some("sha256:abc"));
        assert_eq!(created_at.as_deref(), Some("2026-03-06T08:00:00Z"));
        assert_eq!(size_bytes, Some(12345));
    }

    #[test]
    fn sandbox_events_round_trip_from_custom_state_dir() {
        let temp = tempfile::tempdir().expect("tempdir");
        std::env::set_var("HOMUN_SANDBOX_STATE_DIR", temp.path());

        append_sandbox_event(&SandboxEvent {
            timestamp: "2026-03-06T10:00:00Z".to_string(),
            execution_kind: "shell".to_string(),
            program: "zsh".to_string(),
            args_preview: vec!["-lc".to_string(), "echo hi".to_string()],
            working_dir: "/tmp".to_string(),
            requested_backend: "auto".to_string(),
            resolved_backend: "docker".to_string(),
            strict: false,
            fallback_to_native: false,
            docker_image: Some("node:22-alpine".to_string()),
            status: "prepared".to_string(),
            reason: "Docker sandbox prepared.".to_string(),
        });
        append_sandbox_event(&SandboxEvent {
            timestamp: "2026-03-06T10:01:00Z".to_string(),
            execution_kind: "mcp".to_string(),
            program: "node".to_string(),
            args_preview: vec!["server.js".to_string()],
            working_dir: "/tmp/mcp".to_string(),
            requested_backend: "auto".to_string(),
            resolved_backend: "none".to_string(),
            strict: false,
            fallback_to_native: true,
            docker_image: Some("node:22-alpine".to_string()),
            status: "prepared".to_string(),
            reason: "Native execution prepared.".to_string(),
        });

        let events = list_recent_sandbox_events(10);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].execution_kind, "mcp");
        assert_eq!(events[1].execution_kind, "shell");

        let path = sandbox_events_path();
        assert!(path.exists());
        fs::remove_file(&path).ok();
        std::env::remove_var("HOMUN_SANDBOX_STATE_DIR");
    }
}
