#[cfg(feature = "browser")]
mod inner {
    #[cfg(unix)]
    use std::os::unix::fs::MetadataExt;
    use std::path::Path;
    use std::sync::Arc;

    use axum::extract::{Path as AxumPath, State};
    use axum::http::StatusCode;
    use axum::response::Json;
    use axum::routing::{delete, post, put};
    use axum::Router;
    use serde::{Deserialize, Serialize};

    use crate::config::BrowserProfile;
    use crate::web::server::AppState;

    #[derive(Serialize)]
    struct BrowserTestResponse {
        success: bool,
        message: String,
    }

    #[derive(Serialize)]
    struct ProfileActionResponse {
        success: bool,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        fixed_count: Option<u64>,
    }

    #[derive(Serialize)]
    struct ProfileInfo {
        name: String,
        display_name: String,
        description: Option<String>,
        path: String,
        exists: bool,
        size_bytes: u64,
        wrong_owner_count: u64,
        is_default: bool,
    }

    /// Test if browser can be launched
    async fn test_browser(State(state): State<Arc<AppState>>) -> Json<BrowserTestResponse> {
        let config = state.config.read().await;
        let status = config.browser.runtime_status();
        if !status.available {
            return Json(BrowserTestResponse {
                success: false,
                message: status.reason.unwrap_or_else(|| {
                    "Browser automation is unavailable in the current configuration".to_string()
                }),
            });
        }

        let exe_info = status
            .executable_path
            .map(|p| format!(" (Chrome: {})", p))
            .unwrap_or_default();
        Json(BrowserTestResponse {
            success: true,
            message: format!(
                "Browser prerequisites OK. MCP server (@playwright/mcp) will start on first use{}.",
                exe_info
            ),
        })
    }

    /// List profiles with health info (existence, size, ownership issues).
    async fn list_profiles(State(state): State<Arc<AppState>>) -> Json<Vec<ProfileInfo>> {
        let config = state.config.read().await;
        let uid = current_uid();
        let mut result = Vec::new();

        for (key, profile) in &config.browser.profiles {
            let path = config.browser.profile_user_data_path(key);
            let exists = path.exists();
            let (size_bytes, wrong_owner_count) = if exists {
                dir_stats(&path, uid)
            } else {
                (0, 0)
            };

            result.push(ProfileInfo {
                name: key.clone(),
                display_name: profile.name.clone(),
                description: profile.description.clone(),
                path: path.display().to_string(),
                exists,
                size_bytes,
                wrong_owner_count,
                is_default: *key == config.browser.default_profile,
            });
        }

        Json(result)
    }

