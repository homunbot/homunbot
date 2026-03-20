use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::get;
use axum::Router;
use serde::{Deserialize, Serialize};

use crate::web::auth::{check_admin, AuthUser};
use crate::web::server::AppState;

pub(super) fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/v1/security/sandbox",
            get(get_execution_sandbox).put(put_execution_sandbox),
        )
        .route(
            "/v1/security/sandbox/status",
            get(get_execution_sandbox_status),
        )
        .route(
            "/v1/security/sandbox/presets",
            get(get_execution_sandbox_presets),
        )
        .route(
            "/v1/security/sandbox/image",
            get(get_execution_sandbox_image_status),
        )
        .route(
            "/v1/security/sandbox/image/pull",
            axum::routing::post(pull_execution_sandbox_image),
        )
        .route(
            "/v1/security/sandbox/image/build",
            axum::routing::post(build_execution_sandbox_image),
        )
        .route(
            "/v1/security/sandbox/events",
            get(get_execution_sandbox_events),
        )
}

/// Get process execution sandbox configuration.
async fn get_execution_sandbox(
    State(state): State<Arc<AppState>>,
) -> Json<crate::config::ExecutionSandboxConfig> {
    let config = state.config.read().await;
    Json(config.security.execution_sandbox.clone())
}

/// Update process execution sandbox configuration.
async fn put_execution_sandbox(
    State(state): State<Arc<AppState>>,
    axum::Extension(auth): axum::Extension<AuthUser>,
    Json(sandbox): Json<crate::config::ExecutionSandboxConfig>,
) -> Result<Json<crate::config::ExecutionSandboxConfig>, (StatusCode, String)> {
    check_admin(&auth).map_err(|s| (s, "Admin scope required".into()))?;
    let sandbox = normalize_execution_sandbox(sandbox)?;
    let mut config = state.config.write().await;
    config.security.execution_sandbox = sandbox;

    if let Err(e) = config.save() {
        tracing::error!("Failed to save execution sandbox config: {}", e);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to save execution sandbox config".to_string(),
        ));
    }

    Ok(Json(config.security.execution_sandbox.clone()))
}

fn normalize_execution_sandbox(
    mut sandbox: crate::config::ExecutionSandboxConfig,
) -> Result<crate::config::ExecutionSandboxConfig, (StatusCode, String)> {
    let backend = sandbox.backend.trim().to_ascii_lowercase();
    let docker_network = sandbox.docker_network.trim().to_ascii_lowercase();
    let docker_image = sandbox.docker_image.trim().to_string();
    let runtime_image_policy = sandbox.runtime_image_policy.trim().to_ascii_lowercase();
    let runtime_image_expected_version = sandbox.runtime_image_expected_version.trim().to_string();

    let backend = if backend.is_empty() {
        "auto".to_string()
    } else {
        backend
    };
    if !matches!(
        backend.as_str(),
        "none" | "auto" | "docker" | "linux_native" | "windows_native" | "macos_seatbelt"
    ) {
        return Err((
            StatusCode::BAD_REQUEST,
            "Invalid sandbox backend. Expected one of: auto, docker, linux_native, windows_native, macos_seatbelt, none.".to_string(),
        ));
    }

    let docker_network = if docker_network.is_empty() {
        "none".to_string()
    } else {
        docker_network
    };
    if docker_network != "none" && docker_network != "bridge" && docker_network != "host" {
        return Err((
            StatusCode::BAD_REQUEST,
            "Invalid docker network. Expected one of: none, bridge, host.".to_string(),
        ));
    }

    let runtime_image_policy = if runtime_image_policy.is_empty() {
        "infer".to_string()
    } else {
        runtime_image_policy
    };
    if runtime_image_policy != "infer"
        && runtime_image_policy != "pinned"
        && runtime_image_policy != "versioned_tag"
        && runtime_image_policy != "floating"
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Invalid runtime image policy. Expected one of: infer, pinned, versioned_tag, floating.".to_string(),
        ));
    }

    if !sandbox.docker_cpus.is_finite() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Invalid docker CPU limit. Must be a finite number.".to_string(),
        ));
    }
    if sandbox.docker_cpus < 0.0 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Invalid docker CPU limit. Must be >= 0.".to_string(),
        ));
    }
    if sandbox.docker_cpus > 256.0 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Invalid docker CPU limit. Must be <= 256.".to_string(),
        ));
    }

    if sandbox.docker_memory_mb > 1_048_576 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Invalid docker memory limit. Must be <= 1048576 MB.".to_string(),
        ));
    }

    sandbox.backend = backend;
    sandbox.docker_network = docker_network;
    sandbox.docker_image = if docker_image.is_empty() {
        "node:22-alpine".to_string()
    } else {
        docker_image
    };
    sandbox.runtime_image_policy = runtime_image_policy.clone();
    sandbox.runtime_image_expected_version = if runtime_image_policy == "infer" {
        String::new()
    } else {
        runtime_image_expected_version
    };

    Ok(sandbox)
}

