use serde::{Deserialize, Serialize};

/// Known MCP server preset.
///
/// Presets provide a guided "known good" starting point for setup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerPreset {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<McpEnvVar>,
    pub docs_url: Option<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
}

/// Environment variable requirement for an MCP preset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpEnvVar {
    pub key: String,
    pub description: String,
    pub required: bool,
    /// If true, value should be stored in vault and referenced as `vault://...`.
    pub secret: bool,
    /// Vault key name without `vault://` prefix.
    pub vault_key: String,
}

/// Return all curated MCP server presets.
pub fn all_mcp_presets() -> Vec<McpServerPreset> {
    vec![
        McpServerPreset {
            id: "filesystem".to_string(),
            display_name: "Filesystem".to_string(),
            description: "Read/write local files inside a configured directory.".to_string(),
            command: "npx".to_string(),
            args: vec![
                "-y".to_string(),
                "@modelcontextprotocol/server-filesystem".to_string(),
                "{{workspace}}".to_string(),
            ],
            env: vec![],
            docs_url: Some("https://github.com/modelcontextprotocol/servers".to_string()),
            aliases: vec!["fs".to_string(), "files".to_string()],
            keywords: vec![
                "files".to_string(),
                "document".to_string(),
                "folder".to_string(),
                "filesystem".to_string(),
            ],
        },
        McpServerPreset {
            id: "github".to_string(),
            display_name: "GitHub".to_string(),
            description: "Access GitHub repositories, issues, PRs, and metadata.".to_string(),
            command: "npx".to_string(),
            args: vec![
                "-y".to_string(),
                "@modelcontextprotocol/server-github".to_string(),
            ],
            env: vec![McpEnvVar {
                key: "GITHUB_PERSONAL_ACCESS_TOKEN".to_string(),
                description: "GitHub Personal Access Token (classic or fine-grained).".to_string(),
                required: true,
                secret: true,
                vault_key: "mcp.github.token".to_string(),
            }],
            docs_url: Some("https://github.com/modelcontextprotocol/servers".to_string()),
            aliases: vec!["gh".to_string()],
            keywords: vec![
                "github".to_string(),
                "repo".to_string(),
                "pull request".to_string(),
                "issue".to_string(),
                "code".to_string(),
            ],
        },
        McpServerPreset {
            id: "fetch".to_string(),
            display_name: "Web Fetch".to_string(),
            description: "Fetch and read web pages through MCP.".to_string(),
            command: "npx".to_string(),
            args: vec![
                "-y".to_string(),
                "@modelcontextprotocol/server-fetch".to_string(),
            ],
            env: vec![],
            docs_url: Some("https://github.com/modelcontextprotocol/servers".to_string()),
            aliases: vec!["web".to_string(), "http".to_string()],
            keywords: vec![
                "web".to_string(),
                "url".to_string(),
                "page".to_string(),
                "fetch".to_string(),
            ],
        },
        McpServerPreset {
            id: "gmail".to_string(),
            display_name: "Gmail".to_string(),
            description: "Search and read Gmail messages via OAuth-backed MCP server.".to_string(),
            command: "npx".to_string(),
            args: vec![
                "-y".to_string(),
                "@modelcontextprotocol/server-gmail".to_string(),
            ],
            env: vec![
                McpEnvVar {
                    key: "GOOGLE_CLIENT_ID".to_string(),
                    description: "Google OAuth Client ID.".to_string(),
                    required: true,
                    secret: true,
                    vault_key: "mcp.gmail.client_id".to_string(),
                },
                McpEnvVar {
                    key: "GOOGLE_CLIENT_SECRET".to_string(),
                    description: "Google OAuth Client Secret.".to_string(),
                    required: true,
                    secret: true,
                    vault_key: "mcp.gmail.client_secret".to_string(),
                },
                McpEnvVar {
                    key: "GOOGLE_REFRESH_TOKEN".to_string(),
                    description: "Google OAuth Refresh Token.".to_string(),
                    required: true,
                    secret: true,
                    vault_key: "mcp.gmail.refresh_token".to_string(),
                },
            ],
            docs_url: Some("https://github.com/modelcontextprotocol/servers".to_string()),
            aliases: vec!["email".to_string(), "google-mail".to_string()],
            keywords: vec![
                "gmail".to_string(),
                "mail".to_string(),
                "email".to_string(),
                "inbox".to_string(),
            ],
        },
        McpServerPreset {
            id: "google-calendar".to_string(),
            display_name: "Google Calendar".to_string(),
            description: "Read and manage Google Calendar events.".to_string(),
            command: "npx".to_string(),
            args: vec![
                "-y".to_string(),
                "@modelcontextprotocol/server-google-calendar".to_string(),
            ],
            env: vec![
                McpEnvVar {
                    key: "GOOGLE_CLIENT_ID".to_string(),
                    description: "Google OAuth Client ID.".to_string(),
                    required: true,
                    secret: true,
                    vault_key: "mcp.gcal.client_id".to_string(),
                },
                McpEnvVar {
                    key: "GOOGLE_CLIENT_SECRET".to_string(),
                    description: "Google OAuth Client Secret.".to_string(),
                    required: true,
                    secret: true,
                    vault_key: "mcp.gcal.client_secret".to_string(),
                },
                McpEnvVar {
                    key: "GOOGLE_REFRESH_TOKEN".to_string(),
                    description: "Google OAuth Refresh Token.".to_string(),
                    required: true,
                    secret: true,
                    vault_key: "mcp.gcal.refresh_token".to_string(),
                },
            ],
            docs_url: Some("https://github.com/modelcontextprotocol/servers".to_string()),
            aliases: vec!["calendar".to_string(), "gcal".to_string()],
            keywords: vec![
                "calendar".to_string(),
                "event".to_string(),
                "meeting".to_string(),
                "schedule".to_string(),
            ],
        },
        McpServerPreset {
            id: "notion".to_string(),
            display_name: "Notion".to_string(),
            description: "Access Notion pages and databases.".to_string(),
            command: "npx".to_string(),
            args: vec![
                "-y".to_string(),
                "@modelcontextprotocol/server-notion".to_string(),
            ],
            env: vec![McpEnvVar {
                key: "NOTION_TOKEN".to_string(),
                description: "Notion integration token.".to_string(),
                required: true,
                secret: true,
                vault_key: "mcp.notion.token".to_string(),
            }],
            docs_url: Some("https://github.com/modelcontextprotocol/servers".to_string()),
            aliases: vec!["notes".to_string()],
            keywords: vec![
                "notion".to_string(),
                "notes".to_string(),
                "database".to_string(),
                "workspace".to_string(),
            ],
        },
    ]
}

