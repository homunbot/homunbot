use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event as CrosstermEvent, KeyEvent, KeyEventKind};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// Application events
#[derive(Debug, Clone)]
pub enum Event {
    /// A key was pressed
    Key(KeyEvent),
    /// Periodic tick (for animations, async updates)
    Tick,
    /// WhatsApp pairing code received
    WhatsAppPairingCode { code: String, timeout_secs: u64 },
    /// WhatsApp paired successfully
    WhatsAppPairSuccess,
    /// WhatsApp pairing error
    WhatsAppPairError(String),
    /// WhatsApp connected (logged in)
    WhatsAppConnected,
    /// WhatsApp logged out
    WhatsAppLoggedOut,
    /// WhatsApp QR code data (fallback)
    WhatsAppQrCode { data: String },
    /// Skills: installed skills loaded
    SkillsLoaded(Vec<super::app::SkillEntry>),
    /// Skills: search results received
    SkillSearchResults(Vec<super::app::SkillEntry>),
    /// Skills: skill installed, auto-setup starting (message, skill_name)
    SkillInstalled(String, String),
    /// Skills: auto-setup step update (step_index, updated step)
    SkillSetupStep(usize, super::app::SetupStep),
    /// Skills: auto-setup finished
    SkillSetupDone,
    /// Skills: skill removed successfully
    SkillRemoved(String),
    /// Skills: error occurred
    SkillsError(String),
}

/// Async event handler using crossterm + tokio.
///
/// Spawns a background task that polls for terminal events
/// and sends them through an mpsc channel.
///
/// External tasks (e.g. WhatsApp pairing) can inject events
/// via the `tx()` sender.
pub struct EventHandler {
    rx: mpsc::UnboundedReceiver<Event>,
    tx: mpsc::UnboundedSender<Event>,
    _task: JoinHandle<()>,
}

impl EventHandler {
    /// Create a new event handler with the given tick rate.
    pub fn new(tick_rate: Duration) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();

        let poll_tx = tx.clone();
        let task = tokio::spawn(async move {
            loop {
                // Poll for crossterm events with timeout = tick_rate
                let has_event = tokio::task::spawn_blocking({
                    let tick_rate = tick_rate;
                    move || event::poll(tick_rate).unwrap_or(false)
                })
                .await
                .unwrap_or(false);

                if has_event {
                    // Read the event (blocking but should be immediate since poll said true)
                    let evt = tokio::task::spawn_blocking(event::read)
                        .await
                        .ok()
                        .and_then(|r| r.ok());

                    if let Some(CrosstermEvent::Key(key)) = evt {
                        // Only handle key press events (not release/repeat)
                        if key.kind == KeyEventKind::Press && poll_tx.send(Event::Key(key)).is_err()
                        {
                            break; // Channel closed
                        }
                    }
                } else {
                    // No event within tick_rate — send tick
                    if poll_tx.send(Event::Tick).is_err() {
                        break; // Channel closed
                    }
                }
            }
        });

        Self {
            rx,
            tx,
            _task: task,
        }
    }

    /// Get a clone of the event sender for injecting events from external tasks.
    pub fn tx(&self) -> mpsc::UnboundedSender<Event> {
        self.tx.clone()
    }

    /// Wait for the next event.
    pub async fn next(&mut self) -> Result<Event> {
        self.rx
            .recv()
            .await
            .ok_or_else(|| anyhow::anyhow!("Event channel closed"))
    }
}
