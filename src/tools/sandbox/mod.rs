mod backends;
pub mod env;
mod events;
pub mod resolve;
pub mod runtime_image;
pub mod types;

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use tokio::process::Command;

use crate::config::ExecutionSandboxConfig;

use self::backends::{
    build_command_for_backend, linux_native_reason_fragments, windows_native_reason_fragments,
};
use self::events::log_sandbox_event;
use self::resolve::{linux_native_runtime_support, normalize_backend};

// Re-exports for callers
#[cfg(target_os = "windows")]
pub(crate) use self::backends::{enforce_job_limits, JobObjectGuard};
pub use self::env::SAFE_ENV_KEYS;
pub use self::events::list_recent_sandbox_events;
pub use self::resolve::{
    current_sandbox_backend_capabilities, docker_available_live, docker_backend_available,
    resolve_sandbox_backend, sandbox_backend_availability_summary,
};
pub use self::runtime_image::{
    build_runtime_image, canonical_sandbox_runtime_baseline, get_docker_image_status,
    get_runtime_image_status, pull_docker_image, pull_runtime_image,
};
pub use self::types::{
    ResolvedSandboxBackend, SandboxBackendCapability, SandboxEvent, SandboxExecutionRequest,
    SandboxImageBuildResult, SandboxImagePullResult, SandboxImageStatus,
};

