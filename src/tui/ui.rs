use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Tabs},
    Frame,
};

use super::app::{App, EditField, InputMode, SetupStepStatus, SkillsFocus, SkillSetupProgress, SkillsView, Tab, WhatsAppEditMode, WhatsAppField, WhatsAppStatus};

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
                .title(" Homun Settings "),
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
        Tab::Skills => draw_skills_tab(frame, app, area),
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
        Tab::Skills => {
            match &app.skills_state.focus {
                SkillsFocus::SearchBar => {
                    " Type to search | owner/repo to install | Enter Go | ↓ List | Esc Clear "
                }
                SkillsFocus::SetupInput => {
                    " Type value | Enter Save | Esc Cancel "
                }
                SkillsFocus::List => {
                    match app.skills_state.view {
                        SkillsView::Search => " ↑↓ Navigate | Enter Install | / Search | d Remove | r Refresh | 1 Installed | q Quit ",
                        SkillsView::Installed => " ↑↓ Navigate | / Search | d Remove | r Refresh | 2 Search | q Quit ",
                    }
                }
            }
        }
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
                "  Run 'homun gateway' to start receiving messages.",
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

// ── Skills Tab ──────────────────────────────────────────────────────

fn draw_skills_tab(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Skills ");

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Layout: search bar + view tabs + list + status bar
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Search bar (always visible)
            Constraint::Length(2), // View tabs (Installed / Search)
            Constraint::Min(0),   // Skill list
            Constraint::Length(2), // Status bar
        ])
        .margin(1)
        .split(inner);

    // ── Search bar (always visible at top) ──
    let search_focused = app.skills_state.focus == SkillsFocus::SearchBar;
    let buf = &app.skills_state.search_buffer;

    let search_text = if buf.is_empty() && !search_focused {
        "Type to search, or enter owner/repo to install directly"
    } else if buf.is_empty() {
        ""
    } else {
        buf
    };

    let search_display = if buf.is_empty() && !search_focused {
        Line::from(Span::styled(
            format!("  🔍 {search_text}"),
            Style::default().fg(Color::DarkGray),
        ))
    } else {
        let cursor = if search_focused { "\u{2588}" } else { "" };
        let loading_indicator = if app.skills_state.loading { " ⏳" } else { "" };
        Line::from(vec![
            Span::styled("  🔍 ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{search_text}{cursor}"),
                Style::default().fg(if search_focused { Color::White } else { Color::DarkGray }),
            ),
            Span::styled(loading_indicator, Style::default().fg(Color::Yellow)),
        ])
    };

    let search_border_style = if search_focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let search_bar = Paragraph::new(search_display).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(search_border_style),
    );
    frame.render_widget(search_bar, chunks[0]);

    // ── View tabs ──
    let installed_style = if app.skills_state.view == SkillsView::Installed {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let search_style = if app.skills_state.view == SkillsView::Search {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let n_installed = app.skills_state.installed.len();
    let n_results = app.skills_state.search_results.len();

    let view_tabs = Line::from(vec![
        Span::styled(format!(" [1] Installed ({n_installed})"), installed_style),
        Span::raw("  "),
        Span::styled(format!("[2] Results ({n_results})"), search_style),
    ]);
    frame.render_widget(Paragraph::new(view_tabs), chunks[1]);

    // ── Skill list ──
    let list_items = app.skills_state.current_list();
    let list_focused = app.skills_state.focus == SkillsFocus::List;

    if list_items.is_empty() {
        let empty_msg = match app.skills_state.view {
            SkillsView::Installed => "  No skills installed. Search above to find and install skills.",
            SkillsView::Search => "  No results yet. Type a query above and press Enter.",
        };
        let empty = Paragraph::new(empty_msg)
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(empty, chunks[2]);
    } else {
        let items: Vec<ListItem> = list_items
            .iter()
            .map(|s| {
                let source_tag = match s.source.as_str() {
                    "installed" => "",
                    "github" => " [gh]",
                    "clawhub" => " [claw]",
                    _ => "",
                };
                // Show stats for search results
                let stats = if s.source != "installed" && (s.downloads > 0 || s.stars > 0) {
                    let mut parts = Vec::new();
                    if s.stars > 0 { parts.push(format!("★{}", s.stars)); }
                    if s.downloads > 0 { parts.push(format!("↓{}", format_count(s.downloads))); }
                    format!(" {}", parts.join(" "))
                } else {
                    String::new()
                };
                let desc = if s.description.len() > 45 {
                    format!("{}...", &s.description[..42])
                } else {
                    s.description.clone()
                };
                let content = format!(
                    " {:<28}{}{}{}", s.name, source_tag, stats,
                    if desc.is_empty() { String::new() } else { format!(" — {desc}") }
                );
                ListItem::new(content)
            })
            .collect();

        let highlight_style = if list_focused {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let list = List::new(items)
            .highlight_style(highlight_style)
            .highlight_symbol(if list_focused { "> " } else { "  " });

        frame.render_stateful_widget(list, chunks[2], &mut app.skills_state.list_state);
    }

    // ── Status bar ──
    let status_line = if !app.skills_state.status_message.is_empty() {
        app.skills_state.status_message.clone()
    } else {
        format!("{n_installed} skill(s) installed")
    };
    let status_para = Paragraph::new(format!("  {status_line}"))
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(status_para, chunks[3]);

    // ── Auto-setup progress popup (drawn on top) ──
    if let Some(progress) = &app.skills_state.setup_progress {
        draw_setup_progress(frame, progress, &app.skills_state, area);
    }
}

/// Draw the live auto-setup progress popup with inline env var input.
///
/// Shows each step with a status indicator:
/// · Pending, ⚙ Running, ✓ Done/Skipped, ✗ Failed, ! Manual
fn draw_setup_progress(
    frame: &mut Frame,
    progress: &SkillSetupProgress,
    skills_state: &super::app::SkillsState,
    area: Rect,
) {
    let mut lines: Vec<Line> = Vec::new();
    let is_inputting = skills_state.focus == SkillsFocus::SetupInput;

    // Title
    let title_text = if progress.finished {
        let all_ok = progress.steps.iter().all(|s| {
            matches!(
                s.status,
                SetupStepStatus::Done | SetupStepStatus::Skipped
            )
        });
        if all_ok {
            format!(" ✅ '{}' ready to use!", progress.skill_name)
        } else {
            format!(" ⚠ '{}' needs attention", progress.skill_name)
        }
    } else {
        format!(" ⚙ Setting up '{}'...", progress.skill_name)
    };

    let title_color = if progress.finished {
        let all_ok = progress.steps.iter().all(|s| {
            matches!(s.status, SetupStepStatus::Done | SetupStepStatus::Skipped)
        });
        if all_ok { Color::Green } else { Color::Yellow }
    } else {
        Color::Yellow
    };

    lines.push(Line::from(Span::styled(
        title_text,
        Style::default()
            .fg(title_color)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    // Each step
    for (i, step) in progress.steps.iter().enumerate() {
        let (icon, icon_color) = match &step.status {
            SetupStepStatus::Pending => ("  ·", Color::DarkGray),
            SetupStepStatus::Running => ("  ⚙", Color::Yellow),
            SetupStepStatus::Done => ("  ✓", Color::Green),
            SetupStepStatus::Skipped => ("  ✓", Color::DarkGray),
            SetupStepStatus::Failed(_) => ("  ✗", Color::Red),
            SetupStepStatus::Manual => ("  !", Color::Magenta),
        };

        let label_color = match &step.status {
            SetupStepStatus::Skipped => Color::DarkGray,
            SetupStepStatus::Failed(_) => Color::Red,
            SetupStepStatus::Manual => Color::Magenta,
            _ => Color::White,
        };

        lines.push(Line::from(vec![
            Span::styled(icon, Style::default().fg(icon_color)),
            Span::raw(" "),
            Span::styled(&step.label, Style::default().fg(label_color)),
        ]));

        // Show detail for non-trivial states
        match &step.status {
            SetupStepStatus::Running => {
                lines.push(Line::from(Span::styled(
                    format!("      $ {}", step.detail),
                    Style::default().fg(Color::Cyan),
                )));
            }
            SetupStepStatus::Done => {
                lines.push(Line::from(Span::styled(
                    format!("      {}", step.detail),
                    Style::default().fg(Color::DarkGray),
                )));
            }
            SetupStepStatus::Skipped => {
                lines.push(Line::from(Span::styled(
                    format!("      {}", step.detail),
                    Style::default().fg(Color::DarkGray),
                )));
            }
            SetupStepStatus::Failed(err) => {
                lines.push(Line::from(Span::styled(
                    format!("      {err}"),
                    Style::default().fg(Color::Red),
                )));
            }
            SetupStepStatus::Manual => {
                // If this is the step currently being edited, show input field
                if is_inputting && skills_state.setup_input_step_idx == Some(i) {
                    let var_name = step.detail
                        .strip_prefix("export ")
                        .and_then(|s| s.split('=').next())
                        .unwrap_or(&step.detail);
                    let buf = &skills_state.setup_input_buffer;
                    lines.push(Line::from(vec![
                        Span::styled("      ", Style::default()),
                        Span::styled(
                            format!("{var_name}="),
                            Style::default().fg(Color::Cyan),
                        ),
                        Span::styled(
                            format!("{buf}\u{2588}"),
                            Style::default().fg(Color::White),
                        ),
                    ]));
                } else {
                    lines.push(Line::from(Span::styled(
                        format!("      {}", step.detail),
                        Style::default().fg(Color::Magenta),
                    )));
                }
            }
            _ => {}
        }
    }

    // Footer
    lines.push(Line::from(""));
    if is_inputting {
        lines.push(Line::from(Span::styled(
            " Enter to save | Esc to skip",
            Style::default().fg(Color::DarkGray),
        )));
    } else if progress.finished {
        let has_manual = progress
            .steps
            .iter()
            .any(|s| matches!(s.status, SetupStepStatus::Manual));
        if has_manual {
            lines.push(Line::from(Span::styled(
                " Enter to configure missing items | any other key to dismiss",
                Style::default().fg(Color::Magenta),
            )));
        } else {
            lines.push(Line::from(Span::styled(
                " Press any key to dismiss",
                Style::default().fg(Color::DarkGray),
            )));
        }
    } else {
        lines.push(Line::from(Span::styled(
            " Setting up... (Esc to dismiss)",
            Style::default().fg(Color::DarkGray),
        )));
    }

    let height = (lines.len() + 3).min(22) as u16;
    let popup_area = centered_rect(65, height, area);
    frame.render_widget(Clear, popup_area);

    let popup = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Auto-Setup ")
            .title_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .style(Style::default().fg(Color::White)),
    );
    frame.render_widget(popup, popup_area);
}

// ── MCP Tab ─────────────────────────────────────────────────────────

fn draw_mcp_tab(frame: &mut Frame, app: &mut App, area: Rect) {
    if app.mcp_state.servers.is_empty() {
        let text = Paragraph::new(
            " No MCP servers configured.\n\n Add one with: homun mcp add <name> --command npx --args -y @modelcontextprotocol/server-xxx",
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

// ── Helpers ─────────────────────────────────────────────────────────

/// Format a large number for compact display (e.g., 25623 → "25.6k")
fn format_count(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

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
