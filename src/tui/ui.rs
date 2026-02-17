use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Tabs},
    Frame,
};

use super::app::{App, EditField, InputMode, Tab, WhatsAppEditMode, WhatsAppField, WhatsAppStatus};

/// Main draw function — called every frame
pub fn draw(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Tab bar
            Constraint::Min(0),   // Body
            Constraint::Length(3), // Footer
        ])
        .split(frame.area());

    draw_tab_bar(frame, app, chunks[0]);
    draw_body(frame, app, chunks[1]);
    draw_footer(frame, app, chunks[2]);
}

/// Draw the tab bar at the top
fn draw_tab_bar(frame: &mut Frame, app: &App, area: Rect) {
    let titles: Vec<Line> = Tab::ALL
        .iter()
        .map(|t| {
            let style = if *t == app.current_tab {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            Line::from(Span::styled(t.title(), style))
        })
        .collect();

    let tabs = Tabs::new(titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" HomunBot Settings "),
        )
        .select(app.current_tab.index())
        .style(Style::default().fg(Color::White))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_widget(tabs, area);
}

/// Draw the body content based on the active tab
fn draw_body(frame: &mut Frame, app: &mut App, area: Rect) {
    match app.current_tab {
        Tab::Settings => draw_settings_tab(frame, app, area),
        Tab::Providers => draw_providers_tab(frame, app, area),
        Tab::WhatsApp => draw_whatsapp_tab(frame, app, area),
        Tab::Skills => draw_placeholder_tab(frame, &app.skills_state.message, area),
        Tab::Mcp => draw_mcp_tab(frame, app, area),
    }
}

/// Draw the footer with keybinding hints
fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let hints = match app.current_tab {
        Tab::Settings => {
            if app.settings_state.input_mode == InputMode::Editing {
                " Enter Save | Esc Cancel "
            } else {
                " \u{2191}\u{2193} Navigate | Enter Edit | Tab Next | q Quit "
            }
        }
        Tab::Providers => {
            if app.providers_state.input_mode == InputMode::Editing {
                " Enter Next/Save | Esc Cancel "
            } else {
                " \u{2191}\u{2193} Navigate | Enter Configure | d Remove | Tab Next | q Quit "
            }
        }
        Tab::WhatsApp => {
            match &app.whatsapp_state.input_mode {
                WhatsAppEditMode::EditingPhone | WhatsAppEditMode::AddingNumber => {
                    " Enter Save | Esc Cancel | digits only "
                }
                WhatsAppEditMode::Normal => {
                    match &app.whatsapp_state.status {
                        WhatsAppStatus::Connecting | WhatsAppStatus::WaitingForCode { .. } => {
                            " x Cancel | a Add number | Tab Next | q Quit "
                        }
                        _ => {
                            " \u{2191}\u{2193} Navigate | e Edit phone | a Add number | d Remove | p Pair | q Quit "
                        }
                    }
                }
            }
        }
        Tab::Mcp => " \u{2191}\u{2193} Navigate | Space Toggle | d Remove | Tab Next | q Quit ",
        _ => " Tab Next | q Quit ",
    };

    let footer = Paragraph::new(hints)
        .style(Style::default().fg(Color::DarkGray))
        .block(Block::default().borders(Borders::ALL));

    frame.render_widget(footer, area);
}

// ── Settings Tab ────────────────────────────────────────────────────

