use std::collections::HashMap;

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::traits::*;

/// Native Anthropic Messages API provider.
///
/// Anthropic uses a different API format than OpenAI:
/// - `system` is a top-level parameter, not a message
/// - Messages use content blocks (text, tool_use, tool_result)
/// - Tool definitions use `input_schema` instead of `parameters`
/// - `stop_reason` instead of `finish_reason`
/// - `anthropic-version` header required
///
/// Docs: https://docs.anthropic.com/en/api/messages
pub struct AnthropicProvider {
    client: Client,
    api_key: String,
    api_base: String,
    extra_headers: HashMap<String, String>,
}

impl AnthropicProvider {
    pub fn new(
        api_key: &str,
        api_base: Option<&str>,
        extra_headers: HashMap<String, String>,
    ) -> Self {
        Self {
            client: Client::new(),
            api_key: api_key.to_string(),
            api_base: api_base
                .unwrap_or("https://api.anthropic.com")
                .trim_end_matches('/')
                .to_string(),
            extra_headers,
        }
    }

    /// Strip provider prefix from model: "anthropic/claude-..." → "claude-..."
    fn resolve_model(&self, model: &str) -> String {
        if let Some(stripped) = model.strip_prefix("anthropic/") {
            stripped.to_string()
        } else {
            model.to_string()
        }
    }
}

// --- Anthropic API request types ---

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<AnthropicToolDef>,
    /// Extended thinking configuration.
    /// When enabled, Claude shows its reasoning process before answering.
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<ThinkingConfig>,
}

#[derive(Serialize)]
struct ThinkingConfig {
    #[serde(rename = "type")]
    config_type: String,
    budget_tokens: u32,
}

#[derive(Serialize, Clone)]
struct AnthropicMessage {
    role: String,
    content: AnthropicContent,
}

/// Content can be a simple string or an array of content blocks
#[derive(Serialize, Clone)]
#[serde(untagged)]
enum AnthropicContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { source: ImageSource },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
    },
    /// Extended thinking block — Claude's internal reasoning.
    /// We parse but discard its content (reasoning_filter handles display).
    #[serde(rename = "thinking")]
    Thinking { thinking: String },
}

#[derive(Serialize, Deserialize, Clone)]
struct ImageSource {
    #[serde(rename = "type")]
    source_type: String,
    media_type: String,
    data: String,
}

#[derive(Serialize)]
struct AnthropicToolDef {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

// --- Anthropic API response types ---

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<ContentBlock>,
    stop_reason: Option<String>,
    #[serde(default)]
    usage: AnthropicUsage,
}

#[derive(Deserialize, Default)]
struct AnthropicUsage {
    #[serde(default)]
    input_tokens: u32,
    #[serde(default)]
    output_tokens: u32,
}

#[derive(Deserialize)]
struct AnthropicErrorResponse {
    error: Option<AnthropicError>,
}

#[derive(Deserialize)]
struct AnthropicError {
    message: String,
}

// --- Convert between our unified format and Anthropic format ---