/// Find a preset by id or alias.
pub fn find_mcp_preset(query: &str) -> Option<McpServerPreset> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return None;
    }
    all_mcp_presets().into_iter().find(|preset| {
        preset.id.eq_ignore_ascii_case(&q)
            || preset
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(&q))
    })
}

/// Suggest presets matching free-text user intent.
pub fn suggest_mcp_presets(text: &str) -> Vec<McpServerPreset> {
    let t = text.to_lowercase();
    if t.trim().is_empty() {
        return vec![];
    }

    let mut scored: Vec<(i32, McpServerPreset)> = all_mcp_presets()
        .into_iter()
        .filter_map(|preset| {
            let mut score = 0;
            if t.contains(&preset.id.to_lowercase()) {
                score += 4;
            }
            if preset
                .aliases
                .iter()
                .any(|alias| t.contains(&alias.to_lowercase()))
            {
                score += 3;
            }
            let keyword_hits = preset
                .keywords
                .iter()
                .filter(|kw| t.contains(&kw.to_lowercase()))
                .count() as i32;
            score += keyword_hits;

            if score > 0 {
                Some((score, preset))
            } else {
                None
            }
        })
        .collect();

    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.id.cmp(&b.1.id)));
    scored.into_iter().map(|(_, preset)| preset).collect()
}

#[cfg(test)]
mod tests {
    use super::{find_mcp_preset, suggest_mcp_presets};

    #[test]
    fn find_by_alias() {
        let preset = find_mcp_preset("gh").expect("expected github preset");
        assert_eq!(preset.id, "github");
    }

    #[test]
    fn suggest_from_email_text() {
        let suggestions = suggest_mcp_presets("read my emails and inbox");
        assert!(!suggestions.is_empty());
        assert_eq!(suggestions[0].id, "gmail");
    }
}
