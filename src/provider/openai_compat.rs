use std::collections::HashMap;

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::traits::*;
use futures::StreamExt as _;

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
        // Primary providers
        "openai" => "https://api.openai.com/v1".to_string(),
        "openrouter" => "https://openrouter.ai/api/v1".to_string(),
        "gemini" => "https://generativelanguage.googleapis.com/v1beta/openai".to_string(),
        // Local providers
        "ollama" => "http://localhost:11434/v1".to_string(),
        "ollama_cloud" => "https://ollama.com/v1".to_string(),
        "vllm" => "http://localhost:8000/v1".to_string(),
        // Cloud providers (OpenAI-compatible)
        "deepseek" => "https://api.deepseek.com/v1".to_string(),
        "groq" => "https://api.groq.com/openai/v1".to_string(),
        "mistral" => "https://api.mistral.ai/v1".to_string(),
        "xai" => "https://api.x.ai/v1".to_string(),
        "together" => "https://api.together.xyz/v1".to_string(),
        "fireworks" => "https://api.fireworks.ai/inference/v1".to_string(),
        "perplexity" => "https://api.perplexity.ai".to_string(),
        "cohere" => "https://api.cohere.ai/compatibility/v1".to_string(),
        "venice" => "https://api.venice.ai/api/v1".to_string(),
        // Gateways/aggregators
        "aihubmix" => "https://aihubmix.com/v1".to_string(),
        "vercel" => "https://api.vercel.ai/v1".to_string(),
        "cloudflare" => "https://gateway.ai.cloudflare.com/v1".to_string(),
        "copilot" => "https://api.githubcopilot.com".to_string(),
        "bedrock" => "https://bedrock-runtime.us-east-1.amazonaws.com".to_string(),
        // Chinese providers
        "minimax" => "https://api.minimax.chat/v1".to_string(),
        "dashscope" => "https://dashscope.aliyuncs.com/compatible-mode/v1".to_string(),
        "moonshot" => "https://api.moonshot.cn/v1".to_string(),
        "zhipu" => "https://open.bigmodel.cn/api/paas/v4".to_string(),
        // Fallback
        _ => "https://api.openai.com/v1".to_string(),
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
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

/// SSE streaming chunk from OpenAI-compatible API
#[derive(Deserialize)]
struct OpenAIStreamChunk {
    choices: Vec<OpenAIStreamChoice>,
}

#[derive(Deserialize)]
struct OpenAIStreamChoice {
    delta: OpenAIStreamDelta,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct OpenAIStreamDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<OpenAIStreamToolCall>>,
}

/// A tool call delta in an SSE chunk. The `id` and `function.name` arrive
/// in the first chunk for each tool call; subsequent chunks for the same
/// `index` append to `function.arguments`.
#[derive(Deserialize)]
struct OpenAIStreamToolCall {
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<OpenAIStreamToolFn>,
}

#[derive(Deserialize)]
struct OpenAIStreamToolFn {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
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
            tool_choice: if has_tools {
                Some("auto".to_string())
            } else {
                None
            },
            stream: None,
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

        tracing::debug!(
            provider = %self.provider_name,
            url = %url,
            model = %body.model,
            has_tools = has_tools,
            "Sending chat request to OpenAI-compatible provider"
        );

        let response = req.json(&body).send().await.with_context(|| {
            format!(
                "Failed to send request to {} (provider: {})",
                url, self.provider_name
            )
        })?;

        let status = response.status();
        let response_text = response
            .text()
            .await
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

        let usage = api_response
            .usage
            .map(|u| Usage {
                prompt_tokens: u.prompt_tokens,
                completion_tokens: u.completion_tokens,
                total_tokens: u.total_tokens,
            })
            .unwrap_or_default();

