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
            run.events.push(WebChatRunEvent {
                event_type: event_type.clone(),
                name: msg.delta.clone(),
                tool_call: msg.tool_call_data.clone(),
            });
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
}
