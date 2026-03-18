use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::bus::StreamMessage;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebChatRunEvent {
    pub event_type: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call: Option<crate::provider::ToolCallData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebChatRunSnapshot {
    pub run_id: String,
    pub session_key: String,
    pub status: String,
    pub user_message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_model: Option<String>,
    pub assistant_response: String,
    pub created_at: String,
    pub updated_at: String,
    pub events: Vec<WebChatRunEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Default)]
struct WebRunStoreInner {
    runs: HashMap<String, WebChatRunSnapshot>,
    active_by_session: HashMap<String, String>,
}

#[derive(Debug, Default)]
pub struct WebRunStore {
    next_id: AtomicU64,
    inner: Mutex<WebRunStoreInner>,
}

impl WebRunStore {
    pub fn start_run(
        &self,
        session_key: &str,
        user_message: &str,
    ) -> Result<WebChatRunSnapshot, String> {
        let mut inner = self.inner.lock().expect("web run store lock poisoned");
        if let Some(run_id) = inner.active_by_session.get(session_key) {
            if let Some(run) = inner.runs.get(run_id) {
                if matches!(run.status.as_str(), "running" | "stopping") {
                    return Err("A chat run is already in progress.".to_string());
                }
            }
        }

        let run_id = format!(
            "run_{}_{}",
            Utc::now().timestamp_millis(),
            self.next_id.fetch_add(1, Ordering::Relaxed)
        );
        let now = Utc::now().to_rfc3339();
        let snapshot = WebChatRunSnapshot {
            run_id: run_id.clone(),
            session_key: session_key.to_string(),
            status: "running".to_string(),
            user_message: user_message.to_string(),
            effective_model: None,
            assistant_response: String::new(),
            created_at: now.clone(),
            updated_at: now,
            events: Vec::new(),
            error: None,
        };

        inner
            .active_by_session
            .insert(session_key.to_string(), run_id.clone());
        inner.runs.insert(run_id, snapshot.clone());

        Ok(snapshot)
    }

    pub fn active_snapshot(&self, session_key: &str) -> Option<WebChatRunSnapshot> {
        let inner = self.inner.lock().expect("web run store lock poisoned");
        let run_id = inner.active_by_session.get(session_key)?;
        inner.runs.get(run_id).cloned()
    }

    pub fn append_stream_message(
        &self,
        session_key: &str,
        msg: &StreamMessage,
    ) -> Option<WebChatRunSnapshot> {
        let mut inner = self.inner.lock().expect("web run store lock poisoned");
        let run_id = inner.active_by_session.get(session_key).cloned()?;
        let run = inner.runs.get_mut(&run_id)?;

        run.updated_at = Utc::now().to_rfc3339();
        if let Some(event_type) = &msg.event_type {
            if event_type == "model" && !msg.delta.trim().is_empty() {
                run.effective_model = Some(msg.delta.clone());
            }
            let event = WebChatRunEvent {
                event_type: event_type.clone(),
                name: msg.delta.clone(),
                tool_call: msg.tool_call_data.clone(),
            };
            // Plan events: keep only the latest snapshot (replace, don't accumulate)
            // to avoid replaying stale intermediate states on reconnect.
            if event_type == "plan" {
                if let Some(existing) =
                    run.events.iter_mut().rev().find(|e| e.event_type == "plan")
                {
                    *existing = event;
                } else {
                    run.events.push(event);
                }
            } else {
                run.events.push(event);
            }
        } else if !msg.delta.is_empty() {
            run.assistant_response.push_str(&msg.delta);
        }
        Some(run.clone())
    }

    pub fn complete_run(
        &self,
        session_key: &str,
        final_response: &str,
    ) -> Option<WebChatRunSnapshot> {
        let mut inner = self.inner.lock().expect("web run store lock poisoned");
        let run_id = inner.active_by_session.remove(session_key)?;
        let run = inner.runs.get_mut(&run_id)?;
        run.status = "completed".to_string();
        run.assistant_response = final_response.to_string();
        run.updated_at = Utc::now().to_rfc3339();
        Some(run.clone())
    }