        Ok(ChatResponse {
            content: choice.message.content,
            tool_calls,
            finish_reason,
            usage,
        })
    }

    async fn chat_stream(
        &self,
        request: ChatRequest,
        tx: tokio::sync::mpsc::Sender<StreamChunk>,
    ) -> Result<ChatResponse> {
        let model = self.resolve_model(&request.model);
        let url = format!("{}/chat/completions", self.api_base);

        let body = OpenAIRequest {
            model,
            messages: request.messages,
            max_tokens: Some(request.max_tokens.max(1)),
            temperature: Some(request.temperature),
            tools: request.tools,
            tool_choice: None,
            stream: Some(true),
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
            .with_context(|| format!("Failed to send streaming request to {}", url))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!(
                "Provider {} streaming error: HTTP {}: {}",
                self.provider_name,
                status,
                text
            );
        }

        // Read SSE stream — accumulate text content and tool call deltas.
        let mut full_content = String::new();
        let mut bytes_stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut finish_reason = String::from("stop");

        // Tool call accumulator: index → (id, name, arguments_buffer)
        let mut tc_acc: std::collections::HashMap<usize, (String, String, String)> =
            std::collections::HashMap::new();

        while let Some(chunk_result) = bytes_stream.next().await {
            let chunk = chunk_result.context("Error reading SSE stream")?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            // Process complete SSE lines from the buffer
            while let Some(line_end) = buffer.find('\n') {
                let line = buffer[..line_end].trim().to_string();
                buffer = buffer[line_end + 1..].to_string();

                if line.is_empty() || line.starts_with(':') {
                    continue;
                }

                if let Some(data) = line.strip_prefix("data: ") {
                    if data.trim() == "[DONE]" {
                        // Only send done if we were streaming text (not tool calls)
                        if tc_acc.is_empty() {
                            let _ = tx
                                .send(StreamChunk {
                                    delta: String::new(),
                                    done: true,
                                    event_type: None,
                                    tool_call_data: None,
                                })
                                .await;
                        }
                        break;
                    }

                    if let Ok(chunk) = serde_json::from_str::<OpenAIStreamChunk>(data) {
                        if let Some(choice) = chunk.choices.first() {
                            // Text content delta → forward to client
                            if let Some(ref delta) = choice.delta.content {
                                if !delta.is_empty() {
                                    full_content.push_str(delta);
                                    let _ = tx
                                        .send(StreamChunk {
                                            delta: delta.clone(),
                                            done: false,
                                            event_type: None,
                                            tool_call_data: None,
                                        })
                                        .await;
                                }
                            }

                            // Tool call deltas → accumulate
                            if let Some(ref tool_calls) = choice.delta.tool_calls {
                                for tc_delta in tool_calls {
                                    let entry = tc_acc.entry(tc_delta.index).or_insert_with(|| {
                                        (String::new(), String::new(), String::new())
                                    });

                                    if let Some(ref func) = tc_delta.function {
                                        if let Some(ref name) = func.name {
                                            entry.1 = name.clone();
                                        }
                                        if let Some(ref args) = func.arguments {
                                            entry.2.push_str(args);
                                        }
                                    }
                                    if let Some(ref id) = tc_delta.id {
                                        entry.0 = id.clone();
                                    }
                                }
                            }

                            if let Some(ref reason) = choice.finish_reason {
                                finish_reason = reason.clone();
                                if tc_acc.is_empty() {
                                    let _ = tx
                                        .send(StreamChunk {
                                            delta: String::new(),
                                            done: true,
                                            event_type: None,
                                            tool_call_data: None,
                                        })
                                        .await;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Build tool calls from accumulated deltas
        let mut tool_calls: Vec<ToolCallRequest> = Vec::new();
        if !tc_acc.is_empty() {
            let mut indices: Vec<usize> = tc_acc.keys().copied().collect();
            indices.sort();
            for idx in indices {
                let (id, name, raw_args) = tc_acc.remove(&idx).unwrap();
                let id = if id.is_empty() {
                    format!("call_{idx}")
                } else {
                    id
                };
                let args_str = repair_json(&raw_args);
                let arguments: serde_json::Value =
                    serde_json::from_str(&args_str).unwrap_or(serde_json::json!({}));
                tool_calls.push(ToolCallRequest {
                    id,
                    name,
                    arguments,
                });
            }
            finish_reason = "tool_calls".to_string();
        }

        Ok(ChatResponse {
            content: if full_content.is_empty() {
                None
            } else {
                Some(full_content)
            },
            tool_calls,
            finish_reason,
            usage: Usage::default(),
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
///
/// Public wrapper for use by xml_dispatcher
pub(crate) fn repair_json_public(input: &str) -> String {
    repair_json(input)
}

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
        let provider = OpenAICompatProvider::new(
            "key",
            "https://openrouter.ai/api/v1",
            "openrouter",
            HashMap::new(),
        );
        assert_eq!(
            provider.resolve_model("anthropic/claude-sonnet-4-20250514"),
            "anthropic/claude-sonnet-4-20250514"
        );
    }

    #[test]
    fn test_resolve_model_strip_prefix() {
        let provider =
            OpenAICompatProvider::new("key", "https://api.openai.com/v1", "openai", HashMap::new());
        assert_eq!(provider.resolve_model("openai/gpt-4"), "gpt-4");
    }

    #[test]
    fn test_resolve_model_no_prefix() {
        let provider =
            OpenAICompatProvider::new("key", "http://localhost:11434/v1", "ollama", HashMap::new());
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
