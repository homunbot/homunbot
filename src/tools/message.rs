use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

use crate::bus::OutboundMessage;

use super::registry::{get_optional_string, get_string_param, Tool, ToolContext, ToolResult};

/// Tool that allows the LLM to proactively send messages to the user.
///
/// Unlike other tools that return results to the LLM conversation,
/// this tool actually delivers a message to the user's channel (Telegram, etc.).
/// Useful for:
/// - Sending progress updates during long tasks
/// - Proactive notifications from cron/background jobs
/// - Multi-part responses where the LLM wants to communicate mid-thought
pub struct MessageTool;

impl MessageTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for MessageTool {
    fn name(&self) -> &str {
        "send_message"
    }

    fn description(&self) -> &str {
        "Send a message to the user. Use this to proactively communicate with the user, \
         send progress updates, or deliver notifications. The message is delivered to the \
         same channel the user is communicating from."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "The message content to send to the user"
                },
                "channel": {
                    "type": "string",
                    "description": "Override target channel (e.g. 'telegram'). Defaults to the current channel."
                },
                "chat_id": {
                    "type": "string",
                    "description": "Override target chat ID. Defaults to the current chat."
                }
            },
            "required": ["content"]
        })
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let content = get_string_param(&args, "content")?;
        let channel = get_optional_string(&args, "channel").unwrap_or_else(|| ctx.channel.clone());
        let chat_id = get_optional_string(&args, "chat_id").unwrap_or_else(|| ctx.chat_id.clone());

        let tx = match &ctx.message_tx {
            Some(tx) => tx,
            None => {
                // CLI mode or no channel available — just log it
                tracing::info!(content = %content, "MessageTool: no channel available, message not delivered");
                return Ok(ToolResult::success(
                    "Message noted (no active channel to deliver to)"
                ));
            }
        };

        let outbound = OutboundMessage {
            channel: channel.clone(),
            chat_id: chat_id.clone(),
            content,
        };

        match tx.send(outbound).await {
            Ok(()) => {
                tracing::info!(channel = %channel, chat_id = %chat_id, "Message sent to user");
                Ok(ToolResult::success("Message delivered to user"))
            }
            Err(e) => {
                tracing::error!(error = %e, "Failed to send message");
                Ok(ToolResult::error(format!("Failed to deliver message: {e}")))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ctx() -> ToolContext {
        ToolContext {
            workspace: "/tmp".to_string(),
            channel: "telegram".to_string(),
            chat_id: "123456".to_string(),
            message_tx: None,
        }
    }

    #[test]
    fn test_message_tool_name() {
        let tool = MessageTool::new();
        assert_eq!(tool.name(), "send_message");
    }

    #[test]
    fn test_message_tool_parameters() {
        let tool = MessageTool::new();
        let params = tool.parameters();
        let props = params["properties"].as_object().unwrap();
        assert!(props.contains_key("content"));
        assert!(props.contains_key("channel"));
        assert!(props.contains_key("chat_id"));
        let required = params["required"].as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "content");
    }

    #[tokio::test]
    async fn test_message_no_channel_available() {
        let tool = MessageTool::new();
        let args = serde_json::json!({"content": "Hello user!"});
        let result = tool.execute(args, &test_ctx()).await.unwrap();
        assert!(!result.is_error);
        assert!(result.output.contains("no active channel"));
    }

    #[tokio::test]
    async fn test_message_with_channel() {
        let tool = MessageTool::new();
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);

        let ctx = ToolContext {
            workspace: "/tmp".to_string(),
            channel: "telegram".to_string(),
            chat_id: "123".to_string(),
            message_tx: Some(tx),
        };

        let args = serde_json::json!({"content": "Hello from the agent!"});
        let result = tool.execute(args, &ctx).await.unwrap();
        assert!(!result.is_error);
        assert!(result.output.contains("delivered"));

        // Verify the message was sent
        let msg = rx.recv().await.unwrap();
        assert_eq!(msg.channel, "telegram");
        assert_eq!(msg.chat_id, "123");
        assert_eq!(msg.content, "Hello from the agent!");
    }

    #[tokio::test]
    async fn test_message_with_override_channel() {
        let tool = MessageTool::new();
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);

        let ctx = ToolContext {
            workspace: "/tmp".to_string(),
            channel: "cli".to_string(),
            chat_id: "local".to_string(),
            message_tx: Some(tx),
        };

        let args = serde_json::json!({
            "content": "Alert!",
            "channel": "telegram",
            "chat_id": "999"
        });
        let result = tool.execute(args, &ctx).await.unwrap();
        assert!(!result.is_error);

        let msg = rx.recv().await.unwrap();
        assert_eq!(msg.channel, "telegram");
        assert_eq!(msg.chat_id, "999");
        assert_eq!(msg.content, "Alert!");
    }

    #[tokio::test]
    async fn test_message_missing_content() {
        let tool = MessageTool::new();
        let args = serde_json::json!({});
        let result = tool.execute(args, &test_ctx()).await;
        assert!(result.is_err());
    }
}
