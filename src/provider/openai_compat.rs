use std::collections::HashMap;

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::traits::*;

/// OpenAI-compatible provider — covers OpenRouter, Ollama, OpenAI, DeepSeek, Groq,
/// and any API that implements the OpenAI chat completions format.
pub struct OpenAICompatProvider {
    client: Client,
    api_key: String,
    api_base: String,
    provider_name: String,
    extra_headers: HashMap<String, String>,
}

impl OpenAICompatProvider {
    pub fn new(
        api_key: &str,
        api_base: &str,
        provider_name: &str,
        extra_headers: HashMap<String, String>,
    ) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.to_string(),
            api_base: api_base.trim_end_matches('/').to_string(),
            provider_name: provider_name.to_string(),
            extra_headers,
        }
    }

    /// Create provider from config, auto-detecting the API base URL
    pub fn from_config(
        provider_name: &str,
        api_key: &str,
        api_base: Option<&str>,
        extra_headers: HashMap<String, String>,
    ) -> Self {
        let base = api_base
            .map(|s| s.to_string())
            .unwrap_or_else(|| default_api_base(provider_name));

        Self::new(api_key, &base, provider_name, extra_headers)
    }

    /// Resolve model name — strip provider prefix if present (e.g., "anthropic/claude-*" → "claude-*")
    fn resolve_model(&self, model: &str) -> String {
        // For OpenRouter, keep the full model name (it needs the prefix)
        if self.provider_name == "openrouter" {
            return model.to_string();
        }

        // For other providers, strip the provider prefix
        if let Some(stripped) = model.split_once('/') {
            stripped.1.to_string()
        } else {
            model.to_string()
        }
    }
}

/// Default API base URLs for known providers.
///
/// All these providers expose an OpenAI-compatible chat/completions endpoint.
/// Anthropic is handled by a separate native provider.
fn default_api_base(provider_name: &str) -> String {
    match provider_name {
        "openai"    => "https://api.openai.com/v1".to_string(),
        "openrouter"=> "https://openrouter.ai/api/v1".to_string(),
        "deepseek"  => "https://api.deepseek.com/v1".to_string(),
        "groq"      => "https://api.groq.com/openai/v1".to_string(),
        "gemini"    => "https://generativelanguage.googleapis.com/v1beta/openai".to_string(),
        "minimax"   => "https://api.minimax.chat/v1".to_string(),
        "aihubmix"  => "https://aihubmix.com/v1".to_string(),
        "dashscope" => "https://dashscope.aliyuncs.com/compatible-mode/v1".to_string(),
        "moonshot"  => "https://api.moonshot.cn/v1".to_string(),
        "zhipu"     => "https://open.bigmodel.cn/api/paas/v4".to_string(),
        "ollama"    => "http://localhost:11434/v1".to_string(),
        "vllm"      => "http://localhost:8000/v1".to_string(),
        _           => "https://api.openai.com/v1".to_string(),
    }
}

// --- OpenAI API request/response types ---

#[derive(Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<ToolDefinition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<String>,
}

#[derive(Deserialize)]
struct OpenAIResponse {
    choices: Vec<OpenAIChoice>,
    #[serde(default)]
    usage: Option<OpenAIUsage>,
}

#[derive(Deserialize)]
struct OpenAIChoice {
    message: OpenAIMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct OpenAIMessage {
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<OpenAIToolCall>>,
}

#[derive(Deserialize)]
struct OpenAIToolCall {
    id: String,
    function: OpenAIFunction,
}

#[derive(Deserialize)]
struct OpenAIFunction {
    name: String,
    arguments: String,
}

#[derive(Deserialize)]
struct OpenAIUsage {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
    #[serde(default)]
    total_tokens: u32,
}

#[derive(Deserialize)]
struct OpenAIErrorResponse {
    error: Option<OpenAIError>,
}

#[derive(Deserialize)]
struct OpenAIError {
    message: String,
}

#[async_trait::async_trait]
impl Provider for OpenAICompatProvider {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        let model = self.resolve_model(&request.model);
        let url = format!("{}/chat/completions", self.api_base);

        let has_tools = !request.tools.is_empty();

