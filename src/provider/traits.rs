use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// A single tool call request from the LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRequest {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Unified response from any LLM provider
#[derive(Debug, Clone)]
pub struct ChatResponse {
    /// Text content of the response (None if only tool calls)
    pub content: Option<String>,
    /// Tool calls requested by the LLM
    pub tool_calls: Vec<ToolCallRequest>,
    /// Why generation stopped: "stop", "tool_calls", "error", etc.
    pub finish_reason: String,
    /// Token usage stats
    pub usage: Usage,
}

impl ChatResponse {
    pub fn has_tool_calls(&self) -> bool {
        !self.tool_calls.is_empty()
    }
}

/// Token usage statistics
#[derive(Debug, Clone, Default)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// A message in a chat conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallSerialized>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl ChatMessage {
    pub fn system(content: &str) -> Self {
        Self {
            role: "system".to_string(),
            content: Some(content.to_string()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    pub fn user(content: &str) -> Self {
        Self {
            role: "user".to_string(),
            content: Some(content.to_string()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    pub fn assistant(content: &str) -> Self {
        Self {
            role: "assistant".to_string(),
            content: Some(content.to_string()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    pub fn tool_result(tool_call_id: &str, name: &str, content: &str) -> Self {
        Self {
            role: "tool".to_string(),
            content: Some(content.to_string()),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.to_string()),
            name: Some(name.to_string()),
        }
    }
}

/// Serialized tool call for the OpenAI message format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallSerialized {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: ToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: String,
}

/// Tool definition in OpenAI function-calling format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: FunctionDefinition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Chat request parameters
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<ToolDefinition>,
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f32,
}

/// A streaming chunk from the LLM
#[derive(Debug, Clone)]
pub struct StreamChunk {
    /// Delta text content (may be empty)
    pub delta: String,
    /// True if this is the final chunk
    pub done: bool,
    /// Optional event type for non-text chunks (e.g. "tool_start", "tool_end").
    /// `None` for regular text streaming chunks.
    pub event_type: Option<String>,
    /// Optional tool call data (for tool_call events)
    pub tool_call_data: Option<ToolCallData>,
}

/// Data for tool_call streaming events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallData {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// LLM provider trait — abstracts over different API backends
#[async_trait]
pub trait Provider: Send + Sync {
    /// Send a chat request and get a response
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse>;

    /// Send a chat request and stream text chunks back.
    /// Default implementation falls back to non-streaming chat.
    /// Only used for the final text response (no tool calls).
    async fn chat_stream(
        &self,
        request: ChatRequest,
        tx: tokio::sync::mpsc::Sender<StreamChunk>,
    ) -> Result<ChatResponse> {
        // Default: non-streaming — call chat() and send the full response as one chunk
        let response = self.chat(request).await?;
        if let Some(ref text) = response.content {
            let _ = tx
                .send(StreamChunk {
                    delta: text.clone(),
                    done: true,
                    event_type: None,
                    tool_call_data: None,
                })
                .await;
        }
        Ok(response)
    }

    /// Provider name for logging/display
    fn name(&self) -> &str;
}
