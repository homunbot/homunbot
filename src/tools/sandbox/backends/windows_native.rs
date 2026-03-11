use anyhow::Result;
use tokio::process::Command;

use crate::config::ExecutionSandboxConfig;
use crate::tools::sandbox::env::apply_sanitized_env;
use crate::tools::sandbox::types::SandboxExecutionRequest;

/// Reason fragments describing what the Windows native backend enforces.
pub(crate) fn windows_native_reason_fragments(sandbox: &ExecutionSandboxConfig) -> Vec<String> {
    let mut parts = Vec::new();
    if sandbox.docker_memory_mb > 0 {
        parts.push(format!("memory=job-object:{}MB", sandbox.docker_memory_mb));
    } else {
        parts.push("memory=unbounded".to_string());
    }
    if sandbox.docker_cpus > 0.0 {
        parts.push(format!("cpu=rate-control:{:.1}", sandbox.docker_cpus));
    } else {
        parts.push("cpu=unbounded".to_string());
    }
    parts.push("kill-on-close=enabled".to_string());
    parts.push("network=not-enforced".to_string());
    parts.push("filesystem=not-enforced".to_string());
    parts
}

#[cfg(target_os = "windows")]
pub(crate) fn build_windows_native_command(
    request: &SandboxExecutionRequest<'_>,
    _sandbox: &ExecutionSandboxConfig,
) -> Result<Command> {
    let mut cmd = Command::new(request.program);
    cmd.args(request.args).current_dir(request.working_dir);
    if request.sanitize_env {
        apply_sanitized_env(&mut cmd, false, request.extra_env);
    } else {
        for (k, v) in request.extra_env {
            cmd.env(k, v);
        }
    }
    Ok(cmd)
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn build_windows_native_command(
    _request: &SandboxExecutionRequest<'_>,
    _sandbox: &ExecutionSandboxConfig,
) -> Result<Command> {
    anyhow::bail!("Sandbox backend 'windows_native' is only supported on Windows hosts")
}

// --- Windows Job Object implementation ---

#[cfg(target_os = "windows")]
mod job_objects {
    use std::mem;

    use anyhow::{bail, Result};

    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectCpuRateControlInformation,
        JobObjectExtendedLimitInformation, SetInformationJobObject,
        JOBOBJECT_CPU_RATE_CONTROL_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_CPU_RATE_CONTROL_ENABLE, JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOB_OBJECT_LIMIT_PROCESS_MEMORY,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE,
    };

    use crate::config::ExecutionSandboxConfig;

    /// RAII guard that closes the Job Object handle on drop.
    ///
    /// With `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` set, dropping this handle
    /// terminates all processes still assigned to the job. Callers must hold
    /// this guard alive until the child process exits.
    pub struct JobObjectGuard(isize);

    // Safety: HANDLE is an opaque kernel handle, safe to send across threads.
    unsafe impl Send for JobObjectGuard {}

    impl Drop for JobObjectGuard {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    /// Test whether Job Objects can be created on this host.
    pub fn probe_job_objects() -> (bool, String) {
        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle == 0 {
            let err = std::io::Error::last_os_error();
            return (false, format!("Job Object creation failed: {err}"));
        }
        unsafe {
            CloseHandle(handle);
        }
        (true, "Job Object creation succeeded.".to_string())
    }

    /// Create a Job Object with resource limits and assign the child process to it.
    ///
    /// Enforces:
    /// - memory limit via `JOB_OBJECT_LIMIT_PROCESS_MEMORY`
    /// - CPU rate cap via `JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP`
    /// - process containment via `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`
    pub fn enforce_job_limits(pid: u32, config: &ExecutionSandboxConfig) -> Result<JobObjectGuard> {
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if job == 0 {
            bail!(
                "Failed to create Job Object: {}",
                std::io::Error::last_os_error()
            );
        }

        // --- Extended limits: memory + kill-on-close ---
        let mut ext_info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { mem::zeroed() };
        ext_info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

        if config.docker_memory_mb > 0 {
            ext_info.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_PROCESS_MEMORY;
            ext_info.ProcessMemoryLimit =
                (config.docker_memory_mb as usize).saturating_mul(1024 * 1024);
        }

        if unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &ext_info as *const _ as *const _,
                mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        } == 0
        {
            let err = std::io::Error::last_os_error();
            unsafe { CloseHandle(job) };
            bail!("Failed to set Job Object limits: {err}");
        }

        // --- CPU rate control ---
        if config.docker_cpus > 0.0 {
            let num_cpus = std::thread::available_parallelism()
                .map(|n| n.get() as f32)
                .unwrap_or(1.0);
            // CpuRate: percentage × 100 in range 1–10000
            let rate = ((config.docker_cpus / num_cpus) * 10000.0).clamp(1.0, 10000.0) as u32;

            let mut cpu_info: JOBOBJECT_CPU_RATE_CONTROL_INFORMATION = unsafe { mem::zeroed() };
            cpu_info.ControlFlags =
                JOB_OBJECT_CPU_RATE_CONTROL_ENABLE | JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP;
            cpu_info.Anonymous.CpuRate = rate;

            if unsafe {
                SetInformationJobObject(
                    job,
                    JobObjectCpuRateControlInformation,
                    &cpu_info as *const _ as *const _,
                    mem::size_of::<JOBOBJECT_CPU_RATE_CONTROL_INFORMATION>() as u32,
                )
            } == 0
            {
                // CPU rate failure is non-fatal — memory/kill limits still apply
                tracing::warn!(
                    "Failed to set CPU rate control: {}",
                    std::io::Error::last_os_error()
                );
            }
        }

        // --- Assign child process to Job Object ---
        let process = unsafe { OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid) };
        if process == 0 {
            let err = std::io::Error::last_os_error();
            unsafe { CloseHandle(job) };
            bail!("Failed to open process {pid}: {err}");
        }

        let assigned = unsafe { AssignProcessToJobObject(job, process) };
        unsafe { CloseHandle(process) };

        if assigned == 0 {
            let err = std::io::Error::last_os_error();
            unsafe { CloseHandle(job) };
            bail!("Failed to assign process to Job Object: {err}");
        }

        tracing::debug!(pid, "Process assigned to Job Object with resource limits");
        Ok(JobObjectGuard(job))
    }
}

#[cfg(target_os = "windows")]
pub(crate) use job_objects::{enforce_job_limits, probe_job_objects, JobObjectGuard};