fn draw_settings_tab(frame: &mut Frame, app: &mut App, area: Rect) {
    let items: Vec<ListItem> = app
        .settings_state
        .entries
        .iter()
        .map(|(key, value)| {
            let content = format!("{:<40} {}", key, value);
            ListItem::new(content)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Configuration "),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    frame.render_stateful_widget(list, area, &mut app.settings_state.list_state);

    // Draw edit popup if in editing mode
    if app.settings_state.input_mode == InputMode::Editing {
        let popup_area = centered_rect(60, 7, area);
        frame.render_widget(Clear, popup_area);

        let key = &app.settings_state.edit_key;
        let buf = &app.settings_state.edit_buffer;
        let text = vec![
            Line::from(Span::styled(
                format!(" Key: {key}"),
                Style::default().fg(Color::Cyan),
            )),
            Line::from(""),
            Line::from(format!(" Value: {buf}\u{2588}")),
        ];

        let popup = Paragraph::new(text).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Edit Value ")
                .style(Style::default().fg(Color::White)),
        );

        frame.render_widget(popup, popup_area);
    }
}

// ── Providers Tab ───────────────────────────────────────────────────

fn draw_providers_tab(frame: &mut Frame, app: &mut App, area: Rect) {
    let items: Vec<ListItem> = app
        .providers_state
        .providers
        .iter()
        .map(|p| {
            let status = if p.configured { "\u{2713}" } else { "\u{2717}" };
            let active = if p.is_active { " (active)" } else { "" };
            let content = format!(
                " {status} {:<14} {:<20} {}{active}",
                p.name, p.api_key_masked, p.api_base
            );
            let style = if p.is_active {
                Style::default().fg(Color::Green)
            } else if p.configured {
                Style::default().fg(Color::White)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            ListItem::new(content).style(style)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Providers "),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    frame.render_stateful_widget(list, area, &mut app.providers_state.list_state);

    // Draw edit popup
    if app.providers_state.input_mode == InputMode::Editing {
        let popup_area = centered_rect(60, 9, area);
        frame.render_widget(Clear, popup_area);

        let provider = &app.providers_state.editing_provider;
        let field_label = match app.providers_state.edit_field {
            EditField::ApiKey => "API Key",
            EditField::ApiBase => "Base URL (optional, Enter to skip)",
        };
        let buf = &app.providers_state.edit_buffer;

        let text = vec![
            Line::from(Span::styled(
                format!(" Provider: {provider}"),
                Style::default().fg(Color::Cyan),
            )),
            Line::from(""),
            Line::from(Span::styled(
                format!(" {field_label}:"),
                Style::default().fg(Color::White),
            )),
            Line::from(format!(" {buf}\u{2588}")),
        ];

        let popup = Paragraph::new(text).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Configure Provider ")
                .style(Style::default().fg(Color::White)),
        );

        frame.render_widget(popup, popup_area);
    }
}

// ── WhatsApp Tab ───────────────────────────────────────────────────

fn draw_whatsapp_tab(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" WhatsApp Setup ");

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Layout: phone + allow_from + status
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Phone number
            Constraint::Length(std::cmp::max(app.whatsapp_state.allow_from.len() as u16, 1) + 3), // Allow from list
            Constraint::Min(0),    // Status / pairing code
        ])
        .margin(1)
        .split(inner);

    // Phone number section
    draw_whatsapp_phone(frame, app, chunks[0]);

    // Allowed numbers section
    draw_whatsapp_allow_from(frame, app, chunks[1]);

    // Status / pairing code section
    draw_whatsapp_status(frame, app, chunks[2]);

    // Edit popup overlays
    match &app.whatsapp_state.input_mode {
        WhatsAppEditMode::EditingPhone => {
            let popup_area = centered_rect(60, 7, area);
            frame.render_widget(Clear, popup_area);

            let buf = &app.whatsapp_state.phone_input;
            let text = vec![
                Line::from(Span::styled(
                    " Phone number (with country code, e.g. 393331234567):",
                    Style::default().fg(Color::Cyan),
                )),
                Line::from(""),
                Line::from(format!(" {buf}\u{2588}")),
            ];

            let popup = Paragraph::new(text).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Edit Phone Number ")
                    .style(Style::default().fg(Color::White)),
            );

            frame.render_widget(popup, popup_area);
        }
        WhatsAppEditMode::AddingNumber => {
            let popup_area = centered_rect(60, 7, area);
            frame.render_widget(Clear, popup_area);

            let buf = &app.whatsapp_state.add_number_buffer;
            let text = vec![
                Line::from(Span::styled(
                    " Phone number to allow (e.g. 393331234567):",
                    Style::default().fg(Color::Cyan),
                )),
                Line::from(""),
                Line::from(format!(" {buf}\u{2588}")),
            ];

            let popup = Paragraph::new(text).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Add Allowed Number ")
                    .style(Style::default().fg(Color::White)),
            );

            frame.render_widget(popup, popup_area);
        }
        WhatsAppEditMode::Normal => {}
    }
}

fn draw_whatsapp_phone(frame: &mut Frame, app: &App, area: Rect) {
    let phone = &app.whatsapp_state.phone_input;
    let is_focused = app.whatsapp_state.focused_field == WhatsAppField::Phone
        && app.whatsapp_state.input_mode == WhatsAppEditMode::Normal;
    let display = if phone.is_empty() {
        "(not set)".to_string()
    } else {
        format!("+{phone}")
    };

    let marker = if is_focused { "> " } else { "  " };
    let phone_style = if is_focused {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    };

    let text = vec![Line::from(vec![
        Span::styled(marker, Style::default().fg(Color::Yellow)),
        Span::styled("Phone: ", Style::default().fg(Color::DarkGray)),
        Span::styled(&display, phone_style),
    ])];

    let para = Paragraph::new(text);
    frame.render_widget(para, area);
}

