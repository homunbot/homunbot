use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::Utc;
use tokio::process::Command;

use crate::config::ExecutionSandboxConfig;

use super::events::sandbox_runtime_image_state_path;
use super::resolve::docker_available;
use super::types::{
    ParsedRuntimeImageReference, RuntimeImageAssessment, RuntimeImagePolicyResolution,
    SandboxImageBuildResult, SandboxImagePullResult, SandboxImageStatus, SandboxRuntimeImageState,
};

pub(crate) const DEFAULT_SANDBOX_RUNTIME_IMAGE: &str = "node:22-alpine";
const CANONICAL_SANDBOX_RUNTIME_BASELINE: &str = "homun/runtime-core:2026.03";
const CANONICAL_SANDBOX_RUNTIME_REPOSITORY: &str = "homun/runtime-core";

pub fn canonical_sandbox_runtime_baseline() -> &'static str {
    CANONICAL_SANDBOX_RUNTIME_BASELINE
}

// --- Parsing ---

fn normalize_runtime_image(image: &str) -> String {
    let trimmed = image.trim();
    if trimmed.is_empty() {
        DEFAULT_SANDBOX_RUNTIME_IMAGE.to_string()
    } else {
        trimmed.to_string()
    }
}

pub(crate) fn parse_runtime_image_reference(image: &str) -> ParsedRuntimeImageReference {
    let normalized_image = normalize_runtime_image(image);
    let (without_digest, digest) = if let Some((name, digest)) = normalized_image.split_once('@') {
        (name.to_string(), Some(digest.to_string()))
    } else {
        (normalized_image.clone(), None)
    };

    let last_slash = without_digest.rfind('/');
    let last_colon = without_digest.rfind(':');
    let tag = match (last_slash, last_colon) {
        (_, Some(colon)) if last_slash.map(|slash| colon > slash).unwrap_or(true) => {
            Some(without_digest[colon + 1..].to_string())
        }
        _ => None,
    };

    let repository = if let Some(tag) = &tag {
        without_digest[..without_digest.len().saturating_sub(tag.len() + 1)].to_string()
    } else {
        without_digest.clone()
    };

    let (expected_version, version_policy) = if let Some(digest) = &digest {
        (digest.clone(), "pinned".to_string())
    } else {
        let effective_tag = tag.clone().unwrap_or_else(|| "latest".to_string());
        let policy = if effective_tag == "latest" {
            "floating"
        } else {
            "versioned_tag"
        };
        (effective_tag, policy.to_string())
    };

    ParsedRuntimeImageReference {
        normalized_image,
        repository,
        tag: tag.or_else(|| {
            if digest.is_none() {
                Some("latest".to_string())
            } else {
                None
            }
        }),
        digest,
        expected_version,
        version_policy,
    }
}

// --- Policy resolution ---

fn normalize_runtime_image_policy(raw: &str) -> String {
    let normalized = raw.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        "infer".to_string()
    } else {
        normalized
    }
}

pub(crate) fn resolve_runtime_image_policy(
    image: &ParsedRuntimeImageReference,
    configured_policy: &str,
    configured_expected_version: &str,
) -> RuntimeImagePolicyResolution {
    let configured_policy = normalize_runtime_image_policy(configured_policy);
    let configured_expected_version = configured_expected_version.trim().to_string();
    let configured_expected_version = if configured_expected_version.is_empty() {
        None
    } else {
        Some(configured_expected_version)
    };

    if configured_policy == "infer" {
        return RuntimeImagePolicyResolution {
            configured_policy,
            configured_expected_version: None,
            effective_policy: image.version_policy.clone(),
            expected_version: image.expected_version.clone(),
            policy_source: "inferred".to_string(),
        };
    }

    let expected_version = configured_expected_version
        .clone()
        .or_else(|| match configured_policy.as_str() {
            "pinned" => image.digest.clone(),
            "versioned_tag" => image.tag.clone(),
            "floating" => Some(image.tag.clone().unwrap_or_else(|| "latest".to_string())),
            _ => None,
        })
        .unwrap_or_else(|| image.expected_version.clone());
    let effective_policy = configured_policy.clone();

    RuntimeImagePolicyResolution {
        configured_policy,
        configured_expected_version,
        effective_policy,
        expected_version,
        policy_source: "explicit".to_string(),
    }
}

// --- State persistence ---

pub(crate) fn load_runtime_image_state() -> Option<SandboxRuntimeImageState> {
    let path = sandbox_runtime_image_state_path();
    let raw = std::fs::read_to_string(&path).ok()?;
    match serde_json::from_str::<SandboxRuntimeImageState>(&raw) {
        Ok(state) => Some(state),
        Err(err) => {
            tracing::warn!(error = %err, path = %path.display(), "Failed to parse sandbox runtime image state");
            None
        }
    }
}