#[derive(Serialize)]
struct ExecutionSandboxStatusResponse {
    enabled: bool,
    host_os: String,
    configured_backend: String,
    resolved_backend: String,
    strict: bool,
    docker_available: bool,
    any_backend_available: bool,
    valid: bool,
    fallback_to_native: bool,
    recommended_preset: String,
    message: String,
    availability_summary: String,
    capabilities: Vec<crate::tools::sandbox::SandboxBackendCapability>,
}

#[derive(Serialize)]
struct ExecutionSandboxPresetResponse {
    id: String,
    label: String,
    description: String,
    recommended: bool,
    config: crate::config::ExecutionSandboxConfig,
}

#[derive(Deserialize)]
struct ExecutionSandboxEventsQuery {
    limit: Option<usize>,
}

#[derive(Serialize)]
struct ExecutionSandboxImagePullResponse {
    status: crate::tools::sandbox::SandboxImageStatus,
    output: String,
}

#[derive(Serialize)]
struct ExecutionSandboxImageBuildResponse {
    status: crate::tools::sandbox::SandboxImageStatus,
    built_image: String,
    output: String,
    message: String,
}

fn host_os_label() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "macos"
    }
    #[cfg(target_os = "linux")]
    {
        "linux"
    }
    #[cfg(target_os = "windows")]
    {
        "windows"
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        "unknown"
    }
}

/// Get resolved runtime status for process execution sandbox.
async fn get_execution_sandbox_status(
    State(state): State<Arc<AppState>>,
) -> Json<ExecutionSandboxStatusResponse> {
    let config = state.config.read().await;
    let sandbox = config.security.execution_sandbox.clone();
    drop(config);

    let capabilities = crate::tools::sandbox::current_sandbox_backend_capabilities();
    let availability_summary =
        crate::tools::sandbox::sandbox_backend_availability_summary(&capabilities);
    let configured_backend = sandbox.backend.trim().to_ascii_lowercase();
    // Live check — not cached. The user may start Docker Desktop after gateway boot.
    let docker_available = crate::tools::sandbox::docker_available_live();
    let (resolved_backend, valid, mut message) =
        match crate::tools::sandbox::resolve_sandbox_backend(&sandbox) {
            Ok(resolved) => (resolved.as_str().to_string(), true, String::new()),
            Err(e) => ("none".to_string(), false, e.to_string()),
        };
    let fallback_to_native = sandbox.enabled
        && resolved_backend == "none"
        && configured_backend != "none"
        && !sandbox.strict;
    let recommended_preset = if capabilities.iter().any(|cap| cap.available) {
        "strict"
    } else {
        "safe"
    };
    if valid {
        message = if !sandbox.enabled {
            "Sandbox disabled.".to_string()
        } else if fallback_to_native {
            format!(
                "Configured backend '{}' unavailable; using native fallback.",
                configured_backend
            )
        } else if resolved_backend == "none" {
            "Sandbox enabled with 'none' backend (native execution).".to_string()
        } else {
            format!("Sandbox active with '{}' backend.", resolved_backend)
        };
    }

    let any_backend_available = capabilities.iter().any(|cap| cap.available);

    Json(ExecutionSandboxStatusResponse {
        enabled: sandbox.enabled,
        host_os: host_os_label().to_string(),
        configured_backend: configured_backend.clone(),
        resolved_backend,
        strict: sandbox.strict,
        docker_available,
        any_backend_available,
        valid,
        fallback_to_native,
        recommended_preset: recommended_preset.to_string(),
        message,
        availability_summary,
        capabilities,
    })
}

