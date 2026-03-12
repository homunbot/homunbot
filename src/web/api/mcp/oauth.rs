use axum::http::StatusCode;
use axum::response::Json;
use serde::{Deserialize, Serialize};

// ── Request / response types ─────────────────────────────────────

#[derive(Debug, Deserialize)]
pub(crate) struct GoogleMcpOauthStartRequest {
    service: String,
    client_id: String,
    redirect_uri: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct GoogleMcpOauthStartResponse {
    ok: bool,
    auth_url: String,
    redirect_uri: String,
    scopes: Vec<String>,
    state: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GoogleMcpOauthExchangeRequest {
    service: String,
    code: String,
    client_id: String,
    client_secret: String,
    redirect_uri: String,
}

#[derive(Debug, Deserialize)]
struct GoogleMcpOauthTokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    scope: Option<String>,
    token_type: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct GoogleMcpOauthExchangeResponse {
    ok: bool,
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    scope: Option<String>,
    token_type: Option<String>,
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GitHubMcpOauthStartRequest {
    service: String,
    client_id: String,
    redirect_uri: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct GitHubMcpOauthStartResponse {
    ok: bool,
    auth_url: String,
    redirect_uri: String,
    scopes: Vec<String>,
    state: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GitHubMcpOauthExchangeRequest {
    service: String,
    code: String,
    client_id: String,
    client_secret: String,
    redirect_uri: String,
}

#[derive(Debug, Deserialize)]
struct GitHubMcpOauthTokenResponse {
    access_token: Option<String>,
    scope: Option<String>,
    token_type: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct GitHubMcpOauthExchangeResponse {
    ok: bool,
    access_token: Option<String>,
    scope: Option<String>,
    token_type: Option<String>,
    message: Option<String>,
}

// ── Scope helpers ────────────────────────────────────────────────

fn google_mcp_scopes(service: &str) -> Option<&'static [&'static str]> {
    let normalized = service.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "gmail" => Some(&["https://www.googleapis.com/auth/gmail.readonly"]),
        "google-calendar" | "gcal" | "calendar" => {
            Some(&["https://www.googleapis.com/auth/calendar"])
        }
        _ => None,
    }
}

fn github_mcp_scopes(service: &str) -> Option<&'static [&'static str]> {
    let normalized = service.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "github" | "gh" => Some(&["repo", "read:org", "read:user"]),
        _ => None,
    }
}

fn build_google_mcp_oauth_url(
    service: &str,
    client_id: &str,
    redirect_uri: &str,
    state: &str,
) -> anyhow::Result<(reqwest::Url, Vec<String>)> {
    let scopes = google_mcp_scopes(service)
        .ok_or_else(|| anyhow::anyhow!("unsupported Google MCP OAuth service"))?
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    let mut url = reqwest::Url::parse("https://accounts.google.com/o/oauth2/v2/auth")?;
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("client_id", client_id.trim());
        qp.append_pair("redirect_uri", redirect_uri.trim());
        qp.append_pair("response_type", "code");
        qp.append_pair("scope", &scopes.join(" "));
        qp.append_pair("access_type", "offline");
        qp.append_pair("prompt", "consent");
        qp.append_pair("include_granted_scopes", "true");
        qp.append_pair("state", state);
    }
    Ok((url, scopes))
}

fn build_github_mcp_oauth_url(
    service: &str,
    client_id: &str,
    redirect_uri: &str,
    state: &str,
) -> anyhow::Result<(reqwest::Url, Vec<String>)> {
    let scopes = github_mcp_scopes(service)
        .ok_or_else(|| anyhow::anyhow!("unsupported GitHub MCP OAuth service"))?
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    let mut url = reqwest::Url::parse("https://github.com/login/oauth/authorize")?;
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("client_id", client_id.trim());
        qp.append_pair("redirect_uri", redirect_uri.trim());
        qp.append_pair("scope", &scopes.join(" "));
        qp.append_pair("state", state);
    }
    Ok((url, scopes))
}

// ── Google OAuth handlers ────────────────────────────────────────

pub(super) async fn start_google_mcp_oauth(
    Json(req): Json<GoogleMcpOauthStartRequest>,
) -> Result<Json<GoogleMcpOauthStartResponse>, (StatusCode, Json<serde_json::Value>)> {
    if req.client_id.trim().is_empty() || req.redirect_uri.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "client_id and redirect_uri are required"
            })),
        ));
    }

    let state = uuid::Uuid::new_v4().to_string();
    let (auth_url, scopes) =
        build_google_mcp_oauth_url(&req.service, &req.client_id, &req.redirect_uri, &state)
            .map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "error": e.to_string() })),
                )
            })?;

    Ok(Json(GoogleMcpOauthStartResponse {
        ok: true,
        auth_url: auth_url.to_string(),
        redirect_uri: req.redirect_uri.trim().to_string(),
        scopes,
        state,
    }))
}

