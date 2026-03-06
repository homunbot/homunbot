use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;

use super::loader::parse_skill_md_public;
use super::{adapt_legacy_skill_dir, parse_legacy_manifest};
use super::{scan_skill_package, InstallSecurityOptions, SecurityReport};

/// Skill installer — fetches skills from GitHub and installs them locally.
///
/// Install flow:
/// 1. Resolve `owner/repo` → GitHub API
/// 2. Check for SKILL.md in repo root (validate it's a real skill)
/// 3. Download repo as tarball
/// 4. Extract to `~/.homun/skills/<skill-name>/`
///
/// Supports:
/// - `owner/repo` — latest default branch
/// - `owner/repo@ref` — specific tag/branch/commit
pub struct SkillInstaller {
    client: Client,
    skills_dir: PathBuf,
}

/// GitHub API: repo info
#[derive(Deserialize)]
struct GitHubRepo {
    default_branch: String,
}

/// GitHub API: file content
#[derive(Deserialize)]
struct GitHubContent {
    content: Option<String>,
    encoding: Option<String>,
}

struct RemoteSkillManifest {
    raw_content: String,
    name: String,
    description: String,
}

impl SkillInstaller {
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

    /// Install a skill from a GitHub repository.
    ///
    /// `repo_spec` format: `owner/repo` or `owner/repo@ref`
    pub async fn install(&self, repo_spec: &str) -> Result<InstallResult> {
        self.install_with_options(repo_spec, InstallSecurityOptions::default())
            .await
    }

    pub async fn install_with_options(
        &self,
        repo_spec: &str,
        options: InstallSecurityOptions,
    ) -> Result<InstallResult> {
        let (owner, repo, git_ref) = parse_repo_spec(repo_spec)?;

        tracing::info!(
            owner = %owner,
            repo = %repo,
            git_ref = ?git_ref,
            "Installing skill from GitHub"
        );

        // 1. Get default branch if no ref specified
        let branch = match &git_ref {
            Some(r) => r.clone(),
            None => self.get_default_branch(&owner, &repo).await?,
        };

        // 2. Fetch SKILL.md or a legacy manifest to validate and get skill name
        let remote_manifest = self.fetch_remote_manifest(&owner, &repo, &branch).await?;

        // Security scan before installing
        let security_report = super::security::scan_skill_content(&remote_manifest.raw_content);
        if security_report.is_blocked() {
            tracing::warn!(
                owner = %owner,
                repo = %repo,
                "Skill blocked by security check"
            );
            anyhow::bail!(
                "Skill '{}/{}' blocked by security preflight:\n{}",
                owner,
                repo,
                security_report.summary()
            );
        }
        if !security_report.warnings.is_empty() {
            tracing::info!(
                owner = %owner,
                repo = %repo,
                warnings = security_report.warnings.len(),
                "Skill has security warnings (non-blocking)"
            );
        }

        let skill_name = remote_manifest.name;
        let skill_description = remote_manifest.description;
        let skill_dir = self.skills_dir.join(&skill_name);

        // 3. Check if already installed
        if skill_dir.exists() {
            return Ok(InstallResult {
                name: skill_name,
                path: skill_dir,
                already_existed: true,
                description: skill_description,
                security_report: None,
            });
        }

        // 4. Download and extract the repo
        self.download_and_extract(&owner, &repo, &branch, &skill_dir)
            .await?;

        let adapted = adapt_legacy_skill_dir(&skill_dir).await?;
        let final_description = adapted
            .as_ref()
            .map(|adapted| adapted.description.clone())
            .unwrap_or(skill_description);

        let package_security = scan_skill_package(&skill_dir).await?;
        if package_security.is_blocked() && !options.force {
            tokio::fs::remove_dir_all(&skill_dir).await.ok();
            anyhow::bail!(
                "Skill '{}/{}' blocked by package security scan:\n{}",
                owner,
                repo,
                package_security.summary()
            );
        }
        if !package_security.warnings.is_empty() {
            tracing::info!(
                owner = %owner,
                repo = %repo,
                risk = package_security.risk_score,
                warnings = package_security.warnings.len(),
                forced = options.force,
                "Skill package scan completed with findings"
            );
        }

        tracing::info!(
            skill = %skill_name,
            path = %skill_dir.display(),
            "Skill installed successfully"
        );

        Ok(InstallResult {
            name: skill_name,
            path: skill_dir,
            already_existed: false,
            description: final_description,
            security_report: Some(package_security),
        })
    }