fn draw_whatsapp_allow_from(frame: &mut Frame, app: &App, area: Rect) {
    let is_focused = app.whatsapp_state.focused_field == WhatsAppField::AllowFrom
        && app.whatsapp_state.input_mode == WhatsAppEditMode::Normal;

    let title_style = if is_focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let mut lines = vec![Line::from(Span::styled(
        if app.whatsapp_state.allow_from.is_empty() {
            "  Allowed senders: (all — press 'a' to restrict)"
        } else {
            "  Allowed senders:"
        },
        title_style,
    ))];

    for (i, number) in app.whatsapp_state.allow_from.iter().enumerate() {
        let is_selected = is_focused && app.whatsapp_state.allow_from_selected == Some(i);
        let marker = if is_selected { "  > " } else { "    " };
        let style = if is_selected {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        lines.push(Line::from(Span::styled(
            format!("{marker}+{number}"),
            style,
        )));
    }

    let para = Paragraph::new(lines);
    frame.render_widget(para, area);
}

fn draw_whatsapp_status(frame: &mut Frame, app: &App, area: Rect) {
    let lines = match &app.whatsapp_state.status {
        WhatsAppStatus::NotConfigured => vec![
            Line::from(Span::styled(
                "  No phone number configured. Press 'e' to enter one.",
                Style::default().fg(Color::DarkGray),
            )),
        ],
        WhatsAppStatus::ReadyToPair => vec![
            Line::from(Span::styled(
                "  \u{2713} Ready. Press 'p' to start pairing.",
                Style::default().fg(Color::Green),
            )),
        ],
        WhatsAppStatus::Connecting => vec![
            Line::from(Span::styled(
                "  \u{23f3} Connecting to WhatsApp...",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
        ],
        WhatsAppStatus::WaitingForCode { code, timeout_secs } => vec![
            Line::from(Span::styled(
                "  WhatsApp \u{2192} Linked Devices \u{2192} Link a Device",
                Style::default().fg(Color::Cyan),
            )),
            Line::from(""),
            Line::from(Span::styled(
                format!("     \u{1f517}  {code}"),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                format!("  Valid for {timeout_secs}s"),
                Style::default().fg(Color::DarkGray),
            )),
        ],
        WhatsAppStatus::Paired => vec![
            Line::from(Span::styled(
                "  \u{2705} Paired! Logging in...",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )),
        ],
        WhatsAppStatus::Connected => vec![
            Line::from(Span::styled(
                "  \u{2705} WhatsApp connected!",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "  Run 'homunbot gateway' to start receiving messages.",
                Style::default().fg(Color::DarkGray),
            )),
        ],
        WhatsAppStatus::Error(err) => vec![
            Line::from(Span::styled(
                format!("  \u{274c} {err}"),
                Style::default().fg(Color::Red),
            )),
            Line::from(Span::styled(
                "  Press 'p' to retry.",
                Style::default().fg(Color::DarkGray),
            )),
        ],
    };

    let para = Paragraph::new(lines);
    frame.render_widget(para, area);
}

// ── MCP Tab ─────────────────────────────────────────────────────────

fn draw_mcp_tab(frame: &mut Frame, app: &mut App, area: Rect) {
    if app.mcp_state.servers.is_empty() {
        let text = Paragraph::new(
            " No MCP servers configured.\n\n Add one with: homunbot mcp add <name> --command npx --args -y @modelcontextprotocol/server-xxx",
        )
        .style(Style::default().fg(Color::DarkGray))
        .block(Block::default().borders(Borders::ALL).title(" MCP Servers "));
        frame.render_widget(text, area);
        return;
    }

    let items: Vec<ListItem> = app
        .mcp_state
        .servers
        .iter()
        .map(|s| {
            let status = if s.enabled { "\u{2713}" } else { "\u{2717}" };
            let content = format!(
                " [{status}] {:<16} {:<8} {}",
                s.name, s.transport, s.detail
            );
            let style = if s.enabled {
                Style::default().fg(Color::White)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            ListItem::new(content).style(style)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" MCP Servers "),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    frame.render_stateful_widget(list, area, &mut app.mcp_state.list_state);
}

// ── Placeholder Tab ─────────────────────────────────────────────────

fn draw_placeholder_tab(frame: &mut Frame, message: &str, area: Rect) {
    let text = Paragraph::new(message)
        .style(Style::default().fg(Color::DarkGray))
        .block(Block::default().borders(Borders::ALL));

    frame.render_widget(text, area);
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Create a centered rectangle for popup dialogs.
fn centered_rect(percent_x: u16, height: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length((area.height.saturating_sub(height)) / 2),
            Constraint::Length(height),
            Constraint::Min(0),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