pub(crate) fn save_runtime_image_state(state: &SandboxRuntimeImageState) {
    let path = sandbox_runtime_image_state_path();
    if let Some(parent) = path.parent() {
        if let Err(err) = fs::create_dir_all(parent) {
            tracing::warn!(error = %err, path = %parent.display(), "Failed to create sandbox runtime image state directory");
            return;
        }
    }

    let raw = match serde_json::to_string_pretty(state) {
        Ok(raw) => raw,
        Err(err) => {
            tracing::warn!(error = %err, "Failed to serialize sandbox runtime image state");
            return;
        }
    };

    if let Err(err) = fs::write(&path, raw) {
        tracing::warn!(error = %err, path = %path.display(), "Failed to write sandbox runtime image state");
    }
}

// --- Assessment ---

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

pub(crate) fn assess_runtime_image_status(
    image: &ParsedRuntimeImageReference,
    policy: &RuntimeImagePolicyResolution,
    docker_available: bool,
    present: bool,
    image_id: Option<&String>,
    state: Option<&SandboxRuntimeImageState>,
) -> RuntimeImageAssessment {
    if policy.effective_policy == "pinned"
        && image.digest.as_deref() != Some(policy.expected_version.as_str())
    {
        let (drift_status, message) = if image.digest.is_some() {
            (
                    "config-version-mismatch".to_string(),
                    format!(
                        "Pinned runtime policy expects '{}', but the configured image reference resolves to digest '{}'.",
                        policy.expected_version,
                        image.digest.as_deref().unwrap_or_default()
                    ),
                )
        } else {
            (
                    "not-pinned-reference".to_string(),
                    format!(
                        "Pinned runtime policy expects '{}', but the configured image reference '{}' is not digest-pinned.",
                        policy.expected_version, image.normalized_image
                    ),
                )
        };
        return RuntimeImageAssessment {
            drift_status,
            acceptability: "action_required".to_string(),
            update_recommended: true,
            message,
        };
    }

    if policy.effective_policy == "versioned_tag"
        && image.tag.as_deref() != Some(policy.expected_version.as_str())
    {
        return RuntimeImageAssessment {
            drift_status: "config-version-mismatch".to_string(),
            acceptability: "action_required".to_string(),
            update_recommended: true,
            message: format!(
                "Versioned-tag runtime policy expects tag '{}', but the configured image reference resolves to tag '{}'.",
                policy.expected_version,
                image.tag.as_deref().unwrap_or("latest")
            ),
        };
    }

    if !docker_available {
        return RuntimeImageAssessment {
            drift_status: "docker-unavailable".to_string(),
            acceptability: "unknown".to_string(),
            update_recommended: false,
            message: "Docker is unavailable, so the runtime image cannot be inspected.".to_string(),
        };
    }

    if !present {
        return RuntimeImageAssessment {
            drift_status: "missing".to_string(),
            acceptability: "action_required".to_string(),
            update_recommended: true,
            message: format!(
                "Configured runtime image '{}' is not present locally.",
                image.normalized_image
            ),
        };
    }

    if policy.effective_policy == "floating" {
        return RuntimeImageAssessment {
            drift_status: "tracking-floating-tag".to_string(),
            acceptability: "review".to_string(),
            update_recommended: true,
            message: format!(
                "Runtime image '{}' is present, but it tracks a floating tag; drift is possible.",
                image.normalized_image
            ),
        };
    }

    if let Some(state) = state {
        if let Some(last_pulled_image) = &state.last_pulled_image {
            if last_pulled_image != &image.normalized_image {
                return RuntimeImageAssessment {
                    drift_status: "config-changed-since-last-pull".to_string(),
                    acceptability: "review".to_string(),
                    update_recommended: true,
                    message: format!(
                        "Configured runtime image '{}' differs from the last pulled image '{}'.",
                        image.normalized_image, last_pulled_image
                    ),
                };
            }
        }

        if let (Some(last_pulled_id), Some(current_id)) =
            (state.last_pulled_image_id.as_ref(), image_id)
        {
            if last_pulled_id != current_id {
                return RuntimeImageAssessment {
                    drift_status: "changed-since-last-pull".to_string(),
                    acceptability: "review".to_string(),
                    update_recommended: true,
                    message: format!(
                        "Runtime image '{}' has a different local image ID than the last recorded pull.",
                        image.normalized_image
                    ),
                };
            }
        }

        if state.last_pulled_at.is_some() {
            return RuntimeImageAssessment {
                drift_status: "aligned".to_string(),
                acceptability: "acceptable".to_string(),
                update_recommended: false,
                message: format!(
                    "Runtime image '{}' is present and aligned with the recorded pull state.",
                    image.normalized_image
                ),
            };
        }
    }

    RuntimeImageAssessment {
        drift_status: "present-untracked".to_string(),
        acceptability: "review".to_string(),
        update_recommended: false,
        message: format!(
            "Runtime image '{}' is present, but no pull history has been recorded yet.",
            image.normalized_image
        ),
    }
}

