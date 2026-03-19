use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use super::installer::InstallResult;
use super::loader::parse_skill_md_public;
use super::{
    adapt_legacy_skill_dir, parse_legacy_manifest, scan_skill_package, InstallSecurityOptions,
};

/// ClawHub skill installer — fetches skills from the OpenClaw skills registry on GitHub.
///
/// Skills are stored in the `openclaw/skills` monorepo on GitHub with the structure:
/// `skills/<owner>/<skill-name>/SKILL.md`
///
/// Supports:
/// - `clawhub:<owner>/<skill>` — install from ClawHub registry
/// - `clawhub:search <query>` — search available skills
///
/// Since ClawHub skills follow the same Agent Skills specification,
/// they're fully compatible with Homun's skill system.
pub struct ClawHubInstaller {
    client: Client,
    skills_dir: PathBuf,
}

/// The GitHub monorepo that hosts all ClawHub skills
const CLAWHUB_REPO_OWNER: &str = "openclaw";
const CLAWHUB_REPO_NAME: &str = "skills";
const CLAWHUB_SKILLS_PATH: &str = "skills";
const CLAWHUB_BRANCH: &str = "main";

/// Path to the local catalog cache file
const CATALOG_CACHE_FILENAME: &str = "clawhub-catalog.json";
/// Cache is valid for 6 hours
const CATALOG_CACHE_MAX_AGE_SECS: u64 = 6 * 3600;

/// A cached entry in the local catalog
#[derive(Serialize, Deserialize, Debug, Clone)]
struct CatalogEntry {
    slug: String,
    owner: String,
    name: String,
    description: String,
    downloads: u64,
    stars: u64,
}

/// The full catalog cache file format
#[derive(Serialize, Deserialize, Debug)]
struct CatalogCache {
    /// Unix timestamp when this cache was created
    fetched_at: u64,
    entries: Vec<CatalogEntry>,
}

/// GitHub API: directory listing entry (Contents API)
#[derive(Deserialize, Debug)]
struct GitHubDirEntry {
    name: String,
    path: String,
    #[serde(rename = "type")]
    entry_type: String,
}

/// GitHub API: file content response
#[derive(Deserialize, Debug)]
struct GitHubContent {
    content: Option<String>,
    encoding: Option<String>,
}

struct RemoteSkillManifest {
    raw_content: String,
    name: String,
    description: String,
    has_skill_md: bool,
}

/// GitHub API: code search response
#[derive(Deserialize, Debug)]
struct GitHubCodeSearchResponse {
    items: Vec<GitHubCodeSearchItem>,
}

/// GitHub API: code search item
#[derive(Deserialize, Debug)]
struct GitHubCodeSearchItem {
    path: String,
}

/// ClawHub native API: skills list response
#[derive(Deserialize, Debug)]
struct ClawHubApiResponse {
    items: Vec<ClawHubApiSkill>,
    #[serde(rename = "nextCursor")]
    next_cursor: Option<String>,
}

/// ClawHub native API: skill item
#[derive(Deserialize, Debug)]
struct ClawHubApiSkill {
    slug: String,
    #[serde(rename = "displayName")]
    display_name: String,
    summary: String,
    stats: ClawHubApiStats,
}

/// ClawHub native API: skill stats
#[derive(Deserialize, Debug)]
struct ClawHubApiStats {
    downloads: u64,
    #[serde(rename = "installsAllTime")]
    installs_all_time: u64,
    stars: u64,
}

/// ClawHub native API: single skill detail response
#[derive(Deserialize, Debug)]
struct ClawHubApiSkillDetail {
    #[serde(rename = "skill")]
    _skill: ClawHubApiSkill,
    owner: ClawHubApiOwner,
}

/// ClawHub native API: skill owner
#[derive(Deserialize, Debug)]
struct ClawHubApiOwner {
    handle: String,
}

/// Search result from ClawHub
pub struct ClawHubSearchResult {
    pub owner: String,
    pub skill_name: String,
    pub description: String,
    pub slug: String,
    pub downloads: u64,
    pub stars: u64,
}

/// Base URL for the ClawHub API
const CLAWHUB_API_BASE: &str = "https://clawhub.ai/api/v1";

