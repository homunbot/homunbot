use std::collections::HashMap;

use anyhow::{Context, Result};
use tokio::process::Command;

use crate::config::ExecutionSandboxConfig;
use crate::tools::sandbox::env::{apply_env_map, resolved_request_env, resolved_sanitized_env};
use crate::tools::sandbox::resolve::macos_seatbelt_runtime_support;
use crate::tools::sandbox::types::{PreparedCommandSpec, SandboxExecutionRequest};

const SEATBELT_PROFILE: &str = include_str!("seatbelt_profile.sbpl");
const SEATBELT_PROFILE_NET_LOCAL: &str = include_str!("seatbelt_profile_net_local.sbpl");

fn absolute_working_dir(working_dir: &std::path::Path) -> Result<std::path::PathBuf> {
    if working_dir.is_absolute() {
        return Ok(working_dir.to_path_buf());
    }
    let cwd = std::env::current_dir().context("Failed to resolve current directory")?;
    Ok(cwd.join(working_dir))
}

pub(crate) fn macos_seatbelt_reason_fragments(sandbox: &ExecutionSandboxConfig) -> Vec<String> {
    let mut parts = Vec::new();

    let network_allowed = sandbox.docker_network != "none";
    parts.push(format!(
        "network={}",
        if network_allowed {
            "localhost-only"
        } else {
            "blocked"
        }
    ));
    parts.push("fs=read-only-system+workspace-rw".to_string());
    parts.push("memory=not-enforced".to_string());
    parts.push("cpu=not-enforced".to_string());

    parts
}

pub(crate) fn build_macos_seatbelt_command_spec(
    request: &SandboxExecutionRequest<'_>,
    sandbox: &ExecutionSandboxConfig,
) -> Result<PreparedCommandSpec> {
    let support = macos_seatbelt_runtime_support();
    if !support.sandbox_exec.available {
        anyhow::bail!(
            "Sandbox backend 'macos_seatbelt' requested but {}",
            support.sandbox_exec.reason
        );
    }

    let host_workspace = absolute_working_dir(request.working_dir)?;
    let sandbox_env = resolved_request_env(request, false);

    // Select profile: allow localhost network if docker_network != "none"
    let profile = if sandbox.docker_network == "none" {
        SEATBELT_PROFILE
    } else {
        SEATBELT_PROFILE_NET_LOCAL
    };

    let mut args = vec![
        "-p".to_string(),
        profile.to_string(),
        "-D".to_string(),
        format!("WORKSPACE={}", host_workspace.display()),
        "--".to_string(),
        request.program.to_string(),
    ];
    args.extend(request.args.iter().cloned());

    // Build env: sandbox_exec inherits the parent env, so we must clear and set explicitly
    let mut spec_env: Vec<(String, String)> = sandbox_env.into_iter().collect();

    // Ensure PATH is present
    if !spec_env.iter().any(|(k, _)| k == "PATH") {
        spec_env.push((
            "PATH".to_string(),
            "/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin".to_string(),
        ));
    }

    Ok(PreparedCommandSpec {
        program: "/usr/bin/sandbox-exec".to_string(),
        args,
        env: spec_env,
    })
}

fn command_from_spec(spec: PreparedCommandSpec) -> Command {
    let mut cmd = Command::new(&spec.program);
    cmd.args(&spec.args);
    let env = spec
        .env
        .into_iter()
        .collect::<std::collections::BTreeMap<String, String>>();
    apply_env_map(&mut cmd, true, &env);
    cmd
}

#[cfg(target_os = "macos")]
pub(crate) fn build_macos_seatbelt_command(
    request: &SandboxExecutionRequest<'_>,
    sandbox: &ExecutionSandboxConfig,
) -> Result<Command> {
    let spec = build_macos_seatbelt_command_spec(request, sandbox)?;
    Ok(command_from_spec(spec))
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn build_macos_seatbelt_command(
    _request: &SandboxExecutionRequest<'_>,
    _sandbox: &ExecutionSandboxConfig,
) -> Result<Command> {
    anyhow::bail!("Sandbox backend 'macos_seatbelt' is only supported on macOS hosts")
}
