use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::RwLock;

use super::loader::SkillRegistry;

/// Watches the skills directory for changes and reloads the skills summary.
///
/// When a SKILL.md file is created, modified, or deleted, the watcher
/// re-scans the skills directory and updates the shared skills summary
/// that is read by the agent's `ContextBuilder` on each prompt build.
pub struct SkillWatcher {
    /// Shared handle to the skills summary string (same Arc held by ContextBuilder)
    skills_summary: Arc<RwLock<String>>,
    /// Directory to watch
    skills_dir: PathBuf,
}

impl SkillWatcher {
    pub fn new(skills_summary: Arc<RwLock<String>>, skills_dir: PathBuf) -> Self {
        Self {
            skills_summary,
            skills_dir,
        }
    }

    /// Start watching the skills directory. Returns the join handle.
    /// Runs until cancelled (e.g. when the gateway shuts down).
    pub fn start(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            if let Err(e) = self.watch_loop().await {
                tracing::error!(error = %e, "Skill watcher error");
            }
        })
    }

    async fn watch_loop(self) -> Result<()> {
        // Create the directory if it doesn't exist
        if !self.skills_dir.exists() {
            tokio::fs::create_dir_all(&self.skills_dir).await?;
        }

        let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(10);

        // Create a debounced file watcher.
        // notify sends events synchronously, so we bridge to async via mpsc.
        let mut watcher: RecommendedWatcher = {
            let tx = tx.clone();
            notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
                match res {
                    Ok(event) => {
                        // Only react to content-relevant events
                        if matches!(
                            event.kind,
                            EventKind::Create(_)
                                | EventKind::Modify(_)
                                | EventKind::Remove(_)
                        ) {
                            // Check if any path is a SKILL.md or a directory change
                            let relevant = event.paths.iter().any(|p| {
                                p.file_name()
                                    .map(|n| n == "SKILL.md")
                                    .unwrap_or(false)
                                    || p.is_dir()
                            });
                            if relevant {
                                let _ = tx.try_send(());
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "File watcher error");
                    }
                }
            })?
        };

        watcher.watch(&self.skills_dir, RecursiveMode::Recursive)?;

        tracing::info!(
            path = %self.skills_dir.display(),
            "Skill hot-reload watcher started"
        );

        // Debounce: wait for events, then reload after a brief pause
        // to avoid multiple reloads for a single install operation.
        loop {
            // Wait for at least one event
            if rx.recv().await.is_none() {
                break; // Channel closed
            }

            // Debounce: drain any additional events that arrive within 500ms
            let debounce = tokio::time::sleep(Duration::from_millis(500));
            tokio::pin!(debounce);

            loop {
                tokio::select! {
                    _ = &mut debounce => break,
                    msg = rx.recv() => {
                        if msg.is_none() {
                            return Ok(());
                        }
                        // Reset debounce timer
                    }
                }
            }

            // Re-scan skills and update the shared summary
            tracing::info!("Skills directory changed, reloading...");
            match self.reload_skills().await {
                Ok(count) => {
                    tracing::info!(skills = count, "Skills hot-reloaded successfully");
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to hot-reload skills");
                }
            }
        }

        Ok(())
    }

    /// Re-scan the skills directory and update the shared summary.
    async fn reload_skills(&self) -> Result<usize> {
        let mut registry = SkillRegistry::new();
        registry.scan_directory_public(&self.skills_dir).await?;

        let count = registry.len();
        let summary = registry.build_prompt_summary();

        // Update the shared skills summary atomically
        let mut guard = self.skills_summary.write().await;
        *guard = summary;

        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_reload_skills_empty_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        let summary = Arc::new(RwLock::new("old summary".to_string()));

        let watcher = SkillWatcher::new(summary.clone(), dir.path().to_path_buf());
        let count = watcher.reload_skills().await.unwrap();

        assert_eq!(count, 0);
        // Empty registry produces empty string
        assert_eq!(*summary.read().await, "");
    }

    #[tokio::test]
    async fn test_reload_skills_with_skill() {
        let dir = tempfile::TempDir::new().unwrap();

        // Create a skill
        let skill_dir = dir.path().join("test-skill");
        tokio::fs::create_dir(&skill_dir).await.unwrap();
        tokio::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: test-skill\ndescription: A hot-reloaded skill\n---\n\n# Test\n",
        )
        .await
        .unwrap();

        let summary = Arc::new(RwLock::new(String::new()));
        let watcher = SkillWatcher::new(summary.clone(), dir.path().to_path_buf());
        let count = watcher.reload_skills().await.unwrap();

        assert_eq!(count, 1);
        let s = summary.read().await;
        assert!(s.contains("test-skill"));
        assert!(s.contains("hot-reloaded skill"));
    }
}
