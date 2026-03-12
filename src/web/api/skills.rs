use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::get;
use axum::Router;
use serde::{Deserialize, Serialize};

use super::super::server::AppState;

pub(super) fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/skills", get(list_skills))
        .route("/v1/skills/audit", get(list_skill_audits))
        .route("/v1/skills/search", get(search_skills))
        .route("/v1/skills/install", axum::routing::post(install_skill))
        .route("/v1/skills/create", axum::routing::post(create_skill_api))
        .route(
            "/v1/skills/{name}",
            get(get_skill_detail).delete(delete_skill),
        )
        .route(
            "/v1/skills/{name}/scan",
            axum::routing::post(scan_skill_api),
        )
        .route("/v1/skills/catalog/status", get(catalog_status))
        .route("/v1/skills/catalog/counts", get(catalog_counts))
        .route(
            "/v1/skills/catalog/refresh",
            axum::routing::post(catalog_refresh),
        )
}

#[derive(Serialize)]
struct SkillView {
    name: String,
    description: String,
    path: String,
    source: String,
}

/// Detect the source of an installed skill by checking marker files.
fn detect_skill_source(path: &std::path::Path) -> String {
    if path.join(".clawhub-source").exists() {
        "clawhub".to_string()
    } else if path.join(".openskills-source").exists() {
        "openskills".to_string()
    } else {
        "github".to_string()
    }
}

async fn list_skills() -> Json<Vec<SkillView>> {
    let skills = crate::skills::SkillInstaller::list_installed()
        .await
        .unwrap_or_default();

    Json(
        skills
            .into_iter()
            .map(|s| {
                let source = detect_skill_source(&s.path);
                SkillView {
                    name: s.name,
                    description: s.description,
                    path: s.path.display().to_string(),
                    source,
                }
            })
            .collect(),
    )
}

/// SKL-6: List recent skill audit entries.
async fn list_skill_audits(
    State(state): State<Arc<AppState>>,
    Query(params): Query<AuditQueryParams>,
) -> Json<Vec<SkillAuditView>> {
    let limit = params.limit.unwrap_or(50).min(200);
    let rows = if let Some(ref db) = state.db {
        db.list_skill_audits(limit).await.unwrap_or_default()
    } else {
        Vec::new()
    };
    Json(
        rows.into_iter()
            .map(|r| SkillAuditView {
                id: r.id,
                timestamp: r.timestamp,
                skill_name: r.skill_name,
                channel: r.channel,
                query: r.query,
                activation_type: r.activation_type,
            })
            .collect(),
    )
}

#[derive(Deserialize)]
struct AuditQueryParams {
    limit: Option<i64>,
}

#[derive(Serialize)]
struct SkillAuditView {
    id: i64,
    timestamp: String,
    skill_name: String,
    channel: String,
    query: Option<String>,
    activation_type: String,
}

#[derive(Deserialize)]
struct InstallRequest {
    source: String,
    #[serde(default)]
    force: bool,
}

#[derive(Serialize)]
struct InstallResponse {
    ok: bool,
    name: String,
    message: String,
    security_report: Option<InstallSecurityReportView>,
}

#[derive(Serialize)]
struct InstallSecurityReportView {
    risk_score: u8,
    blocked: bool,
    warnings: usize,
    scanned_files: usize,
    summary: String,
}

fn install_security_view(
    report: Option<&crate::skills::SecurityReport>,
) -> Option<InstallSecurityReportView> {
    report.map(|report| InstallSecurityReportView {
        risk_score: report.risk_score,
        blocked: report.blocked,
        warnings: report.warnings.len(),
        scanned_files: report.scanned_files,
        summary: report.summary(),
    })
}

