use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::Mutex;

use super::registry::{get_optional_string, get_string_param, Tool, ToolContext, ToolResult};
use crate::rag::RagEngine;

pub struct KnowledgeTool {
    engine: Arc<Mutex<RagEngine>>,
}

impl KnowledgeTool {
    pub fn new(engine: Arc<Mutex<RagEngine>>) -> Self {
        Self { engine }
    }
}

#[async_trait]
impl Tool for KnowledgeTool {
    fn name(&self) -> &str {
        "knowledge"
    }

    fn description(&self) -> &str {
        "Search and manage the user's personal knowledge base (indexed documents). \
         Use 'search' to retrieve the actual text content from indexed files — returns full chunk text, not just filenames. \
         This is the ONLY way to read the content of files uploaded by the user. \
         Use 'list' to see which files are indexed. \
         Use 'ingest' to add a new file or directory. \
         Use 'remove' to delete a source by ID."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["search", "ingest", "list", "remove"],
                    "description": "Action to perform"
                },
                "query": {
                    "type": "string",
                    "description": "Search query to find content in indexed documents (for 'search' action). Use keywords or the filename to retrieve the actual text content."
                },
                "path": {
                    "type": "string",
                    "description": "File or directory path (for 'ingest' action)"
                },
                "source_id": {
                    "type": "integer",
                    "description": "Source ID to remove (for 'remove' action)"
                },
                "recursive": {
                    "type": "boolean",
                    "description": "Recurse into subdirectories (for 'ingest' on a directory, default false)"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<ToolResult> {
        let action = get_string_param(&args, "action")?;

        match action.as_str() {
            "search" => {
                let query = get_string_param(&args, "query")?;
                let mut engine = self.engine.lock().await;
                let results = engine.search(&query, 5).await?;

                if results.is_empty() {
                    return Ok(ToolResult {
                        output: "No results found in knowledge base.".to_string(),
                        is_error: false,
                    });
                }

                let mut output = format!("Found {} results:\n\n", results.len());
                for (i, r) in results.iter().enumerate() {
                    let sensitive_tag = if r.chunk.sensitive {
                        " [SENSITIVE]"
                    } else {
                        ""
                    };
                    output.push_str(&format!(
                        "{}. [{}] (chunk {}, score {:.3}){}\n{}\n\n",
                        i + 1,
                        r.source_file,
                        r.chunk.chunk_index,
                        r.score,
                        sensitive_tag,
                        r.chunk.content,
                    ));
                }

                Ok(ToolResult {
                    output,
                    is_error: false,
                })
            }

            "ingest" => {
                let path_str = get_string_param(&args, "path")?;
                let path = expand_tilde(&path_str);
                let recursive = args
                    .get("recursive")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let mut engine = self.engine.lock().await;

                if path.is_dir() {
                    let ids = engine.ingest_directory(&path, recursive, "tool").await?;
                    Ok(ToolResult {
                        output: format!("Ingested {} files from {}", ids.len(), path.display()),
                        is_error: false,
                    })
                } else if path.is_file() {
                    match engine.ingest_file(&path, "tool").await? {
                        Some(id) => Ok(ToolResult {
                            output: format!("File {} indexed (source_id={})", path.display(), id),
                            is_error: false,
                        }),
                        None => Ok(ToolResult {
                            output: format!("File {} already indexed (skipped)", path.display()),
                            is_error: false,
                        }),
                    }
                } else {
                    Ok(ToolResult {
                        output: format!("Path not found: {}", path.display()),
                        is_error: true,
                    })
                }
            }

            "list" => {
                let engine = self.engine.lock().await;
                let sources = engine.list_sources().await?;

                if sources.is_empty() {
                    return Ok(ToolResult {
                        output: "Knowledge base is empty. Use 'ingest' to add files.".to_string(),
                        is_error: false,
                    });
                }

                let mut output = format!("{} sources indexed:\n\n", sources.len());
                for s in &sources {
                    output.push_str(&format!(
                        "- [{}] {} ({}, {} chunks, {})\n",
                        s.id, s.file_name, s.doc_type, s.chunk_count, s.status,
                    ));
                }

                Ok(ToolResult {
                    output,
                    is_error: false,
                })
            }

            "remove" => {
                let id_str = get_optional_string(&args, "source_id").or_else(|| {
                    args.get("source_id")
                        .and_then(|v| v.as_i64())
                        .map(|v| v.to_string())
                });

                let source_id: i64 = match id_str {
                    Some(s) => s
                        .parse()
                        .map_err(|_| anyhow::anyhow!("Invalid source_id"))?,
                    None => anyhow::bail!("Missing required parameter: source_id"),
                };

                let mut engine = self.engine.lock().await;
                let deleted = engine.remove_source(source_id).await?;

                Ok(ToolResult {
                    output: if deleted {
                        format!("Source {} removed from knowledge base.", source_id)
                    } else {
                        format!("Source {} not found.", source_id)
                    },
                    is_error: false,
                })
            }

            other => Ok(ToolResult {
                output: format!(
                    "Unknown action '{}'. Use: search, ingest, list, remove.",
                    other
                ),
                is_error: true,
            }),
        }
    }
}

/// Expand ~ to home directory.
fn expand_tilde(path: &str) -> std::path::PathBuf {
    if path.starts_with("~/") {
        dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(&path[2..])
    } else {
        std::path::PathBuf::from(path)
    }
}
