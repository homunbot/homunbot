use std::collections::{BTreeSet, HashSet};

use anyhow::{Context as _, Result};
use tokio::sync::mpsc;

use crate::config::{Config, McpServerConfig};
use crate::provider::{ChatContentPart, ChatMessage, StreamChunk};
use crate::web::chat_attachments::{ChatAttachment, ChatMcpServerRef};

const MAX_ATTACHMENT_ANALYSIS_CHARS: usize = 6_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectedProviderMode {
    Text,
    Multimodal,
    Preprocessed,
}

impl SelectedProviderMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Multimodal => "multimodal",
            Self::Preprocessed => "preprocessed",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PreparedAttachmentTurn {
    pub selected_model: String,
    pub selected_provider_mode: SelectedProviderMode,
    pub user_message: ChatMessage,
    pub attachment_analysis_summary: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedAttachmentInput {
    pub name: String,
    pub path: String,
    pub preview_url: String,
    pub media_type: String,
    pub kind: String,
}

pub async fn prepare_turn(
    config: &Config,
    raw_content: &str,
    stream_tx: Option<&mpsc::Sender<StreamChunk>>,
) -> Result<PreparedAttachmentTurn> {
    let parsed = crate::web::chat_attachments::parse_message_content(raw_content);
    let text = parsed.text.trim().to_string();

    if parsed.attachments.is_empty() {
        return Ok(PreparedAttachmentTurn {
            selected_model: config.agent.model.clone(),
            selected_provider_mode: SelectedProviderMode::Text,
            user_message: ChatMessage::user(&text),
            attachment_analysis_summary: None,
        });
    }

    let active_model = config.agent.model.trim().to_string();
    let active_provider = config
        .resolve_provider(&active_model)
        .map(|(name, _)| name)
        .unwrap_or("unknown");
    let active_capabilities = config
        .agent
        .effective_model_capabilities(active_provider, &active_model);
    let vision_model = config
        .agent
        .vision_model
        .trim()
        .to_string()
        .trim()
        .to_string();
    let preferred_mcp = parsed
        .mcp_servers
        .iter()
        .map(|server| server.name.to_ascii_lowercase())
        .collect::<HashSet<_>>();

    let mut local_sections = Vec::new();
    let mut binary_documents = Vec::new();
    let mut images = Vec::new();
    let mut sent_extracting_status = false;

    for attachment in &parsed.attachments {
        if crate::agent::stop::is_stop_requested() {
            return Err(crate::agent::stop::cancellation_error());
        }

        let resolved = resolve_attachment(attachment);
        if resolved.kind == "image" {
            images.push(resolved);
            continue;
        }

        if let Some(extracted) = extract_local_document_text(&resolved).await? {
            if !sent_extracting_status {
                send_status(stream_tx, "Extracting text from document").await;
                sent_extracting_status = true;
            }
            local_sections.push(format_attachment_analysis(
                &resolved.name,
                "local document extraction",
                &extracted,
            ));
        } else {
            binary_documents.push(resolved);
        }
    }

    let mut selected_model = active_model.clone();
    let mut provider_mode = if local_sections.is_empty() {
        SelectedProviderMode::Text
    } else {
        SelectedProviderMode::Preprocessed
    };
    let mut native_images = Vec::new();
    let mut mcp_sections = Vec::new();

    if !binary_documents.is_empty() {
        let document_sections = analyze_via_mcp(
            config,
            &preferred_mcp,
            &binary_documents,
            &text,
            &["document-extraction", "pdf-reading", "ocr"],
            stream_tx,
        )
        .await?;
        mcp_sections.extend(document_sections);
        provider_mode = SelectedProviderMode::Preprocessed;
    }

    if !images.is_empty() {
        if active_capabilities.image_input || active_capabilities.multimodal {
            send_status(stream_tx, "Analyzing image").await;
            native_images = images.clone();
            provider_mode = SelectedProviderMode::Multimodal;
        } else if !vision_model.is_empty() {
            let vision_provider = config
                .resolve_provider(&vision_model)
                .map(|(name, _)| name)
                .unwrap_or("unknown");
            let vision_capabilities = config
                .agent
                .effective_model_capabilities(vision_provider, &vision_model);
            if vision_capabilities.image_input || vision_capabilities.multimodal {
                selected_model = vision_model.clone();
                native_images = images.clone();
                provider_mode = SelectedProviderMode::Multimodal;
                send_status(stream_tx, "Analyzing image with vision model").await;
                send_model(stream_tx, &selected_model).await;
            }
        }

        if native_images.is_empty() {
            let image_sections = analyze_via_mcp(
                config,
                &preferred_mcp,
                &images,
                &text,
                &["image-analysis", "ocr"],
                stream_tx,
            )
            .await?;
            mcp_sections.extend(image_sections);
            provider_mode = SelectedProviderMode::Preprocessed;
        }
    }

    let mut attachment_sections = local_sections;
    attachment_sections.extend(mcp_sections);
    let attachment_analysis_summary = if attachment_sections.is_empty() {
        None
    } else {
        Some(attachment_sections.join("\n\n"))
    };
    let prompt_text = compose_prompt_text(
        &text,
        attachment_analysis_summary.as_deref(),
        !native_images.is_empty(),
    );

    let user_message = if native_images.is_empty() {
        ChatMessage::user(&prompt_text)
    } else {
        let mut parts = vec![ChatContentPart::Text { text: prompt_text }];
        for image in native_images {
            parts.push(ChatContentPart::Image {
                path: image.path,
                media_type: image.media_type,
            });
        }
        ChatMessage::user_parts(parts)
    };

    Ok(PreparedAttachmentTurn {
        selected_model,
        selected_provider_mode: provider_mode,
        user_message,
        attachment_analysis_summary,
    })
}

fn resolve_attachment(attachment: &ChatAttachment) -> ResolvedAttachmentInput {
    ResolvedAttachmentInput {
        name: attachment.name.clone(),
        path: attachment.stored_path.clone(),
        preview_url: attachment.preview_url.clone(),
        media_type: attachment.content_type.clone(),
        kind: attachment.kind.clone(),
    }
}

async fn extract_local_document_text(
    attachment: &ResolvedAttachmentInput,
) -> Result<Option<String>> {
    if !is_locally_extractable_document(attachment) {
        return Ok(None);
    }

    let bytes = tokio::fs::read(&attachment.path)
        .await
        .with_context(|| format!("Failed to read attachment '{}'", attachment.name))?;
    let text = String::from_utf8_lossy(&bytes).trim().to_string();
    if text.is_empty() {
        return Ok(None);
    }

    Ok(Some(truncate_analysis(&text)))
}

fn is_locally_extractable_document(attachment: &ResolvedAttachmentInput) -> bool {
    let path = attachment.path.to_ascii_lowercase();
    let media = attachment.media_type.to_ascii_lowercase();
    path.ends_with(".txt")
        || path.ends_with(".md")
        || path.ends_with(".json")
        || path.ends_with(".toml")
        || path.ends_with(".yaml")
        || path.ends_with(".yml")
        || media.starts_with("text/")
        || media == "application/json"
        || media == "application/toml"
}

async fn analyze_via_mcp(
    config: &Config,
    preferred_mcp: &HashSet<String>,
    attachments: &[ResolvedAttachmentInput],
    user_prompt: &str,
    required_capabilities: &[&str],
    stream_tx: Option<&mpsc::Sender<StreamChunk>>,
) -> Result<Vec<String>> {
    let candidates = candidate_mcp_servers(config, preferred_mcp, required_capabilities);
    if candidates.is_empty() {
        anyhow::bail!(
            "No enabled MCP server exposes the required capabilities ({}). Configure one in MCP settings or set a multimodal vision model.",
            required_capabilities.join(", ")
        );
    }

    let mut out = Vec::new();
    for attachment in attachments {
        if crate::agent::stop::is_stop_requested() {
            return Err(crate::agent::stop::cancellation_error());
        }

        let mut last_error = None;
        for (server_name, server_config) in &candidates {
            send_status(stream_tx, &format!("Using MCP: {server_name}")).await;
            match analyze_attachment_with_server(
                server_name,
                server_config,
                &config.security.execution_sandbox,
                attachment,
                user_prompt,
                required_capabilities,
            )
            .await
            {
                Ok(summary) => {
                    out.push(format_attachment_analysis(
                        &attachment.name,
                        &format!("MCP server {server_name}"),
                        &summary,
                    ));
                    last_error = None;
                    break;
                }
                Err(error) => {
                    last_error = Some(format!("{server_name}: {error}"));
                }
            }
        }

        if let Some(error) = last_error {
            anyhow::bail!(
                "Unable to analyze attachment '{}' with the configured MCP fallback: {}",
                attachment.name,
                error
            );
        }
    }

    Ok(out)
}

fn candidate_mcp_servers<'a>(
    config: &'a Config,
    preferred_mcp: &HashSet<String>,
    required_capabilities: &[&str],
) -> Vec<(&'a String, &'a McpServerConfig)> {
    let required = required_capabilities
        .iter()
        .map(|item| item.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    let mut preferred = Vec::new();
    let mut others = Vec::new();

    for (name, server) in &config.mcp.servers {
        if !server.enabled {
            continue;
        }
        let capabilities = server
            .capabilities
            .iter()
            .map(|item| item.trim().to_ascii_lowercase())
            .filter(|item| !item.is_empty())
            .collect::<BTreeSet<_>>();
        if capabilities.is_disjoint(&required) {
            continue;
        }

        if preferred_mcp.contains(&name.to_ascii_lowercase()) {
            preferred.push((name, server));
        } else {
            others.push((name, server));
        }
    }

    preferred.sort_by(|a, b| a.0.cmp(b.0));
    others.sort_by(|a, b| a.0.cmp(b.0));
    preferred.extend(others);
    preferred
}

async fn analyze_attachment_with_server(
    server_name: &str,
    server_config: &McpServerConfig,
    sandbox_config: &crate::config::ExecutionSandboxConfig,
    attachment: &ResolvedAttachmentInput,
    user_prompt: &str,
    required_capabilities: &[&str],
) -> Result<String> {
    #[cfg(feature = "mcp")]
    {
        let tools =
            crate::tools::mcp::list_tools_once(server_name, server_config, sandbox_config).await?;
        let mut matches = tools
            .iter()
            .filter(|tool| tool_matches_capabilities(tool, required_capabilities))
            .collect::<Vec<_>>();
        matches.sort_by(|a, b| a.name.cmp(&b.name));

        if matches.is_empty() {
            anyhow::bail!(
                "No MCP tool matches capabilities {}",
                required_capabilities.join(", ")
            );
        }

        for tool in matches {
            if crate::agent::stop::is_stop_requested() {
                return Err(crate::agent::stop::cancellation_error());
            }

            let args = serde_json::json!({
                "path": attachment.path,
                "file_path": attachment.path,
                "stored_path": attachment.path,
                "name": attachment.name,
                "mime_type": attachment.media_type,
                "content_type": attachment.media_type,
                "kind": attachment.kind,
                "preview_url": attachment.preview_url,
                "prompt": user_prompt,
                "user_prompt": user_prompt,
                "question": user_prompt,
                "instruction": default_attachment_instruction(attachment),
            });

            let response = tokio::select! {
                result = crate::tools::mcp::call_tool_once(server_name, server_config, sandbox_config, &tool.name, args) => result,
                _ = crate::agent::stop::wait_for_stop() => Err(crate::agent::stop::cancellation_error()),
            };
            if let Ok(result) = response {
                if !result.trim().is_empty() {
                    return Ok(truncate_analysis(&result));
                }
            }
        }

        anyhow::bail!("No compatible MCP tool returned usable output")
    }

    #[cfg(not(feature = "mcp"))]
    {
        let _ = (
            server_name,
            server_config,
            sandbox_config,
            attachment,
            user_prompt,
            required_capabilities,
        );
        anyhow::bail!("MCP support is not enabled in this build")
    }
}

#[cfg(feature = "mcp")]
fn tool_matches_capabilities(
    tool: &crate::tools::mcp::McpToolInfo,
    required_capabilities: &[&str],
) -> bool {
    let haystack = format!(
        "{} {}",
        tool.name.to_ascii_lowercase(),
        tool.description.to_ascii_lowercase()
    );

    required_capabilities
        .iter()
        .any(|capability| match *capability {
            "image-analysis" => {
                haystack.contains("image")
                    || haystack.contains("vision")
                    || haystack.contains("caption")
            }
            "ocr" => haystack.contains("ocr") || haystack.contains("text from image"),
            "document-extraction" => {
                haystack.contains("document")
                    || haystack.contains("extract")
                    || haystack.contains("read_file")
                    || haystack.contains("parse")
            }
            "pdf-reading" => haystack.contains("pdf"),
            _ => false,
        })
}

fn default_attachment_instruction(attachment: &ResolvedAttachmentInput) -> &'static str {
    if attachment.kind == "image" {
        "Analyze the image and return the relevant findings as plain text."
    } else {
        "Extract the document text or summarize the document content as plain text."
    }
}

