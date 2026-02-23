//! Bootstrap file watcher — hot-reloads USER.md, SOUL.md, etc.
//!
//! Watches both:
//! - `~/.homun/brain/` — agent-written memory files
//! - `~/.homun/` — user-placed config files
//!
//! When any bootstrap file changes, updates both:
//! - `bootstrap_content` (legacy string format)
//! - `bootstrap_files` (new Vec<(filename, content)> format)

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::RwLock;

/// Bootstrap file names that the agent can edit and read.
const BOOTSTRAP_FILES: &[&str] = &[
    "USER.md",
    "SOUL.md",
    "AGENTS.md",
    "INSTRUCTIONS.md",
];

/// Legacy bootstrap content (string format for backward compatibility).
pub type BootstrapContent = Arc<RwLock<String>>;

/// New bootstrap files format (filename, content) pairs for modular prompt system.
pub type BootstrapFiles = Arc<RwLock<Vec<(String, String)>>>;

/// Watches bootstrap files for changes and reloads their content.
///
/// Updates both legacy and new format so all parts of the system stay synchronized.
pub struct BootstrapWatcher {
    /// Legacy format: concatenated string
    bootstrap_content: BootstrapContent,
    /// New format: (filename, content) pairs
    bootstrap_files: BootstrapFiles,
    /// Base data directory (~/.homun/)
    data_dir: PathBuf,
}

/// Handle to a running watcher. Drop it to stop watching.
pub struct WatcherHandle {
    stop_tx: Option<tokio::sync::oneshot::Sender<()>>,
    join_handle: Option<tokio::task::JoinHandle<()>>,
}

impl Drop for WatcherHandle {
    fn drop(&mut self) {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.join_handle.take() {
            handle.abort();
        }
    }
}

impl BootstrapWatcher {
    /// Create a new watcher with both legacy and new format handles.
    pub fn new(
        bootstrap_content: BootstrapContent,
        bootstrap_files: BootstrapFiles,
        data_dir: PathBuf,
    ) -> Self {
        Self {
            bootstrap_content,
            bootstrap_files,
            data_dir,
        }
    }

    /// Start watching the bootstrap directories. Returns a handle that stops on drop.
    pub fn start(self) -> WatcherHandle {
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
        
        let join_handle = tokio::spawn(async move {
            if let Err(e) = self.watch_loop(stop_rx).await {
                if !e.to_string().contains("channel closed") {
                    tracing::error!(error = %e, "Bootstrap watcher error");
                }
            }
        });
        
        WatcherHandle {
            stop_tx: Some(stop_tx),
            join_handle: Some(join_handle),
        }
    }

    async fn watch_loop(self, mut stop_rx: tokio::sync::oneshot::Receiver<()>) -> Result<()> {
        // Create directories if they don't exist
        let brain_dir = self.data_dir.join("brain");
        if !brain_dir.exists() {
            tokio::fs::create_dir_all(&brain_dir).await?;
        }
        if !self.data_dir.exists() {
            tokio::fs::create_dir_all(&self.data_dir).await?;
        }

        let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(10);

        // Create file watcher
        let mut watcher: RecommendedWatcher = {
            let tx = tx.clone();
            notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
                match res {
                    Ok(event) => {
                        if matches!(
                            event.kind,
                            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                        ) {
                            // Check if any path is a bootstrap file
                            let relevant = event.paths.iter().any(|p| {
                                p.file_name()
                                    .map(|n| BOOTSTRAP_FILES.contains(&n.to_string_lossy().as_ref()))
                                    .unwrap_or(false)
                            });
                            if relevant {
                                let _ = tx.blocking_send(());
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Bootstrap watcher error");
                    }
                }
            })?
        };

        // Watch both directories
        watcher.watch(&brain_dir, RecursiveMode::NonRecursive)?;
        watcher.watch(&self.data_dir, RecursiveMode::NonRecursive)?;

        tracing::info!(
            brain_dir = %brain_dir.display(),
            data_dir = %self.data_dir.display(),
            "Bootstrap watcher started"
        );

        // Debounce loop
        let debounce = Duration::from_millis(200);
        let mut last_event = std::time::Instant::now() - debounce;

        loop {
            tokio::select! {
                _ = &mut stop_rx => {
                    tracing::debug!("Bootstrap watcher received stop signal");
                    break;
                }
                _ = rx.recv() => {
                    let now = std::time::Instant::now();
                    if now.duration_since(last_event) < debounce {
                        continue;
                    }
                    last_event = now;

                    tracing::info!("Bootstrap files changed, reloading...");
                    match self.reload_bootstrap_files().await {
                        Ok(_) => {
                            tracing::info!("Bootstrap files hot-reloaded successfully");
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "Failed to hot-reload bootstrap files");
                        }
                    }
                }
            }
        }