impl ClawHubInstaller {
    pub fn new() -> Self {
        let skills_dir = Config::skills_dir();

        Self {
            client: Client::builder()
                .user_agent("homun")
                .build()
                .expect("Failed to create HTTP client"),
            skills_dir,
        }
    }

    /// Install a skill from ClawHub.
    ///
    /// `slug` format: `owner/skill-name` (maps to openclaw/skills repo path)
    pub async fn install(&self, slug: &str) -> Result<InstallResult> {
        self.install_with_options(slug, InstallSecurityOptions::default())
            .await
    }

    pub async fn install_with_options(
        &self,
        slug: &str,
        options: InstallSecurityOptions,
    ) -> Result<InstallResult> {
        let (owner, skill_name) = parse_clawhub_slug(slug)?;
        // The monorepo uses lowercase paths even if the ClawHub handle has mixed case
        let owner_lower = owner.to_lowercase();
        let skill_lower = skill_name.to_lowercase();

        tracing::info!(
            owner = %owner,
            skill = %skill_name,
            "Installing skill from ClawHub"
        );

        let skill_repo_path = format!("{}/{}/{}", CLAWHUB_SKILLS_PATH, owner_lower, skill_lower);
        let remote_manifest = self
            .fetch_remote_manifest(&skill_repo_path)
            .await
            .with_context(|| {
                format!(
                    "Skill '{}/{}' not found on ClawHub. Check the name at clawhub.ai",
                    owner, skill_name
                )
            })?;

        // 2. Security check before parsing/installing
        let security_report = super::security::scan_skill_content(&remote_manifest.raw_content);
        if security_report.is_blocked() {
            tracing::warn!(
                owner = %owner,
                skill = %skill_name,
                "Skill blocked by security check"
            );
            anyhow::bail!(
                "Skill '{}/{}' blocked by security scan:\n{}",
                owner,
                skill_name,
                security_report.summary()
            );
        }
        if !security_report.warnings.is_empty() {
            tracing::info!(
                owner = %owner,
                skill = %skill_name,
                warnings = security_report.warnings.len(),
                "Skill has security warnings (non-blocking)"
            );
        }

        let installed_name = remote_manifest.name;
        let installed_description = remote_manifest.description;
        let skill_dir = self.skills_dir.join(&installed_name);

        // 3. Check if already installed
        if skill_dir.exists() {
            return Ok(InstallResult {
                name: installed_name,
                path: skill_dir,
                already_existed: true,
                description: installed_description,
                security_report: None,
            });
        }

        // 4. Download the full skill directory, then adapt if it is still in legacy format.
        self.download_skill_dir(&skill_repo_path, &skill_dir)
            .await?;
        let adapted = if remote_manifest.has_skill_md {
            None
        } else {
            adapt_legacy_skill_dir(&skill_dir).await?
        };
        let final_description = adapted
            .as_ref()
            .map(|adapted| adapted.description.clone())
            .unwrap_or(installed_description);

        make_scripts_executable(&skill_dir.join("scripts")).await;

        let package_security = scan_skill_package(&skill_dir).await?;
        if package_security.is_blocked() && !options.force {
            tokio::fs::remove_dir_all(&skill_dir).await.ok();
            anyhow::bail!(
                "Skill '{}/{}' blocked by package security scan:\n{}",
                owner,
                skill_name,
                package_security.summary()
            );
        }

        // 5. Write a source marker so we know where it came from
        let source_file = skill_dir.join(".clawhub-source");
        let source_content = format!("clawhub:{}/{}\n", owner, skill_name);
        tokio::fs::write(&source_file, source_content).await.ok();

        tracing::info!(
            skill = %installed_name,
            source = %format!("clawhub:{}/{}", owner, skill_name),
            path = %skill_dir.display(),
            "ClawHub skill installed successfully"
        );

        Ok(InstallResult {
            name: installed_name,
            path: skill_dir,
            already_existed: false,
            description: final_description,
            security_report: Some(package_security),
        })
    }