    /// Fix ownership/permissions on a browser profile directory.
    async fn fix_profile_permissions(
        State(state): State<Arc<AppState>>,
        AxumPath(profile_name): AxumPath<String>,
    ) -> Json<ProfileActionResponse> {
        let config = state.config.read().await;
        let profile_path = config.browser.profile_user_data_path(&profile_name);

        if !profile_path.exists() {
            return Json(ProfileActionResponse {
                success: false,
                message: format!(
                    "Profile directory does not exist: {}",
                    profile_path.display()
                ),
                fixed_count: None,
            });
        }

        let uid = current_uid();
        let (_, wrong_count) = dir_stats(&profile_path, uid);

        if wrong_count == 0 {
            return Json(ProfileActionResponse {
                success: true,
                message: "All files already have correct ownership.".to_string(),
                fixed_count: Some(0),
            });
        }

        let output = tokio::process::Command::new("chown")
            .args(["-R", &format!("{}:{}", whoami::username(), "staff")])
            .arg(&profile_path)
            .output()
            .await;

        match output {
            Ok(out) if out.status.success() => Json(ProfileActionResponse {
                success: true,
                message: format!(
                    "Fixed ownership on {} files in {}.",
                    wrong_count,
                    profile_path.display()
                ),
                fixed_count: Some(wrong_count),
            }),
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                Json(ProfileActionResponse {
                    success: false,
                    message: format!("chown failed (may need sudo): {}", stderr.trim()),
                    fixed_count: None,
                })
            }
            Err(e) => Json(ProfileActionResponse {
                success: false,
                message: format!("Failed to run chown: {e}"),
                fixed_count: None,
            }),
        }
    }

    /// Delete a browser profile's user-data directory entirely (clean slate).
    async fn delete_profile_data(
        State(state): State<Arc<AppState>>,
        AxumPath(profile_name): AxumPath<String>,
    ) -> Json<ProfileActionResponse> {
        let config = state.config.read().await;
        let profile_path = config.browser.profile_user_data_path(&profile_name);

        if !profile_path.exists() {
            return Json(ProfileActionResponse {
                success: true,
                message: "Profile directory already does not exist.".to_string(),
                fixed_count: None,
            });
        }

        let output = tokio::process::Command::new("rm")
            .args(["-rf"])
            .arg(&profile_path)
            .output()
            .await;

        match output {
            Ok(out) if out.status.success() => Json(ProfileActionResponse {
                success: true,
                message: format!("Profile data deleted: {}", profile_path.display()),
                fixed_count: None,
            }),
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                Json(ProfileActionResponse {
                    success: false,
                    message: format!(
                        "Could not delete all files (some may be root-owned). \
                         Run manually: sudo rm -rf '{}'. Error: {}",
                        profile_path.display(),
                        stderr.trim()
                    ),
                    fixed_count: None,
                })
            }
            Err(e) => Json(ProfileActionResponse {
                success: false,
                message: format!("Failed to run rm: {e}"),
                fixed_count: None,
            }),
        }
    }

    /// Get current process UID by reading metadata of the executable.
    #[cfg(unix)]
    fn current_uid() -> u32 {
        std::env::current_exe()
            .ok()
            .and_then(|p| std::fs::metadata(p).ok())
            .map(|m| m.uid())
            .unwrap_or(501)
    }

    #[cfg(not(unix))]
    fn current_uid() -> u32 {
        0 // Ownership checks not applicable on Windows
    }

    /// Walk a directory recursively and return (total_size, wrong_owner_count).
    fn dir_stats(path: &Path, _expected_uid: u32) -> (u64, u64) {
        let mut size = 0u64;
        let mut wrong = 0u64;
        walk_dir(path, &mut |meta| {
            if meta.is_file() {
                size += meta.len();
            }
            #[cfg(unix)]
            if meta.uid() != _expected_uid {
                wrong += 1;
            }
        });
        (size, wrong)
    }

    /// Simple recursive directory walker (no external crate needed).
    fn walk_dir(path: &Path, cb: &mut dyn FnMut(&std::fs::Metadata)) {
        let entries = match std::fs::read_dir(path) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                cb(&meta);
                if meta.is_dir() {
                    walk_dir(&entry.path(), cb);
                }
            }
        }
    }

    // ─── Profile CRUD ─────────────────────────────────────────────

    #[derive(Deserialize)]
    struct CreateProfileRequest {
        key: String,
        name: String,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        browser_type: Option<String>,
        #[serde(default)]
        headless: Option<bool>,
        #[serde(default)]
        proxy: Option<String>,
        #[serde(default)]
        user_agent: Option<String>,
    }

    #[derive(Deserialize)]
    struct UpdateProfileRequest {
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        browser_type: Option<String>,
        #[serde(default)]
        headless: Option<bool>,
        #[serde(default)]
        proxy: Option<String>,
        #[serde(default)]
        user_agent: Option<String>,
    }

    /// Create a new browser profile.
    async fn create_profile(
        State(state): State<Arc<AppState>>,
        Json(req): Json<CreateProfileRequest>,
    ) -> Result<Json<ProfileActionResponse>, StatusCode> {
        // Validate key: kebab-case, non-empty
        if req.key.is_empty() || !req.key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Ok(Json(ProfileActionResponse {
                success: false,
                message: "Key must be non-empty kebab-case (a-z, 0-9, hyphens)".to_string(),
                fixed_count: None,
            }));
        }

        let mut config = state.config.write().await;
        if config.browser.profiles.contains_key(&req.key) {
            return Ok(Json(ProfileActionResponse {
                success: false,
                message: format!("Profile '{}' already exists", req.key),
                fixed_count: None,
            }));
        }

        config.browser.profiles.insert(
            req.key.clone(),
            BrowserProfile {
                name: req.name,
                description: req.description,
                browser_type: req.browser_type,
                headless: req.headless,
                proxy: req.proxy,
                user_agent: req.user_agent,
                ..Default::default()
            },
        );

        if let Err(e) = config.save() {
            tracing::error!("Failed to save config after profile create: {e}");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }

        Ok(Json(ProfileActionResponse {
            success: true,
            message: format!("Profile '{}' created", req.key),
            fixed_count: None,
        }))
    }

    /// Update an existing browser profile.
    async fn update_profile(
        State(state): State<Arc<AppState>>,
        AxumPath(profile_key): AxumPath<String>,
        Json(req): Json<UpdateProfileRequest>,
    ) -> Result<Json<ProfileActionResponse>, StatusCode> {
        let mut config = state.config.write().await;
        let profile = match config.browser.profiles.get_mut(&profile_key) {
            Some(p) => p,
            None => {
                return Ok(Json(ProfileActionResponse {
                    success: false,
                    message: format!("Profile '{}' not found", profile_key),
                    fixed_count: None,
                }));
            }
        };

        if let Some(name) = req.name {
            profile.name = name;
        }
        if let Some(desc) = req.description {
            profile.description = Some(desc);
        }
        if let Some(bt) = req.browser_type {
            profile.browser_type = if bt.is_empty() { None } else { Some(bt) };
        }
        if let Some(h) = req.headless {
            profile.headless = Some(h);
        }
        if let Some(proxy) = req.proxy {
            profile.proxy = if proxy.is_empty() { None } else { Some(proxy) };
        }
        if let Some(ua) = req.user_agent {
            profile.user_agent = if ua.is_empty() { None } else { Some(ua) };
        }

        if let Err(e) = config.save() {
            tracing::error!("Failed to save config after profile update: {e}");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }

        Ok(Json(ProfileActionResponse {
            success: true,
            message: format!("Profile '{}' updated", profile_key),
            fixed_count: None,
        }))
    }

    /// Set a profile as the default.
    async fn set_default_profile(
        State(state): State<Arc<AppState>>,
        AxumPath(profile_key): AxumPath<String>,
    ) -> Result<Json<ProfileActionResponse>, StatusCode> {
        let mut config = state.config.write().await;
        if !config.browser.profiles.contains_key(&profile_key) {
            return Ok(Json(ProfileActionResponse {
                success: false,
                message: format!("Profile '{}' not found", profile_key),
                fixed_count: None,
            }));
        }

        config.browser.default_profile = profile_key.clone();

        if let Err(e) = config.save() {
            tracing::error!("Failed to save config after set-default: {e}");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }

        Ok(Json(ProfileActionResponse {
            success: true,
            message: format!("'{}' is now the default profile", profile_key),
            fixed_count: None,
        }))
    }

    /// Delete a browser profile from config (and optionally its data directory).
    async fn delete_profile(
        State(state): State<Arc<AppState>>,
        AxumPath(profile_key): AxumPath<String>,
    ) -> Result<Json<ProfileActionResponse>, StatusCode> {
        let mut config = state.config.write().await;

        if profile_key == config.browser.default_profile {
            return Ok(Json(ProfileActionResponse {
                success: false,
                message: "Cannot delete the default profile. Set another as default first."
                    .to_string(),
                fixed_count: None,
            }));
        }

        if config.browser.profiles.remove(&profile_key).is_none() {
            return Ok(Json(ProfileActionResponse {
                success: false,
                message: format!("Profile '{}' not found", profile_key),
                fixed_count: None,
            }));
        }

        // Also remove the user data directory if it exists
        let profile_path = config.browser.profile_user_data_path(&profile_key);
        if profile_path.exists() {
            let _ = tokio::fs::remove_dir_all(&profile_path).await;
        }

        if let Err(e) = config.save() {
            tracing::error!("Failed to save config after profile delete: {e}");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }

        Ok(Json(ProfileActionResponse {
            success: true,
            message: format!("Profile '{}' deleted", profile_key),
            fixed_count: None,
        }))
    }

    pub(crate) fn routes() -> Router<Arc<AppState>> {
        Router::new()
            .route("/v1/browser/test", post(test_browser))
            .route(
                "/v1/browser/profiles",
                axum::routing::get(list_profiles).post(create_profile),
            )
            .route(
                "/v1/browser/profiles/{name}",
                put(update_profile).delete(delete_profile_data),
            )
            .route(
                "/v1/browser/profiles/{name}/fix-permissions",
                post(fix_profile_permissions),
            )
            .route(
                "/v1/browser/profiles/{name}/set-default",
                post(set_default_profile),
            )
            .route(
                "/v1/browser/profiles/{name}/delete",
                post(delete_profile),
            )
    }
}

#[cfg(feature = "browser")]
pub(super) use inner::routes;