        let body = OpenAIRequest {
            model,
            messages: request.messages,
            max_tokens: Some(request.max_tokens.max(1)),
            temperature: Some(request.temperature),
            tools: request.tools,
            tool_choice: if has_tools { Some("auto".to_string()) } else { None },
        };

        let mut req = self
            .client
            .post(&url)
            .header("Content-Type", "application/json");

        if !self.api_key.is_empty() {
            req = req.bearer_auth(&self.api_key);
        }

        for (key, value) in &self.extra_headers {
            req = req.header(key, value);
        }

        let response = req
            .json(&body)
            .send()
            .await
            .with_context(|| format!("Failed to send request to {}", url))?;

        let status = response.status();
        let response_text = response.text().await
            .context("Failed to read response body")?;

        if !status.is_success() {
            let error_msg = serde_json::from_str::<OpenAIErrorResponse>(&response_text)
                .ok()
                .and_then(|e| e.error)
                .map(|e| e.message)
                .unwrap_or_else(|| format!("HTTP {}: {}", status, response_text));
            anyhow::bail!("Provider {} error: {}", self.provider_name, error_msg);
        }

        let api_response: OpenAIResponse = serde_json::from_str(&response_text)
            .with_context(|| format!("Failed to parse response from {}", self.provider_name))?;

        let choice = api_response
            .choices
            .into_iter()
            .next()
            .context("No choices in response")?;

        let tool_calls: Vec<ToolCallRequest> = choice
            .message
            .tool_calls
            .unwrap_or_default()
            .into_iter()
            .map(|tc| {
                let arguments = serde_json::from_str(&tc.function.arguments)
                    .or_else(|_| {
                        // Attempt JSON repair for malformed LLM output
                        let repaired = repair_json(&tc.function.arguments);
                        tracing::debug!(
                            original = %tc.function.arguments,
                            repaired = %repaired,
                            "Repaired malformed tool call JSON"
                        );
                        serde_json::from_str(&repaired)
                    })
                    .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
                ToolCallRequest {
                    id: tc.id,
                    name: tc.function.name,
                    arguments,
                }
            })
            .collect();

        let finish_reason = choice.finish_reason.unwrap_or_else(|| "stop".to_string());

        let usage = api_response.usage.map(|u| Usage {
            prompt_tokens: u.prompt_tokens,
            completion_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
        }).unwrap_or_default();

        Ok(ChatResponse {
            content: choice.message.content,
            tool_calls,
            finish_reason,
            usage,
        })
    }

    fn name(&self) -> &str {
        &self.provider_name
    }
}