    /// Search for skills on ClawHub.
    ///
    /// Uses a local cache of the full skill catalog if available (instant).
    /// Falls back to paginated ClawHub API (slow) and caches the result.
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<ClawHubSearchResult>> {
        // Try local cache first (instant)
        if let Some(results) = self.search_cached(query, limit).await {
            return Ok(results);
        }

        // Cache miss or stale — fetch from ClawHub API and rebuild cache
        tracing::info!("ClawHub catalog cache miss, fetching from API (this may take a moment)");
        match self.refresh_catalog_cache().await {
            Ok(()) => {
                // Retry with fresh cache
                if let Some(results) = self.search_cached(query, limit).await {
                    return Ok(results);
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to refresh ClawHub catalog cache");
            }
        }

        // Last resort: try GitHub Code Search
        self.search_github(query, limit).await
    }

    /// Search the local catalog cache. Returns None if cache doesn't exist or is stale.
    async fn search_cached(&self, query: &str, limit: usize) -> Option<Vec<ClawHubSearchResult>> {
        let cache_path = self.catalog_cache_path();
        let content = tokio::fs::read_to_string(&cache_path).await.ok()?;
        let cache: CatalogCache = serde_json::from_str(&content).ok()?;

        // Check if cache is still fresh
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if now - cache.fetched_at > CATALOG_CACHE_MAX_AGE_SECS {
            return None;
        }

        let query_lower = query.to_lowercase();
        let query_terms: Vec<&str> = query_lower.split_whitespace().collect();

        let results: Vec<ClawHubSearchResult> = cache
            .entries
            .iter()
            .filter(|e| {
                let slug = e.slug.to_lowercase();
                let name = e.name.to_lowercase();
                let desc = e.description.to_lowercase();
                query_terms
                    .iter()
                    .all(|term| slug.contains(term) || name.contains(term) || desc.contains(term))
            })
            .take(limit)
            .map(|e| ClawHubSearchResult {
                slug: e.slug.clone(),
                owner: e.owner.clone(),
                skill_name: e.name.clone(),
                description: e.description.clone(),
                downloads: e.downloads,
                stars: e.stars,
            })
            .collect();

        if results.is_empty() {
            return Some(Vec::new()); // Cache is valid, just no matches
        }

        // Fetch owner handles for matched results (parallel, fast for small result sets)
        let mut enriched = Vec::with_capacity(results.len());
        let mut futures = Vec::new();
        for r in &results {
            let client = self.client.clone();
            let slug = r.skill_name.clone();
            futures.push(tokio::spawn(async move {
                let url = format!("{}/skills/{}", CLAWHUB_API_BASE, urlencoded(&slug));
                match client.get(&url).send().await {
                    Ok(resp) => match resp.json::<ClawHubApiSkillDetail>().await {
                        Ok(detail) => detail.owner.handle,
                        Err(_) => "unknown".to_string(),
                    },
                    Err(_) => "unknown".to_string(),
                }
            }));
        }
        for (mut r, fut) in results.into_iter().zip(futures) {
            let owner = fut.await.unwrap_or_else(|_| "unknown".to_string());
            r.slug = format!("{}/{}", owner, r.skill_name);
            r.owner = owner;
            enriched.push(r);
        }

        Some(enriched)
    }

    /// Refresh the local catalog cache by paginating through the entire ClawHub API.
    /// This is slow (~30-80s) but only happens once every 6 hours.
    pub async fn refresh_catalog_cache(&self) -> Result<()> {
        let mut entries: Vec<CatalogEntry> = Vec::new();
        let mut cursor: Option<String> = None;
        let max_pages = 60;

        for page in 0..max_pages {
            let mut url = format!("{}/skills?sort=downloads&limit=200", CLAWHUB_API_BASE);
            if let Some(ref c) = cursor {
                url.push_str(&format!("&cursor={}", c));
            }

            let response = self.client.get(&url).send().await?;
            if !response.status().is_success() {
                break;
            }

            let api_resp: ClawHubApiResponse = response.json().await?;
            if api_resp.items.is_empty() {
                break;
            }

            for skill in &api_resp.items {
                entries.push(CatalogEntry {
                    slug: skill.slug.clone(),
                    owner: String::new(), // filled later for matched results
                    name: skill.slug.clone(),
                    description: skill.summary.clone(),
                    downloads: skill.stats.downloads,
                    stars: skill.stats.stars,
                });
            }

            if api_resp.next_cursor.is_none() {
                break;
            }
            cursor = api_resp.next_cursor;

            if page % 10 == 9 {
                tracing::debug!(entries = entries.len(), "ClawHub catalog fetch progress");
            }
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let cache = CatalogCache {
            fetched_at: now,
            entries,
        };

        let cache_path = self.catalog_cache_path();
        let json = serde_json::to_string(&cache)?;
        tokio::fs::write(&cache_path, json).await?;

        tracing::info!(
            skills = cache.entries.len(),
            path = %cache_path.display(),
            "ClawHub catalog cache refreshed"
        );

        Ok(())
    }

    /// Path to the catalog cache file
    fn catalog_cache_path(&self) -> PathBuf {
        self.skills_dir
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(CATALOG_CACHE_FILENAME)
    }

    /// Search using the native ClawHub REST API with cursor-based pagination.
    ///
    /// The ClawHub API doesn't support server-side text search — the `q=` parameter
    /// is ignored. We paginate through all skills and filter locally by matching
    /// query terms against slug, displayName, and summary.
    async fn search_native_api(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<ClawHubSearchResult>> {
        let query_lower = query.to_lowercase();
        let query_terms: Vec<&str> = query_lower.split_whitespace().collect();

        let mut matching_skills: Vec<ClawHubApiSkill> = Vec::new();
        let mut cursor: Option<String> = None;
        let max_pages = 60; // ~9k skills at 200/page

        for _ in 0..max_pages {
            let mut url = format!("{}/skills?sort=downloads&limit=200", CLAWHUB_API_BASE);
            if let Some(ref c) = cursor {
                url.push_str(&format!("&cursor={}", c));
            }

            let response = self
                .client
                .get(&url)
                .send()
                .await
                .context("Failed to reach ClawHub API")?;

            if !response.status().is_success() {
                break;
            }

            let api_resp: ClawHubApiResponse = response
                .json()
                .await
                .context("Failed to parse ClawHub API response")?;

            if api_resp.items.is_empty() {
                break;
            }

            // Filter this page locally
            for skill in api_resp.items {
                let slug_lower = skill.slug.to_lowercase();
                let name_lower = skill.display_name.to_lowercase();
                let summary_lower = skill.summary.to_lowercase();

                if query_terms.iter().all(|term| {
                    slug_lower.contains(term)
                        || name_lower.contains(term)
                        || summary_lower.contains(term)
                }) {
                    matching_skills.push(skill);
                    if matching_skills.len() >= limit {
                        break;
                    }
                }
            }

            // Stop if we have enough results or no more pages
            if matching_skills.len() >= limit || api_resp.next_cursor.is_none() {
                break;
            }
            cursor = api_resp.next_cursor;
        }

        if matching_skills.is_empty() {
            return Ok(Vec::new());
        }

        // Fetch owner handles in parallel for matched skills
        let mut owner_futures = Vec::with_capacity(matching_skills.len());
        for skill in &matching_skills {
            let client = self.client.clone();
            let slug = skill.slug.clone();
            owner_futures.push(tokio::spawn(async move {
                let url = format!("{}/skills/{}", CLAWHUB_API_BASE, urlencoded(&slug));
                match client.get(&url).send().await {
                    Ok(resp) => match resp.json::<ClawHubApiSkillDetail>().await {
                        Ok(detail) => detail.owner.handle,
                        Err(_) => "unknown".to_string(),
                    },
                    Err(_) => "unknown".to_string(),
                }
            }));
        }

        // Collect results with owner handles
        let mut results = Vec::with_capacity(matching_skills.len());
        for (skill, owner_handle) in matching_skills.into_iter().zip(owner_futures) {
            let owner = owner_handle.await.unwrap_or_else(|_| "unknown".to_string());
            results.push(ClawHubSearchResult {
                slug: format!("{}/{}", owner, skill.slug),
                owner,
                skill_name: skill.slug,
                description: skill.summary,
                downloads: skill.stats.downloads,
                stars: skill.stats.stars,
            });
        }

        Ok(results)
    }

    /// Fallback search using GitHub Code Search API
    async fn search_github(&self, query: &str, limit: usize) -> Result<Vec<ClawHubSearchResult>> {
        let search_query = format!(
            "{} repo:{}/{} filename:SKILL.md",
            query, CLAWHUB_REPO_OWNER, CLAWHUB_REPO_NAME
        );

        let url = format!(
            "https://api.github.com/search/code?q={}&per_page={}",
            urlencoded(&search_query),
            limit.min(30)
        );

        let response = self
            .client
            .get(&url)
            .header("Accept", "application/vnd.github.v3+json")
            .send()
            .await
            .context("Failed to search ClawHub via GitHub")?;

        if !response.status().is_success() {
            return self.search_by_path(query, limit).await;
        }

        let search_resp: GitHubCodeSearchResponse = response
            .json()
            .await
            .context("Failed to parse GitHub search response")?;

        let mut results = Vec::new();
        let prefix = format!("{}/", CLAWHUB_SKILLS_PATH);

        for item in search_resp.items {
            if !item.path.starts_with(&prefix) || !item.path.ends_with("/SKILL.md") {
                continue;
            }

            let rel = &item.path[prefix.len()..];
            let parts: Vec<&str> = rel.split('/').collect();
            if parts.len() != 3 {
                continue;
            }

            let owner = parts[0].to_string();
            let skill_name = parts[1].to_string();
            let slug = format!("{}/{}", owner, skill_name);

            if let Ok(content) = self.fetch_file_from_monorepo(&item.path).await {
                if let Ok((meta, _)) = parse_skill_md_public(&content) {
                    results.push(ClawHubSearchResult {
                        owner,
                        skill_name,
                        description: meta.description,
                        slug,
                        downloads: 0,
                        stars: 0,
                    });
                }
            }

            if results.len() >= limit {
                break;
            }
        }

        Ok(results)
    }

    /// Fallback search: use GitHub repo search for SKILL.md files
    /// that match the query in path names.
    async fn search_by_path(&self, query: &str, limit: usize) -> Result<Vec<ClawHubSearchResult>> {
        let query_lower = query.to_lowercase();
        let query_terms: Vec<&str> = query_lower.split_whitespace().collect();

        // Use GitHub Search API for repositories — but we want code in a specific repo
        // Alternative: use the regular search API with path qualifier
        let search_query = format!(
            "path:{} {} repo:{}/{}",
            CLAWHUB_SKILLS_PATH, query, CLAWHUB_REPO_OWNER, CLAWHUB_REPO_NAME
        );

        let url = format!(
            "https://api.github.com/search/code?q={}&per_page={}",
            urlencoded(&search_query),
            limit.min(30)
        );

        let response = self
            .client
            .get(&url)
            .header("Accept", "application/vnd.github.v3+json")
            .send()
            .await;

        // If Code Search works, use it
        if let Ok(resp) = response {
            if resp.status().is_success() {
                if let Ok(search_resp) = resp.json::<GitHubCodeSearchResponse>().await {
                    let mut results = Vec::new();
                    let prefix = format!("{}/", CLAWHUB_SKILLS_PATH);

                    for item in search_resp.items {
                        if !item.path.ends_with("/SKILL.md") || !item.path.starts_with(&prefix) {
                            continue;
                        }
                        let rel = &item.path[prefix.len()..];
                        let parts: Vec<&str> = rel.split('/').collect();
                        if parts.len() != 3 {
                            continue;
                        }

                        let owner = parts[0].to_string();
                        let skill_name = parts[1].to_string();

                        if let Ok(content) = self.fetch_file_from_monorepo(&item.path).await {
                            if let Ok((meta, _)) = parse_skill_md_public(&content) {
                                results.push(ClawHubSearchResult {
                                    owner,
                                    skill_name: skill_name.clone(),
                                    description: meta.description,
                                    slug: format!("{}/{}", parts[0], skill_name),
                                    downloads: 0,
                                    stars: 0,
                                });
                            }
                        }

                        if results.len() >= limit {
                            break;
                        }
                    }

                    if !results.is_empty() {
                        return Ok(results);
                    }
                }
            }
        }

        // Ultimate fallback: list owner directories and scan for matching skill names
        let dir_url = format!(
            "https://api.github.com/repos/{}/{}/contents/{}?ref={}",
            CLAWHUB_REPO_OWNER, CLAWHUB_REPO_NAME, CLAWHUB_SKILLS_PATH, CLAWHUB_BRANCH
        );

        let entries: Vec<GitHubDirEntry> = self
            .client
            .get(&dir_url)
            .header("Accept", "application/vnd.github.v3+json")
            .send()
            .await
            .context("Failed to list ClawHub owners")?
            .error_for_status()?
            .json()
            .await
            .context("Failed to parse ClawHub owners")?;

        let mut results = Vec::new();

        for owner_entry in &entries {
            if owner_entry.entry_type != "dir" {
                continue;
            }
            let owner = &owner_entry.name;
            let owner_lower = owner.to_lowercase();

            // Check if owner matches any query term
            let owner_matches = query_terms.iter().any(|t| owner_lower.contains(t));
            if !owner_matches {
                continue;
            }

            // List skills for this owner
            let owner_url = format!(
                "https://api.github.com/repos/{}/{}/contents/{}/{}?ref={}",
                CLAWHUB_REPO_OWNER, CLAWHUB_REPO_NAME, CLAWHUB_SKILLS_PATH, owner, CLAWHUB_BRANCH
            );

            if let Ok(resp) = self
                .client
                .get(&owner_url)
                .header("Accept", "application/vnd.github.v3+json")
                .send()
                .await
            {
                if let Ok(skill_entries) = resp.json::<Vec<GitHubDirEntry>>().await {
                    for skill_entry in &skill_entries {
                        if skill_entry.entry_type != "dir" {
                            continue;
                        }

                        let skill_md_path = format!("{}/SKILL.md", skill_entry.path);
                        if let Ok(content) = self.fetch_file_from_monorepo(&skill_md_path).await {
                            if let Ok((meta, _)) = parse_skill_md_public(&content) {
                                results.push(ClawHubSearchResult {
                                    owner: owner.clone(),
                                    skill_name: skill_entry.name.clone(),
                                    description: meta.description,
                                    slug: format!("{}/{}", owner, skill_entry.name),
                                    downloads: 0,
                                    stars: 0,
                                });
                            }
                        }

                        if results.len() >= limit {
                            return Ok(results);
                        }
                    }
                }
            }

            if results.len() >= limit {
                break;
            }
        }

        Ok(results)
    }

    /// Browse skills by a specific owner — returns all skills from a publisher
    pub async fn browse_owner(&self, owner: &str) -> Result<Vec<ClawHubSearchResult>> {
        let owner_url = format!(
            "https://api.github.com/repos/{}/{}/contents/{}/{}?ref={}",
            CLAWHUB_REPO_OWNER, CLAWHUB_REPO_NAME, CLAWHUB_SKILLS_PATH, owner, CLAWHUB_BRANCH
        );

        let skill_entries: Vec<GitHubDirEntry> = self
            .client
            .get(&owner_url)
            .header("Accept", "application/vnd.github.v3+json")
            .send()
            .await
            .with_context(|| format!("Failed to list skills for owner '{}'", owner))?
            .error_for_status()
            .with_context(|| format!("Owner '{}' not found on ClawHub", owner))?
            .json()
            .await
            .context("Failed to parse owner skills listing")?;

        let mut results = Vec::new();

        for entry in &skill_entries {
            if entry.entry_type != "dir" {
                continue;
            }

            let skill_md_path = format!("{}/SKILL.md", entry.path);
            if let Ok(content) = self.fetch_file_from_monorepo(&skill_md_path).await {
                if let Ok((meta, _)) = parse_skill_md_public(&content) {
                    results.push(ClawHubSearchResult {
                        owner: owner.to_string(),
                        skill_name: entry.name.clone(),
                        description: meta.description,
                        slug: format!("{}/{}", owner, entry.name),
                        downloads: 0,
                        stars: 0,
                    });
                }
            }
        }

        Ok(results)
    }

    // --- Private helpers ---

    /// Fetch a single file from the ClawHub monorepo.
    /// Tries raw.githubusercontent.com first (no rate limit), falls back to Contents API.
    async fn fetch_file_from_monorepo(&self, path: &str) -> Result<String> {
        // Primary: raw.githubusercontent.com (no rate limit, fast)
        let raw_url = format!(
            "https://raw.githubusercontent.com/{}/{}/{}/{}",
            CLAWHUB_REPO_OWNER, CLAWHUB_REPO_NAME, CLAWHUB_BRANCH, path
        );

        let response = self.client.get(&raw_url).send().await;
        if let Ok(resp) = response {
            if resp.status().is_success() {
                return resp
                    .text()
                    .await
                    .context("Failed to read raw content from GitHub");
            }
        }

        // Fallback: GitHub Contents API (rate-limited but more reliable for edge cases)
        let api_url = format!(
            "https://api.github.com/repos/{}/{}/contents/{}?ref={}",
            CLAWHUB_REPO_OWNER, CLAWHUB_REPO_NAME, path, CLAWHUB_BRANCH
        );

        let resp: GitHubContent = self
            .client
            .get(&api_url)
            .header("Accept", "application/vnd.github.v3+json")
            .send()
            .await
            .with_context(|| format!("Failed to fetch {} from ClawHub", path))?
            .error_for_status()
            .with_context(|| format!("File {} not found on ClawHub", path))?
            .json()
            .await
            .context("Failed to parse ClawHub content response")?;

        let content = resp.content.context("No content in ClawHub response")?;

        let encoding = resp.encoding.unwrap_or_default();
        if encoding == "base64" {
            let cleaned = content.replace('\n', "");
            let decoded =
                base64_decode(&cleaned).context("Failed to decode base64 content from ClawHub")?;
            String::from_utf8(decoded).context("ClawHub file content is not valid UTF-8")
        } else {
            Ok(content)
        }
    }

    async fn fetch_remote_manifest(&self, skill_repo_path: &str) -> Result<RemoteSkillManifest> {
        let skill_md_path = format!("{skill_repo_path}/SKILL.md");
        if let Ok(content) = self.fetch_file_from_monorepo(&skill_md_path).await {
            let (meta, _body) = parse_skill_md_public(&content)
                .with_context(|| "Failed to parse SKILL.md frontmatter from ClawHub skill")?;
            return Ok(RemoteSkillManifest {
                raw_content: content,
                name: meta.name,
                description: meta.description,
                has_skill_md: true,
            });
        }

        for candidate in ["SKILL.toml", "manifest.json"] {
            let path = format!("{skill_repo_path}/{candidate}");
            if let Ok(content) = self.fetch_file_from_monorepo(&path).await {
                let manifest = parse_legacy_manifest(candidate, &content).with_context(|| {
                    format!("Failed to parse legacy manifest {candidate} from ClawHub")
                })?;
                return Ok(RemoteSkillManifest {
                    raw_content: content,
                    name: manifest.name,
                    description: manifest.description,
                    has_skill_md: false,
                });
            }
        }

        anyhow::bail!("No SKILL.md, SKILL.toml, or manifest.json found for skill");
    }

    /// Download extra files (scripts, etc.) from a skill directory, skipping SKILL.md.
    /// Non-fatal — most skills only have SKILL.md which is already written.
    async fn download_skill_dir(&self, repo_path: &str, dest: &Path) -> Result<()> {
        tokio::fs::create_dir_all(dest)
            .await
            .with_context(|| format!("Failed to create directory {}", dest.display()))?;

        self.download_dir_recursive(repo_path, dest).await?;

        let scripts_dir = dest.join("scripts");
        if scripts_dir.exists() {
            make_scripts_executable(&scripts_dir).await;
        }

        Ok(())
    }

    /// Recursively download a directory from the monorepo using Contents API
    fn download_dir_recursive<'a>(
        &'a self,
        repo_path: &'a str,
        dest: &'a Path,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let url = format!(
                "https://api.github.com/repos/{}/{}/contents/{}?ref={}",
                CLAWHUB_REPO_OWNER, CLAWHUB_REPO_NAME, repo_path, CLAWHUB_BRANCH
            );

            let response = self
                .client
                .get(&url)
                .header("Accept", "application/vnd.github.v3+json")
                .send()
                .await
                .with_context(|| format!("Failed to list directory {} on ClawHub", repo_path))?
                .error_for_status()
                .with_context(|| format!("Directory {} not found on ClawHub", repo_path))?;

            let entries: Vec<GitHubDirEntry> = response
                .json()
                .await
                .context("Failed to parse directory listing from ClawHub")?;

            for entry in &entries {
                let local_path = dest.join(&entry.name);

                match entry.entry_type.as_str() {
                    "file" => {
                        // Download file content
                        match self.fetch_file_from_monorepo(&entry.path).await {
                            Ok(content) => {
                                tokio::fs::write(&local_path, &content).await.with_context(
                                    || format!("Failed to write {}", local_path.display()),
                                )?;
                                tracing::debug!(file = %entry.name, "Downloaded");
                            }
                            Err(e) => {
                                tracing::warn!(
                                    file = %entry.path,
                                    error = %e,
                                    "Failed to download file, skipping"
                                );
                            }
                        }
                    }
                    "dir" => {
                        // Recurse into subdirectory
                        tokio::fs::create_dir_all(&local_path).await.ok();
                        if let Err(e) = self.download_dir_recursive(&entry.path, &local_path).await
                        {
                            tracing::warn!(
                                dir = %entry.path,
                                error = %e,
                                "Failed to download subdirectory, skipping"
                            );
                        }
                    }
                    _ => {
                        tracing::debug!(
                            entry_type = %entry.entry_type,
                            path = %entry.path,
                            "Skipping non-file entry"
                        );
                    }
                }
            }

            Ok(())
        }) // Box::pin
    }
}