/// List opinionated execution sandbox presets for the current host.
async fn get_execution_sandbox_presets() -> Json<Vec<ExecutionSandboxPresetResponse>> {
    let safe_cfg = crate::config::ExecutionSandboxConfig {
        enabled: true,
        backend: "auto".to_string(),
        strict: false,
        docker_image: crate::tools::sandbox::canonical_sandbox_runtime_baseline().to_string(),
        runtime_image_policy: "versioned_tag".to_string(),
        runtime_image_expected_version: "2026.03".to_string(),
        docker_network: "none".to_string(),
        docker_read_only_rootfs: true,
        docker_mount_workspace: true,
        ..Default::default()
    };

    let mut strict_cfg = safe_cfg.clone();
    strict_cfg.strict = true;

    let docker_available = crate::tools::sandbox::docker_available_live();
    let host = host_os_label();

    Json(vec![
        ExecutionSandboxPresetResponse {
            id: "safe".to_string(),
            label: format!("{host} Safe"),
            description: "Prefers sandbox backend but allows native fallback when unavailable."
                .to_string(),
            recommended: !docker_available,
            config: safe_cfg,
        },
        ExecutionSandboxPresetResponse {
            id: "strict".to_string(),
            label: format!("{host} Strict"),
            description: "Requires sandbox backend; blocks execution if backend is unavailable."
                .to_string(),
            recommended: docker_available,
            config: strict_cfg,
        },
    ])
}

/// Inspect the configured sandbox runtime image.
async fn get_execution_sandbox_image_status(
    State(state): State<Arc<AppState>>,
) -> Json<crate::tools::sandbox::SandboxImageStatus> {
    let config = state.config.read().await;
    let sandbox = config.security.execution_sandbox.clone();
    drop(config);

    Json(crate::tools::sandbox::get_runtime_image_status(&sandbox))
}