    /// Remove an installed skill by name
    pub async fn remove(name: &str) -> Result<()> {
        let skills_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".homun")
            .join("skills");

        let skill_dir = skills_dir.join(name);
        if !skill_dir.exists() {
            anyhow::bail!("Skill '{}' is not installed", name);
        }

        tokio::fs::remove_dir_all(&skill_dir)
            .await
            .with_context(|| format!("Failed to remove skill directory {}", skill_dir.display()))?;

        tracing::info!(skill = %name, "Skill removed");
        Ok(())
    }

    /// List installed skills (reads from ~/.homun/skills/)
    pub async fn list_installed() -> Result<Vec<InstalledSkillInfo>> {
        let skills_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".homun")
            .join("skills");

        if !skills_dir.exists() {
            return Ok(Vec::new());
        }

        let mut skills = Vec::new();
        let mut entries = tokio::fs::read_dir(&skills_dir)
            .await
            .with_context(|| format!("Failed to read {}", skills_dir.display()))?;

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.is_dir() {
                let skill_md = path.join("SKILL.md");
                if skill_md.exists() {
                    let content = tokio::fs::read_to_string(&skill_md).await.ok();
                    let (name, description) = if let Some(ref c) = content {
                        match parse_skill_md_public(c) {
                            Ok((meta, _)) => (meta.name, meta.description),
                            Err(_) => {
                                let dir_name = path
                                    .file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or("unknown")
                                    .to_string();
                                (dir_name, "(invalid SKILL.md)".to_string())
                            }
                        }
                    } else {
                        let dir_name = path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("unknown")
                            .to_string();
                        (dir_name, "(unreadable)".to_string())
                    };

                    skills.push(InstalledSkillInfo {
                        name,
                        description,
                        path,
                    });
                }
            }
        }

        skills.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(skills)
    }

    // --- Private helpers ---

    /// Get the default branch of a GitHub repo
    async fn get_default_branch(&self, owner: &str, repo: &str) -> Result<String> {
        let url = format!("https://api.github.com/repos/{}/{}", owner, repo);
        let resp: GitHubRepo = self
            .client
            .get(&url)
            .header("User-Agent", "homun")
            .header("Accept", "application/vnd.github.v3+json")
            .send()
            .await
            .with_context(|| format!("Failed to fetch repo info for {}/{}", owner, repo))?
            .error_for_status()
            .with_context(|| format!("GitHub repo {}/{} not found or inaccessible", owner, repo))?
            .json()
            .await
            .context("Failed to parse GitHub repo response")?;

        Ok(resp.default_branch)
    }

    /// Fetch a single file from the repo via GitHub Contents API
    async fn fetch_file(
        &self,
        owner: &str,
        repo: &str,
        git_ref: &str,
        path: &str,
    ) -> Result<String> {
        let url = format!(
            "https://api.github.com/repos/{}/{}/contents/{}?ref={}",
            owner, repo, path, git_ref
        );

        let resp: GitHubContent = self
            .client
            .get(&url)
            .header("User-Agent", "homun")
            .header("Accept", "application/vnd.github.v3+json")
            .send()
            .await
            .with_context(|| format!("Failed to fetch {} from {}/{}", path, owner, repo))?
            .error_for_status()
            .with_context(|| format!("File {} not found in {}/{}", path, owner, repo))?
            .json()
            .await
            .context("Failed to parse GitHub content response")?;

        let content = resp.content.context("No content in GitHub response")?;

        let encoding = resp.encoding.unwrap_or_default();
        if encoding == "base64" {
            // GitHub returns base64-encoded content with newlines
            let cleaned = content.replace('\n', "");
            let decoded =
                base64_decode(&cleaned).context("Failed to decode base64 content from GitHub")?;
            String::from_utf8(decoded).context("GitHub file content is not valid UTF-8")
        } else {
            Ok(content)
        }
    }

    async fn fetch_remote_manifest(
        &self,
        owner: &str,
        repo: &str,
        git_ref: &str,
    ) -> Result<RemoteSkillManifest> {
        if let Ok(content) = self.fetch_file(owner, repo, git_ref, "SKILL.md").await {
            let (meta, _body) = parse_skill_md_public(&content)
                .with_context(|| "Failed to parse SKILL.md frontmatter")?;
            return Ok(RemoteSkillManifest {
                raw_content: content,
                name: meta.name,
                description: meta.description,
            });
        }

        for candidate in ["SKILL.toml", "manifest.json"] {
            if let Ok(content) = self.fetch_file(owner, repo, git_ref, candidate).await {
                let manifest = parse_legacy_manifest(candidate, &content).with_context(|| {
                    format!("Failed to parse legacy manifest {candidate} from {owner}/{repo}")
                })?;
                return Ok(RemoteSkillManifest {
                    raw_content: content,
                    name: manifest.name,
                    description: manifest.description,
                });
            }
        }

        anyhow::bail!(
            "No SKILL.md, SKILL.toml, or manifest.json found in {}/{}. Is this a supported skill repo?",
            owner,
            repo
        );
    }

    /// Download the repo as a tarball and extract it to the skill directory
    async fn download_and_extract(
        &self,
        owner: &str,
        repo: &str,
        git_ref: &str,
        dest: &Path,
    ) -> Result<()> {
        let url = format!(
            "https://api.github.com/repos/{}/{}/tarball/{}",
            owner, repo, git_ref
        );

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "homun")
            .header("Accept", "application/vnd.github.v3+json")
            .send()
            .await
            .with_context(|| format!("Failed to download tarball for {}/{}", owner, repo))?
            .error_for_status()
            .with_context(|| format!("Failed to download tarball: {}/{}", owner, repo))?;

        let bytes = response
            .bytes()
            .await
            .context("Failed to read tarball bytes")?;

        // Create a temporary directory for extraction
        let tmp_dir = dest
            .parent()
            .unwrap_or_else(|| Path::new("/tmp"))
            .join(format!(".tmp-{}-{}", owner, repo));

        // Clean up any previous tmp dir
        if tmp_dir.exists() {
            tokio::fs::remove_dir_all(&tmp_dir).await.ok();
        }
        tokio::fs::create_dir_all(&tmp_dir)
            .await
            .context("Failed to create temp directory")?;

        // Extract tarball (gzip-compressed tar)
        extract_tarball(&bytes, &tmp_dir)?;

        // GitHub tarballs extract into a directory like `owner-repo-sha/`
        // Find the first directory inside tmp_dir
        let extracted_dir = find_extracted_dir(&tmp_dir).await?;

        // Ensure parent exists
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .context("Failed to create skills directory")?;
        }

        // Move extracted content to final destination
        tokio::fs::rename(&extracted_dir, dest).await.or_else(|_| {
            // rename can fail across filesystems, fall back to copy
            let src = extracted_dir.clone();
            let dst = dest.to_path_buf();
            // Use blocking copy since we need recursive dir copy
            std::thread::spawn(move || copy_dir_recursive(&src, &dst))
                .join()
                .map_err(|_| anyhow::anyhow!("Copy thread panicked"))?
        })?;

        // Clean up tmp dir
        tokio::fs::remove_dir_all(&tmp_dir).await.ok();

        Ok(())
    }
}

