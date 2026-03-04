use std::fmt;
use std::sync::OnceLock;

use serde::Serialize;
use tokio::sync::broadcast;
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer};

const LOG_STREAM_CAPACITY: usize = 2048;

#[derive(Debug, Clone, Serialize)]
pub struct LogRecord {
    pub timestamp: String,
    pub level: String,
    pub target: String,
    pub message: String,
}

static LOG_STREAM: OnceLock<broadcast::Sender<LogRecord>> = OnceLock::new();

fn log_stream() -> &'static broadcast::Sender<LogRecord> {
    LOG_STREAM.get_or_init(|| {
        let (tx, _rx) = broadcast::channel(LOG_STREAM_CAPACITY);
        tx
    })
}

pub fn subscribe() -> broadcast::Receiver<LogRecord> {
    log_stream().subscribe()
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SseLogLayer;

impl<S> Layer<S> for SseLogLayer
where
    S: Subscriber,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let metadata = event.metadata();
        let mut visitor = LogFieldVisitor::default();
        event.record(&mut visitor);

        let extra_fields = visitor.render_extra_fields();
        let mut message = visitor.message.take().unwrap_or_default();
        if !extra_fields.is_empty() {
            if !message.is_empty() {
                message.push(' ');
            }
            message.push_str(&extra_fields);
        }
        if message.trim().is_empty() {
            message = metadata.name().to_string();
        }

        let record = LogRecord {
            timestamp: chrono::Utc::now().to_rfc3339(),
            level: metadata.level().as_str().to_ascii_lowercase(),
            target: metadata.target().to_string(),
            message,
        };

        let _ = log_stream().send(record);
    }
}

#[derive(Default)]
struct LogFieldVisitor {
    message: Option<String>,
    extra_fields: Vec<(String, String)>,
}

impl LogFieldVisitor {
    fn push_field(&mut self, key: &str, value: String) {
        if key == "message" {
            self.message = Some(value);
        } else {
            self.extra_fields.push((key.to_string(), value));
        }
    }

    fn render_extra_fields(&self) -> String {
        self.extra_fields
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

impl Visit for LogFieldVisitor {
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.push_field(field.name(), value.to_string());
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.push_field(field.name(), value.to_string());
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.push_field(field.name(), value.to_string());
    }

    fn record_i128(&mut self, field: &Field, value: i128) {
        self.push_field(field.name(), value.to_string());
    }

    fn record_u128(&mut self, field: &Field, value: u128) {
        self.push_field(field.name(), value.to_string());
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.push_field(field.name(), value.to_string());
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.push_field(field.name(), value.to_string());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.push_field(field.name(), format!("{value:?}"));
    }
}
