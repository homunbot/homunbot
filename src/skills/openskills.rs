/// Open Skills — community skill repository (besoeasy/open-skills on GitHub).
///
/// This provides an additional source of curated skills beyond ClawHub.
/// Skills follow the same SKILL.md format (Agent Skills specification).
///
/// Inspired by ZeroClaw's Open Skills integration, but uses GitHub API
/// instead of `git clone` to avoid adding git2 as a dependency.
use std::path::PathBuf;

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::installer::InstallResult;
use super::loader::parse_skill_md_public;
use super::{
    adapt_legacy_skill_dir, parse_legacy_manifest, scan_skill_package, InstallSecurityOptions,
};

const REPO_OWNER: &str = "besoeasy";
const REPO_NAME: &str = "open-skills";
const BRANCH: &str = "main";
const SKILLS_PATH: &str = "skills";

/// Catalog cache for Open Skills (same pattern as ClawHub)
const CACHE_FILENAME: &str = "openskills-catalog.json";
const CACHE_MAX_AGE_SECS: u64 = 24 * 3600; // 24 hours (smaller repo, less frequent updates)

/// An Open Skills search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenSkillsResult {
    pub name: String,
    pub description: String,
    pub source: String,
}

/// Cached entry for a skill
#[derive(Serialize, Deserialize, Debug, Clone)]
struct CatalogEntry {
    dir_name: String,
    name: String,
    description: String,
}

/// The full catalog cache
#[derive(Serialize, Deserialize, Debug)]
struct CatalogCache {
    fetched_at: u64,
    entries: Vec<CatalogEntry>,
}

/// GitHub tree API response
#[derive(Deserialize)]
struct GitHubTree {
    tree: Vec<GitHubTreeEntry>,
}

#[derive(Deserialize)]
struct GitHubTreeEntry {
    path: String,
    #[serde(rename = "type")]
    entry_type: String,
}

#[derive(Deserialize)]
struct GitHubContentDirEntry {
    name: String,
    path: String,
    #[serde(rename = "type")]
    entry_type: String,
}

struct RemoteSkillManifest {
    raw_content: String,
    name: String,
    description: String,
    has_skill_md: bool,
}

pub struct OpenSkillsSource {
    client: Client,
    skills_dir: PathBuf,
}

impl OpenSkillsSource {
    pub fn new() -> Self {
        let skills_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".homun")
            .join("skills");

