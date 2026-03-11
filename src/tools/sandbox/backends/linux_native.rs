use std::collections::{BTreeMap, HashMap};

use anyhow::{Context, Result};
use tokio::process::Command;

use crate::config::ExecutionSandboxConfig;
use crate::tools::sandbox::env::{apply_env_map, resolved_request_env, resolved_sanitized_env};
use crate::tools::sandbox::resolve::linux_native_runtime_support;
use crate::tools::sandbox::types::{
    LinuxNativeRuntimeSupport, PreparedCommandSpec, SandboxExecutionRequest,
};

fn absolute_working_dir(working_dir: &std::path::Path) -> Result<std::path::PathBuf> {
    if working_dir.is_absolute() {
        return Ok(working_dir.to_path_buf());
    }
    let cwd = std::env::current_dir().context("Failed to resolve current directory")?;
    Ok(cwd.join(working_dir))
}

pub(crate) fn linux_native_reason_fragments(
    sandbox: &ExecutionSandboxConfig,
    support: &LinuxNativeRuntimeSupport,
) -> Vec<String> {
    let mut parts = Vec::new();
    parts.push(format!("network={}", sandbox.docker_network));
    parts.push(format!(
        "userns={}",
        if support.user_namespace {
            "isolated"
        } else {
            "host-user"
        }
    ));
    parts.push(format!(
        "memory={}",
        if sandbox.docker_memory_mb > 0 {
            if support.prlimit_available {
                format!("prlimit:{}MB", sandbox.docker_memory_mb)
            } else {
                format!("not-enforced:{}MB-requested", sandbox.docker_memory_mb)
            }
        } else {
            "unbounded".to_string()
        }
    ));
    if sandbox.docker_network == "none" && !support.network_namespace {
        parts.push("network-isolation=not-enforced".to_string());
    }
    if sandbox.docker_read_only_rootfs {
        parts.push("rootfs=read-only-bind".to_string());
    }
    if support.cgroup_v2_available {
        parts.push("cgroups-v2=present".to_string());
    }
    parts
}

pub(crate) fn build_linux_native_command_spec(
    request: &SandboxExecutionRequest<'_>,
    sandbox: &ExecutionSandboxConfig,
    support: &LinuxNativeRuntimeSupport,
) -> Result<PreparedCommandSpec> {
    if !support.bubblewrap.available {
        anyhow::bail!(
            "Sandbox backend 'linux_native' requested but {}",
            support.bubblewrap.reason
        );
    }

    let host_workspace = absolute_working_dir(request.working_dir)?;
    let sandbox_env = resolved_request_env(request, false);

    let mut bwrap_args = vec![
        "--die-with-parent".to_string(),
        "--new-session".to_string(),
        "--clearenv".to_string(),
        "--proc".to_string(),
        "/proc".to_string(),
        "--dev".to_string(),
        "/dev".to_string(),
        "--tmpfs".to_string(),
        "/tmp".to_string(),
        "--dir".to_string(),
        "/run".to_string(),
        "--dir".to_string(),
        "/tmp/homun-home".to_string(),
        "--setenv".to_string(),
        "HOME".to_string(),
        "/tmp/homun-home".to_string(),
        "--setenv".to_string(),
        "TMPDIR".to_string(),
        "/tmp".to_string(),
    ];

    for (key, value) in &sandbox_env {
        if key == "HOME" || key == "TMPDIR" {
            continue;
        }
        bwrap_args.push("--setenv".to_string());
        bwrap_args.push(key.clone());
        bwrap_args.push(value.clone());
    }

    bwrap_args.extend([
        "--ro-bind".to_string(),
        "/".to_string(),
        "/".to_string(),
        "--unshare-ipc".to_string(),
        "--unshare-pid".to_string(),
        "--unshare-uts".to_string(),
    ]);

    if support.user_namespace {
        bwrap_args.extend([
            "--unshare-user".to_string(),
            "--uid".to_string(),
            "65534".to_string(),
            "--gid".to_string(),
            "65534".to_string(),
        ]);
    }

    if sandbox.docker_network == "none" && support.network_namespace {
        bwrap_args.push("--unshare-net".to_string());
    }

    if sandbox.docker_mount_workspace {
        let workspace = host_workspace.display().to_string();
        bwrap_args.extend(["--bind".to_string(), workspace.clone(), workspace]);
    }

    bwrap_args.extend([
        "--chdir".to_string(),
        host_workspace.display().to_string(),
        request.program.to_string(),
    ]);
    bwrap_args.extend(request.args.iter().cloned());

    let mut spec = PreparedCommandSpec {
        program: "bwrap".to_string(),
        args: bwrap_args,
        env: resolved_sanitized_env(false, &HashMap::new())
            .into_iter()
            .collect(),
    };

    if support.prlimit_available && sandbox.docker_memory_mb > 0 {
        let memory_bytes = sandbox.docker_memory_mb.saturating_mul(1024 * 1024);
        let mut args = vec![
            format!("--as={memory_bytes}"),
            "--".to_string(),
            spec.program,
        ];
        args.extend(spec.args);
        spec = PreparedCommandSpec {
            program: "prlimit".to_string(),
            args,
            env: spec.env,
        };
    }

    Ok(spec)
}

fn command_from_spec(spec: PreparedCommandSpec) -> Command {
    let mut cmd = Command::new(&spec.program);
    cmd.args(&spec.args);
    let env = spec.env.into_iter().collect::<BTreeMap<String, String>>();
    apply_env_map(&mut cmd, true, &env);
    cmd
}

#[cfg(target_os = "linux")]
pub(crate) fn build_linux_native_command(
    request: &SandboxExecutionRequest<'_>,
    sandbox: &ExecutionSandboxConfig,
) -> Result<Command> {
    let support = linux_native_runtime_support();
    let spec = build_linux_native_command_spec(request, sandbox, &support)?;
    Ok(command_from_spec(spec))
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn build_linux_native_command(
    _request: &SandboxExecutionRequest<'_>,
    _sandbox: &ExecutionSandboxConfig,
) -> Result<Command> {
    anyhow::bail!("Sandbox backend 'linux_native' is only supported on Linux hosts")
}
