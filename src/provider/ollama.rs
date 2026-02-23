//! Native Ollama provider using `/api/chat` endpoint.
//!
//! Key advantages over the OpenAI-compatible shim (`/v1/chat/completions`):
//! - `think: false` parameter to disable reasoning on cloud models (30-120s → 2-8s)
//! - Tool call arguments returned as JSON objects (no string parsing / repair needed)
//! - NDJSON streaming (simpler than SSE)
//! - Access to Ollama-specific timing metrics
//! - Bearer token auth for Ollama cloud direct access (no local Ollama needed)
//!
//! Supports two modes:
//! 1. **Local**: `api_base = "http://localhost:11434"`, no api_key
//! 2. **Cloud direct**: `api_base = "https://ollama.com"`, api_key from ollama.com/settings/keys

use anyhow::{Context as _, Result};
use futures_util::StreamExt as _;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::traits::*;

// ─── Provider ───────────────────────────────────────────────────────────────

/// Native Ollama provider — calls `/api/chat` with NDJSON streaming.
pub struct OllamaProvider {
    client: Client,
    api_base: String,
    api_key: String,
}

impl OllamaProvider {
    /// Create a new Ollama provider.
    ///
    /// `api_key`: empty for local Ollama, Bearer token for cloud direct.
    /// `api_base`: defaults to `http://localhost:11434`. Strips `/v1` suffix
    /// for backward-compatibility with existing configs.
    pub fn new(api_key: &str, api_base: Option<&str>) -> Self {
        let base = api_base
            .unwrap_or("http://localhost:11434")
            .trim_end_matches('/')
            .trim_end_matches("/v1")
            .trim_end_matches('/')
            .to_string();

        Self {
            client: Client::new(),
            api_base: base,
            api_key: api_key.to_string(),
        }
    }

    /// Strip `ollama/` prefix from model name.
    fn resolve_model(&self, model: &str) -> String {
        model.strip_prefix("ollama/").unwrap_or(model).to_string()
    }

    /// Cloud models (`:cloud` suffix) have reasoning enabled by default,
    /// causing 30-120s latency. Send `think: false` to disable it.
    fn should_disable_think(&self, model: &str) -> bool {
        model.contains(":cloud")
    }

    /// Build an HTTP request with optional Bearer auth.
    fn build_request(&self, url: &str) -> reqwest::RequestBuilder {
        let mut req = self
            .client
            .post(url)
            .header("Content-Type", "application/json");

        if !self.api_key.is_empty() {
            req = req.bearer_auth(&self.api_key);
        }

        req
    }

    /// Convert unified `ChatMessage` list to Ollama's native format.
    ///
    /// Key differences from OpenAI format:
    /// - Tool call arguments are JSON objects, not strings
    /// - No `tool_call_id` field (Ollama uses positional matching)
    fn convert_messages(messages: &[ChatMessage]) -> Vec<OllamaMessage> {
        messages
            .iter()
            .map(|msg| {
                // Convert assistant tool calls from OpenAI format (string args)
                // to Ollama format (object args)
                let tool_calls = msg.tool_calls.as_ref().and_then(|tcs| {
                    let converted: Vec<OllamaToolCallOut> = tcs
                        .iter()
                        .map(|tc| {
                            let arguments =
                                serde_json::from_str(&tc.function.arguments).unwrap_or_default();
                            OllamaToolCallOut {
                                function: OllamaFunctionOut {
                                    name: tc.function.name.clone(),
                                    arguments,
                                },
                            }
                        })
                        .collect();
                    if converted.is_empty() {
                        None
                    } else {
                        Some(converted)
                    }
                });

                OllamaMessage {
                    role: msg.role.clone(),
                    content: msg.content.clone().unwrap_or_default(),
                    tool_calls,
                }
            })
            .collect()
    }

    /// Normalize a tool call from Ollama, handling malformed patterns:
    ///
    /// 1. **Nested wrapper**: tool named `"tool_call"` or `"tool.call"` where
    ///    arguments contain `{"name": "real_tool", "arguments": {...}}`
    /// 2. **Dot-prefixed**: `"tool.shell"` → `"shell"`
    /// 3. **Normal**: used as-is
    ///
    /// Also generates unique IDs since Ollama doesn't provide them.
    fn normalize_tool_call(tc: OllamaToolCall, index: usize) -> ToolCallRequest {
        let mut name = tc.function.name;
        let mut arguments = tc.function.arguments;

        // Pattern 1: nested wrapper (tool_call or tool.call)
        if name == "tool_call" || name == "tool.call" {
            if let Some(inner_name) = arguments.get("name").and_then(|v| v.as_str()) {
                let inner_name = inner_name.to_string();
                if let Some(inner_args) = arguments.get("arguments") {
                    arguments = inner_args.clone();
                }
                name = inner_name;
            }
        }

        // Pattern 2: dot-prefixed names
        if let Some(stripped) = name.strip_prefix("tool.") {
            name = stripped.to_string();
        }

        ToolCallRequest {
            id: format!("ollama_call_{index}"),
            name,
            arguments,
        }
    }
}

