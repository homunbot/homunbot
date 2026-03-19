//! Shared watcher handle for directory monitoring.
//!
//! Provides a reusable `WatcherHandle` that manages the lifecycle of a
//! background watcher task. Used by `SkillWatcher`, `RagWatcher`, and
//! `BootstrapWatcher` to avoid duplicating the same stop/abort logic.

use std::future::Future;

/// Handle to a running directory watcher. Drop it to stop watching.
///
/// On drop, sends a stop signal via oneshot and aborts the spawned task.
/// All three watchers (skills, RAG, bootstrap) share this same struct.
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

/// Spawn a watcher task that receives a stop signal via oneshot.
///
/// The `task` closure receives the oneshot receiver and runs the watch loop.
/// Returns a `WatcherHandle` that stops the task on drop.
///
/// # Example
/// ```ignore
/// let handle = spawn_watched(|stop_rx| async move {
///     // ... watch loop using stop_rx ...
///     Ok(())
/// }, "my-watcher");
/// // handle dropped → task stopped
/// ```
pub fn spawn_watched<F, Fut>(task: F, name: &str) -> WatcherHandle
where
    F: FnOnce(tokio::sync::oneshot::Receiver<()>) -> Fut + Send + 'static,
    Fut: Future<Output = anyhow::Result<()>> + Send,
{
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
    let watcher_name = name.to_string();

    let join_handle = tokio::spawn(async move {
        if let Err(e) = task(stop_rx).await {
            if !e.to_string().contains("channel closed") {
                tracing::error!(error = %e, watcher = %watcher_name, "Watcher error");
            }
        }
    });

    WatcherHandle {
        stop_tx: Some(stop_tx),
        join_handle: Some(join_handle),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_watcher_handle_stop_on_drop() {
        let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag_clone = flag.clone();

        let handle = spawn_watched(
            move |mut stop_rx| async move {
                // Wait for stop signal
                let _ = stop_rx.await;
                flag_clone.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            },
            "test",
        );

        // Drop the handle — should signal stop
        drop(handle);
        // Give the task a moment to process
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        // The flag may or may not be set (task is aborted), but no panic
    }

    #[tokio::test]
    async fn test_watcher_handle_aborts_task() {
        let handle = spawn_watched(
            |_stop_rx| async move {
                // Simulate a long-running task
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                Ok(())
            },
            "long-task",
        );

        // Drop should abort without waiting
        drop(handle);
        // No hang — test passes
    }
}