async fn install_skill(
    Json(req): Json<InstallRequest>,
) -> Result<Json<InstallResponse>, StatusCode> {
    let security_options = crate::skills::InstallSecurityOptions { force: req.force };
    let result = if let Some(slug) = req.source.strip_prefix("clawhub:") {
        let hub = crate::skills::ClawHubInstaller::new();
        hub.install_with_options(slug, security_options.clone())
            .await
    } else if let Some(dir_name) = req.source.strip_prefix("openskills:") {
        let source = crate::skills::OpenSkillsSource::new();
        source
            .install_with_options(dir_name, security_options.clone())
            .await
    } else {
        let installer = crate::skills::SkillInstaller::new();
        installer
            .install_with_options(&req.source, security_options)
            .await
    };

    match result {
        Ok(r) => Ok(Json(InstallResponse {
            ok: true,
            name: r.name,
            message: r.description,
            security_report: install_security_view(r.security_report.as_ref()),
        })),
        Err(e) => Ok(Json(InstallResponse {
            ok: false,
            name: String::new(),
            message: e.to_string(),
            security_report: None,
        })),
    }
}

// --- Create skill ---

#[derive(Deserialize)]
struct CreateSkillRequest {
    prompt: String,
    name: Option<String>,
    language: Option<String>,
    #[serde(default)]
    overwrite: bool,
}

#[derive(Serialize)]
struct CreateSkillResponse {
    ok: bool,
    name: String,
    path: String,
    language: String,
    reused_skills: Vec<String>,
    smoke_test_passed: bool,
    validation_notes: Vec<String>,
    message: String,
    security_report: Option<SecurityReportDetailView>,
}

#[derive(Serialize)]
struct SecurityReportDetailView {
    risk_score: u8,
    blocked: bool,
    scanned_files: usize,
    summary: String,
    warnings: Vec<SecurityWarningView>,
}

#[derive(Serialize)]
struct SecurityWarningView {
    severity: String,
    category: String,
    description: String,
    file: Option<String>,
    line: Option<usize>,
}

fn security_detail_view(report: &crate::skills::SecurityReport) -> SecurityReportDetailView {
    SecurityReportDetailView {
        risk_score: report.risk_score,
        blocked: report.blocked,
        scanned_files: report.scanned_files,
        summary: report.summary(),
        warnings: report
            .warnings
            .iter()
            .map(|w| SecurityWarningView {
                severity: format!("{:?}", w.severity),
                category: format!("{:?}", w.category),
                description: w.description.clone(),
                file: w.file.clone(),
                line: w.line,
            })
            .collect(),
    }
}

async fn create_skill_api(
    Json(req): Json<CreateSkillRequest>,
) -> Result<Json<CreateSkillResponse>, StatusCode> {
    let request = crate::skills::SkillCreationRequest {
        prompt: req.prompt,
        name: req.name,
        language: req.language,
        overwrite: req.overwrite,
    };

    match crate::skills::create_skill(request).await {
        Ok(result) => Ok(Json(CreateSkillResponse {
            ok: true,
            name: result.name,
            path: result.path.display().to_string(),
            language: result.script_language,
            reused_skills: result.reused_skills,
            smoke_test_passed: result.smoke_test_passed,
            validation_notes: result.validation_notes,
            message: String::new(),
            security_report: Some(security_detail_view(&result.security_report)),
        })),
        Err(e) => Ok(Json(CreateSkillResponse {
            ok: false,
            name: String::new(),
            path: String::new(),
            language: String::new(),
            reused_skills: vec![],
            smoke_test_passed: false,
            validation_notes: vec![],
            message: e.to_string(),
            security_report: None,
        })),
    }
}

// --- Scan skill ---

async fn scan_skill_api(Path(name): Path<String>) -> Result<Json<serde_json::Value>, StatusCode> {
    let skills_dir = dirs::home_dir()
        .unwrap_or_default()
        .join(".homun")
        .join("skills");
    let skill_dir = skills_dir.join(&name);

    if !skill_dir.exists() {
        return Ok(Json(serde_json::json!({
            "ok": false,
            "message": format!("Skill '{}' not found", name),
        })));
    }

    match crate::skills::scan_skill_package(&skill_dir).await {
        Ok(report) => Ok(Json(serde_json::json!({
            "ok": true,
            "report": security_detail_view(&report),
        }))),
        Err(e) => Ok(Json(serde_json::json!({
            "ok": false,
            "message": e.to_string(),
        }))),
    }
}