fn decorate_runtime_image_message(base: String, configured_was_empty: bool) -> String {
    if configured_was_empty {
        format!(
            "No runtime image configured; using canonical baseline '{}'. {}",
            DEFAULT_SANDBOX_RUNTIME_IMAGE, base
        )
    } else {
        base
    }
}

fn canonical_runtime_baseline_note(image: &ParsedRuntimeImageReference) -> String {
    if image.normalized_image == CANONICAL_SANDBOX_RUNTIME_BASELINE {
        "Configured image matches the canonical core runtime baseline for sandboxed skills and common MCP workloads.".to_string()
    } else {
        format!(
            "Canonical core baseline is '{}'. The current image may still work, but alignment is operator-managed.",
            CANONICAL_SANDBOX_RUNTIME_BASELINE
        )
    }
}

fn runtime_image_build_script_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("build_sandbox_runtime_image.sh")
}

fn resolve_runtime_image_build_target(sandbox: &ExecutionSandboxConfig) -> Result<String> {
    let parsed = parse_runtime_image_reference(&sandbox.docker_image);
    if parsed.repository == CANONICAL_SANDBOX_RUNTIME_REPOSITORY {
        return Ok(parsed.normalized_image);
    }

    anyhow::bail!(
        "Build action only supports '{}' tags. Set sandbox.docker_image to '{}' or another '{}' tag, then retry.",
        CANONICAL_SANDBOX_RUNTIME_REPOSITORY,
        CANONICAL_SANDBOX_RUNTIME_BASELINE,
        CANONICAL_SANDBOX_RUNTIME_REPOSITORY
    )
}

// --- Public API ---

