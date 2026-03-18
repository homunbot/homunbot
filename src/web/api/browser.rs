#[cfg(feature = "browser")]
mod inner {
    use std::os::unix::fs::MetadataExt;
    use std::path::Path;
    use std::sync::Arc;

    use axum::extract::{Path as AxumPath, State};
    use axum::response::Json;
    use axum::routing::{delete, post};
    use axum::Router;
    use serde::Serialize;

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
                message: format!("Profile directory does not exist: {}", profile_path.display()),
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
    fn current_uid() -> u32 {
        // Read uid from /proc or from our own binary metadata
        std::env::current_exe()
            .ok()
            .and_then(|p| std::fs::metadata(p).ok())
            .map(|m| m.uid())
            .unwrap_or(501) // fallback to typical macOS user uid
    }

    /// Walk a directory recursively and return (total_size, wrong_owner_count).
    fn dir_stats(path: &Path, expected_uid: u32) -> (u64, u64) {
        let mut size = 0u64;
        let mut wrong = 0u64;
        walk_dir(path, &mut |meta| {
            if meta.is_file() {
                size += meta.len();
            }
            if meta.uid() != expected_uid {
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

    pub(crate) fn routes() -> Router<Arc<AppState>> {
        Router::new()
            .route("/v1/browser/test", post(test_browser))
            .route("/v1/browser/profiles", axum::routing::get(list_profiles))
            .route(
                "/v1/browser/profiles/{name}/fix-permissions",
                post(fix_profile_permissions),
            )
            .route(
                "/v1/browser/profiles/{name}",
                delete(delete_profile_data),
            )
    }
}

#[cfg(feature = "browser")]
pub(super) use inner::routes;
