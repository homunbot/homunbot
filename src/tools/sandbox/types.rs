use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub struct SandboxExecutionRequest<'a> {
    pub execution_kind: &'a str,
    pub program: &'a str,
    pub args: &'a [String],
    pub working_dir: &'a Path,
    pub extra_env: &'a HashMap<String, String>,
    pub sanitize_env: bool,
}

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
    pub repository: String,
    pub tag: Option<String>,
    pub digest: Option<String>,
    pub configured_policy: String,
    pub configured_expected_version: Option<String>,
    pub policy_source: String,
    pub expected_version: String,
    pub version_policy: String,
    pub drift_status: String,
    pub acceptability: String,
    pub update_recommended: bool,
    pub canonical_baseline: String,
    pub canonical_baseline_profile: String,
    pub canonical_baseline_aligned: bool,
    pub canonical_baseline_note: String,
    pub last_pulled_at: Option<String>,
    pub last_pulled_image: Option<String>,
    pub last_pulled_image_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxImagePullResult {
    pub status: SandboxImageStatus,
    pub output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxImageBuildResult {
    pub status: SandboxImageStatus,
    pub built_image: String,
    pub output: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxBackendCapability {
    pub backend: String,
    pub label: String,
    pub available: bool,
    pub supported_on_host: bool,
    pub implemented: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedSandboxBackend {
    None,
    Docker,
    LinuxNative,
    WindowsNative,
    MacosSeatbelt,
}

impl ResolvedSandboxBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Docker => "docker",
            Self::LinuxNative => "linux_native",
            Self::WindowsNative => "windows_native",
            Self::MacosSeatbelt => "macos_seatbelt",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "Native",
            Self::Docker => "Docker",
            Self::LinuxNative => "Linux Native",
            Self::WindowsNative => "Windows Native",
            Self::MacosSeatbelt => "macOS Seatbelt",
        }
    }
}

// --- Internal types (pub(crate) for cross-module use within sandbox) ---

#[derive(Debug, Clone)]
pub(crate) struct BackendProbe {
    pub available: bool,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub(crate) struct LinuxNativeRuntimeSupport {
    pub bubblewrap: BackendProbe,
    pub user_namespace: bool,
    pub network_namespace: bool,
    pub prlimit_available: bool,
    pub cgroup_v2_available: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct MacosSeatbeltRuntimeSupport {
    pub sandbox_exec: BackendProbe,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreparedCommandSpec {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct SandboxRuntimeImageState {
    pub last_pulled_at: Option<String>,
    pub last_pulled_image: Option<String>,
    pub last_pulled_image_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedRuntimeImageReference {
    pub normalized_image: String,
    pub repository: String,
    pub tag: Option<String>,
    pub digest: Option<String>,
    pub expected_version: String,
    pub version_policy: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeImageAssessment {
    pub drift_status: String,
    pub acceptability: String,
    pub update_recommended: bool,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeImagePolicyResolution {
    pub configured_policy: String,
    pub configured_expected_version: Option<String>,
    pub effective_policy: String,
    pub expected_version: String,
    pub policy_source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SandboxBackendAvailability {
    pub docker: bool,
    pub linux_native: bool,
    pub windows_native: bool,
    pub macos_seatbelt: bool,
}