pub fn get_runtime_image_status(sandbox: &ExecutionSandboxConfig) -> SandboxImageStatus {
    let checked_at = Utc::now().to_rfc3339();
    let parsed = parse_runtime_image_reference(&sandbox.docker_image);
    let policy = resolve_runtime_image_policy(
        &parsed,
        &sandbox.runtime_image_policy,
        &sandbox.runtime_image_expected_version,
    );
    let persisted = load_runtime_image_state();
    let configured_was_empty = sandbox.docker_image.trim().is_empty();
    let baseline_aligned = parsed.normalized_image == CANONICAL_SANDBOX_RUNTIME_BASELINE;
    let baseline_note = canonical_runtime_baseline_note(&parsed);

    let docker_avail = docker_available();
    if !docker_avail {
        let assessment = assess_runtime_image_status(
            &parsed,
            &policy,
            docker_avail,
            false,
            None,
            persisted.as_ref(),
        );
        let message =
            decorate_runtime_image_message(assessment.message.clone(), configured_was_empty);
        return build_image_status(
            parsed,
            docker_avail,
            false,
            None,
            None,
            None,
            checked_at,
            message,
            policy,
            assessment,
            baseline_aligned,
            baseline_note,
            persisted,
        );
    }

    let output = std::process::Command::new("docker")
        .arg("image")
        .arg("inspect")
        .arg(&parsed.normalized_image)
        .arg("--format")
        .arg("{{.Id}}\n{{.Created}}\n{{.Size}}")
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let (image_id, created_at, size_bytes) = parse_docker_inspect_fields(&stdout);
            let assessment = assess_runtime_image_status(
                &parsed,
                &policy,
                docker_avail,
                true,
                image_id.as_ref(),
                persisted.as_ref(),
            );
            let message =
                decorate_runtime_image_message(assessment.message.clone(), configured_was_empty);
            build_image_status(
                parsed,
                docker_avail,
                true,
                image_id,
                created_at,
                size_bytes,
                checked_at,
                message,
                policy,
                assessment,
                baseline_aligned,
                baseline_note,
                persisted,
            )
        }
        Ok(_) => {
            let assessment = assess_runtime_image_status(
                &parsed,
                &policy,
                docker_avail,
                false,
                None,
                persisted.as_ref(),
            );
            let message =
                decorate_runtime_image_message(assessment.message.clone(), configured_was_empty);
            build_image_status(
                parsed,
                docker_avail,
                false,
                None,
                None,
                None,
                checked_at,
                message,
                policy,
                assessment,
                baseline_aligned,
                baseline_note,
                persisted,
            )
        }
        Err(err) => {
            let mut assessment = assess_runtime_image_status(
                &parsed,
                &policy,
                docker_avail,
                false,
                None,
                persisted.as_ref(),
            );
            assessment.message = format!("Failed to inspect runtime image: {err}");
            let message =
                decorate_runtime_image_message(assessment.message.clone(), configured_was_empty);
            let mut status = build_image_status(
                parsed,
                docker_avail,
                false,
                None,
                None,
                None,
                checked_at,
                message,
                policy,
                assessment,
                baseline_aligned,
                baseline_note,
                persisted,
            );
            status.drift_status = "inspect-error".to_string();
            status.acceptability = "unknown".to_string();
            status.update_recommended = false;
            status
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn build_image_status(
    parsed: ParsedRuntimeImageReference,
    docker_available: bool,
    present: bool,
    image_id: Option<String>,
    created_at: Option<String>,
    size_bytes: Option<u64>,
    checked_at: String,
    message: String,
    policy: RuntimeImagePolicyResolution,
    assessment: RuntimeImageAssessment,
    baseline_aligned: bool,
    baseline_note: String,
    persisted: Option<SandboxRuntimeImageState>,
) -> SandboxImageStatus {
    SandboxImageStatus {
        image: parsed.normalized_image,
        docker_available,
        present,
        image_id,
        created_at,
        size_bytes,
        checked_at,
        message,
        repository: parsed.repository,
        tag: parsed.tag,
        digest: parsed.digest,
        configured_policy: policy.configured_policy,
        configured_expected_version: policy.configured_expected_version,
        policy_source: policy.policy_source,
        expected_version: policy.expected_version,
        version_policy: policy.effective_policy,
        drift_status: assessment.drift_status,
        acceptability: assessment.acceptability,
        update_recommended: assessment.update_recommended,
        canonical_baseline: CANONICAL_SANDBOX_RUNTIME_BASELINE.to_string(),
        canonical_baseline_profile: "core".to_string(),
        canonical_baseline_aligned: baseline_aligned,
        canonical_baseline_note: baseline_note,
        last_pulled_at: persisted
            .as_ref()
            .and_then(|state| state.last_pulled_at.clone()),
        last_pulled_image: persisted
            .as_ref()
            .and_then(|state| state.last_pulled_image.clone()),
        last_pulled_image_id: persisted
            .as_ref()
            .and_then(|state| state.last_pulled_image_id.clone()),
    }
}

pub fn get_docker_image_status(image: &str) -> SandboxImageStatus {
    let sandbox = ExecutionSandboxConfig {
        docker_image: normalize_runtime_image(image),
        ..ExecutionSandboxConfig::default()
    };
    get_runtime_image_status(&sandbox)
}

pub async fn pull_runtime_image(
    sandbox: &ExecutionSandboxConfig,
) -> Result<SandboxImagePullResult> {
    let image = normalize_runtime_image(&sandbox.docker_image);
    if !docker_available() {
        anyhow::bail!("Docker is unavailable; cannot pull sandbox runtime image");
    }

    let output = Command::new("docker")
        .arg("pull")
        .arg(&image)
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

    let inspect_status = get_runtime_image_status(sandbox);
    save_runtime_image_state(&SandboxRuntimeImageState {
        last_pulled_at: Some(Utc::now().to_rfc3339()),
        last_pulled_image: Some(image.clone()),
        last_pulled_image_id: inspect_status.image_id.clone(),
    });

    Ok(SandboxImagePullResult {
        status: get_runtime_image_status(sandbox),
        output: combined,
    })
}

pub async fn build_runtime_image(
    sandbox: &ExecutionSandboxConfig,
) -> Result<SandboxImageBuildResult> {
    if !docker_available() {
        anyhow::bail!("Docker is unavailable; cannot build sandbox runtime image");
    }

    let build_target = resolve_runtime_image_build_target(sandbox)?;
    let script_path = runtime_image_build_script_path();
    if !script_path.exists() {
        anyhow::bail!(
            "Sandbox runtime build script is missing: {}",
            script_path.display()
        );
    }

    let output = Command::new("bash")
        .arg(&script_path)
        .arg(&build_target)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .await
        .with_context(|| {
            format!(
                "Failed to build sandbox runtime image '{}' via {}",
                build_target,
                script_path.display()
            )
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let combined = [stdout, stderr]
        .into_iter()
        .filter(|chunk| !chunk.is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    if !output.status.success() {
        anyhow::bail!(
            "Failed to build sandbox runtime image '{}': {}",
            build_target,
            if combined.is_empty() {
                "build script returned a non-zero exit status".to_string()
            } else {
                combined
            }
        );
    }

    Ok(SandboxImageBuildResult {
        status: get_runtime_image_status(sandbox),
        built_image: build_target.clone(),
        output: combined,
        message: format!("Built sandbox runtime image '{}'.", build_target),
    })
}

pub async fn pull_docker_image(image: &str) -> Result<SandboxImagePullResult> {
    let sandbox = ExecutionSandboxConfig {
        docker_image: normalize_runtime_image(image),
        ..ExecutionSandboxConfig::default()
    };
    pull_runtime_image(&sandbox).await
}
