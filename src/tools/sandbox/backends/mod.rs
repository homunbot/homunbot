mod docker;
mod linux_native;
mod native;
mod windows_native;

use anyhow::Result;
use tokio::process::Command;

use crate::config::ExecutionSandboxConfig;

use super::types::{ResolvedSandboxBackend, SandboxExecutionRequest};

pub(crate) use linux_native::{build_linux_native_command_spec, linux_native_reason_fragments};
pub(crate) use windows_native::windows_native_reason_fragments;
#[cfg(target_os = "windows")]
pub(crate) use windows_native::{enforce_job_limits, probe_job_objects, JobObjectGuard};

pub(crate) fn build_command_for_backend(
    request: &SandboxExecutionRequest<'_>,
    sandbox: &ExecutionSandboxConfig,
    backend: ResolvedSandboxBackend,
) -> Result<Command> {
    match backend {
        ResolvedSandboxBackend::None => Ok(native::build_native_command(request)),
        ResolvedSandboxBackend::Docker => docker::build_docker_command(request, sandbox),
        ResolvedSandboxBackend::LinuxNative => {
            linux_native::build_linux_native_command(request, sandbox)
        }
        ResolvedSandboxBackend::WindowsNative => {
            windows_native::build_windows_native_command(request, sandbox)
        }
    }
}
