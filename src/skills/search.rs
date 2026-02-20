use anyhow::{Context, Result};
use serde::Deserialize;

/// Search result from GitHub for agent skills
pub struct SkillSearchResult {
    pub full_name: String,
    pub description: String,
    pub stars: u32,
    pub updated_at: String,
    pub url: String,
}

/// Search for agent skills on GitHub using the Search API
pub struct SkillSearcher {
    client: reqwest::Client,
}

impl SkillSearcher {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .user_agent("homun")
            .build()
            .expect("Failed to create HTTP client");
        Self { client }
    }

    /// Search GitHub for repositories matching query with `agentskills` topic
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<SkillSearchResult>> {
        let search_query = format!("{query} topic:agentskills");
        let url = format!(
            "https://api.github.com/search/repositories?q={}&sort=stars&order=desc&per_page={}",
            urlencoded(&search_query),
            limit.min(30) // GitHub API max per_page is 100, keep it reasonable
        );

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to search GitHub")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("GitHub API error ({status}): {body}");
        }

        let search_response: GitHubSearchResponse = response
            .json()
            .await
            .context("Failed to parse GitHub search response")?;

        let results = search_response
            .items
            .into_iter()
            .take(limit)
            .map(|item| SkillSearchResult {
                full_name: item.full_name,
                description: item.description.unwrap_or_default(),
                stars: item.stargazers_count,
                updated_at: item.updated_at.chars().take(10).collect(), // YYYY-MM-DD
                url: item.html_url,
            })
            .collect();

        Ok(results)
    }
}

/// Simple URL encoding for query parameters
fn urlencoded(s: &str) -> String {
    s.replace(' ', "+")
        .replace('&', "%26")
        .replace('=', "%3D")
        .replace('#', "%23")
}

// --- GitHub API response types ---

#[derive(Deserialize)]
struct GitHubSearchResponse {
    items: Vec<GitHubRepo>,
}

#[derive(Deserialize)]
struct GitHubRepo {
    full_name: String,
    description: Option<String>,
    stargazers_count: u32,
    updated_at: String,
    html_url: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_github_response() {
        let json = r#"{
            "total_count": 1,
            "items": [
                {
                    "full_name": "owner/skill-repo",
                    "description": "A test skill",
                    "stargazers_count": 42,
                    "updated_at": "2025-01-15T10:30:00Z",
                    "html_url": "https://github.com/owner/skill-repo"
                }
            ]
        }"#;

        let response: GitHubSearchResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.items.len(), 1);
        assert_eq!(response.items[0].full_name, "owner/skill-repo");
        assert_eq!(response.items[0].stargazers_count, 42);
    }

    #[test]
    fn test_urlencoded() {
        assert_eq!(urlencoded("hello world"), "hello+world");
        assert_eq!(urlencoded("a&b=c"), "a%26b%3Dc");
    }
}