        Self {
            client: Client::new(),
            skills_dir,
        }
    }

    /// Search for skills in the Open Skills catalog.
    /// Uses a local cache when available.
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<OpenSkillsResult>> {
        // Try cache first
        if let Some(results) = self.search_cached(query, limit).await {
            return Ok(results);
        }

        // Cache miss — refresh
        tracing::info!("Open Skills catalog cache miss, fetching from GitHub");
        if let Err(e) = self.refresh_cache().await {
            tracing::warn!(error = %e, "Failed to refresh Open Skills cache");
            return Ok(Vec::new());
        }

        // Retry with fresh cache
        Ok(self.search_cached(query, limit).await.unwrap_or_default())
    }

    /// Install a skill from the Open Skills repository.
    /// `dir_name` is the directory name under `skills/` (e.g. "free-weather-data")
    pub async fn install(&self, dir_name: &str) -> Result<InstallResult> {
        self.install_with_options(dir_name, InstallSecurityOptions::default())
            .await
    }

    pub async fn install_with_options(
        &self,
        dir_name: &str,
        options: InstallSecurityOptions,
    ) -> Result<InstallResult> {
        tracing::info!(skill = %dir_name, "Installing skill from Open Skills");

        let skill_repo_path = format!("{}/{}", SKILLS_PATH, dir_name);
        let remote_manifest = self
            .fetch_remote_manifest(&skill_repo_path)
            .await
            .with_context(|| format!("Skill '{}' not found in Open Skills repo", dir_name))?;

        // Security check
        let security_report = super::security::scan_skill_content(&remote_manifest.raw_content);
        if security_report.is_blocked() {
            anyhow::bail!(
                "Skill '{}' blocked by security scan:\n{}",
                dir_name,
                security_report.summary()
            );
        }

        let skill_dir = self.skills_dir.join(&remote_manifest.name);

        // Check if already installed
        if skill_dir.exists() {
            return Ok(InstallResult {
                name: remote_manifest.name,
                path: skill_dir,
                already_existed: true,
                description: remote_manifest.description,
                security_report: None,
            });
        }

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
            .unwrap_or(remote_manifest.description);

        // Write source marker
        let source = format!("openskills:{}\n", dir_name);
        tokio::fs::write(skill_dir.join(".openskills-source"), source)
            .await
            .ok();

        let package_security = scan_skill_package(&skill_dir).await?;
        if package_security.is_blocked() && !options.force {
            tokio::fs::remove_dir_all(&skill_dir).await.ok();
            anyhow::bail!(
                "Skill '{}' blocked by package security scan:\n{}",
                dir_name,
                package_security.summary()
            );
        }

        tracing::info!(
            skill = %remote_manifest.name,
            source = %format!("openskills:{}", dir_name),
            "Open Skills skill installed"
        );

        Ok(InstallResult {
            name: remote_manifest.name,
            path: skill_dir,
            already_existed: false,
            description: final_description,
            security_report: Some(package_security),
        })
    }

    /// Search the local cache for matching skills
    async fn search_cached(&self, query: &str, limit: usize) -> Option<Vec<OpenSkillsResult>> {
        let cache_path = Self::cache_path();
        let data = tokio::fs::read_to_string(&cache_path).await.ok()?;
        let cache: CatalogCache = serde_json::from_str(&data).ok()?;

        // Check freshness
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if now - cache.fetched_at > CACHE_MAX_AGE_SECS {
            return None; // Stale
        }

        let query_lower = query.to_lowercase();
        let terms: Vec<&str> = query_lower.split_whitespace().collect();

        let results: Vec<OpenSkillsResult> = cache
            .entries
            .iter()
            .filter(|e| {
                let haystack =
                    format!("{} {} {}", e.dir_name, e.name, e.description).to_lowercase();
                terms.iter().all(|t| haystack.contains(t))
            })
            .take(limit)
            .map(|e| OpenSkillsResult {
                name: e.name.clone(),
                description: e.description.clone(),
                source: format!("openskills:{}", e.dir_name),
            })
            .collect();

        Some(results)
    }

    /// Refresh the local catalog cache by fetching the repo tree + parsing each SKILL.md
    pub async fn refresh_cache(&self) -> Result<()> {
        // Get the repo tree to list skill directories
        let tree_url = format!(
            "https://api.github.com/repos/{}/{}/git/trees/{}?recursive=1",
            REPO_OWNER, REPO_NAME, BRANCH
        );

        let tree: GitHubTree = self
            .client
            .get(&tree_url)
            .header("User-Agent", "homun")
            .header("Accept", "application/vnd.github.v3+json")
            .send()
            .await
            .context("Failed to fetch Open Skills repo tree")?
            .error_for_status()
            .context("Open Skills repo tree request failed")?
            .json()
            .await
            .context("Failed to parse Open Skills tree")?;

        // Find all SKILL.md files under skills/
        let skill_paths: Vec<String> = tree
            .tree
            .iter()
            .filter(|e| {
                e.entry_type == "blob"
                    && e.path.starts_with("skills/")
                    && e.path.ends_with("/SKILL.md")
            })
            .map(|e| e.path.clone())
            .collect();

        tracing::info!(count = skill_paths.len(), "Found Open Skills to index");

        let mut entries = Vec::new();

        for path in &skill_paths {
            // Extract dir name: "skills/free-weather-data/SKILL.md" → "free-weather-data"
            let dir_name = path
                .strip_prefix("skills/")
                .and_then(|p| p.strip_suffix("/SKILL.md"))
                .unwrap_or_default();

            if dir_name.is_empty() {
                continue;
            }

            // Fetch and parse SKILL.md (via raw.githubusercontent.com — no rate limit)
            match self.fetch_file(path).await {
                Ok(content) => {
                    let (name, description) = match parse_skill_md_public(&content) {
                        Ok((meta, _)) => (meta.name, meta.description),
                        Err(_) => (dir_name.to_string(), String::new()),
                    };
                    entries.push(CatalogEntry {
                        dir_name: dir_name.to_string(),
                        name,
                        description,
                    });
                }
                Err(e) => {
                    tracing::debug!(path = %path, error = %e, "Failed to fetch Open Skills SKILL.md");
                    // Still add with minimal info
                    entries.push(CatalogEntry {
                        dir_name: dir_name.to_string(),
                        name: dir_name.replace('-', " "),
                        description: String::new(),
                    });
                }
            }
        }

        // Save cache
        let cache = CatalogCache {
            fetched_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            entries,
        };

        let cache_path = Self::cache_path();
        if let Some(parent) = cache_path.parent() {
            tokio::fs::create_dir_all(parent).await.ok();
        }

        let json = serde_json::to_string(&cache)?;
        tokio::fs::write(&cache_path, json).await?;

        tracing::info!(
            skills = cache.entries.len(),
            "Open Skills catalog cache saved"
        );

        Ok(())
    }

    /// Get the catalog cache status (for API/UI)
    pub async fn cache_status() -> CacheStatus {
        let cache_path = Self::cache_path();
        let data = match tokio::fs::read_to_string(&cache_path).await {
            Ok(d) => d,
            Err(_) => {
                return CacheStatus {
                    cached: false,
                    stale: true,
                    skill_count: 0,
                    age_secs: 0,
                }
            }
        };

        let cache: CatalogCache = match serde_json::from_str(&data) {
            Ok(c) => c,
            Err(_) => {
                return CacheStatus {
                    cached: false,
                    stale: true,
                    skill_count: 0,
                    age_secs: 0,
                }
            }
        };

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let age = now.saturating_sub(cache.fetched_at);

        CacheStatus {
            cached: true,
            stale: age > CACHE_MAX_AGE_SECS,
            skill_count: cache.entries.len(),
            age_secs: age,
        }
    }

    /// Fetch a file from the Open Skills repo via raw.githubusercontent.com
    async fn fetch_file(&self, path: &str) -> Result<String> {
        let url = format!(
            "https://raw.githubusercontent.com/{}/{}/{}/{}",
            REPO_OWNER, REPO_NAME, BRANCH, path
        );

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "homun")
            .send()
            .await
            .with_context(|| format!("Failed to fetch {} from Open Skills", path))?;

        if !response.status().is_success() {
            anyhow::bail!(
                "File {} not found in Open Skills (HTTP {})",
                path,
                response.status()
            );
        }

        response
            .text()
            .await
            .context("Failed to read Open Skills file content")
    }

    async fn fetch_remote_manifest(&self, skill_repo_path: &str) -> Result<RemoteSkillManifest> {
        let skill_md_path = format!("{skill_repo_path}/SKILL.md");
        if let Ok(content) = self.fetch_file(&skill_md_path).await {
            let (meta, _body) = parse_skill_md_public(&content)
                .with_context(|| "Failed to parse SKILL.md from Open Skills")?;
            return Ok(RemoteSkillManifest {
                raw_content: content,
                name: meta.name,
                description: meta.description,
                has_skill_md: true,
            });
        }

        for candidate in ["SKILL.toml", "manifest.json"] {
            let path = format!("{skill_repo_path}/{candidate}");
            if let Ok(content) = self.fetch_file(&path).await {
                let manifest = parse_legacy_manifest(candidate, &content).with_context(|| {
                    format!("Failed to parse legacy manifest {candidate} from Open Skills")
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

    async fn download_skill_dir(&self, repo_path: &str, dest: &PathBuf) -> Result<()> {
        tokio::fs::create_dir_all(dest).await?;
        self.download_dir_recursive(repo_path, dest).await
    }

    fn download_dir_recursive<'a>(
        &'a self,
        repo_path: &'a str,
        dest: &'a std::path::Path,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let url = format!(
                "https://api.github.com/repos/{}/{}/contents/{}?ref={}",
                REPO_OWNER, REPO_NAME, repo_path, BRANCH
            );

            let response = self
                .client
                .get(&url)
                .header("User-Agent", "homun")
                .header("Accept", "application/vnd.github.v3+json")
                .send()
                .await
                .with_context(|| format!("Failed to list directory {} in Open Skills", repo_path))?
                .error_for_status()
                .with_context(|| format!("Directory {} not found in Open Skills", repo_path))?;

            let entries: Vec<GitHubContentDirEntry> = response
                .json()
                .await
                .context("Failed to parse Open Skills directory listing")?;

            for entry in &entries {
                let local_path = dest.join(&entry.name);
                match entry.entry_type.as_str() {
                    "file" => {
                        let content = self.fetch_file(&entry.path).await?;
                        tokio::fs::write(&local_path, content)
                            .await
                            .with_context(|| {
                                format!("Failed to write downloaded file {}", local_path.display())
                            })?;
                    }
                    "dir" => {
                        tokio::fs::create_dir_all(&local_path).await.ok();
                        self.download_dir_recursive(&entry.path, &local_path)
                            .await?;
                    }
                    _ => {}
                }
            }

            Ok(())
        })
    }

    fn cache_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".homun")
            .join(CACHE_FILENAME)
    }
}

/// Cache status info for API responses
pub struct CacheStatus {
    pub cached: bool,
    pub stale: bool,
    pub skill_count: usize,
    pub age_secs: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_path() {
        let path = OpenSkillsSource::cache_path();
        assert!(path.to_string_lossy().contains("openskills-catalog.json"));
    }
}