// ─── Provider trait ─────────────────────────────────────────────────────────

#[async_trait::async_trait]
impl Provider for OllamaProvider {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        let model = self.resolve_model(&request.model);
        let url = format!("{}/api/chat", self.api_base);

        let think = if self.should_disable_think(&model) {
            Some(false)
        } else {
            None
        };

        let body = OllamaRequest {
            model,
            messages: Self::convert_messages(&request.messages),
            tools: request.tools,
            stream: Some(false),
            think,
            options: Some(OllamaOptions {
                temperature: Some(request.temperature),
                num_predict: Some(request.max_tokens),
            }),
        };

        let response = self
            .build_request(&url)
            .json(&body)
            .send()
            .await
            .with_context(|| format!("Failed to send request to Ollama at {url}"))?;

        let status = response.status();
        let response_text = response
            .text()
            .await
            .context("Failed to read Ollama response body")?;

        if !status.is_success() {
            let error_msg = serde_json::from_str::<OllamaErrorResponse>(&response_text)
                .ok()
                .and_then(|e| e.error)
                .unwrap_or_else(|| format!("HTTP {status}: {response_text}"));
            anyhow::bail!("Ollama error: {error_msg}");
        }

        let resp: OllamaResponse = serde_json::from_str(&response_text)
            .with_context(|| "Failed to parse Ollama response")?;

        // Log timing metrics
        if let Some(duration_ns) = resp.total_duration {
            tracing::debug!(
                duration_ms = duration_ns / 1_000_000,
                prompt_tokens = resp.prompt_eval_count,
                completion_tokens = resp.eval_count,
                "Ollama response timing"
            );
        }