/// Simple URL encoding for query parameters
fn urlencoded(s: &str) -> String {
    s.replace(' ', "+")
        .replace('&', "%26")
        .replace('=', "%3D")
        .replace('#', "%23")
        .replace(':', "%3A")
        .replace('/', "%2F")
}

/// Parse a ClawHub slug: `owner/skill-name`
fn parse_clawhub_slug(slug: &str) -> Result<(String, String)> {
    let (owner, skill) = slug
        .split_once('/')
        .context("Invalid ClawHub slug. Expected: owner/skill-name")?;

    if owner.is_empty() || skill.is_empty() {
        anyhow::bail!("Invalid ClawHub slug. Both owner and skill name must be non-empty");
    }

    Ok((owner.to_string(), skill.to_string()))
}

/// Simple base64 decoder (same as installer.rs — reused here to avoid coupling)
fn base64_decode(input: &str) -> Result<Vec<u8>> {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    fn char_to_val(c: u8) -> Option<u8> {
        CHARS.iter().position(|&x| x == c).map(|p| p as u8)
    }

    let input: Vec<u8> = input.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    let mut output = Vec::with_capacity(input.len() * 3 / 4);

    for chunk in input.chunks(4) {
        let mut buf = [0u8; 4];
        let mut count = 0;

        for (i, &byte) in chunk.iter().enumerate() {
            if byte == b'=' {
                break;
            }
            buf[i] = char_to_val(byte)
                .with_context(|| format!("Invalid base64 character: {}", byte as char))?;
            count += 1;
        }

        if count >= 2 {
            output.push((buf[0] << 2) | (buf[1] >> 4));
        }
        if count >= 3 {
            output.push((buf[1] << 4) | (buf[2] >> 2));
        }
        if count >= 4 {
            output.push((buf[2] << 6) | buf[3]);
        }
    }

    Ok(output)
}