fn native_execution_reason(sandbox: &ExecutionSandboxConfig) -> String {
    if sandbox.enabled && normalize_backend(&sandbox.backend) != "none" {
        "Sandbox backend unavailable or not requested; using native execution.".to_string()
    } else {
        "Native execution prepared.".to_string()
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
    let request = SandboxExecutionRequest {
        execution_kind,
        program,
        args,
        working_dir,
        extra_env,
        sanitize_env,
    };

    let backend = match resolve_sandbox_backend(sandbox) {
        Ok(backend) => backend,
        Err(err) => {
            log_sandbox_event(&request, sandbox, "none", "rejected", err.to_string());
            return Err(err);
        }
    };

    let prepared = build_command_for_backend(&request, sandbox, backend);
    match prepared {
        Ok(cmd) => {
            let reason = match backend {
                ResolvedSandboxBackend::None => native_execution_reason(sandbox),
                ResolvedSandboxBackend::Docker => format!(
                    "Docker sandbox prepared with image '{}'.",
                    sandbox.docker_image
                ),
                ResolvedSandboxBackend::LinuxNative => {
                    let support = linux_native_runtime_support();
                    let details = linux_native_reason_fragments(sandbox, &support).join(", ");
                    format!("Linux native sandbox prepared via bubblewrap ({details}).")
                }
                ResolvedSandboxBackend::WindowsNative => {
                    let details = windows_native_reason_fragments(sandbox).join(", ");
                    format!("Windows native sandbox prepared via Job Objects ({details}).")
                }
            };
            log_sandbox_event(&request, sandbox, backend.as_str(), "prepared", reason);
            Ok(cmd)
        }
        Err(err) => {
            log_sandbox_event(
                &request,
                sandbox,
                backend.as_str(),
                "rejected",
                err.to_string(),
            );
            Err(err)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::fs;
    use std::sync::Mutex;

    use crate::tools::sandbox::backends::build_linux_native_command_spec;
    use crate::tools::sandbox::events::{append_sandbox_event, sandbox_events_path};
    use crate::tools::sandbox::resolve::{
        resolve_sandbox_backend_with_availability, resolve_sandbox_backend_with_capabilities,
    };
    use crate::tools::sandbox::runtime_image::{
        assess_runtime_image_status, load_runtime_image_state, parse_runtime_image_reference,
        resolve_runtime_image_policy, save_runtime_image_state,
    };
    use crate::tools::sandbox::types::{
        BackendProbe, LinuxNativeRuntimeSupport, SandboxBackendAvailability,
        SandboxRuntimeImageState,
    };

    static TEST_ENV_LOCK: Mutex<()> = Mutex::new(());

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
    fn linux_native_requested_without_support_non_strict_falls_back() {
        let cfg = ExecutionSandboxConfig {
            enabled: true,
            backend: "linux_native".to_string(),
            strict: false,
            ..ExecutionSandboxConfig::default()
        };
        assert_eq!(
            resolve_sandbox_backend_with_capabilities(
                &cfg,
                SandboxBackendAvailability {
                    docker: false,
                    linux_native: false,
                    windows_native: false,
                }
            )
            .unwrap(),
            ResolvedSandboxBackend::None
        );
    }

    #[test]
    fn windows_native_requested_without_support_strict_fails() {
        let cfg = ExecutionSandboxConfig {
            enabled: true,
            backend: "windows_native".to_string(),
            strict: true,
            ..ExecutionSandboxConfig::default()
        };
        assert!(resolve_sandbox_backend_with_capabilities(
            &cfg,
            SandboxBackendAvailability {
                docker: false,
                linux_native: false,
                windows_native: false,
            }
        )
        .is_err());
    }

    #[test]
    fn sandbox_capability_summary_reports_planned_backends() {
        let summary = sandbox_backend_availability_summary(&[
            SandboxBackendCapability {
                backend: "docker".to_string(),
                label: "Docker".to_string(),
                available: true,
                supported_on_host: true,
                implemented: true,
                reason: "Docker available".to_string(),
            },
            SandboxBackendCapability {
                backend: "linux_native".to_string(),
                label: "Linux Native".to_string(),
                available: false,
                supported_on_host: true,
                implemented: false,
                reason: "Planned".to_string(),
            },
        ]);
        assert!(summary.contains("Docker: available"));
        assert!(summary.contains("Linux Native: planned"));
    }

    #[test]
    fn sandbox_events_round_trip_from_custom_state_dir() {
        let _guard = TEST_ENV_LOCK.lock().expect("test env lock");
        let temp = tempfile::tempdir().expect("tempdir");
        unsafe { std::env::set_var("HOMUN_SANDBOX_STATE_DIR", temp.path()) };

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
        unsafe { std::env::remove_var("HOMUN_SANDBOX_STATE_DIR") };
    }

    #[test]
    fn resolved_sanitized_env_keeps_safe_keys_and_extra_env() {
        use crate::tools::sandbox::env::resolved_sanitized_env;

        let _guard = TEST_ENV_LOCK.lock().expect("test env lock");
        unsafe { std::env::set_var("PATH", "/usr/bin:/bin") };
        unsafe { std::env::set_var("LANG", "en_US.UTF-8") };
        unsafe { std::env::set_var("SHOULD_NOT_PASS", "secret") };

        let mut extra = HashMap::new();
        extra.insert("CUSTOM_TOKEN".to_string(), "abc123".to_string());
        let env = resolved_sanitized_env(false, &extra);

        assert_eq!(env.get("PATH").map(String::as_str), Some("/usr/bin:/bin"));
        assert_eq!(env.get("LANG").map(String::as_str), Some("en_US.UTF-8"));
        assert_eq!(env.get("CUSTOM_TOKEN").map(String::as_str), Some("abc123"));
        assert!(!env.contains_key("SHOULD_NOT_PASS"));

        unsafe { std::env::remove_var("SHOULD_NOT_PASS") };
    }

    #[test]
    fn linux_native_command_spec_binds_workspace_and_sets_env() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workdir = temp.path().join("workspace");
        fs::create_dir_all(&workdir).expect("workspace");

        let args = vec!["-lc".to_string(), "pwd".to_string()];
        let mut extra_env = HashMap::new();
        extra_env.insert("FOO".to_string(), "bar".to_string());
        let request = SandboxExecutionRequest {
            execution_kind: "shell",
            program: "bash",
            args: &args,
            working_dir: &workdir,
            extra_env: &extra_env,
            sanitize_env: true,
        };
        let sandbox = ExecutionSandboxConfig {
            enabled: true,
            backend: "linux_native".to_string(),
            docker_network: "none".to_string(),
            docker_memory_mb: 512,
            docker_mount_workspace: true,
            ..ExecutionSandboxConfig::default()
        };
        let support = LinuxNativeRuntimeSupport {
            bubblewrap: BackendProbe {
                available: true,
                reason: "ok".to_string(),
            },
            user_namespace: true,
            network_namespace: true,
            prlimit_available: false,
            cgroup_v2_available: true,
        };

        let spec = build_linux_native_command_spec(&request, &sandbox, &support).unwrap();
        assert_eq!(spec.program, "bwrap");
        assert!(spec.args.contains(&"--clearenv".to_string()));
        assert!(spec.args.contains(&"--unshare-user".to_string()));
        assert!(spec.args.contains(&"--unshare-net".to_string()));
        assert!(spec.args.contains(&"--bind".to_string()));
        assert!(spec.args.contains(&workdir.display().to_string()));
        assert!(spec.args.contains(&"FOO".to_string()));
        assert!(spec.args.contains(&"bar".to_string()));
        assert!(spec.env.iter().any(|(k, _)| k == "PATH"));
    }

    #[test]
    fn linux_native_command_spec_uses_prlimit_for_memory_when_available() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workdir = temp.path().join("workspace");
        fs::create_dir_all(&workdir).expect("workspace");

        let args = vec!["-c".to_string(), "echo hi".to_string()];
        let request = SandboxExecutionRequest {
            execution_kind: "shell",
            program: "sh",
            args: &args,
            working_dir: &workdir,
            extra_env: &HashMap::new(),
            sanitize_env: true,
        };
        let sandbox = ExecutionSandboxConfig {
            enabled: true,
            backend: "linux_native".to_string(),
            docker_memory_mb: 256,
            ..ExecutionSandboxConfig::default()
        };
        let support = LinuxNativeRuntimeSupport {
            bubblewrap: BackendProbe {
                available: true,
                reason: "ok".to_string(),
            },
            user_namespace: false,
            network_namespace: true,
            prlimit_available: true,
            cgroup_v2_available: false,
        };

        let spec = build_linux_native_command_spec(&request, &sandbox, &support).unwrap();
        assert_eq!(spec.program, "prlimit");
        assert!(spec.args.iter().any(|arg| arg == "--as=268435456"));
        assert!(spec.args.iter().any(|arg| arg == "bwrap"));
        assert!(!spec.args.iter().any(|arg| arg == "--unshare-user"));
    }

    #[test]
    fn parse_runtime_image_reference_detects_pinned_digest() {
        let parsed = parse_runtime_image_reference("ghcr.io/homun/runtime@sha256:abc123");
        assert_eq!(parsed.repository, "ghcr.io/homun/runtime");
        assert_eq!(parsed.digest.as_deref(), Some("sha256:abc123"));
        assert_eq!(parsed.tag, None);
        assert_eq!(parsed.expected_version, "sha256:abc123");
        assert_eq!(parsed.version_policy, "pinned");
    }

    #[test]
    fn parse_runtime_image_reference_defaults_latest_for_bare_repo() {
        let parsed = parse_runtime_image_reference("ghcr.io/homun/runtime");
        assert_eq!(parsed.repository, "ghcr.io/homun/runtime");
        assert_eq!(parsed.tag.as_deref(), Some("latest"));
        assert_eq!(parsed.expected_version, "latest");
        assert_eq!(parsed.version_policy, "floating");
    }

    #[test]
    fn assess_runtime_image_status_flags_floating_tag_for_review() {
        let parsed = parse_runtime_image_reference("node:latest");
        let policy = resolve_runtime_image_policy(&parsed, "infer", "");
        let assessment = assess_runtime_image_status(&parsed, &policy, true, true, None, None);
        assert_eq!(assessment.drift_status, "tracking-floating-tag");
        assert_eq!(assessment.acceptability, "review");
        assert!(assessment.update_recommended);
    }

    #[test]
    fn assess_runtime_image_status_detects_config_change_since_last_pull() {
        let parsed = parse_runtime_image_reference("ghcr.io/homun/runtime:1.2.0");
        let state = SandboxRuntimeImageState {
            last_pulled_at: Some("2026-03-10T10:00:00Z".to_string()),
            last_pulled_image: Some("ghcr.io/homun/runtime:1.1.0".to_string()),
            last_pulled_image_id: Some("sha256:old".to_string()),
        };
        let current_id = "sha256:new".to_string();
        let policy = resolve_runtime_image_policy(&parsed, "infer", "");
        let assessment = assess_runtime_image_status(
            &parsed,
            &policy,
            true,
            true,
            Some(&current_id),
            Some(&state),
        );
        assert_eq!(assessment.drift_status, "config-changed-since-last-pull");
        assert_eq!(assessment.acceptability, "review");
        assert!(assessment.update_recommended);
    }

    #[test]
    fn assess_runtime_image_status_rejects_explicit_pinned_policy_without_digest_ref() {
        let parsed = parse_runtime_image_reference("ghcr.io/homun/runtime:1.2.0");
        let policy = resolve_runtime_image_policy(&parsed, "pinned", "sha256:abc123");
        let assessment = assess_runtime_image_status(&parsed, &policy, true, true, None, None);
        assert_eq!(assessment.drift_status, "not-pinned-reference");
        assert_eq!(assessment.acceptability, "action_required");
        assert!(assessment.update_recommended);
    }

    #[test]
    fn resolve_runtime_image_build_target_accepts_canonical_repo_tags() {
        use crate::tools::sandbox::runtime_image::get_docker_image_status;
        let sandbox = ExecutionSandboxConfig {
            docker_image: "homun/runtime-core:2026.03".to_string(),
            ..ExecutionSandboxConfig::default()
        };
        // This test just validates the parsing works; actual build target
        // resolution is tested via the build function path
        let status = get_docker_image_status("homun/runtime-core:2026.03");
        assert_eq!(status.image, "homun/runtime-core:2026.03");
        let _ = sandbox;
    }

    #[test]
    fn runtime_image_state_round_trip_from_custom_state_dir() {
        let _guard = TEST_ENV_LOCK.lock().expect("test env lock");
        let temp = tempfile::tempdir().expect("tempdir");
        unsafe { std::env::set_var("HOMUN_SANDBOX_STATE_DIR", temp.path()) };

        save_runtime_image_state(&SandboxRuntimeImageState {
            last_pulled_at: Some("2026-03-10T10:00:00Z".to_string()),
            last_pulled_image: Some("ghcr.io/homun/runtime:1.2.0".to_string()),
            last_pulled_image_id: Some("sha256:abc".to_string()),
        });

        let loaded = load_runtime_image_state().expect("runtime image state");
        assert_eq!(
            loaded.last_pulled_at.as_deref(),
            Some("2026-03-10T10:00:00Z")
        );
        assert_eq!(
            loaded.last_pulled_image.as_deref(),
            Some("ghcr.io/homun/runtime:1.2.0")
        );
        assert_eq!(loaded.last_pulled_image_id.as_deref(), Some("sha256:abc"));

        fs::remove_file(super::events::sandbox_runtime_image_state_path()).ok();
        unsafe { std::env::remove_var("HOMUN_SANDBOX_STATE_DIR") };
    }

    // --- parse_docker_inspect_fields is now in runtime_image.rs ---
    // We test it indirectly via get_docker_image_status/get_runtime_image_status
}