        Ok(self.build_chat_response(resp))
    }

    async fn chat_stream(
        &self,
        request: ChatRequest,
        tx: tokio::sync::mpsc::Sender<StreamChunk>,
    ) -> Result<ChatResponse> {
        let model = self.resolve_model(&request.model);
        let url = format!("{}/api/chat", self.api_base);

        let think = if self.should_disable_think(&model) {
            Some(false)
        } else {
            None
        };

        let body = OllamaRequest {
            model,
            messages: Self::convert_messages(&request.messages),
            tools: request.tools,
            stream: Some(true),
            think,
            options: Some(OllamaOptions {
                temperature: Some(request.temperature),
                num_predict: Some(request.max_tokens),
            }),
        };

        let response = self
            .build_request(&url)
            .json(&body)
            .send()
            .await
            .with_context(|| format!("Failed to send streaming request to Ollama at {url}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("Ollama streaming error: HTTP {status}: {text}");
        }

        // NDJSON streaming: each line is a complete JSON object
        let mut full_content = String::new();
        let mut bytes_stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut finish_reason = String::from("stop");
        let mut tool_calls: Vec<ToolCallRequest> = Vec::new();
        let mut usage = Usage::default();

        while let Some(chunk_result) = bytes_stream.next().await {
            let chunk = chunk_result.context("Error reading NDJSON stream")?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            // Process complete lines
            while let Some(line_end) = buffer.find('\n') {
                let line = buffer[..line_end].trim().to_string();
                buffer = buffer[line_end + 1..].to_string();

                if line.is_empty() {
                    continue;
                }

                let Ok(sc) = serde_json::from_str::<OllamaStreamChunk>(&line) else {
                    continue;
                };

                // Text delta → forward to client
                if !sc.message.content.is_empty() {
                    full_content.push_str(&sc.message.content);
                    let _ = tx
                        .send(StreamChunk {
                            delta: sc.message.content,
                            done: false,
                            event_type: None,
                            tool_call_data: None,
                        })
                        .await;
                }

                // Tool calls arrive in a single chunk (not streamed incrementally)
                if let Some(tcs) = sc.message.tool_calls {
                    let offset = tool_calls.len();
                    for (i, tc) in tcs.into_iter().enumerate() {
                        tool_calls.push(Self::normalize_tool_call(tc, offset + i));
                    }
                }

                // Final chunk
                if sc.done {
                    usage = Usage {
                        prompt_tokens: sc.prompt_eval_count.unwrap_or(0),
                        completion_tokens: sc.eval_count.unwrap_or(0),
                        total_tokens: sc.prompt_eval_count.unwrap_or(0)
                            + sc.eval_count.unwrap_or(0),
                    };

                    finish_reason = if !tool_calls.is_empty() {
                        "tool_calls".to_string()
                    } else {
                        sc.done_reason.unwrap_or_else(|| "stop".to_string())
                    };

                    // Signal stream end
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

        Ok(ChatResponse {
            content: if full_content.is_empty() {
                None
            } else {
                Some(full_content)
            },
            tool_calls,
            finish_reason,
            usage,
        })
    }

    fn name(&self) -> &str {
        "ollama"
    }
}

// ─── Response builder (shared between chat and chat_stream) ─────────────────

impl OllamaProvider {
    fn build_chat_response(&self, resp: OllamaResponse) -> ChatResponse {
        let tool_calls: Vec<ToolCallRequest> = resp
            .message
            .tool_calls
            .unwrap_or_default()
            .into_iter()
            .enumerate()
            .map(|(i, tc)| Self::normalize_tool_call(tc, i))
            .collect();

        let content = if resp.message.content.is_empty() {
            None
        } else {
            Some(resp.message.content)
        };

        let finish_reason = if !tool_calls.is_empty() {
            "tool_calls".to_string()
        } else {
            resp.done_reason.unwrap_or_else(|| "stop".to_string())
        };

        let usage = Usage {
            prompt_tokens: resp.prompt_eval_count.unwrap_or(0),
            completion_tokens: resp.eval_count.unwrap_or(0),
            total_tokens: resp.prompt_eval_count.unwrap_or(0) + resp.eval_count.unwrap_or(0),
        };

        ChatResponse {
            content,
            tool_calls,
            finish_reason,
            usage,
        }
    }
}

// ─── Request types ──────────────────────────────────────────────────────────

#[derive(Serialize)]
struct OllamaRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<ToolDefinition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    think: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
}

#[derive(Serialize)]
struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<u32>,
}

#[derive(Serialize, Clone)]
struct OllamaMessage {
    role: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OllamaToolCallOut>>,
}

/// Outbound tool call (serialized into assistant messages for conversation history).
#[derive(Serialize, Clone)]
struct OllamaToolCallOut {
    function: OllamaFunctionOut,
}

#[derive(Serialize, Clone)]
struct OllamaFunctionOut {
    name: String,
    arguments: serde_json::Value,
}

// ─── Response types ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct OllamaResponse {
    #[serde(default)]
    message: OllamaResponseMessage,
    #[serde(default)]
    done: bool,
    #[serde(default)]
    done_reason: Option<String>,
    #[serde(default)]
    prompt_eval_count: Option<u32>,
    #[serde(default)]
    eval_count: Option<u32>,
    #[serde(default)]
    total_duration: Option<u64>,
}

#[derive(Deserialize, Default)]
struct OllamaResponseMessage {
    #[serde(default)]
    content: String,
    #[serde(default)]
    tool_calls: Option<Vec<OllamaToolCall>>,
}

/// Tool call as returned by Ollama — no ID, arguments is a JSON object.
#[derive(Deserialize)]
struct OllamaToolCall {
    function: OllamaFunction,
}

#[derive(Deserialize)]
struct OllamaFunction {
    name: String,
    #[serde(default)]
    arguments: serde_json::Value,
}

#[derive(Deserialize)]
struct OllamaErrorResponse {
    error: Option<String>,
}

/// Streaming chunk (NDJSON line).
#[derive(Deserialize)]
struct OllamaStreamChunk {
    #[serde(default)]
    message: OllamaResponseMessage,
    #[serde(default)]
    done: bool,
    #[serde(default)]
    done_reason: Option<String>,
    #[serde(default)]
    prompt_eval_count: Option<u32>,
    #[serde(default)]
    eval_count: Option<u32>,
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_model_strip_prefix() {
        let provider = OllamaProvider::new("", None);
        assert_eq!(provider.resolve_model("ollama/llama3:8b"), "llama3:8b");
        assert_eq!(provider.resolve_model("glm-5:cloud"), "glm-5:cloud");
        assert_eq!(provider.resolve_model("ollama/glm-5:cloud"), "glm-5:cloud");
    }