/// Make scripts in a directory executable (chmod +x)
#[cfg(unix)]
async fn make_scripts_executable(dir: &Path) {
    use std::os::unix::fs::PermissionsExt;

    if let Ok(mut entries) = tokio::fs::read_dir(dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.is_file() {
                if let Ok(metadata) = tokio::fs::metadata(&path).await {
                    let mut perms = metadata.permissions();
                    perms.set_mode(perms.mode() | 0o111);
                    tokio::fs::set_permissions(&path, perms).await.ok();
                }
            }
        }
    }
}

#[cfg(not(unix))]
async fn make_scripts_executable(_dir: &Path) {
    // No-op on non-Unix platforms
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_clawhub_slug_basic() {
        let (owner, skill) = parse_clawhub_slug("tolibear/promptify-skill").unwrap();
        assert_eq!(owner, "tolibear");
        assert_eq!(skill, "promptify-skill");
    }

    #[test]
    fn test_parse_clawhub_slug_invalid_no_slash() {
        assert!(parse_clawhub_slug("just-a-name").is_err());
    }

    #[test]
    fn test_parse_clawhub_slug_invalid_empty_parts() {
        assert!(parse_clawhub_slug("/skill").is_err());
        assert!(parse_clawhub_slug("owner/").is_err());
    }

    #[test]
    fn test_base64_decode_basic() {
        let encoded = "SGVsbG8gV29ybGQ=";
        let decoded = base64_decode(encoded).unwrap();
        assert_eq!(String::from_utf8(decoded).unwrap(), "Hello World");
    }

    #[test]
    fn test_base64_decode_with_newlines() {
        let encoded = "SGVs\nbG8g\nV29y\nbGQ=";
        let decoded = base64_decode(encoded).unwrap();
        assert_eq!(String::from_utf8(decoded).unwrap(), "Hello World");
    }
}