// --- Search skills ---

#[derive(Deserialize)]
struct SkillSearchQuery {
    q: String,
}

#[derive(Serialize)]
struct SkillSearchResultView {
    name: String,
    description: String,
    source: String,
    downloads: u64,
    stars: u64,
    recommended: bool,
    recommended_reason: Option<String>,
    decision_tags: Vec<String>,
    why_choose: Option<String>,
    tradeoff: Option<String>,
}

fn skill_query_terms(query: &str) -> Vec<String> {
    query
        .to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|part| part.len() >= 2)
        .map(ToString::to_string)
        .collect::<Vec<_>>()
}

fn skill_searchable_text(skill: &SkillSearchResultView) -> String {
    format!("{} {}", skill.name, skill.description).to_ascii_lowercase()
}

fn skill_recommendation_score(skill: &SkillSearchResultView, query: &str) -> i64 {
    let query_lower = query.trim().to_ascii_lowercase();
    let searchable = skill_searchable_text(skill);
    let mut score = 0i64;

    if !query_lower.is_empty() && searchable.contains(&query_lower) {
        score += 80;
    }
    for term in skill_query_terms(query) {
        if searchable.contains(&term) {
            score += 18;
        }
    }

    score += match skill.source.as_str() {
        "clawhub" => 40,
        "openskills" => 24,
        "github" => 12,
        _ => 0,
    };
    score += (skill.stars.min(5000) / 50) as i64;
    score += (skill.downloads.min(500_000) / 5_000) as i64;
    if skill.description.to_ascii_lowercase().contains("agent")
        || skill.description.to_ascii_lowercase().contains("workflow")
    {
        score += 6;
    }

    score
}

fn skill_recommended_reason(skill: &SkillSearchResultView) -> String {
    let mut reasons = Vec::new();
    match skill.source.as_str() {
        "clawhub" => reasons.push("curated in ClawHub".to_string()),
        "openskills" => reasons.push("community-curated in Open Skills".to_string()),
        "github" => reasons.push("direct GitHub source".to_string()),
        _ => {}
    }
    if skill.downloads > 0 {
        reasons.push(format!("{} downloads", skill.downloads));
    }
    if skill.stars > 0 {
        reasons.push(format!("{} GitHub stars", skill.stars));
    }
    if reasons.is_empty() {
        "best overall match for this search".to_string()
    } else {
        reasons.truncate(3);
        reasons.join(", ")
    }
}

fn skill_decision_tags(skill: &SkillSearchResultView) -> Vec<String> {
    let mut tags = Vec::new();
    match skill.source.as_str() {
        "clawhub" => tags.push("Curated".to_string()),
        "openskills" => tags.push("Open Skills".to_string()),
        "github" => tags.push("GitHub".to_string()),
        _ => {}
    }
    if skill.downloads >= 1_000 {
        tags.push("Popular".to_string());
    }
    if skill.stars >= 100 {
        tags.push("High signal".to_string());
    }
    tags.truncate(4);
    tags
}

fn skill_why_choose(skill: &SkillSearchResultView) -> String {
    match skill.source.as_str() {
        "clawhub" => {
            "Choose this if you want the safest default pick from a curated catalog.".to_string()
        }
        "openskills" => {
            "Choose this if you want a community-curated option with a cleaner install path."
                .to_string()
        }
        "github" => {
            "Choose this if you want the original repository or the broadest ecosystem coverage."
                .to_string()
        }
        _ => "Choose this if it matches your use case better than the default option.".to_string(),
    }
}

fn skill_tradeoff(skill: &SkillSearchResultView) -> String {
    match skill.source.as_str() {
        "clawhub" => {
            "Tradeoff: more opinionated curation, so niche variants may be missing.".to_string()
        }
        "openskills" => {
            "Tradeoff: quality varies by contributor and popularity signals may be weaker."
                .to_string()
        }
        "github" => {
            "Tradeoff: less curated, so install quality and maintenance can vary more.".to_string()
        }
        _ => "Tradeoff: not the clearest default option for a non-technical user.".to_string(),
    }
}

