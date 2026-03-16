use std::path::Path;
use std::sync::OnceLock;

use anyhow::Result;

use super::types::{
    BackendProbe, LinuxNativeRuntimeSupport, MacosSeatbeltRuntimeSupport,
    ResolvedSandboxBackend, SandboxBackendAvailability, SandboxBackendCapability,
};
use crate::config::ExecutionSandboxConfig;

// --- Probe functions ---

pub(crate) fn docker_available() -> bool {
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

/// Live check — no cache. Use for API status endpoints where the user may
/// start Docker Desktop after the gateway is already running.
pub fn docker_available_live() -> bool {
    std::process::Command::new("docker")
        .arg("info")
        .arg("--format")
        .arg("{{.ServerVersion}}")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn linux_native_backend_available() -> bool {
    linux_native_runtime_support().bubblewrap.available
}

fn windows_native_backend_available() -> bool {
    windows_native_backend_probe().available
}

pub(crate) fn windows_native_backend_probe() -> BackendProbe {
    #[cfg(target_os = "windows")]
    {
        static CACHED: OnceLock<BackendProbe> = OnceLock::new();
        CACHED
            .get_or_init(|| {
                let (available, reason) = super::backends::probe_job_objects();
                BackendProbe { available, reason }
            })
            .clone()
    }

    #[cfg(not(target_os = "windows"))]
    {
        BackendProbe {
            available: false,
            reason: "Windows native isolation only applies on Windows hosts.".to_string(),
        }
    }
}

pub(crate) fn linux_native_backend_probe() -> BackendProbe {
    linux_native_runtime_support().bubblewrap
}

pub(crate) fn linux_native_runtime_support() -> LinuxNativeRuntimeSupport {
    #[cfg(target_os = "linux")]
    {
        static CACHED: OnceLock<LinuxNativeRuntimeSupport> = OnceLock::new();
        CACHED
            .get_or_init(|| {
                let bubblewrap = probe_bubblewrap([
                    "--die-with-parent",
                    "--ro-bind",
                    "/",
                    "/",
                    "--proc",
                    "/proc",
                    "--dev",
                    "/dev",
                    "/bin/true",
                ]);

                if !bubblewrap.available {
                    return LinuxNativeRuntimeSupport {
                        bubblewrap,
                        user_namespace: false,
                        network_namespace: false,
                        prlimit_available: false,
                        cgroup_v2_available: false,
                    };
                }

                let user_namespace = probe_bubblewrap([
                    "--die-with-parent",
                    "--ro-bind",
                    "/",
                    "/",
                    "--proc",
                    "/proc",
                    "--dev",
                    "/dev",
                    "--unshare-user",
                    "--uid",
                    "65534",
                    "--gid",
                    "65534",
                    "/bin/true",
                ])
                .available;

                let network_namespace = probe_bubblewrap([
                    "--die-with-parent",
                    "--ro-bind",
                    "/",
                    "/",
                    "--proc",
                    "/proc",
                    "--dev",
                    "/dev",
                    "--unshare-net",
                    "/bin/true",
                ])
                .available;

                let prlimit_available = std::process::Command::new("prlimit")
                    .arg("--version")
                    .output()
                    .map(|output| output.status.success())
                    .unwrap_or(false);

                let cgroup_v2_available = Path::new("/sys/fs/cgroup/cgroup.controllers").exists();

                let mut notes = Vec::new();
                notes.push(
                    "Bubblewrap is installed and a minimal sandbox probe succeeded.".to_string(),
                );
                notes.push(format!(
                    "user namespaces: {}",
                    if user_namespace {
                        "available"
                    } else {
                        "unavailable"
                    }
                ));
                notes.push(format!(
                    "network namespaces: {}",
                    if network_namespace {
                        "available"
                    } else {
                        "unavailable"
                    }
                ));
                notes.push(format!(
                    "prlimit: {}",
                    if prlimit_available {
                        "available"
                    } else {
                        "unavailable"
                    }
                ));
                notes.push(format!(
                    "cgroups v2: {}",
                    if cgroup_v2_available {
                        "available"
                    } else {
                        "unavailable"
                    }
                ));

                LinuxNativeRuntimeSupport {
                    bubblewrap: BackendProbe {
                        available: true,
                        reason: notes.join(" "),
                    },
                    user_namespace,
                    network_namespace,
                    prlimit_available,
                    cgroup_v2_available,
                }
            })
            .clone()
    }

    #[cfg(not(target_os = "linux"))]
    {
        LinuxNativeRuntimeSupport {
            bubblewrap: BackendProbe {
                available: false,
                reason: "Linux native isolation only applies on Linux hosts.".to_string(),
            },
            user_namespace: false,
            network_namespace: false,
            prlimit_available: false,
            cgroup_v2_available: false,
        }
    }
}

#[cfg(target_os = "linux")]
fn probe_bubblewrap<const N: usize>(args: [&str; N]) -> BackendProbe {
    let output = std::process::Command::new("bwrap").args(args).output();

    match output {
        Ok(output) if output.status.success() => BackendProbe {
            available: true,
            reason: "Bubblewrap probe succeeded.".to_string(),
        },
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let detail = [stderr, stdout]
                .into_iter()
                .find(|value| !value.is_empty())
                .unwrap_or_else(|| "bubblewrap probe failed without diagnostic output".to_string());
            BackendProbe {
                available: false,
                reason: format!("Bubblewrap is present but unusable: {detail}"),
            }
        }
        Err(err) => BackendProbe {
            available: false,
            reason: format!("Bubblewrap is unavailable: {err}"),
        },
    }
}

fn macos_seatbelt_backend_available() -> bool {
    macos_seatbelt_runtime_support().sandbox_exec.available
}

pub(crate) fn macos_seatbelt_backend_probe() -> BackendProbe {
    macos_seatbelt_runtime_support().sandbox_exec
}

pub(crate) fn macos_seatbelt_runtime_support() -> MacosSeatbeltRuntimeSupport {
    #[cfg(target_os = "macos")]
    {
        static CACHED: OnceLock<MacosSeatbeltRuntimeSupport> = OnceLock::new();
        CACHED
            .get_or_init(|| {
                let sandbox_exec = probe_sandbox_exec();
                MacosSeatbeltRuntimeSupport { sandbox_exec }
            })
            .clone()
    }

    #[cfg(not(target_os = "macos"))]
    {
        MacosSeatbeltRuntimeSupport {
            sandbox_exec: BackendProbe {
                available: false,
                reason: "macOS Seatbelt isolation only applies on macOS hosts.".to_string(),
            },
        }
    }
}

#[cfg(target_os = "macos")]
fn probe_sandbox_exec() -> BackendProbe {
    let path = std::path::Path::new("/usr/bin/sandbox-exec");
    if !path.exists() {
        return BackendProbe {
            available: false,
            reason: "/usr/bin/sandbox-exec is not present on this system.".to_string(),
        };
    }

    // Minimal probe: deny-all + allow reads + allow process, run /usr/bin/true
    let profile =
        "(version 1)(deny default)(allow process-exec)(allow process-fork)(allow file-read*)";
    let output = std::process::Command::new("/usr/bin/sandbox-exec")
        .args(["-p", profile, "--", "/usr/bin/true"])
        .output();

    match output {
        Ok(output) if output.status.success() => BackendProbe {
            available: true,
            reason: "sandbox-exec probe succeeded.".to_string(),
        },
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            BackendProbe {
                available: false,
                reason: format!(
                    "sandbox-exec is present but probe failed: {}",
                    if stderr.is_empty() {
                        "no diagnostic output"
                    } else {
                        &stderr
                    }
                ),
            }
        }
        Err(err) => BackendProbe {
            available: false,
            reason: format!("sandbox-exec probe failed: {err}"),
        },
    }
}

