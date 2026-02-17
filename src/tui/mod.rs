pub mod app;
pub mod event;
pub mod ui;

use std::io;

use anyhow::Result;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;

use crate::config::Config;

/// Run the TUI dashboard.
///
/// Enters alternate screen + raw mode, runs the event loop,
/// then restores terminal on exit (even on panic).
pub async fn run_dashboard(config: Config) -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stderr = io::stderr();
    execute!(stderr, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stderr);
    let mut terminal = Terminal::new(backend)?;

    // Create app state
    let mut events = event::EventHandler::new(std::time::Duration::from_millis(250));
    let mut app = app::App::new(config);
    app.set_event_tx(events.tx());

    // Main loop
    loop {
        // Draw
        terminal.draw(|frame| ui::draw(frame, &mut app))?;

        // Handle events
        let event = events.next().await?;
        app.handle_event(event);

        if app.should_quit {
            break;
        }
    }

    // Abort any running WhatsApp pairing task
    app.whatsapp_state.abort_pairing();

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    // Save config if modified
    if app.config_modified {
        app.config.save()?;
        println!("Configuration saved.");
    }

    Ok(())
}