/// Attempt to repair malformed JSON from LLM tool call arguments.
///
/// Common issues from small/local models:
/// - Trailing commas: `{"key": "value",}`
/// - Single quotes: `{'key': 'value'}`
/// - Unquoted keys: `{key: "value"}`
/// - Trailing text after JSON: `{"key": "value"} and some other text`
/// - Missing closing braces: `{"key": "value"`
fn repair_json(input: &str) -> String {
    let mut s = input.trim().to_string();

    // Extract just the JSON object/array if there's trailing text
    if let Some(start) = s.find('{') {
        let mut depth = 0;
        let mut in_string = false;
        let mut escape = false;
        let mut end = s.len();

        for (i, c) in s[start..].char_indices() {
            if escape {
                escape = false;
                continue;
            }
            match c {
                '\\' if in_string => escape = true,
                '"' => in_string = !in_string,
                '{' if !in_string => depth += 1,
                '}' if !in_string => {
                    depth -= 1;
                    if depth == 0 {
                        end = start + i + 1;
                        break;
                    }
                }
                _ => {}
            }
        }

        s = s[start..end].to_string();

        // If braces are unbalanced, add closing ones
        let open = s.chars().filter(|c| *c == '{').count();
        let close = s.chars().filter(|c| *c == '}').count();
        for _ in 0..(open.saturating_sub(close)) {
            s.push('}');
        }
    }

    // Replace single quotes with double quotes (but not inside already-double-quoted strings)
    // Simple heuristic: if no double quotes exist at all, replace all single quotes
    if !s.contains('"') {
        s = s.replace('\'', "\"");
    }

    // Remove trailing commas before } or ]
    let re_trailing = regex::Regex::new(r",\s*([}\]])").unwrap();
    s = re_trailing.replace_all(&s, "$1").to_string();

    // Quote unquoted keys: {key: "value"} → {"key": "value"}
    let re_unquoted = regex::Regex::new(r"(?m)\{\s*([a-zA-Z_]\w*)\s*:").unwrap();
    s = re_unquoted.replace_all(&s, r#"{"$1":"#).to_string();
    // Also handle comma-separated unquoted keys
    let re_unquoted2 = regex::Regex::new(r",\s*([a-zA-Z_]\w*)\s*:").unwrap();
    s = re_unquoted2.replace_all(&s, r#","$1":"#).to_string();

    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_model_openrouter() {
        let provider = OpenAICompatProvider::new("key", "https://openrouter.ai/api/v1", "openrouter", HashMap::new());
        assert_eq!(provider.resolve_model("anthropic/claude-sonnet-4-20250514"), "anthropic/claude-sonnet-4-20250514");
    }

    #[test]
    fn test_resolve_model_strip_prefix() {
        let provider = OpenAICompatProvider::new("key", "https://api.openai.com/v1", "openai", HashMap::new());
        assert_eq!(provider.resolve_model("openai/gpt-4"), "gpt-4");
    }

    #[test]
    fn test_resolve_model_no_prefix() {
        let provider = OpenAICompatProvider::new("key", "http://localhost:11434/v1", "ollama", HashMap::new());
        assert_eq!(provider.resolve_model("llama3"), "llama3");
    }

    #[test]
    fn test_default_api_bases() {
        assert!(default_api_base("openrouter").contains("openrouter.ai"));
        assert!(default_api_base("ollama").contains("localhost:11434"));
        assert!(default_api_base("deepseek").contains("deepseek.com"));
    }

    // --- JSON repair tests ---

    #[test]
    fn test_repair_trailing_comma() {
        let input = r#"{"command": "echo hello",}"#;
        let repaired = repair_json(input);
        let parsed: serde_json::Value = serde_json::from_str(&repaired).unwrap();
        assert_eq!(parsed["command"], "echo hello");
    }

    #[test]
    fn test_repair_single_quotes() {
        let input = "{'command': 'ls -la'}";
        let repaired = repair_json(input);
        let parsed: serde_json::Value = serde_json::from_str(&repaired).unwrap();
        assert_eq!(parsed["command"], "ls -la");
    }

    #[test]
    fn test_repair_trailing_text() {
        let input = r#"{"path": "/tmp"} Let me read that file for you."#;
        let repaired = repair_json(input);
        let parsed: serde_json::Value = serde_json::from_str(&repaired).unwrap();
        assert_eq!(parsed["path"], "/tmp");
    }

    #[test]
    fn test_repair_unquoted_keys() {
        let input = r#"{command: "echo test"}"#;
        let repaired = repair_json(input);
        let parsed: serde_json::Value = serde_json::from_str(&repaired).unwrap();
        assert_eq!(parsed["command"], "echo test");
    }

    #[test]
    fn test_repair_missing_closing_brace() {
        let input = r#"{"command": "echo hello""#;
        let repaired = repair_json(input);
        let parsed: serde_json::Value = serde_json::from_str(&repaired).unwrap();
        assert_eq!(parsed["command"], "echo hello");
    }

    #[test]
    fn test_repair_valid_json_unchanged() {
        let input = r#"{"command": "echo hello", "working_dir": "/tmp"}"#;
        let repaired = repair_json(input);
        let parsed: serde_json::Value = serde_json::from_str(&repaired).unwrap();
        assert_eq!(parsed["command"], "echo hello");
        assert_eq!(parsed["working_dir"], "/tmp");
    }

    #[test]
    fn test_repair_multiple_unquoted_keys() {
        let input = r#"{command: "echo test", working_dir: "/tmp"}"#;
        let repaired = repair_json(input);
        let parsed: serde_json::Value = serde_json::from_str(&repaired).unwrap();
        assert_eq!(parsed["command"], "echo test");
        assert_eq!(parsed["working_dir"], "/tmp");
    }
}