// --- Backend resolution ---

impl SandboxBackendAvailability {
    pub(crate) fn detect() -> Self {
        Self {
            docker: docker_available(),
            linux_native: linux_native_backend_available(),
            windows_native: windows_native_backend_available(),
            macos_seatbelt: macos_seatbelt_backend_available(),
        }
    }

    pub(crate) fn is_available(self, backend: ResolvedSandboxBackend) -> bool {
        match backend {
            ResolvedSandboxBackend::None => true,
            ResolvedSandboxBackend::Docker => self.docker,
            ResolvedSandboxBackend::LinuxNative => self.linux_native,
            ResolvedSandboxBackend::WindowsNative => self.windows_native,
            ResolvedSandboxBackend::MacosSeatbelt => self.macos_seatbelt,
        }
    }

    pub(crate) fn preferred_auto_backend(self) -> Option<ResolvedSandboxBackend> {
        #[cfg(target_os = "linux")]
        if self.linux_native {
            return Some(ResolvedSandboxBackend::LinuxNative);
        }

        #[cfg(target_os = "windows")]
        if self.windows_native {
            return Some(ResolvedSandboxBackend::WindowsNative);
        }

        #[cfg(target_os = "macos")]
        if self.macos_seatbelt {
            return Some(ResolvedSandboxBackend::MacosSeatbelt);
        }

        if self.docker {
            Some(ResolvedSandboxBackend::Docker)
        } else {
            None
        }
    }

    pub(crate) fn capabilities(self) -> Vec<SandboxBackendCapability> {
        vec![
            self.capability_for(ResolvedSandboxBackend::Docker),
            self.capability_for(ResolvedSandboxBackend::LinuxNative),
            self.capability_for(ResolvedSandboxBackend::WindowsNative),
            self.capability_for(ResolvedSandboxBackend::MacosSeatbelt),
        ]
    }

