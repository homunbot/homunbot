use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;

use super::registry::{get_string_param, Tool, ToolContext, ToolResult};

/// Maximum content length for web_fetch (chars)
const MAX_FETCH_CHARS: usize = 50_000;
/// Maximum redirects for web_fetch
const MAX_REDIRECTS: usize = 5;

// =============================================================================
// WebSearchTool — Brave Search API
// =============================================================================

/// Web search via Brave Search API.
///
/// Returns structured search results (title, URL, description).
/// Follows nanobot's WebSearchTool pattern.
pub struct WebSearchTool {
    client: Client,
    api_key: String,
    max_results: u32,
}

impl WebSearchTool {
    pub fn new(api_key: &str, max_results: u32) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.to_string(),
            max_results,
        }
    }
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the web using Brave Search. Returns a list of relevant results with titles, URLs, and descriptions. \
         Use this to discover URLs, then use web_fetch to read specific pages."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query"
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<ToolResult> {
        let query = get_string_param(&args, "query")?;

        if self.api_key.is_empty() {
            return Ok(ToolResult::error(
                "Brave Search API key not configured. Set it in [tools.web_search] api_key"
                    .to_string(),
            ));
        }

        let response = self
            .client
            .get("https://api.search.brave.com/res/v1/web/search")
            .header("Accept", "application/json")
            .header("X-Subscription-Token", &self.api_key)
            .query(&[
                ("q", query.as_str()),
                ("count", &self.max_results.to_string()),
            ])
            .send()
            .await;

        let response = match response {
            Ok(r) => r,
            Err(e) => return Ok(ToolResult::error(format!("Search request failed: {e}"))),
        };

        if !response.status().is_success() {
            return Ok(ToolResult::error(format!(
                "Brave Search API error: HTTP {}",
                response.status()
            )));
        }

        let body: BraveSearchResponse = match response.json().await {
            Ok(b) => b,
            Err(e) => {
                return Ok(ToolResult::error(format!(
                    "Failed to parse search response: {e}"
                )))
            }
        };

        let results = body.web.map(|w| w.results).unwrap_or_default();

        if results.is_empty() {
            return Ok(ToolResult::success(format!(
                "No results found for: {query}"
            )));
        }

        let mut output = format!("Results for: {query}\n");
        for (i, result) in results.iter().enumerate() {
            output.push_str(&format!(
                "\n{}. {}\n   {}\n   {}\n",
                i + 1,
                result.title,
                result.url,
                result.description.as_deref().unwrap_or("(no description)")
            ));
        }

        Ok(ToolResult::success(output))
    }
}

// --- Brave Search API response types ---

#[derive(Deserialize)]
struct BraveSearchResponse {
    web: Option<BraveWebResults>,
}

#[derive(Deserialize)]
struct BraveWebResults {
    results: Vec<BraveResult>,
}

#[derive(Deserialize)]
struct BraveResult {
    title: String,
    url: String,
    description: Option<String>,
}

// =============================================================================
// WebFetchTool — fetch a URL and return its text content
// =============================================================================

/// Fetch a web page and return its text content.
///
/// Uses reqwest to GET the URL, extracts text, truncates if too long.
/// Validates URLs and limits redirects (following nanobot's safety pattern).
pub struct WebFetchTool {
    client: Client,
}

impl WebFetchTool {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .redirect(reqwest::redirect::Policy::limited(MAX_REDIRECTS))
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| Client::new()),
        }
    }
}

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn description(&self) -> &str {
        "Fetch and extract readable content from a URL you already know (HTML → text). \
         This is a simple HTTP GET request, NOT a browser. It cannot render JavaScript, \
         interact with pages, or perform searches.\n\
         \n\
         ⚠️ ROUTING RULES:\n\
         - User says \"vai su\", \"apri\", \"cerca su Google\", \"naviga\" → use BROWSER, not this tool\n\
         - You need to read a known article/doc URL → use this tool\n\
         - You need to click, type, fill forms, or browse interactively → use BROWSER\n\
         - You need search results but have no web_search tool → use BROWSER to navigate to a search engine\n\
         \n\
         This tool is faster than browser but ONLY works for static content at known URLs."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to fetch"
                }
            },
            "required": ["url"]
        })
    }

    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<ToolResult> {
        let url = get_string_param(&args, "url")?;

        // Basic URL validation
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Ok(ToolResult::error(
                "Invalid URL: must start with http:// or https://".to_string(),
            ));
        }

        let response = match self.client.get(&url).send().await {
            Ok(r) => r,
            Err(e) => return Ok(ToolResult::error(format!("Failed to fetch URL: {e}"))),
        };

        let status = response.status();
        let final_url = response.url().to_string();

        if !status.is_success() {
            return Ok(ToolResult::error(format!(
                "HTTP error: {status} for {final_url}"
            )));
        }

        let body = match response.text().await {
            Ok(t) => t,
            Err(e) => {
                return Ok(ToolResult::error(format!(
                    "Failed to read response body: {e}"
                )))
            }
        };

        // Strip HTML tags (basic — for a proper solution we'd use a crate like `scraper`)
        let text = strip_html_tags(&body);

        let truncated = text.len() > MAX_FETCH_CHARS;
        let content = if truncated {
            &text[..MAX_FETCH_CHARS]
        } else {
            &text
        };

        let mut output = String::new();
        if final_url != url {
            output.push_str(&format!("Redirected to: {final_url}\n\n"));
        }
        output.push_str(content);
        if truncated {
            output.push_str(&format!(
                "\n\n... [truncated at {MAX_FETCH_CHARS} chars, total: {} chars]",
                text.len()
            ));
        }

        Ok(ToolResult::success(output))
    }
}