/// Pull the configured sandbox runtime image.
async fn pull_execution_sandbox_image(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ExecutionSandboxImagePullResponse>, (StatusCode, String)> {
    let config = state.config.read().await;
    let sandbox = config.security.execution_sandbox.clone();
    drop(config);

    let result = crate::tools::sandbox::pull_runtime_image(&sandbox)
        .await
        .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;

    Ok(Json(ExecutionSandboxImagePullResponse {
        status: result.status,
        output: result.output,
    }))
}

/// Build the configured sandbox runtime image when it targets the repo baseline.
async fn build_execution_sandbox_image(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ExecutionSandboxImageBuildResponse>, (StatusCode, String)> {
    let config = state.config.read().await;
    let sandbox = config.security.execution_sandbox.clone();
    drop(config);

    let result = crate::tools::sandbox::build_runtime_image(&sandbox)
        .await
        .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;

    Ok(Json(ExecutionSandboxImageBuildResponse {
        status: result.status,
        built_image: result.built_image,
        output: result.output,
        message: result.message,
    }))
}

/// Return the most recent sandbox preparation events.
async fn get_execution_sandbox_events(
    Query(query): Query<ExecutionSandboxEventsQuery>,
) -> Json<Vec<crate::tools::sandbox::SandboxEvent>> {
    let limit = query.limit.unwrap_or(12).clamp(1, 50);
    Json(crate::tools::sandbox::list_recent_sandbox_events(limit))
}

#[cfg(test)]
mod sandbox_config_tests {
    use super::get_execution_sandbox_presets;
    use super::normalize_execution_sandbox;
    use crate::config::ExecutionSandboxConfig;
    use axum::http::StatusCode;

    #[test]
    fn normalize_sandbox_accepts_valid_values() {
        let cfg = ExecutionSandboxConfig {
            backend: "DoCkEr".to_string(),
            docker_network: "Bridge".to_string(),
            docker_image: " node:22-alpine ".to_string(),
            runtime_image_policy: " Pinned ".to_string(),
            runtime_image_expected_version: " sha256:abc ".to_string(),
            ..ExecutionSandboxConfig::default()
        };
        let normalized = normalize_execution_sandbox(cfg).expect("valid sandbox config");
        assert_eq!(normalized.backend, "docker");
        assert_eq!(normalized.docker_network, "bridge");
        assert_eq!(normalized.docker_image, "node:22-alpine");
        assert_eq!(normalized.runtime_image_policy, "pinned");
        assert_eq!(normalized.runtime_image_expected_version, "sha256:abc");
    }

    #[test]
    fn normalize_sandbox_rejects_invalid_backend() {
        let cfg = ExecutionSandboxConfig {
            backend: "firecracker".to_string(),
            ..ExecutionSandboxConfig::default()
        };
        let err = normalize_execution_sandbox(cfg).expect_err("expected backend validation error");
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn normalize_sandbox_accepts_linux_native_backend() {
        let cfg = ExecutionSandboxConfig {
            backend: "LiNuX_NaTiVe".to_string(),
            ..ExecutionSandboxConfig::default()
        };
        let normalized = normalize_execution_sandbox(cfg).expect("valid linux native backend");
        assert_eq!(normalized.backend, "linux_native");
    }

    #[test]
    fn normalize_sandbox_accepts_macos_seatbelt_backend() {
        let cfg = ExecutionSandboxConfig {
            backend: "MacOS_Seatbelt".to_string(),
            ..ExecutionSandboxConfig::default()
        };
        let normalized = normalize_execution_sandbox(cfg).expect("valid macos seatbelt backend");
        assert_eq!(normalized.backend, "macos_seatbelt");
    }

    #[test]
    fn normalize_sandbox_rejects_invalid_network() {
        let cfg = ExecutionSandboxConfig {
            docker_network: "custom".to_string(),
            ..ExecutionSandboxConfig::default()
        };
        let err = normalize_execution_sandbox(cfg).expect_err("expected network validation error");
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn normalize_sandbox_defaults_empty_image() {
        let cfg = ExecutionSandboxConfig {
            docker_image: "   ".to_string(),
            ..ExecutionSandboxConfig::default()
        };
        let normalized = normalize_execution_sandbox(cfg).expect("expected default image");
        assert_eq!(normalized.docker_image, "node:22-alpine");
    }

    #[test]
    fn normalize_sandbox_clears_expected_version_when_policy_is_infer() {
        let cfg = ExecutionSandboxConfig {
            runtime_image_policy: "infer".to_string(),
            runtime_image_expected_version: "sha256:abc".to_string(),
            ..ExecutionSandboxConfig::default()
        };
        let normalized =
            normalize_execution_sandbox(cfg).expect("expected normalized sandbox config");
        assert_eq!(normalized.runtime_image_expected_version, "");
    }

    #[test]
    fn normalize_sandbox_rejects_invalid_runtime_image_policy() {
        let cfg = ExecutionSandboxConfig {
            runtime_image_policy: "manual".to_string(),
            ..ExecutionSandboxConfig::default()
        };
        let err =
            normalize_execution_sandbox(cfg).expect_err("expected runtime image policy error");
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn sandbox_presets_include_safe_and_strict() {
        let presets = get_execution_sandbox_presets().await.0;
        assert!(presets.iter().any(|p| p.id == "safe"));
        assert!(presets.iter().any(|p| p.id == "strict"));

        let recommended_count = presets.iter().filter(|p| p.recommended).count();
        assert_eq!(recommended_count, 1);
    }

    #[tokio::test]
    async fn sandbox_presets_point_to_canonical_runtime_baseline() {
        let presets = get_execution_sandbox_presets().await.0;
        for preset in presets {
            if preset.id == "safe" || preset.id == "strict" {
                assert_eq!(preset.config.docker_image, "homun/runtime-core:2026.03");
                assert_eq!(preset.config.runtime_image_policy, "versioned_tag");
                assert_eq!(preset.config.runtime_image_expected_version, "2026.03");
            }
        }
    }
}