fn compose_prompt_text(
    base_text: &str,
    attachment_summary: Option<&str>,
    has_native_media: bool,
) -> String {
    let mut prompt = base_text.trim().to_string();

    if let Some(summary) = attachment_summary.filter(|summary| !summary.trim().is_empty()) {
        if !prompt.is_empty() {
            prompt.push_str("\n\n");
        }
        prompt.push_str("Attachment analysis:\n");
        prompt.push_str(summary.trim());
    }

    if prompt.trim().is_empty() {
        if has_native_media {
            "Analyze the attached media and answer the user's request.".to_string()
        } else {
            "Use the attachment analysis to answer the user's request.".to_string()
        }
    } else {
        prompt
    }
}

fn format_attachment_analysis(name: &str, source: &str, body: &str) -> String {
    format!(
        "- {} ({})\n{}",
        name,
        source,
        indent_block(body.trim(), "  ")
    )
}

fn indent_block(body: &str, prefix: &str) -> String {
    body.lines()
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn truncate_analysis(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.len() <= MAX_ATTACHMENT_ANALYSIS_CHARS {
        return trimmed.to_string();
    }
    format!(
        "{}\n\n[truncated {} chars]",
        &trimmed[..MAX_ATTACHMENT_ANALYSIS_CHARS],
        trimmed.len() - MAX_ATTACHMENT_ANALYSIS_CHARS
    )
}

async fn send_status(stream_tx: Option<&mpsc::Sender<StreamChunk>>, label: &str) {
    if let Some(tx) = stream_tx {
        let _ = tx
            .send(StreamChunk {
                delta: label.to_string(),
                done: false,
                event_type: Some("status".to_string()),
                tool_call_data: None,
            })
            .await;
    }
}

async fn send_model(stream_tx: Option<&mpsc::Sender<StreamChunk>>, model: &str) {
    if let Some(tx) = stream_tx {
        let _ = tx
            .send(StreamChunk {
                delta: model.to_string(),
                done: false,
                event_type: Some("model".to_string()),
                tool_call_data: None,
            })
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::{candidate_mcp_servers, compose_prompt_text, prepare_turn, SelectedProviderMode};
    use crate::config::{Config, McpServerConfig};
    use crate::web::chat_attachments::{encode_inline_context, ChatAttachment, ChatMcpServerRef};
    use std::collections::HashMap;

    #[tokio::test]
    async fn multimodal_turn_prefers_active_model_when_supported() {
        let mut config = Config::default();
        config.agent.model = "openai/gpt-4o".to_string();
        let attachment = ChatAttachment {
            kind: "image".to_string(),
            name: "photo.png".to_string(),
            stored_path: "/tmp/photo.png".to_string(),
            preview_url: "/api/v1/chat/uploads/default/photo.png".to_string(),
            content_type: "image/png".to_string(),
            size_bytes: 1,
        };
        let raw = encode_inline_context("describe this", &[attachment], &[]).unwrap();

        let prepared = prepare_turn(&config, &raw, None).await.unwrap();
        assert_eq!(prepared.selected_model, "openai/gpt-4o");
        assert_eq!(
            prepared.selected_provider_mode,
            SelectedProviderMode::Multimodal
        );
        assert!(prepared.user_message.content_parts.is_some());
    }

    #[tokio::test]
    async fn router_falls_back_to_vision_model_for_images() {
        let mut config = Config::default();
        config.agent.model = "openai/o3-mini".to_string();
        config.agent.vision_model = "openai/gpt-4o".to_string();
        let attachment = ChatAttachment {
            kind: "image".to_string(),
            name: "photo.png".to_string(),
            stored_path: "/tmp/photo.png".to_string(),
            preview_url: "/api/v1/chat/uploads/default/photo.png".to_string(),
            content_type: "image/png".to_string(),
            size_bytes: 1,
        };
        let raw = encode_inline_context("describe this", &[attachment], &[]).unwrap();

        let prepared = prepare_turn(&config, &raw, None).await.unwrap();
        assert_eq!(prepared.selected_model, "openai/gpt-4o");
    }

    #[test]
    fn candidate_servers_prefer_selected_mcp_servers() {
        let mut config = Config::default();
        config.mcp.servers.insert(
            "zeta".to_string(),
            McpServerConfig {
                transport: "stdio".to_string(),
                command: Some("echo".to_string()),
                args: vec![],
                url: None,
                env: HashMap::new(),
                capabilities: vec!["image-analysis".to_string()],
                enabled: true,
                recipe_id: None,
            },
        );
        config.mcp.servers.insert(
            "alpha".to_string(),
            McpServerConfig {
                transport: "stdio".to_string(),
                command: Some("echo".to_string()),
                args: vec![],
                url: None,
                env: HashMap::new(),
                capabilities: vec!["image-analysis".to_string()],
                enabled: true,
                recipe_id: None,
            },
        );

        let preferred = [ChatMcpServerRef {
            name: "zeta".to_string(),
            transport: "stdio".to_string(),
        }]
        .iter()
        .map(|item| item.name.to_ascii_lowercase())
        .collect();
        let ordered = candidate_mcp_servers(&config, &preferred, &["image-analysis"]);
        assert_eq!(ordered[0].0.as_str(), "zeta");
    }

    #[test]
    fn compose_prompt_text_adds_analysis_block() {
        let rendered = compose_prompt_text("hello", Some("- file\n  text"), false);
        assert!(rendered.contains("Attachment analysis:"));
        assert!(rendered.contains("hello"));
    }
}