    fn capability_for(self, backend: ResolvedSandboxBackend) -> SandboxBackendCapability {
        let (supported_on_host, implemented, reason) = backend_capability_metadata(backend, self);
        SandboxBackendCapability {
            backend: backend.as_str().to_string(),
            label: backend.label().to_string(),
            available: self.is_available(backend),
            supported_on_host,
            implemented,
            reason,
        }
    }
}

pub fn current_sandbox_backend_capabilities() -> Vec<SandboxBackendCapability> {
    SandboxBackendAvailability::detect().capabilities()
}

pub fn sandbox_backend_availability_summary(capabilities: &[SandboxBackendCapability]) -> String {
    if capabilities.is_empty() {
        return "Managed backends: none detected.".to_string();
    }

    let joined = capabilities
        .iter()
        .map(|cap| {
            let status = if cap.available {
                "available"
            } else if !cap.supported_on_host {
                "unsupported on this host"
            } else if !cap.implemented {
                "planned"
            } else {
                "unavailable"
            };
            format!("{}: {}", cap.label, status)
        })
        .collect::<Vec<_>>()
        .join(" · ");

    format!("Managed backends: {joined}.")
}

pub(crate) fn normalize_backend(raw: &str) -> String {
    raw.trim().to_ascii_lowercase()
}

pub fn resolve_sandbox_backend(config: &ExecutionSandboxConfig) -> Result<ResolvedSandboxBackend> {
    resolve_sandbox_backend_with_capabilities(config, SandboxBackendAvailability::detect())
}

pub(crate) fn resolve_sandbox_backend_with_availability(
    config: &ExecutionSandboxConfig,
    docker_is_available: bool,
) -> Result<ResolvedSandboxBackend> {
    resolve_sandbox_backend_with_capabilities(
        config,
        SandboxBackendAvailability {
            docker: docker_is_available,
            linux_native: false,
            windows_native: false,
            macos_seatbelt: false,
        },
    )
}

pub(crate) fn resolve_sandbox_backend_with_capabilities(
    config: &ExecutionSandboxConfig,
    availability: SandboxBackendAvailability,
) -> Result<ResolvedSandboxBackend> {
    if !config.enabled {
        return Ok(ResolvedSandboxBackend::None);
    }

    let backend = normalize_backend(&config.backend);
    match backend.as_str() {
        "none" => Ok(ResolvedSandboxBackend::None),
        "docker" => resolve_requested_backend(config, availability, ResolvedSandboxBackend::Docker),
        "linux_native" => {
            resolve_requested_backend(config, availability, ResolvedSandboxBackend::LinuxNative)
        }
        "windows_native" => {
            resolve_requested_backend(config, availability, ResolvedSandboxBackend::WindowsNative)
        }
        "macos_seatbelt" => {
            resolve_requested_backend(config, availability, ResolvedSandboxBackend::MacosSeatbelt)
        }
        "auto" => {
            if let Some(auto_backend) = availability.preferred_auto_backend() {
                Ok(auto_backend)
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

fn resolve_requested_backend(
    config: &ExecutionSandboxConfig,
    availability: SandboxBackendAvailability,
    requested: ResolvedSandboxBackend,
) -> Result<ResolvedSandboxBackend> {
    if availability.is_available(requested) {
        return Ok(requested);
    }

    if config.strict {
        anyhow::bail!(
            "Sandbox backend '{}' requested but {} (strict mode)",
            requested.as_str(),
            backend_unavailable_reason(requested, availability)
        );
    }

    tracing::warn!(
        backend = requested.as_str(),
        "Sandbox backend unavailable; falling back to native"
    );
    Ok(ResolvedSandboxBackend::None)
}

fn backend_capability_metadata(
    backend: ResolvedSandboxBackend,
    availability: SandboxBackendAvailability,
) -> (bool, bool, String) {
    match backend {
        ResolvedSandboxBackend::None => (
            true,
            true,
            "Native execution is always available as the fallback path.".to_string(),
        ),
        ResolvedSandboxBackend::Docker => (
            true,
            true,
            if availability.docker {
                "Docker CLI and daemon are reachable.".to_string()
            } else {
                "Docker CLI or daemon is unavailable on this machine.".to_string()
            },
        ),
        ResolvedSandboxBackend::LinuxNative => {
            let probe = linux_native_backend_probe();
            (
                cfg!(target_os = "linux"),
                cfg!(target_os = "linux"),
                probe.reason,
            )
        }
        ResolvedSandboxBackend::WindowsNative => {
            let probe = windows_native_backend_probe();
            (
                cfg!(target_os = "windows"),
                cfg!(target_os = "windows"),
                probe.reason,
            )
        }
        ResolvedSandboxBackend::MacosSeatbelt => {
            let probe = macos_seatbelt_backend_probe();
            (
                cfg!(target_os = "macos"),
                cfg!(target_os = "macos"),
                probe.reason,
            )
        }
    }
}

fn backend_unavailable_reason(
    backend: ResolvedSandboxBackend,
    availability: SandboxBackendAvailability,
) -> String {
    backend_capability_metadata(backend, availability).2
}
