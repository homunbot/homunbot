use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::{Deserialize, Serialize};

const INLINE_PREFIX: &str = "[[homun-attachments:";
const INLINE_SUFFIX: &str = "]]";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatAttachment {
    pub kind: String,
    pub name: String,
    pub stored_path: String,
    pub preview_url: String,
    pub content_type: String,
    pub size_bytes: u64,
}

pub type StoredChatAttachment = ChatAttachment;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatMcpServerRef {
    pub name: String,
    pub transport: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ChatInlineContext {
    #[serde(default)]
    attachments: Vec<ChatAttachment>,
    #[serde(default)]
    mcp_servers: Vec<ChatMcpServerRef>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParsedChatMessageContent {
    pub text: String,
    pub attachments: Vec<ChatAttachment>,
    pub mcp_servers: Vec<ChatMcpServerRef>,
}

pub fn encode_inline_context(
    text: &str,
    attachments: &[ChatAttachment],
    mcp_servers: &[ChatMcpServerRef],
) -> Option<String> {
    if attachments.is_empty() && mcp_servers.is_empty() {
        return None;
    }

    let json = serde_json::to_vec(&ChatInlineContext {
        attachments: attachments.to_vec(),
        mcp_servers: mcp_servers.to_vec(),
    })
    .ok()?;
    let payload = BASE64.encode(json);
    Some(format!("{INLINE_PREFIX}{payload}{INLINE_SUFFIX}\n{text}"))
}

pub fn parse_message_content(raw: &str) -> ParsedChatMessageContent {
    if !raw.starts_with(INLINE_PREFIX) {
        return ParsedChatMessageContent {
            text: raw.to_string(),
            attachments: Vec::new(),
            mcp_servers: Vec::new(),
        };
    }

    let Some(suffix_idx) = raw.find(INLINE_SUFFIX) else {
        return ParsedChatMessageContent {
            text: raw.to_string(),
            attachments: Vec::new(),
            mcp_servers: Vec::new(),
        };
    };

    let encoded = &raw[INLINE_PREFIX.len()..suffix_idx];
    let context = BASE64
        .decode(encoded)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<ChatInlineContext>(&bytes).ok())
        .unwrap_or_default();

    let text = raw[suffix_idx + INLINE_SUFFIX.len()..]
        .strip_prefix('\n')
        .unwrap_or(&raw[suffix_idx + INLINE_SUFFIX.len()..])
        .to_string();

    ParsedChatMessageContent {
        text,
        attachments: context.attachments,
        mcp_servers: context.mcp_servers,
    }
}

pub fn content_for_model(raw: &str) -> String {
    let parsed = parse_message_content(raw);
    if parsed.attachments.is_empty() && parsed.mcp_servers.is_empty() {
        return parsed.text;
    }

    let mut rendered = parsed.text;
    if !rendered.trim().is_empty() {
        rendered.push_str("\n\n");
    }
    if !parsed.mcp_servers.is_empty() {
        rendered.push_str("Preferred MCP servers for this request:\n");
        for server in parsed.mcp_servers {
            rendered.push_str(&format!("- {} ({})\n", server.name, server.transport));
        }
        rendered.push('\n');
    }
    if !parsed.attachments.is_empty() {
        rendered.push_str("Attached files:\n");
        for attachment in parsed.attachments {
            let size_kib = ((attachment.size_bytes as f64) / 1024.0).ceil() as u64;
            rendered.push_str(&format!(
                "- [{}] {} ({}, {} KiB) saved at {}\n",
                attachment.kind,
                attachment.name,
                attachment.content_type,
                size_kib.max(1),
                attachment.stored_path
            ));
        }
    }

    rendered
}

#[cfg(test)]
mod tests {
    use super::{
        content_for_model, encode_inline_context, parse_message_content, ChatAttachment,
        ChatMcpServerRef,
    };

    #[test]
    fn inline_attachments_round_trip() {
        let attachments = vec![ChatAttachment {
            kind: "image".to_string(),
            name: "photo.png".to_string(),
            stored_path: "/tmp/photo.png".to_string(),
            preview_url: "/api/v1/chat/uploads/default/photo.png".to_string(),
            content_type: "image/png".to_string(),
            size_bytes: 1536,
        }];
        let mcp_servers = vec![ChatMcpServerRef {
            name: "github".to_string(),
            transport: "stdio".to_string(),
        }];

        let encoded = encode_inline_context("look at this", &attachments, &mcp_servers).unwrap();
        let parsed = parse_message_content(&encoded);
        assert_eq!(parsed.text, "look at this");
        assert_eq!(parsed.attachments, attachments);
        assert_eq!(parsed.mcp_servers, mcp_servers);

        let model = content_for_model(&encoded);
        assert!(model.contains("look at this"));
        assert!(model.contains("Preferred MCP servers for this request:"));
        assert!(model.contains("github (stdio)"));
        assert!(model.contains("Attached files:"));
        assert!(model.contains("/tmp/photo.png"));
    }
}
