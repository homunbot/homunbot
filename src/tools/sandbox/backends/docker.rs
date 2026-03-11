use std::collections::HashMap;

use anyhow::Result;
use tokio::process::Command;

use crate::config::ExecutionSandboxConfig;
use crate::tools::sandbox::env::apply_sanitized_env;
use crate::tools::sandbox::types::SandboxExecutionRequest;

fn absolute_working_dir(working_dir: &std::path::Path) -> Result<std::path::PathBuf> {
    if working_dir.is_absolute() {
        return Ok(working_dir.to_path_buf());
    }
    let cwd =
        std::env::current_dir().context("Failed to resolve current directory")?;
    Ok(cwd.join(working_dir))
}

use anyhow::Context;

pub(crate) fn build_docker_command(
    request: &SandboxExecutionRequest<'_>,
    sandbox: &ExecutionSandboxConfig,
) -> Result<Command> {
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
        let host_workspace = absolute_working_dir(request.working_dir)?;
        cmd.arg("--volume")
            .arg(format!("{}:/workspace:rw", host_workspace.display()))
            .arg("--workdir")
            .arg("/workspace");
    }

    for (k, v) in request.extra_env {
        cmd.arg("-e").arg(format!("{k}={v}"));
    }

    cmd.arg(&sandbox.docker_image);
    cmd.arg(request.program);
    cmd.args(request.args);

    if request.sanitize_env {
        apply_sanitized_env(&mut cmd, true, &HashMap::new());
    }

    Ok(cmd)
}