/// Basic HTML tag stripper — removes tags and decodes common entities.
/// Not perfect, but good enough for extracting readable text from most pages.
fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len() / 2);
    let mut in_tag = false;
    let mut in_script = false;
    let mut in_style = false;

    let chars: Vec<char> = html.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        if in_script || in_style {
            // Look for closing tag
            if chars[i] == '<' && i + 1 < len && chars[i + 1] == '/' {
                let rest: String = chars[i..].iter().take(20).collect();
                let rest_lower = rest.to_lowercase();
                if in_script && rest_lower.starts_with("</script") {
                    in_script = false;
                } else if in_style && rest_lower.starts_with("</style") {
                    in_style = false;
                }
            }
            i += 1;
            continue;
        }

        if chars[i] == '<' {
            // Check for script/style opening tags
            let rest: String = chars[i..].iter().take(20).collect();
            let rest_lower = rest.to_lowercase();
            if rest_lower.starts_with("<script") {
                in_script = true;
            } else if rest_lower.starts_with("<style") {
                in_style = true;
            }
            in_tag = true;
            i += 1;
            continue;
        }

        if chars[i] == '>' && in_tag {
            in_tag = false;
            // Add space/newline for block elements
            result.push(' ');
            i += 1;
            continue;
        }

        if !in_tag {
            // Handle HTML entities
            if chars[i] == '&' {
                let entity: String = chars[i..]
                    .iter()
                    .take(10)
                    .take_while(|c| **c != ';')
                    .collect();
                let entity_with_semi = format!("{entity};");
                let decoded = match entity_with_semi.as_str() {
                    "&amp;" => "&",
                    "&lt;" => "<",
                    "&gt;" => ">",
                    "&quot;" => "\"",
                    "&apos;" => "'",
                    "&nbsp;" => " ",
                    _ => {
                        result.push(chars[i]);
                        i += 1;
                        continue;
                    }
                };
                result.push_str(decoded);
                i += entity_with_semi.len();
                continue;
            }

            result.push(chars[i]);
        }

        i += 1;
    }

    // Collapse multiple whitespace/newlines
    let mut collapsed = String::with_capacity(result.len());
    let mut last_was_space = false;
    for c in result.chars() {
        if c.is_whitespace() {
            if !last_was_space {
                collapsed.push(if c == '\n' { '\n' } else { ' ' });
            }
            last_was_space = true;
        } else {
            collapsed.push(c);
            last_was_space = false;
        }
    }

    collapsed.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_html_basic() {
        let html = "<html><body><h1>Hello</h1><p>World</p></body></html>";
        let text = strip_html_tags(html);
        assert!(text.contains("Hello"));
        assert!(text.contains("World"));
        assert!(!text.contains("<"));
    }

    #[test]
    fn test_strip_html_entities() {
        let html = "Hello &amp; World &lt;3&gt;";
        let text = strip_html_tags(html);
        assert!(text.contains("Hello & World <3>"));
    }

    #[test]
    fn test_strip_html_script() {
        let html = "<p>Before</p><script>var x = 1;</script><p>After</p>";
        let text = strip_html_tags(html);
        assert!(text.contains("Before"));
        assert!(text.contains("After"));
        assert!(!text.contains("var x"));
    }

    #[test]
    fn test_strip_html_style() {
        let html = "<p>Text</p><style>.foo { color: red; }</style><p>More</p>";
        let text = strip_html_tags(html);
        assert!(text.contains("Text"));
        assert!(text.contains("More"));
        assert!(!text.contains("color"));
    }

    fn test_ctx() -> ToolContext {
        ToolContext {
            workspace: "/tmp".to_string(),
            channel: "cli".to_string(),
            chat_id: "test".to_string(),
            message_tx: None,
            approval_manager: None,
            skill_env: None,
        }
    }

    #[tokio::test]
    async fn test_web_search_no_api_key() {
        let tool = WebSearchTool::new("", 5);
        let args = serde_json::json!({"query": "test"});
        let result = tool.execute(args, &test_ctx()).await.unwrap();
        assert!(result.is_error);
        assert!(result.output.contains("API key not configured"));
    }

    #[tokio::test]
    async fn test_web_fetch_invalid_url() {
        let tool = WebFetchTool::new();
        let args = serde_json::json!({"url": "not-a-url"});
        let result = tool.execute(args, &test_ctx()).await.unwrap();
        assert!(result.is_error);
        assert!(result.output.contains("Invalid URL"));
    }
}