        drop(watcher);
        tracing::debug!("Bootstrap watcher stopped cleanly");

        Ok(())
    }

    /// Re-load all bootstrap files and update both formats.
    async fn reload_bootstrap_files(&self) -> Result<()> {
        let (content, files) = Self::load_bootstrap_files(&self.data_dir);

        // Update legacy format
        {
            let mut guard = self.bootstrap_content.write().await;
            *guard = content;
        }

        // Update new format
        {
            let mut guard = self.bootstrap_files.write().await;
            *guard = files;
        }

        Ok(())
    }

    /// Load bootstrap files from disk.
    fn load_bootstrap_files(data_dir: &std::path::Path) -> (String, Vec<(String, String)>) {
        let mut content = String::new();
        let mut files = Vec::new();
        let brain_dir = data_dir.join("brain");

        for filename in BOOTSTRAP_FILES {
            let label = match *filename {
                "SOUL.md" => "Personality & Identity",
                "AGENTS.md" => "Agent Directives",
                "USER.md" => "User Context",
                "INSTRUCTIONS.md" => "Learned Instructions",
                _ => continue,
            };

            // Try brain/ first (agent-written), then data_dir (user-placed)
            let candidates = [brain_dir.join(filename), data_dir.join(filename)];
            let file_path = match candidates.iter().find(|p| p.exists()) {
                Some(p) => p,
                None => continue,
            };

            match std::fs::read_to_string(file_path) {
                Ok(text) => {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        tracing::debug!(
                            file = %filename,
                            source = %file_path.display(),
                            "Loaded bootstrap file"
                        );
                        // Legacy format
                        content.push_str(&format!("\n\n## {label}\n{trimmed}"));
                        // New format
                        files.push((filename.to_string(), trimmed.to_string()));
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        file = %filename,
                        error = %e,
                        "Failed to read bootstrap file"
                    );
                }
            }
        }

        (content, files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_reload_bootstrap_files_empty_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        let content = Arc::new(RwLock::new("old content".to_string()));
        let files = Arc::new(RwLock::new(vec![("test".to_string(), "test".to_string())]));

        let watcher = BootstrapWatcher::new(content.clone(), files.clone(), dir.path().to_path_buf());
        watcher.reload_bootstrap_files().await.unwrap();

        // Empty directory produces empty content and files
        assert!(content.read().await.is_empty());
        assert!(files.read().await.is_empty());
    }

    #[tokio::test]
    async fn test_reload_bootstrap_files_with_user_md() {
        let dir = tempfile::TempDir::new().unwrap();

        // Create a USER.md
        std::fs::write(
            dir.path().join("USER.md"),
            "The user is a Rust developer named Fabio.",
        )
        .unwrap();

        let content = Arc::new(RwLock::new(String::new()));
        let files = Arc::new(RwLock::new(Vec::new()));

        let watcher = BootstrapWatcher::new(content.clone(), files.clone(), dir.path().to_path_buf());
        watcher.reload_bootstrap_files().await.unwrap();

        let loaded_content = content.read().await;
        let loaded_files = files.read().await;

        // Legacy format
        assert!(loaded_content.contains("User Context"));
        assert!(loaded_content.contains("Fabio"));

        // New format
        assert_eq!(loaded_files.len(), 1);
        assert_eq!(loaded_files[0].0, "USER.md");
        assert!(loaded_files[0].1.contains("Fabio"));
    }

    #[tokio::test]
    async fn test_reload_bootstrap_files_brain_priority() {
        let dir = tempfile::TempDir::new().unwrap();
        let brain_dir = dir.path().join("brain");
        std::fs::create_dir(&brain_dir).unwrap();

        // Create USER.md in both locations
        std::fs::write(
            dir.path().join("USER.md"),
            "User-placed content",
        ).unwrap();
        std::fs::write(
            brain_dir.join("USER.md"),
            "Agent-written content",
        ).unwrap();

        let content = Arc::new(RwLock::new(String::new()));
        let files = Arc::new(RwLock::new(Vec::new()));

        let watcher = BootstrapWatcher::new(content.clone(), files.clone(), dir.path().to_path_buf());
        watcher.reload_bootstrap_files().await.unwrap();

        let loaded_content = content.read().await;
        let loaded_files = files.read().await;

        // brain/ should take priority
        assert!(loaded_content.contains("Agent-written content"));
        assert!(!loaded_content.contains("User-placed content"));
        assert!(loaded_files[0].1.contains("Agent-written content"));
    }
}