fn annotate_skill_search_results(results: &mut [SkillSearchResultView], query: &str) {
    if results.is_empty() {
        return;
    }

    for item in results.iter_mut() {
        item.recommended = false;
        item.recommended_reason = None;
        item.decision_tags = skill_decision_tags(item);
        item.why_choose = Some(skill_why_choose(item));
        item.tradeoff = Some(skill_tradeoff(item));
    }

    results.sort_by(|a, b| {
        skill_recommendation_score(b, query)
            .cmp(&skill_recommendation_score(a, query))
            .then_with(|| b.downloads.cmp(&a.downloads))
            .then_with(|| b.stars.cmp(&a.stars))
            .then_with(|| a.name.cmp(&b.name))
    });

    if let Some(first) = results.first_mut() {
        first.recommended = true;
        first.recommended_reason = Some(skill_recommended_reason(first));
        if !first.decision_tags.iter().any(|tag| tag == "Recommended") {
            first.decision_tags.insert(0, "Recommended".to_string());
        }
    }
}

async fn search_skills(Query(params): Query<SkillSearchQuery>) -> Json<Vec<SkillSearchResultView>> {
    let query = params.q.trim().to_string();
    if query.len() < 2 {
        return Json(Vec::new());
    }

    let query_ch = query.clone();
    let query_gh = query.clone();
    let query_os = query.clone();

    // Search ClawHub, GitHub, and Open Skills in parallel
    let (ch_result, gh_result, os_result) = tokio::join!(
        async {
            let installer = crate::skills::ClawHubInstaller::new();
            installer.search(&query_ch, 10).await
        },
        async {
            let searcher = crate::skills::search::SkillSearcher::new();
            searcher.search(&query_gh, 10).await
        },
        async {
            let source = crate::skills::OpenSkillsSource::new();
            source.search(&query_os, 10).await
        }
    );

    let mut results: Vec<SkillSearchResultView> = Vec::new();

    // ClawHub results first (curated registry)
    match ch_result {
        Ok(items) => {
            results.extend(items.into_iter().map(|r| SkillSearchResultView {
                name: format!("clawhub:{}", r.slug),
                description: r.description,
                source: "clawhub".to_string(),
                downloads: r.downloads,
                stars: r.stars,
                recommended: false,
                recommended_reason: None,
                decision_tags: vec![],
                why_choose: None,
                tradeoff: None,
            }));
        }
        Err(e) => {
            tracing::warn!(error = %e, "ClawHub search failed, skipping");
        }
    }

    // Open Skills results (community curated)
    match os_result {
        Ok(items) => {
            results.extend(items.into_iter().map(|r| SkillSearchResultView {
                name: r.source,
                description: r.description,
                source: "openskills".to_string(),
                downloads: 0,
                stars: 0,
                recommended: false,
                recommended_reason: None,
                decision_tags: vec![],
                why_choose: None,
                tradeoff: None,
            }));
        }
        Err(e) => {
            tracing::warn!(error = %e, "Open Skills search failed, skipping");
        }
    }

    // GitHub results
    match gh_result {
        Ok(items) => {
            results.extend(items.into_iter().map(|r| SkillSearchResultView {
                name: r.full_name,
                description: r.description,
                source: "github".to_string(),
                downloads: 0,
                stars: r.stars as u64,
                recommended: false,
                recommended_reason: None,
                decision_tags: vec![],
                why_choose: None,
                tradeoff: None,
            }));
        }
        Err(e) => {
            tracing::warn!(error = %e, "GitHub skill search failed, skipping");
        }
    }

    annotate_skill_search_results(&mut results, &query);
    Json(results)
}

// --- Delete skill ---

#[derive(Serialize)]
struct DeleteSkillResponse {
    ok: bool,
    message: String,
}