    #[test]
    fn test_should_disable_think() {
        let provider = OllamaProvider::new("", None);
        assert!(provider.should_disable_think("glm-5:cloud"));
        assert!(provider.should_disable_think("qwen3:cloud"));
        assert!(!provider.should_disable_think("llama3:8b"));
        assert!(!provider.should_disable_think("qwen2.5:latest"));
    }

    #[test]
    fn test_api_base_normalization() {
        // With /v1 suffix (from existing configs)
        let p = OllamaProvider::new("", Some("http://localhost:11434/v1"));
        assert_eq!(p.api_base, "http://localhost:11434");

        // With /v1/ suffix
        let p = OllamaProvider::new("", Some("http://localhost:11434/v1/"));
        assert_eq!(p.api_base, "http://localhost:11434");

        // Without /v1 suffix
        let p = OllamaProvider::new("", Some("http://localhost:11434"));
        assert_eq!(p.api_base, "http://localhost:11434");

        // With trailing slash
        let p = OllamaProvider::new("", Some("http://localhost:11434/"));
        assert_eq!(p.api_base, "http://localhost:11434");

        // Cloud URL preserved
        let p = OllamaProvider::new("ol-key", Some("https://ollama.com"));
        assert_eq!(p.api_base, "https://ollama.com");

        // Default
        let p = OllamaProvider::new("", None);
        assert_eq!(p.api_base, "http://localhost:11434");
    }

    #[test]
    fn test_normalize_tool_call_simple() {
        let tc = OllamaToolCall {
            function: OllamaFunction {
                name: "shell".to_string(),
                arguments: serde_json::json!({"command": "ls"}),
            },
        };
        let normalized = OllamaProvider::normalize_tool_call(tc, 0);
        assert_eq!(normalized.name, "shell");
        assert_eq!(normalized.arguments["command"], "ls");
        assert_eq!(normalized.id, "ollama_call_0");
    }

    #[test]
    fn test_normalize_tool_call_nested_wrapper() {
        let tc = OllamaToolCall {
            function: OllamaFunction {
                name: "tool_call".to_string(),
                arguments: serde_json::json!({
                    "name": "shell",
                    "arguments": {"command": "echo hello"}
                }),
            },
        };
        let normalized = OllamaProvider::normalize_tool_call(tc, 0);
        assert_eq!(normalized.name, "shell");
        assert_eq!(normalized.arguments["command"], "echo hello");
    }

    #[test]
    fn test_normalize_tool_call_dot_prefix() {
        let tc = OllamaToolCall {
            function: OllamaFunction {
                name: "tool.shell".to_string(),
                arguments: serde_json::json!({"command": "pwd"}),
            },
        };
        let normalized = OllamaProvider::normalize_tool_call(tc, 0);
        assert_eq!(normalized.name, "shell");
    }

    #[test]
    fn test_normalize_tool_call_dot_call_wrapper() {
        let tc = OllamaToolCall {
            function: OllamaFunction {
                name: "tool.call".to_string(),
                arguments: serde_json::json!({
                    "name": "read_file",
                    "arguments": {"path": "/tmp/test.txt"}
                }),
            },
        };
        let normalized = OllamaProvider::normalize_tool_call(tc, 0);
        assert_eq!(normalized.name, "read_file");
        assert_eq!(normalized.arguments["path"], "/tmp/test.txt");
    }

    #[test]
    fn test_normalize_tool_call_unique_ids() {
        let make_tc = |name: &str| OllamaToolCall {
            function: OllamaFunction {
                name: name.to_string(),
                arguments: serde_json::json!({}),
            },
        };
        let tc0 = OllamaProvider::normalize_tool_call(make_tc("a"), 0);
        let tc1 = OllamaProvider::normalize_tool_call(make_tc("b"), 1);
        assert_eq!(tc0.id, "ollama_call_0");
        assert_eq!(tc1.id, "ollama_call_1");
    }

    #[test]
    fn test_convert_messages_basic() {
        let messages = vec![
            ChatMessage::system("You are helpful."),
            ChatMessage::user("Hello"),
            ChatMessage::assistant("Hi!"),
        ];
        let converted = OllamaProvider::convert_messages(&messages);
        assert_eq!(converted.len(), 3);
        assert_eq!(converted[0].role, "system");
        assert_eq!(converted[0].content, "You are helpful.");
        assert_eq!(converted[1].role, "user");
        assert_eq!(converted[2].role, "assistant");
        assert!(converted[0].tool_calls.is_none());
    }

