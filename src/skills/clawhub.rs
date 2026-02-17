use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;

use super::installer::InstallResult;
use super::loader::parse_skill_md_public;

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
/// they're fully compatible with HomunBot's skill system.
pub struct ClawHubInstaller {
    client: Client,
    skills_dir: PathBuf,
}

/// The GitHub monorepo that hosts all ClawHub skills
const CLAWHUB_REPO_OWNER: &str = "openclaw";
const CLAWHUB_REPO_NAME: &str = "skills";
const CLAWHUB_SKILLS_PATH: &str = "skills";
const CLAWHUB_BRANCH: &str = "main";

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

/// Search result from ClawHub
pub struct ClawHubSearchResult {
    pub owner: String,
    pub skill_name: String,
    pub description: String,
    pub slug: String,
}

impl ClawHubInstaller {
    pub fn new() -> Self {
        let skills_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".homunbot")
            .join("skills");

        Self {
            client: Client::builder()
                .user_agent("homunbot")
                .build()
                .expect("Failed to create HTTP client"),
            skills_dir,
        }
    }

    /// Install a skill from ClawHub.
    ///
    /// `slug` format: `owner/skill-name` (maps to openclaw/skills repo path)
    pub async fn install(&self, slug: &str) -> Result<InstallResult> {
        let (owner, skill_name) = parse_clawhub_slug(slug)?;

        tracing::info!(
            owner = %owner,
            skill = %skill_name,
            "Installing skill from ClawHub"
        );

        // 1. Fetch SKILL.md from the monorepo
        let skill_md_path = format!("{}/{}/{}/SKILL.md", CLAWHUB_SKILLS_PATH, owner, skill_name);
        let skill_md_content = self
            .fetch_file_from_monorepo(&skill_md_path)
            .await
            .with_context(|| {
                format!(
                    "Skill '{}/{}' not found on ClawHub. Check the name at clawhub.ai",
                    owner, skill_name
                )
            })?;

        // 2. Parse metadata
        let (meta, _body) = parse_skill_md_public(&skill_md_content)
            .with_context(|| "Failed to parse SKILL.md frontmatter from ClawHub skill")?;

        let installed_name = meta.name.clone();
        let skill_dir = self.skills_dir.join(&installed_name);

        // 3. Check if already installed
        if skill_dir.exists() {
            return Ok(InstallResult {
                name: installed_name,
                path: skill_dir,
                already_existed: true,
                description: meta.description,
            });
        }

        // 4. Download the skill directory from the monorepo
        let skill_repo_path = format!("{}/{}/{}", CLAWHUB_SKILLS_PATH, owner, skill_name);
        self.download_skill_dir(&skill_repo_path, &skill_dir)
            .await?;

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
            description: meta.description,
        })
    }

    /// Search for skills on ClawHub using GitHub Code Search API.
    ///
    /// Searches for SKILL.md files in the openclaw/skills repo that contain
    /// the query terms in their filename path or content.
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<ClawHubSearchResult>> {
        // Use GitHub Code Search: find SKILL.md files matching the query
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
            .context("Failed to search ClawHub")?;

        if !response.status().is_success() {
            // Fallback: try direct path matching
            return self.search_by_path(query, limit).await;
        }

        let search_resp: GitHubCodeSearchResponse = response
            .json()
            .await
            .context("Failed to parse ClawHub search response")?;

        let mut results = Vec::new();
        let prefix = format!("{}/", CLAWHUB_SKILLS_PATH);

        for item in search_resp.items {
            // Parse path: skills/<owner>/<skill-name>/SKILL.md
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

            // Fetch SKILL.md for description
            if let Ok(content) = self.fetch_file_from_monorepo(&item.path).await {
                if let Ok((meta, _)) = parse_skill_md_public(&content) {
                    results.push(ClawHubSearchResult {
                        owner,
                        skill_name,
                        description: meta.description,
                        slug,
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
                    });
                }
            }
        }

        Ok(results)
    }

    // --- Private helpers ---

    /// Fetch a single file from the ClawHub monorepo via GitHub Contents API
    async fn fetch_file_from_monorepo(&self, path: &str) -> Result<String> {
        let url = format!(
            "https://api.github.com/repos/{}/{}/contents/{}?ref={}",
            CLAWHUB_REPO_OWNER, CLAWHUB_REPO_NAME, path, CLAWHUB_BRANCH
        );

        let resp: GitHubContent = self
            .client
            .get(&url)
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
            let decoded = base64_decode(&cleaned)
                .context("Failed to decode base64 content from ClawHub")?;
            String::from_utf8(decoded).context("ClawHub file content is not valid UTF-8")
        } else {
            Ok(content)
        }
    }

    /// Download all files in a skill directory from the monorepo.
    ///
    /// Uses the GitHub Contents API to list directory contents (works for any repo size),
    /// then recursively downloads files and subdirectories.
    async fn download_skill_dir(&self, repo_path: &str, dest: &Path) -> Result<()> {
        // Create destination directory
        tokio::fs::create_dir_all(dest)
            .await
            .with_context(|| format!("Failed to create directory {}", dest.display()))?;

        // Recursively download the directory
        self.download_dir_recursive(repo_path, dest).await?;

        // Make scripts executable
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
                            tokio::fs::write(&local_path, &content)
                                .await
                                .with_context(|| {
                                    format!("Failed to write {}", local_path.display())
                                })?;
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
                    if let Err(e) = self
                        .download_dir_recursive(&entry.path, &local_path)
                        .await
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