    pub fn request_stop(&self, session_key: &str) -> Option<WebChatRunSnapshot> {
        let mut inner = self.inner.lock().expect("web run store lock poisoned");
        let run_id = inner.active_by_session.get(session_key).cloned()?;
        let run = inner.runs.get_mut(&run_id)?;
        run.status = "stopping".to_string();
        run.updated_at = Utc::now().to_rfc3339();
        Some(run.clone())
    }

    pub fn clear_session(&self, session_key: &str) {
        let mut inner = self.inner.lock().expect("web run store lock poisoned");
        if let Some(run_id) = inner.active_by_session.remove(session_key) {
            inner.runs.remove(&run_id);
        }
        inner.runs.retain(|_, run| run.session_key != session_key);
    }

    /// Mark runs that have been "running" or "stopping" for too long as
    /// "interrupted".  Prevents orphaned runs when the agent crashes or
    /// the WebSocket disconnects without a clean completion.
    pub fn expire_stale_runs(&self, max_age_secs: u64) {
        let cutoff = Utc::now() - chrono::Duration::seconds(max_age_secs as i64);
        let mut inner = self.inner.lock().expect("web run store lock poisoned");
        let mut expired = Vec::new();
        for run in inner.runs.values_mut() {
            if matches!(run.status.as_str(), "running" | "stopping") {
                if let Ok(created) = chrono::DateTime::parse_from_rfc3339(&run.created_at) {
                    if created < cutoff {
                        run.status = "interrupted".to_string();
                        run.updated_at = Utc::now().to_rfc3339();
                        expired.push(run.session_key.clone());
                    }
                }
            }
        }
        for key in &expired {
            inner.active_by_session.remove(key);
        }
        if !expired.is_empty() {
            tracing::info!(count = expired.len(), "Expired stale web chat runs");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::WebRunStore;
    use crate::bus::StreamMessage;

    #[test]
    fn run_store_tracks_active_run_and_completion() {
        let store = WebRunStore::default();
        let snapshot = store.start_run("web:default", "ciao").unwrap();
        assert_eq!(snapshot.status, "running");
        assert!(store.active_snapshot("web:default").is_some());

        store.append_stream_message(
            "web:default",
            &StreamMessage {
                chat_id: "default".to_string(),
                delta: "hello".to_string(),
                done: false,
                event_type: None,
                tool_call_data: None,
            },
        );

        let active = store.active_snapshot("web:default").unwrap();
        assert_eq!(active.assistant_response, "hello");

        let done = store.complete_run("web:default", "hello world").unwrap();
        assert_eq!(done.status, "completed");
        assert!(store.active_snapshot("web:default").is_none());
    }

    #[test]
    fn expire_stale_runs_marks_old_as_interrupted() {
        let store = WebRunStore::default();
        let run = store.start_run("web:stale", "old message").unwrap();
        assert_eq!(run.status, "running");

        // With max_age=0 every running run is considered stale
        store.expire_stale_runs(0);

        // Run should be interrupted and no longer active
        assert!(store.active_snapshot("web:stale").is_none());
        let inner = store.inner.lock().unwrap();
        let expired = inner.runs.values().next().unwrap();
        assert_eq!(expired.status, "interrupted");
    }

    #[test]
    fn plan_events_keep_only_latest() {
        let store = WebRunStore::default();
        store.start_run("web:plan", "do something").unwrap();

        // Send two plan events
        store.append_stream_message(
            "web:plan",
            &StreamMessage {
                chat_id: "plan".into(),
                delta: r#"{"objective":"step 1"}"#.into(),
                done: false,
                event_type: Some("plan".into()),
                tool_call_data: None,
            },
        );
        store.append_stream_message(
            "web:plan",
            &StreamMessage {
                chat_id: "plan".into(),
                delta: r#"{"objective":"step 2"}"#.into(),
                done: false,
                event_type: Some("plan".into()),
                tool_call_data: None,
            },
        );

        let snap = store.active_snapshot("web:plan").unwrap();
        let plan_events: Vec<_> = snap
            .events
            .iter()
            .filter(|e| e.event_type == "plan")
            .collect();
        assert_eq!(plan_events.len(), 1, "should keep only the latest plan event");
        assert!(plan_events[0].name.contains("step 2"));
    }
}