pub(super) async fn exchange_google_mcp_oauth_code(
    Json(req): Json<GoogleMcpOauthExchangeRequest>,
) -> Result<Json<GoogleMcpOauthExchangeResponse>, (StatusCode, Json<serde_json::Value>)> {
    if req.client_id.trim().is_empty()
        || req.client_secret.trim().is_empty()
        || req.code.trim().is_empty()
        || req.redirect_uri.trim().is_empty()
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "service, code, client_id, client_secret, and redirect_uri are required"
            })),
        ));
    }

    if google_mcp_scopes(&req.service).is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("Unsupported Google MCP OAuth service: {}", req.service)
            })),
        ));
    }

    let client = reqwest::Client::builder()
        .use_rustls_tls()
        .build()
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
        })?;

    let response = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("code", req.code.trim()),
            ("client_id", req.client_id.trim()),
            ("client_secret", req.client_secret.trim()),
            ("redirect_uri", req.redirect_uri.trim()),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
        })?;

    let status = response.status();
    let body = response
        .json::<GoogleMcpOauthTokenResponse>()
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
        })?;

    if !status.is_success() {
        let detail = body
            .error_description
            .clone()
            .or(body.error.clone())
            .unwrap_or_else(|| "Google OAuth token exchange failed".to_string());
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({ "error": detail })),
        ));
    }

    let message = if body.refresh_token.as_deref().unwrap_or_default().is_empty() {
        Some(
            "Token exchange succeeded, but Google did not return a refresh token. Retry consent with prompt=consent and offline access."
                .to_string(),
        )
    } else {
        Some("Google OAuth token exchange succeeded.".to_string())
    };

    Ok(Json(GoogleMcpOauthExchangeResponse {
        ok: true,
        access_token: body.access_token,
        refresh_token: body.refresh_token,
        expires_in: body.expires_in,
        scope: body.scope,
        token_type: body.token_type,
        message,
    }))
}

// ── GitHub OAuth handlers ────────────────────────────────────────

pub(super) async fn start_github_mcp_oauth(
    Json(req): Json<GitHubMcpOauthStartRequest>,
) -> Result<Json<GitHubMcpOauthStartResponse>, (StatusCode, Json<serde_json::Value>)> {
    if req.client_id.trim().is_empty() || req.redirect_uri.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "client_id and redirect_uri are required"
            })),
        ));
    }

    let state = uuid::Uuid::new_v4().to_string();
    let (auth_url, scopes) =
        build_github_mcp_oauth_url(&req.service, &req.client_id, &req.redirect_uri, &state)
            .map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "error": e.to_string() })),
                )
            })?;

    Ok(Json(GitHubMcpOauthStartResponse {
        ok: true,
        auth_url: auth_url.to_string(),
        redirect_uri: req.redirect_uri.trim().to_string(),
        scopes,
        state,
    }))
}

pub(super) async fn exchange_github_mcp_oauth_code(
    Json(req): Json<GitHubMcpOauthExchangeRequest>,
) -> Result<Json<GitHubMcpOauthExchangeResponse>, (StatusCode, Json<serde_json::Value>)> {
    if req.client_id.trim().is_empty()
        || req.client_secret.trim().is_empty()
        || req.code.trim().is_empty()
        || req.redirect_uri.trim().is_empty()
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "service, code, client_id, client_secret, and redirect_uri are required"
            })),
        ));
    }

    if github_mcp_scopes(&req.service).is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("Unsupported GitHub MCP OAuth service: {}", req.service)
            })),
        ));
    }

    let client = reqwest::Client::builder()
        .use_rustls_tls()
        .build()
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
        })?;

    let response = client
        .post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .form(&[
            ("client_id", req.client_id.trim()),
            ("client_secret", req.client_secret.trim()),
            ("code", req.code.trim()),
            ("redirect_uri", req.redirect_uri.trim()),
        ])
        .send()
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
        })?;

    let status = response.status();
    let body = response
        .json::<GitHubMcpOauthTokenResponse>()
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
        })?;

    if !status.is_success() || body.access_token.is_none() {
        let detail = body
            .error_description
            .clone()
            .or(body.error.clone())
            .unwrap_or_else(|| "GitHub OAuth token exchange failed".to_string());
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({ "error": detail })),
        ));
    }

    Ok(Json(GitHubMcpOauthExchangeResponse {
        ok: true,
        access_token: body.access_token,
        scope: body.scope,
        token_type: body.token_type,
        message: Some("GitHub OAuth token exchange succeeded.".to_string()),
    }))
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod google_oauth_tests {
    use super::{
        build_github_mcp_oauth_url, build_google_mcp_oauth_url, github_mcp_scopes,
        google_mcp_scopes,
    };

    #[test]
    fn google_oauth_scopes_support_known_services() {
        assert_eq!(
            google_mcp_scopes("gmail"),
            Some(&["https://www.googleapis.com/auth/gmail.readonly"][..])
        );
        assert_eq!(
            google_mcp_scopes("google-calendar"),
            Some(&["https://www.googleapis.com/auth/calendar"][..])
        );
        assert!(google_mcp_scopes("github").is_none());
    }

    #[test]
    fn build_google_oauth_url_contains_offline_access_flags() {
        let (url, scopes) = build_google_mcp_oauth_url(
            "gmail",
            "client-123",
            "http://localhost:8080/mcp/oauth/google/callback",
            "state-xyz",
        )
        .expect("oauth url");
        let rendered = url.as_str().to_string();
        assert_eq!(
            scopes,
            vec!["https://www.googleapis.com/auth/gmail.readonly"]
        );
        assert!(rendered.contains("access_type=offline"));
        assert!(rendered.contains("prompt=consent"));
        assert!(rendered.contains("state=state-xyz"));
    }

    #[test]
    fn github_oauth_scopes_support_github_service() {
        assert_eq!(
            github_mcp_scopes("github"),
            Some(&["repo", "read:org", "read:user"][..])
        );
        assert!(github_mcp_scopes("gmail").is_none());
    }

    #[test]
    fn build_github_oauth_url_contains_state_and_scope() {
        let (url, scopes) = build_github_mcp_oauth_url(
            "github",
            "gh-client",
            "http://localhost:8080/mcp/oauth/github/callback",
            "state-123",
        )
        .expect("oauth url");
        let rendered = url.as_str().to_string();
        assert_eq!(scopes, vec!["repo", "read:org", "read:user"]);
        assert!(rendered.contains("client_id=gh-client"));
        assert!(rendered.contains("state=state-123"));
        assert!(rendered.contains("scope=repo+read%3Aorg+read%3Auser"));
    }
}