async fn delete_skill(
    Path(name): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Json<DeleteSkillResponse> {
    match crate::skills::SkillInstaller::remove(&name).await {
        Ok(()) => {
            let mut message = format!("Skill '{}' removed", name);
            if let Some(db) = &state.db {
                let reason = format!("Missing skill dependency: {name}");
                match db
                    .invalidate_automations_by_dependency("skill", &name, &reason)
                    .await
                {
                    Ok(affected) if affected > 0 => {
                        message = format!("{message}. Invalidated {affected} automation(s).");
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            skill = %name,
                            "Failed to invalidate dependent automations after skill removal"
                        );
                    }
                }
            }
            Json(DeleteSkillResponse { ok: true, message })
        }
        Err(e) => Json(DeleteSkillResponse {
            ok: false,
            message: e.to_string(),
        }),
    }
}

// --- Catalog cache ---

#[derive(Serialize)]
struct CatalogStatusResponse {
    cached: bool,
    stale: bool,
    skill_count: usize,
    age_secs: u64,
}

async fn catalog_status() -> Json<CatalogStatusResponse> {
    let cache_path = dirs::home_dir()
        .unwrap_or_default()
        .join(".homun")
        .join("clawhub-catalog.json");

    if !cache_path.exists() {
        return Json(CatalogStatusResponse {
            cached: false,
            stale: true,
            skill_count: 0,
            age_secs: 0,
        });
    }

    // Read and parse the cache to get metadata
    match tokio::fs::read_to_string(&cache_path).await {
        Ok(content) => {
            #[derive(Deserialize)]
            struct Cache {
                fetched_at: u64,
                entries: Vec<serde_json::Value>,
            }
            match serde_json::from_str::<Cache>(&content) {
                Ok(cache) => {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    let age = now.saturating_sub(cache.fetched_at);
                    Json(CatalogStatusResponse {
                        cached: true,
                        stale: age > 6 * 3600,
                        skill_count: cache.entries.len(),
                        age_secs: age,
                    })
                }
                Err(_) => Json(CatalogStatusResponse {
                    cached: false,
                    stale: true,
                    skill_count: 0,
                    age_secs: 0,
                }),
            }
        }
        Err(_) => Json(CatalogStatusResponse {
            cached: false,
            stale: true,
            skill_count: 0,
            age_secs: 0,
        }),
    }
}

#[derive(Serialize)]
struct CatalogRefreshResponse {
    ok: bool,
    skill_count: usize,
    message: String,
}

async fn catalog_refresh() -> Json<CatalogRefreshResponse> {
    let installer = crate::skills::ClawHubInstaller::new();
    match installer.refresh_catalog_cache().await {
        Ok(()) => {
            // Read back the count from cache
            let cache_path = dirs::home_dir()
                .unwrap_or_default()
                .join(".homun")
                .join("clawhub-catalog.json");
            let count = tokio::fs::read_to_string(&cache_path)
                .await
                .ok()
                .and_then(|c| {
                    #[derive(Deserialize)]
                    struct Cache {
                        entries: Vec<serde_json::Value>,
                    }
                    serde_json::from_str::<Cache>(&c).ok()
                })
                .map(|c| c.entries.len())
                .unwrap_or(0);

            Json(CatalogRefreshResponse {
                ok: true,
                skill_count: count,
                message: format!("{} skills cached", count),
            })
        }
        Err(e) => Json(CatalogRefreshResponse {
            ok: false,
            skill_count: 0,
            message: e.to_string(),
        }),
    }
}

// --- Catalog counts (all sources) ---

#[derive(Serialize)]
struct CatalogCountsResponse {
    clawhub: usize,
    github: usize,
    openskills: usize,
}

async fn catalog_counts() -> Json<CatalogCountsResponse> {
    let home = dirs::home_dir().unwrap_or_default().join(".homun");

    // ClawHub count from catalog cache
    let clawhub = tokio::fs::read_to_string(home.join("clawhub-catalog.json"))
        .await
        .ok()
        .and_then(|c| {
            #[derive(Deserialize)]
            struct Cache {
                entries: Vec<serde_json::Value>,
            }
            serde_json::from_str::<Cache>(&c).ok()
        })
        .map(|c| c.entries.len())
        .unwrap_or(0);

    // Open Skills count from their cache
    let openskills = tokio::fs::read_to_string(home.join("openskills-catalog.json"))
        .await
        .ok()
        .and_then(|c| {
            #[derive(Deserialize)]
            struct Cache {
                entries: Vec<serde_json::Value>,
            }
            serde_json::from_str::<Cache>(&c).ok()
        })
        .map(|c| c.entries.len())
        .unwrap_or(0);

    // GitHub: no catalog cache, just show a generic number
    let github = 0; // client will show "GitHub" without a number

    Json(CatalogCountsResponse {
        clawhub,
        github,
        openskills,
    })
}