/// Convert unified ChatMessage list to Anthropic format.
/// Extracts the system message and converts tool calls/results to content blocks.
fn convert_messages(messages: &[ChatMessage]) -> (Option<String>, Vec<AnthropicMessage>) {
    let mut system: Option<String> = None;
    let mut anthropic_msgs: Vec<AnthropicMessage> = Vec::new();

    for msg in messages {
        match msg.role.as_str() {
            "system" => {
                // Anthropic: system is a top-level parameter
                system = msg.content.clone();
            }
            "user" => {
                anthropic_msgs.push(AnthropicMessage {
                    role: "user".to_string(),
                    content: message_content_to_anthropic(msg).unwrap_or_else(|_| {
                        AnthropicContent::Text(msg.rendered_text().unwrap_or_default())
                    }),
                });
            }
            "assistant" => {
                // Check if the assistant message has tool calls
                if let Some(tool_calls) = &msg.tool_calls {
                    let mut blocks: Vec<ContentBlock> = Vec::new();

                    // Add text content if present
                    if let Some(text) = &msg.content {
                        if !text.is_empty() {
                            blocks.push(ContentBlock::Text { text: text.clone() });
                        }
                    }

                    // Add tool_use blocks
                    for tc in tool_calls {
                        let input: serde_json::Value = serde_json::from_str(&tc.function.arguments)
                            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
                        blocks.push(ContentBlock::ToolUse {
                            id: tc.id.clone(),
                            name: tc.function.name.clone(),
                            input,
                        });
                    }

                    anthropic_msgs.push(AnthropicMessage {
                        role: "assistant".to_string(),
                        content: AnthropicContent::Blocks(blocks),
                    });
                } else {
                    anthropic_msgs.push(AnthropicMessage {
                        role: "assistant".to_string(),
                        content: AnthropicContent::Text(msg.rendered_text().unwrap_or_default()),
                    });
                }
            }
            "tool" => {
                // Anthropic: tool results are user messages with tool_result content blocks.
                // We need to merge consecutive tool results into a single user message.
                let block = ContentBlock::ToolResult {
                    tool_use_id: msg.tool_call_id.clone().unwrap_or_default(),
                    content: msg.content.clone().unwrap_or_default(),
                };

                // Check if the last message is already a user message with blocks
                // (we need to merge tool_result blocks into one user message)
                if let Some(last) = anthropic_msgs.last_mut() {
                    if last.role == "user" {
                        if let AnthropicContent::Blocks(ref mut blocks) = last.content {
                            blocks.push(block);
                            continue;
                        }
                    }
                }

                anthropic_msgs.push(AnthropicMessage {
                    role: "user".to_string(),
                    content: AnthropicContent::Blocks(vec![block]),
                });
            }
            _ => {
                // Unknown role — treat as user
                anthropic_msgs.push(AnthropicMessage {
                    role: "user".to_string(),
                    content: AnthropicContent::Text(msg.rendered_text().unwrap_or_default()),
                });
            }
        }
    }

    // Anthropic requires messages to start with a user message
    // and alternate user/assistant. Merge consecutive same-role messages.
    let merged = merge_consecutive_messages(anthropic_msgs);

    (system, merged)
}

fn message_content_to_anthropic(msg: &ChatMessage) -> Result<AnthropicContent> {
    if let Some(parts) = &msg.content_parts {
        let mut blocks = Vec::new();
        for part in parts {
            match part {
                ChatContentPart::Text { text } => {
                    if !text.trim().is_empty() {
                        blocks.push(ContentBlock::Text { text: text.clone() });
                    }
                }
                ChatContentPart::Image { path, media_type } => {
                    let data =
                        BASE64.encode(std::fs::read(path).with_context(|| {
                            format!("Failed to read image attachment '{}'", path)
                        })?);
                    blocks.push(ContentBlock::Image {
                        source: ImageSource {
                            source_type: "base64".to_string(),
                            media_type: media_type.clone(),
                            data,
                        },
                    });
                }
                ChatContentPart::File {
                    path,
                    media_type,
                    name,
                } => blocks.push(ContentBlock::Text {
                    text: format!("Attached file: {} ({}) at {}", name, media_type, path),
                }),
            }
        }

        if !blocks.is_empty() {
            return Ok(AnthropicContent::Blocks(blocks));
        }
    }

    Ok(AnthropicContent::Text(
        msg.rendered_text().unwrap_or_default(),
    ))
}

/// Merge consecutive messages with the same role into a single message.
/// Anthropic requires strictly alternating user/assistant messages.
fn merge_consecutive_messages(msgs: Vec<AnthropicMessage>) -> Vec<AnthropicMessage> {
    let mut result: Vec<AnthropicMessage> = Vec::new();

    for msg in msgs {
        if let Some(last) = result.last_mut() {
            if last.role == msg.role {
                // Merge: convert both to blocks and combine
                let existing_blocks = content_to_blocks(last.content.clone());
                let new_blocks = content_to_blocks(msg.content);
                let mut combined = existing_blocks;
                combined.extend(new_blocks);
                last.content = AnthropicContent::Blocks(combined);
                continue;
            }
        }
        result.push(msg);
    }

    result
}

/// Convert AnthropicContent to a Vec<ContentBlock>
fn content_to_blocks(content: AnthropicContent) -> Vec<ContentBlock> {
    match content {
        AnthropicContent::Blocks(blocks) => blocks,
        AnthropicContent::Text(text) => {
            if text.is_empty() {
                vec![]
            } else {
                vec![ContentBlock::Text { text }]
            }
        }
    }
}

