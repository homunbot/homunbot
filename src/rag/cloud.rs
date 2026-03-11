//! Cloud source sync via MCP resources.
//!
//! Downloads resources exposed by MCP servers and ingests them
//! into the RAG knowledge base. This is a framework for future
//! cloud integrations (Google Drive, Notion, etc.) via MCP.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context as _, Result};
use rmcp::model::ResourceContents;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use crate::tools::mcp::McpPeer;

use super::engine::RagEngine;

/// Report of a sync operation.
#[derive(Debug, Default)]
pub struct SyncReport {
    pub new_files: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub errors: Vec<String>,
}

impl std::fmt::Display for SyncReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "new={}, updated={}, unchanged={}, errors={}",
            self.new_files,
            self.updated,
            self.unchanged,
            self.errors.len()
        )
    }
}

/// Syncs MCP server resources into the RAG engine.
pub struct CloudSync {
    engine: Arc<Mutex<RagEngine>>,
    sync_dir: PathBuf,
}

impl CloudSync {
    pub fn new(engine: Arc<Mutex<RagEngine>>, sync_dir: PathBuf) -> Self {
        Self { engine, sync_dir }
    }

    /// Sync resources from a single MCP server into the knowledge base.
    pub async fn sync_from_mcp(&self, peer: &McpPeer, server_name: &str) -> Result<SyncReport> {
        let resources = peer
            .list_resources()
            .await
            .context("Failed to list MCP resources")?;

        if resources.is_empty() {
            tracing::info!(server = server_name, "No resources found on MCP server");
            return Ok(SyncReport::default());
        }

        let server_dir = self.sync_dir.join(server_name);
        std::fs::create_dir_all(&server_dir)
            .with_context(|| format!("Cannot create sync dir {}", server_dir.display()))?;

        let mut report = SyncReport::default();

        for resource in &resources {
            let uri = &resource.raw.uri;
            let name = &resource.raw.name;

            // Derive a safe filename from the resource name or URI
            let filename = safe_filename(name, uri);
            let file_path = server_dir.join(&filename);

            match peer.read_resource(uri).await {
                Ok(contents) => {
                    let data = extract_text_content(&contents);
                    if data.is_empty() {
                        tracing::debug!(uri, "Skipping empty resource");
                        continue;
                    }

                    // Check if file changed
                    let new_hash = hex_sha256(data.as_bytes());
                    if file_path.exists() {
                        let existing = std::fs::read(&file_path).unwrap_or_default();
                        if hex_sha256(&existing) == new_hash {
                            report.unchanged += 1;
                            continue;
                        }
                        report.updated += 1;
                    } else {
                        report.new_files += 1;
                    }

                    // Write to sync dir
                    if let Err(e) = std::fs::write(&file_path, &data) {
                        report.errors.push(format!("{filename}: write failed: {e}"));
                        continue;
                    }

                    // Ingest into RAG
                    let mut engine = self.engine.lock().await;
                    let source = format!("mcp:{server_name}");
                    if let Err(e) = engine.reingest_file(&file_path, &source).await {
                        report
                            .errors
                            .push(format!("{filename}: ingest failed: {e}"));
                    }
                }
                Err(e) => {
                    report.errors.push(format!("{name}: {e}"));
                }
            }
        }

        tracing::info!(
            server = server_name,
            %report,
            "Cloud sync completed"
        );

        Ok(report)
    }
}

/// Extract text content from MCP resource contents.
fn extract_text_content(contents: &[ResourceContents]) -> String {
    let mut text = String::new();
    for content in contents {
        match content {
            ResourceContents::TextResourceContents { text: t, .. } => {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(t);
            }
            ResourceContents::BlobResourceContents { blob, .. } => {
                // Try to decode base64 blob as UTF-8 text
                if let Ok(decoded) = base64_decode(blob) {
                    if let Ok(s) = String::from_utf8(decoded) {
                        if !text.is_empty() {
                            text.push('\n');
                        }
                        text.push_str(&s);
                    }
                }
            }
        }
    }
    text
}

fn base64_decode(s: &str) -> Result<Vec<u8>, base64::DecodeError> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(s)
}

/// Derive a filesystem-safe filename from a resource name or URI.
fn safe_filename(name: &str, uri: &str) -> String {
    let base = if name.is_empty() || name == uri {
        // Use last path segment of URI
        uri.rsplit('/').next().unwrap_or("resource")
    } else {
        name
    };

    // Replace unsafe chars
    let safe: String = base
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();

    if safe.is_empty() {
        "resource.txt".to_string()
    } else if !safe.contains('.') {
        format!("{safe}.txt")
    } else {
        safe
    }
}

fn hex_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}