/// Result of a skill installation
pub struct InstallResult {
    pub name: String,
    pub path: PathBuf,
    pub already_existed: bool,
    pub description: String,
    pub security_report: Option<SecurityReport>,
}

/// Info about an installed skill
pub struct InstalledSkillInfo {
    pub name: String,
    pub description: String,
    pub path: PathBuf,
}

// --- Helper functions ---

/// Parse a repo spec like `owner/repo` or `owner/repo@ref`
fn parse_repo_spec(spec: &str) -> Result<(String, String, Option<String>)> {
    let (repo_part, git_ref) = if let Some((r, reference)) = spec.split_once('@') {
        (r, Some(reference.to_string()))
    } else {
        (spec, None)
    };

    let (owner, repo) = repo_part
        .split_once('/')
        .context("Invalid repo format. Expected: owner/repo or owner/repo@ref")?;

    if owner.is_empty() || repo.is_empty() {
        anyhow::bail!("Invalid repo format. Both owner and repo must be non-empty");
    }

    // Strip any trailing slashes or .git suffix
    let repo = repo.trim_end_matches('/').trim_end_matches(".git");

    Ok((owner.to_string(), repo.to_string(), git_ref))
}

/// Simple base64 decoder (avoids adding a dependency)
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

/// Extract a gzip-compressed tarball to a directory
fn extract_tarball(data: &[u8], dest: &Path) -> Result<()> {
    use std::io::Read;

    // Decompress gzip
    let mut decoder = flate2::read::GzDecoder::new(data);
    let mut decompressed = Vec::new();
    decoder
        .read_to_end(&mut decompressed)
        .context("Failed to decompress gzip tarball")?;

    // Parse tar
    let mut archive = tar::Archive::new(decompressed.as_slice());
    archive
        .unpack(dest)
        .context("Failed to extract tar archive")?;

    Ok(())
}