/// Convert our unified ToolDefinition list to Anthropic format
fn convert_tools(tools: &[ToolDefinition]) -> Vec<AnthropicToolDef> {
    tools
        .iter()
        .map(|t| AnthropicToolDef {
            name: t.function.name.clone(),
            description: t.function.description.clone(),
            input_schema: t.function.parameters.clone(),
        })
        .collect()
}

#[async_trait::async_trait]
impl Provider for AnthropicProvider {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        let model = self.resolve_model(&request.model);
        let url = format!("{}/v1/messages", self.api_base);

        let (system, messages) = convert_messages(&request.messages);
        let tools = convert_tools(&request.tools);

        // Extended thinking: when enabled, temperature must be omitted (API constraint)
        // and we allocate thinking budget as half of max_tokens (min 1024).
        let (thinking, temperature) = if request.think == Some(true) {
            let budget = (request.max_tokens / 2).max(1024);
            (
                Some(ThinkingConfig {
                    config_type: "enabled".to_string(),
                    budget_tokens: budget,
                }),
                None, // temperature must be omitted with thinking
            )
        } else {
            (None, Some(request.temperature))
        };

        let body = AnthropicRequest {
            model,
            max_tokens: request.max_tokens.max(1),
            messages,
            system,
            temperature,
            tools,
            thinking,
        };

        let mut req = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01");

        for (key, value) in &self.extra_headers {
            req = req.header(key, value);
        }

        let response = req
            .json(&body)
            .send()
            .await
            .with_context(|| format!("Failed to send request to Anthropic API at {}", url))?;

        let status = response.status();
        let response_text = response
            .text()
            .await
            .context("Failed to read Anthropic response body")?;

        if !status.is_success() {
            let error_msg = serde_json::from_str::<AnthropicErrorResponse>(&response_text)
                .ok()
                .and_then(|e| e.error)
                .map(|e| e.message)
                .unwrap_or_else(|| format!("HTTP {}: {}", status, response_text));
            anyhow::bail!("Anthropic error: {}", error_msg);
        }

        let api_response: AnthropicResponse = serde_json::from_str(&response_text)
            .with_context(|| "Failed to parse Anthropic response")?;

        // Extract text content and tool calls from content blocks
        let mut text_parts: Vec<String> = Vec::new();
        let mut tool_calls: Vec<ToolCallRequest> = Vec::new();

        for block in &api_response.content {
            match block {
                ContentBlock::Text { text } => {
                    text_parts.push(text.clone());
                }
                ContentBlock::Image { .. } => {
                    // Image blocks are only relevant on the request path.
                }
                ContentBlock::ToolUse { id, name, input } => {
                    tool_calls.push(ToolCallRequest {
                        id: id.clone(),
                        name: name.clone(),
                        arguments: input.clone(),
                    });
                }
                ContentBlock::ToolResult { .. } => {
                    // Should not appear in responses, only in requests
                }
                ContentBlock::Thinking { .. } => {
                    // Extended thinking blocks — reasoning is internal, we skip it.
                    // The agent loop's reasoning_filter handles display if needed.
                }
            }
        }

        let content = if text_parts.is_empty() {
            None
        } else {
            Some(text_parts.join("\n"))
        };

        let finish_reason = api_response
            .stop_reason
            .unwrap_or_else(|| "stop".to_string());

        let usage = Usage {
            prompt_tokens: api_response.usage.input_tokens,
            completion_tokens: api_response.usage.output_tokens,
            total_tokens: api_response.usage.input_tokens + api_response.usage.output_tokens,
        };