    #[test]
    fn test_convert_messages_with_tool_calls() {
        let msg = ChatMessage {
            role: "assistant".to_string(),
            content: None,
            tool_calls: Some(vec![ToolCallSerialized {
                id: "call_1".to_string(),
                call_type: "function".to_string(),
                function: ToolCallFunction {
                    name: "shell".to_string(),
                    arguments: r#"{"command":"ls"}"#.to_string(),
                },
            }]),
            tool_call_id: None,
            name: None,
        };
        let converted = OllamaProvider::convert_messages(&[msg]);
        let tcs = converted[0].tool_calls.as_ref().unwrap();
        assert_eq!(tcs.len(), 1);
        assert_eq!(tcs[0].function.name, "shell");
        assert_eq!(tcs[0].function.arguments["command"], "ls");
    }

    #[test]
    fn test_convert_messages_tool_result() {
        let msg = ChatMessage::tool_result("call_1", "shell", "file1.txt\nfile2.txt");
        let converted = OllamaProvider::convert_messages(&[msg]);
        assert_eq!(converted[0].role, "tool");
        assert_eq!(converted[0].content, "file1.txt\nfile2.txt");
        assert!(converted[0].tool_calls.is_none());
    }

    #[test]
    fn test_parse_response_text_only() {
        let json = r#"{
            "message": {"role": "assistant", "content": "Hello!"},
            "done": true,
            "done_reason": "stop",
            "prompt_eval_count": 26,
            "eval_count": 5,
            "total_duration": 500000000
        }"#;
        let resp: OllamaResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.message.content, "Hello!");
        assert!(resp.done);
        assert_eq!(resp.prompt_eval_count, Some(26));
        assert_eq!(resp.eval_count, Some(5));
    }

    #[test]
    fn test_parse_response_with_tool_calls() {
        let json = r#"{
            "message": {
                "role": "assistant",
                "content": "",
                "tool_calls": [{
                    "function": {
                        "name": "shell",
                        "arguments": {"command": "ls -la"}
                    }
                }]
            },
            "done": true,
            "done_reason": "stop"
        }"#;
        let resp: OllamaResponse = serde_json::from_str(json).unwrap();
        let tcs = resp.message.tool_calls.unwrap();
        assert_eq!(tcs.len(), 1);
        assert_eq!(tcs[0].function.name, "shell");
        // Arguments are a JSON object, not a string
        assert_eq!(tcs[0].function.arguments["command"], "ls -la");
    }

    #[test]
    fn test_build_chat_response_text() {
        let provider = OllamaProvider::new("", None);
        let resp = OllamaResponse {
            message: OllamaResponseMessage {
                content: "Hello!".to_string(),
                tool_calls: None,
            },
            done: true,
            done_reason: Some("stop".to_string()),
            prompt_eval_count: Some(10),
            eval_count: Some(5),
            total_duration: Some(1_000_000_000),
        };
        let chat_resp = provider.build_chat_response(resp);
        assert_eq!(chat_resp.content.as_deref(), Some("Hello!"));
        assert!(chat_resp.tool_calls.is_empty());
        assert_eq!(chat_resp.finish_reason, "stop");
        assert_eq!(chat_resp.usage.prompt_tokens, 10);
        assert_eq!(chat_resp.usage.completion_tokens, 5);
        assert_eq!(chat_resp.usage.total_tokens, 15);
    }

    #[test]
    fn test_build_chat_response_tool_calls() {
        let provider = OllamaProvider::new("", None);
        let resp = OllamaResponse {
            message: OllamaResponseMessage {
                content: String::new(),
                tool_calls: Some(vec![OllamaToolCall {
                    function: OllamaFunction {
                        name: "shell".to_string(),
                        arguments: serde_json::json!({"command": "pwd"}),
                    },
                }]),
            },
            done: true,
            done_reason: Some("stop".to_string()),
            prompt_eval_count: None,
            eval_count: None,
            total_duration: None,
        };
        let chat_resp = provider.build_chat_response(resp);
        assert!(chat_resp.content.is_none());
        assert_eq!(chat_resp.tool_calls.len(), 1);
        assert_eq!(chat_resp.tool_calls[0].name, "shell");
        assert_eq!(chat_resp.finish_reason, "tool_calls");
    }

    #[test]
    fn test_provider_name() {
        let provider = OllamaProvider::new("", None);
        assert_eq!(provider.name(), "ollama");
    }
}
