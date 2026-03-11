use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

use chrono::Utc;

use crate::config::ExecutionSandboxConfig;

use super::types::{SandboxEvent, SandboxExecutionRequest};

const SANDBOX_EVENTS_LIMIT: usize = 100;

fn sandbox_state_dir() -> PathBuf {
    if let Ok(path) = std::env::var("HOMUN_SANDBOX_STATE_DIR") {
        return PathBuf::from(path);
    }

    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".homun")
        .join("logs")
}

pub(crate) fn sandbox_events_path() -> PathBuf {
    sandbox_state_dir().join("sandbox-events.jsonl")
}

pub(crate) fn sandbox_runtime_image_state_path() -> PathBuf {
    sandbox_state_dir().join("sandbox-runtime-image.json")
}

pub(crate) fn append_sandbox_event(event: &SandboxEvent) {
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

fn normalize_backend(raw: &str) -> String {
    raw.trim().to_ascii_lowercase()
}

fn preview_args(args: &[String]) -> Vec<String> {
    args.iter().take(4).cloned().collect()
}

pub(crate) fn log_sandbox_event(
    request: &SandboxExecutionRequest<'_>,
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
        execution_kind: request.execution_kind.to_string(),
        program: request.program.to_string(),
        args_preview: preview_args(request.args),
        working_dir: request.working_dir.display().to_string(),
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