        Ok(ChatResponse {
            content,
            tool_calls,
            finish_reason,
            usage,
        })
    }

    fn name(&self) -> &str {
        "anthropic"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_model_strip_prefix() {
        let provider = AnthropicProvider::new("key", None, HashMap::new());
        assert_eq!(
            provider.resolve_model("anthropic/claude-sonnet-4-20250514"),
            "claude-sonnet-4-20250514"
        );
    }

    #[test]
    fn test_resolve_model_no_prefix() {
        let provider = AnthropicProvider::new("key", None, HashMap::new());
        assert_eq!(
            provider.resolve_model("claude-sonnet-4-20250514"),
            "claude-sonnet-4-20250514"
        );
    }

    #[test]
    fn test_convert_messages_system_extracted() {
        let messages = vec![
            ChatMessage::system("You are a helpful assistant."),
            ChatMessage::user("Hello!"),
        ];
        let (system, msgs) = convert_messages(&messages);
        assert_eq!(system, Some("You are a helpful assistant.".to_string()));
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].role, "user");
    }

    #[test]
    fn test_convert_messages_user_assistant() {
        let messages = vec![
            ChatMessage::user("Hi"),
            ChatMessage {
                role: "assistant".to_string(),
                content: Some("Hello!".to_string()),
                content_parts: None,
                tool_calls: None,
                tool_call_id: None,
                name: None,
            },
            ChatMessage::user("How are you?"),
        ];
        let (system, msgs) = convert_messages(&messages);
        assert!(system.is_none());
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].role, "user");
        assert_eq!(msgs[1].role, "assistant");
        assert_eq!(msgs[2].role, "user");
    }

    #[test]
    fn test_convert_messages_tool_result_merging() {
        // Realistic scenario: user → assistant (with tool_use) → tool results
        // Tool results become a user message with tool_result blocks.
        let messages = vec![
            ChatMessage::user("Do two things"),
            ChatMessage {
                role: "assistant".to_string(),
                content: Some("I'll do both.".to_string()),
                content_parts: None,
                tool_calls: Some(vec![
                    ToolCallSerialized {
                        id: "call_1".to_string(),
                        call_type: "function".to_string(),
                        function: ToolCallFunction {
                            name: "tool_a".to_string(),
                            arguments: "{}".to_string(),
                        },
                    },
                    ToolCallSerialized {
                        id: "call_2".to_string(),
                        call_type: "function".to_string(),
                        function: ToolCallFunction {
                            name: "tool_b".to_string(),
                            arguments: "{}".to_string(),
                        },
                    },
                ]),
                tool_call_id: None,
                name: None,
            },
            ChatMessage::tool_result("call_1", "tool_a", "Result A"),
            ChatMessage::tool_result("call_2", "tool_b", "Result B"),
        ];
        let (_system, msgs) = convert_messages(&messages);
        // user → assistant (with tool_use blocks) → user (with tool_result blocks)
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].role, "user");
        assert_eq!(msgs[1].role, "assistant");
        assert_eq!(msgs[2].role, "user");
        if let AnthropicContent::Blocks(blocks) = &msgs[2].content {
            assert_eq!(blocks.len(), 2); // Two tool_result blocks
        } else {
            panic!("Expected blocks content for merged tool results");
        }
    }

    #[test]
    fn test_convert_tools() {
        let tools = vec![ToolDefinition {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "shell".to_string(),
                description: "Execute a command".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "command": { "type": "string" }
                    }
                }),
            },
        }];
        let converted = convert_tools(&tools);
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].name, "shell");
        assert!(converted[0].input_schema.is_object());
    }

    #[test]
    fn test_merge_consecutive_same_role() {
        let msgs = vec![
            AnthropicMessage {
                role: "user".to_string(),
                content: AnthropicContent::Text("First".to_string()),
            },
            AnthropicMessage {
                role: "user".to_string(),
                content: AnthropicContent::Text("Second".to_string()),
            },
        ];
        let merged = merge_consecutive_messages(msgs);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].role, "user");
        if let AnthropicContent::Blocks(blocks) = &merged[0].content {
            assert_eq!(blocks.len(), 2);
        } else {
            panic!("Expected blocks after merge");
        }
    }

    #[test]
    fn test_convert_multimodal_user_message() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), b"fake").unwrap();
        let messages = vec![ChatMessage::user_parts(vec![
            ChatContentPart::Text {
                text: "Look at this".to_string(),
            },
            ChatContentPart::Image {
                path: temp.path().to_string_lossy().to_string(),
                media_type: "image/png".to_string(),
            },
        ])];

        let (_system, msgs) = convert_messages(&messages);
        if let AnthropicContent::Blocks(blocks) = &msgs[0].content {
            assert_eq!(blocks.len(), 2);
        } else {
            panic!("Expected anthropic blocks");
        }
    }
}