// --- Skill detail ---

#[derive(Serialize)]
struct SkillDetailView {
    name: String,
    description: String,
    path: String,
    source: String,
    /// SKILL.md rendered to HTML via pulldown-cmark
    content_html: String,
    scripts: Vec<String>,
}

/// Strip YAML frontmatter from a SKILL.md string, returning just the body.
fn strip_frontmatter(md: &str) -> &str {
    if let Some(rest) = md.strip_prefix("---\n") {
        if let Some((_fm, body)) = rest.split_once("\n---") {
            // Skip the closing "---" line and any leading newline
            return body.strip_prefix('\n').unwrap_or(body);
        }
    }
    md
}

/// Render markdown to HTML using pulldown-cmark.
fn render_md_to_html(md: &str) -> String {
    use pulldown_cmark::{html, Options, Parser};
    let opts = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let parser = Parser::new_ext(md, opts);
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);
    html_output
}

async fn get_skill_detail(Path(name): Path<String>) -> Result<Json<SkillDetailView>, StatusCode> {
    let skills_dir = dirs::home_dir()
        .unwrap_or_default()
        .join(".homun")
        .join("skills");
    let skill_dir = skills_dir.join(&name);

    if !skill_dir.exists() {
        return Err(StatusCode::NOT_FOUND);
    }

    // Read SKILL.md
    let skill_md_path = skill_dir.join("SKILL.md");
    let content = tokio::fs::read_to_string(&skill_md_path)
        .await
        .unwrap_or_default();

    // Parse frontmatter for description (handles YAML multiline | and > blocks)
    let description = content
        .strip_prefix("---\n")
        .and_then(|s| s.split_once("\n---"))
        .and_then(|(fm, _)| {
            let lines: Vec<&str> = fm.lines().collect();
            let desc_idx = lines.iter().position(|l| l.starts_with("description:"))?;
            let after_colon = lines[desc_idx].trim_start_matches("description:").trim();

            if after_colon == "|"
                || after_colon == ">"
                || after_colon == "|+"
                || after_colon == ">-"
            {
                // YAML multiline block scalar: collect indented continuation lines
                let mut parts = Vec::new();
                for line in &lines[desc_idx + 1..] {
                    if line.starts_with("  ") || line.starts_with("\t") {
                        parts.push(line.trim());
                    } else {
                        break;
                    }
                }
                let sep = if after_colon.starts_with('>') {
                    " "
                } else {
                    "\n"
                };
                Some(parts.join(sep))
            } else {
                // Inline value
                Some(after_colon.trim_matches('"').to_string())
            }
        })
        .unwrap_or_default();

    // Render markdown body (without frontmatter) to HTML
    let body = strip_frontmatter(&content);
    let content_html = render_md_to_html(body);

    // List scripts
    let scripts_dir = skill_dir.join("scripts");
    let scripts = if scripts_dir.exists() {
        let mut entries = Vec::new();
        if let Ok(mut rd) = tokio::fs::read_dir(&scripts_dir).await {
            while let Ok(Some(entry)) = rd.next_entry().await {
                if let Some(fname) = entry.file_name().to_str() {
                    entries.push(fname.to_string());
                }
            }
        }
        entries.sort();
        entries
    } else {
        Vec::new()
    };

    let source = detect_skill_source(&skill_dir);

    Ok(Json(SkillDetailView {
        name,
        description,
        path: skill_dir.display().to_string(),
        source,
        content_html,
        scripts,
    }))
}