/// Find the first directory inside a directory (GitHub tarballs extract into owner-repo-sha/)
async fn find_extracted_dir(parent: &Path) -> Result<PathBuf> {
    let mut entries = tokio::fs::read_dir(parent)
        .await
        .context("Failed to read extraction directory")?;

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.is_dir() {
            return Ok(path);
        }
    }

    anyhow::bail!("No directory found after tarball extraction")
}

/// Recursively copy a directory
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)
        .with_context(|| format!("Failed to create directory {}", dst.display()))?;

    for entry in std::fs::read_dir(src)
        .with_context(|| format!("Failed to read directory {}", src.display()))?
    {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path).with_context(|| {
                format!(
                    "Failed to copy {} to {}",
                    src_path.display(),
                    dst_path.display()
                )
            })?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_repo_spec_basic() {
        let (owner, repo, git_ref) = parse_repo_spec("octocat/hello-world").unwrap();
        assert_eq!(owner, "octocat");
        assert_eq!(repo, "hello-world");
        assert!(git_ref.is_none());
    }

    #[test]
    fn test_parse_repo_spec_with_ref() {
        let (owner, repo, git_ref) = parse_repo_spec("owner/repo@v1.0").unwrap();
        assert_eq!(owner, "owner");
        assert_eq!(repo, "repo");
        assert_eq!(git_ref.as_deref(), Some("v1.0"));
    }

    #[test]
    fn test_parse_repo_spec_with_git_suffix() {
        let (owner, repo, _) = parse_repo_spec("owner/repo.git").unwrap();
        assert_eq!(repo, "repo");
        let _ = owner;
    }

    #[test]
    fn test_parse_repo_spec_invalid() {
        assert!(parse_repo_spec("noslash").is_err());
        assert!(parse_repo_spec("/empty-owner").is_err());
        assert!(parse_repo_spec("empty-repo/").is_err());
    }

    #[test]
    fn test_base64_decode() {
        let encoded = "SGVsbG8gV29ybGQ=";
        let decoded = base64_decode(encoded).unwrap();
        assert_eq!(String::from_utf8(decoded).unwrap(), "Hello World");
    }

    #[test]
    fn test_base64_decode_no_padding() {
        let encoded = "SGk";
        let decoded = base64_decode(encoded).unwrap();
        assert_eq!(String::from_utf8(decoded).unwrap(), "Hi");
    }

    #[test]
    fn test_base64_decode_with_newlines() {
        let encoded = "SGVs\nbG8g\nV29y\nbGQ=";
        let decoded = base64_decode(encoded).unwrap();
        assert_eq!(String::from_utf8(decoded).unwrap(), "Hello World");
    }
}
