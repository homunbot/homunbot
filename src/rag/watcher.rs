//! Directory watcher for automatic RAG ingestion.
//!
//! Monitors configured directories and auto-ingests new or modified files
//! into the knowledge base. Follows the same pattern as `SkillWatcher`.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::Mutex;

use super::chunker::is_supported;
use super::engine::RagEngine;
use crate::utils::watcher::{spawn_watched, WatcherHandle};

/// Watches directories for file changes and auto-ingests into the RAG engine.
pub struct RagWatcher {
    engine: Arc<Mutex<RagEngine>>,
    watch_dirs: Vec<PathBuf>,
}

impl RagWatcher {
    pub fn new(engine: Arc<Mutex<RagEngine>>, watch_dirs: Vec<PathBuf>) -> Self {
        Self { engine, watch_dirs }
    }

    pub fn start(self) -> WatcherHandle {
        spawn_watched(
            move |stop_rx| self.watch_loop(stop_rx),
            "rag-watcher",
        )
    }

    async fn watch_loop(self, mut stop_rx: tokio::sync::oneshot::Receiver<()>) -> Result<()> {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<PathBuf>(100);

        let mut watcher: RecommendedWatcher = {
            let tx = tx.clone();
            notify::recommended_watcher(move |res: Result<Event, notify::Error>| match res {
                Ok(event) => {
                    if matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
                        for path in event.paths {
                            if path.is_file() && is_supported(&path) {
                                let _ = tx.try_send(path);
                            }
                        }
                    }
                }
                Err(e) => tracing::warn!(error = %e, "RAG watcher error"),
            })?
        };

        for dir in &self.watch_dirs {
            if dir.exists() {
                watcher.watch(dir, RecursiveMode::Recursive)?;
                tracing::info!(path = %dir.display(), "RAG watcher started");
            } else {
                tracing::warn!(path = %dir.display(), "Watch dir does not exist, skipping");
            }
        }

        loop {
            // Wait for a file event or stop signal
            tokio::select! {
                _ = &mut stop_rx => break,
                path = rx.recv() => {
                    let Some(first_path) = path else { break };
                    // Debounce: collect paths for 500ms
                    let mut paths = vec![first_path];
                    let debounce = tokio::time::sleep(Duration::from_millis(500));
                    tokio::pin!(debounce);
                    loop {
                        tokio::select! {
                            _ = &mut debounce => break,
                            _ = &mut stop_rx => {
                                drop(watcher);
                                return Ok(());
                            }
                            more = rx.recv() => {
                                match more {
                                    Some(p) => {
                                        if !paths.contains(&p) {
                                            paths.push(p);
                                        }
                                    }
                                    None => break,
                                }
                            }
                        }
                    }
                    // Ingest collected files
                    let mut engine = self.engine.lock().await;
                    for p in paths {
                        match engine.reingest_file(&p, "watcher").await {
                            Ok(Some(id)) => {
                                tracing::info!(path = %p.display(), source_id = id, "Auto-ingested file");
                            }
                            Ok(None) => {} // unchanged or already indexed
                            Err(e) => {
                                tracing::warn!(path = %p.display(), error = %e, "Failed to auto-ingest");
                            }
                        }
                    }
                }
            }
        }

        drop(watcher);
        tracing::debug!("RAG watcher stopped cleanly");
        Ok(())
    }
}
