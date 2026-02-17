use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::config::{self, Config, ProvidersConfig};
use crate::config::dotpath;

use super::event::Event;

/// Active tab in the TUI dashboard
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Settings,
    Providers,
    WhatsApp,
    Skills,
    Mcp,
}

impl Tab {
    pub const ALL: [Tab; 5] = [Tab::Settings, Tab::Providers, Tab::WhatsApp, Tab::Skills, Tab::Mcp];

    pub fn title(&self) -> &'static str {
        match self {
            Tab::Settings => "Settings",
            Tab::Providers => "Providers",
            Tab::WhatsApp => "WhatsApp",
            Tab::Skills => "Skills",
            Tab::Mcp => "MCP",
        }
    }

    pub fn index(&self) -> usize {
        match self {
            Tab::Settings => 0,
            Tab::Providers => 1,
            Tab::WhatsApp => 2,
            Tab::Skills => 3,
            Tab::Mcp => 4,
        }
    }

    pub fn next(&self) -> Tab {
        Tab::ALL[(self.index() + 1) % Tab::ALL.len()]
    }

    pub fn prev(&self) -> Tab {
        let idx = if self.index() == 0 {
            Tab::ALL.len() - 1
        } else {
            self.index() - 1
        };
        Tab::ALL[idx]
    }
}

/// Input mode — whether the user is in normal navigation or editing a field
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Editing,
}

/// State for the Settings tab
pub struct SettingsState {
    pub list_state: ListState,
    pub entries: Vec<(String, String)>, // (dot-path key, display value)
    pub input_mode: InputMode,
    pub edit_buffer: String,
    pub edit_key: String,
}

impl SettingsState {
    pub fn new(config: &Config) -> Self {
        let entries = dotpath::config_list_keys(config);
        let mut list_state = ListState::default();
        if !entries.is_empty() {
            list_state.select(Some(0));
        }
        Self {
            list_state,
            entries,
            input_mode: InputMode::Normal,
            edit_buffer: String::new(),
            edit_key: String::new(),
        }
    }

    pub fn refresh(&mut self, config: &Config) {
        self.entries = dotpath::config_list_keys(config);
    }

    pub fn selected_index(&self) -> Option<usize> {
        self.list_state.selected()
    }

    pub fn move_up(&mut self) {
        if let Some(i) = self.list_state.selected() {
            if i > 0 {
                self.list_state.select(Some(i - 1));
            }
        }
    }

    pub fn move_down(&mut self) {
        if let Some(i) = self.list_state.selected() {
            if i < self.entries.len().saturating_sub(1) {
                self.list_state.select(Some(i + 1));
            }
        }
    }

    /// Start editing the selected entry
    pub fn start_edit(&mut self) {
        if let Some(i) = self.list_state.selected() {
            if let Some((key, value)) = self.entries.get(i) {
                self.edit_key = key.clone();
                // Don't pre-fill masked values
                self.edit_buffer = if value.contains("***") {
                    String::new()
                } else {
                    value.clone()
                };
                self.input_mode = InputMode::Editing;
            }
        }
    }

    pub fn cancel_edit(&mut self) {
        self.input_mode = InputMode::Normal;
        self.edit_buffer.clear();
        self.edit_key.clear();
    }
}

/// State for the Providers tab
pub struct ProvidersState {
    pub list_state: ListState,
    pub providers: Vec<ProviderInfo>,
    pub input_mode: InputMode,
    pub edit_field: EditField,
    pub edit_buffer: String,
    pub editing_provider: String,
}

pub struct ProviderInfo {
    pub name: String,
    pub configured: bool,
    pub api_key_masked: String,
    pub api_base: String,
    pub is_active: bool,
}

/// Which field is being edited in the provider popup
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditField {
    ApiKey,
    ApiBase,
}

impl ProvidersState {
    pub fn new(config: &Config) -> Self {
        let providers = Self::build_provider_list(config);
        let mut list_state = ListState::default();
        if !providers.is_empty() {
            list_state.select(Some(0));
        }
        Self {
            list_state,
            providers,
            input_mode: InputMode::Normal,
            edit_field: EditField::ApiKey,
            edit_buffer: String::new(),
            editing_provider: String::new(),
        }
    }

    pub fn refresh(&mut self, config: &Config) {
        self.providers = Self::build_provider_list(config);
    }

    fn build_provider_list(config: &Config) -> Vec<ProviderInfo> {
        let active_provider = config
            .resolve_provider(&config.agent.model)
            .map(|(name, _)| name.to_string());

        config
            .providers
            .iter()
            .map(|(name, pc)| {
                let configured = !pc.api_key.is_empty() || pc.api_base.is_some();
                let api_key_masked = if pc.api_key.is_empty() {
                    "—".to_string()
                } else if pc.api_key.len() > 6 {
                    format!("{}***", &pc.api_key[..6])
                } else {
                    "***".to_string()
                };
                let api_base = pc
                    .api_base
                    .as_deref()
                    .unwrap_or("(default)")
                    .to_string();
                let is_active = active_provider.as_deref() == Some(name);

                ProviderInfo {
                    name: name.to_string(),
                    configured,
                    api_key_masked,
                    api_base,
                    is_active,
                }
            })
            .collect()
    }

    pub fn move_up(&mut self) {
        if let Some(i) = self.list_state.selected() {
            if i > 0 {
                self.list_state.select(Some(i - 1));
            }
        }
    }

    pub fn move_down(&mut self) {
        if let Some(i) = self.list_state.selected() {
            if i < self.providers.len().saturating_sub(1) {
                self.list_state.select(Some(i + 1));
            }
        }
    }

    pub fn start_edit(&mut self) {
        if let Some(i) = self.list_state.selected() {
            if let Some(info) = self.providers.get(i) {
                self.editing_provider = info.name.clone();
                self.edit_field = EditField::ApiKey;
                self.edit_buffer.clear();
                self.input_mode = InputMode::Editing;
            }
        }
    }

    pub fn cancel_edit(&mut self) {
        self.input_mode = InputMode::Normal;
        self.edit_buffer.clear();
        self.editing_provider.clear();
    }
}

/// State for the Skills tab (placeholder, populated in Phase 3)
pub struct SkillsState {
    pub list_state: ListState,
    pub message: String,
}

impl SkillsState {
    pub fn new() -> Self {
        Self {
            list_state: ListState::default(),
            message: "Skills tab — coming soon. Use CLI: homunbot skills list".to_string(),
        }
    }
}

/// Info about an MCP server for display in the TUI
pub struct McpServerEntry {
    pub name: String,
    pub transport: String,
    pub detail: String,
    pub enabled: bool,
}

/// State for the MCP tab
pub struct McpState {
    pub list_state: ListState,
    pub servers: Vec<McpServerEntry>,
}

impl McpState {
    pub fn new(config: &Config) -> Self {
        let servers = Self::build_server_list(config);
        let mut list_state = ListState::default();
        if !servers.is_empty() {
            list_state.select(Some(0));
        }
        Self { list_state, servers }
    }

    pub fn refresh(&mut self, config: &Config) {
        self.servers = Self::build_server_list(config);
    }

    fn build_server_list(config: &Config) -> Vec<McpServerEntry> {
        config
            .mcp
            .servers
            .iter()
            .map(|(name, server)| {
                let detail = match server.transport.as_str() {
                    "stdio" => {
                        let cmd = server.command.as_deref().unwrap_or("?");
                        let args = server.args.join(" ");
                        format!("{cmd} {args}")
                    }
                    "http" => server.url.as_deref().unwrap_or("?").to_string(),
                    _ => server.transport.clone(),
                };
                McpServerEntry {
                    name: name.clone(),
                    transport: server.transport.clone(),
                    detail,
                    enabled: server.enabled,
                }
            })
            .collect()
    }

    pub fn move_up(&mut self) {
        if let Some(i) = self.list_state.selected() {
            if i > 0 {
                self.list_state.select(Some(i - 1));
            }
        }
    }

    pub fn move_down(&mut self) {
        if let Some(i) = self.list_state.selected() {
            if i < self.servers.len().saturating_sub(1) {
                self.list_state.select(Some(i + 1));
            }
        }
    }
}

/// WhatsApp pairing status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WhatsAppStatus {
    /// Not configured (no phone number)
    NotConfigured,
    /// Phone number entered, ready to pair
    ReadyToPair,
    /// Pairing in progress, waiting for code
    Connecting,
    /// Pairing code received, waiting for user to enter on phone
    WaitingForCode { code: String, timeout_secs: u64 },
    /// Pairing succeeded
    Paired,
    /// Connected and ready
    Connected,
    /// Error occurred
    Error(String),
}

/// Which field is focused in the WhatsApp tab
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhatsAppField {
    Phone,
    AllowFrom,
}

/// What kind of editing is happening in the WhatsApp tab
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WhatsAppEditMode {
    /// Normal navigation
    Normal,
    /// Editing the phone number
    EditingPhone,
    /// Adding a new allow_from number
    AddingNumber,
}

/// State for the WhatsApp tab
pub struct WhatsAppState {
    pub status: WhatsAppStatus,
    pub phone_input: String,
    pub input_mode: WhatsAppEditMode,
    /// Which field is currently focused
    pub focused_field: WhatsAppField,
    /// Allowed sender numbers
    pub allow_from: Vec<String>,
    /// Selected index in the allow_from list
    pub allow_from_selected: Option<usize>,
    /// Buffer for adding a new number
    pub add_number_buffer: String,
    /// Handle to the running pairing task (so we can abort on exit)
    pub pairing_task: Option<JoinHandle<()>>,
}

impl WhatsAppState {
    pub fn new(config: &Config) -> Self {
        let wa = &config.channels.whatsapp;
        let status = if wa.phone_number.is_empty() {
            WhatsAppStatus::NotConfigured
        } else {
            WhatsAppStatus::ReadyToPair
        };
        let allow_from = wa.allow_from.clone();
        let allow_from_selected = if allow_from.is_empty() { None } else { Some(0) };
        Self {
            status,
            phone_input: wa.phone_number.clone(),
            input_mode: WhatsAppEditMode::Normal,
            focused_field: WhatsAppField::Phone,
            allow_from,
            allow_from_selected,
            add_number_buffer: String::new(),
            pairing_task: None,
        }
    }

    /// Abort any running pairing task
    pub fn abort_pairing(&mut self) {
        if let Some(handle) = self.pairing_task.take() {
            handle.abort();
        }
        // Reset status if was in pairing flow
        if matches!(
            self.status,
            WhatsAppStatus::Connecting | WhatsAppStatus::WaitingForCode { .. }
        ) {
            if self.phone_input.is_empty() {
                self.status = WhatsAppStatus::NotConfigured;
            } else {
                self.status = WhatsAppStatus::ReadyToPair;
            }
        }
    }
}

/// Main TUI application state
pub struct App {
    pub current_tab: Tab,
    pub should_quit: bool,
    pub config: Config,
    pub config_modified: bool,

    pub settings_state: SettingsState,
    pub providers_state: ProvidersState,
    pub whatsapp_state: WhatsAppState,
    pub skills_state: SkillsState,
    pub mcp_state: McpState,

    /// Event sender for injecting async events (WhatsApp pairing, etc.)
    pub event_tx: Option<mpsc::UnboundedSender<Event>>,
}

impl App {
    pub fn new(config: Config) -> Self {
        let settings_state = SettingsState::new(&config);
        let providers_state = ProvidersState::new(&config);
        let whatsapp_state = WhatsAppState::new(&config);
        let mcp_state = McpState::new(&config);
        Self {
            current_tab: Tab::Settings,
            should_quit: false,
            config,
            config_modified: false,
            settings_state,
            providers_state,
            whatsapp_state,
            skills_state: SkillsState::new(),
            mcp_state,
            event_tx: None,
        }
    }

    /// Set the event sender (called from TUI main loop after creating EventHandler)
    pub fn set_event_tx(&mut self, tx: mpsc::UnboundedSender<Event>) {
        self.event_tx = Some(tx);
    }

    /// Handle an incoming event
    pub fn handle_event(&mut self, event: Event) {
        match event {
            Event::Key(key) => self.handle_key(key),
            Event::Tick => {} // Nothing to do on tick for now
            Event::WhatsAppPairingCode { code, timeout_secs } => {
                self.whatsapp_state.status = WhatsAppStatus::WaitingForCode { code, timeout_secs };
            }
            Event::WhatsAppPairSuccess => {
                self.whatsapp_state.status = WhatsAppStatus::Paired;
            }
            Event::WhatsAppPairError(err) => {
                self.whatsapp_state.status = WhatsAppStatus::Error(err);
            }
            Event::WhatsAppConnected => {
                self.whatsapp_state.status = WhatsAppStatus::Connected;
            }
            Event::WhatsAppLoggedOut => {
                self.whatsapp_state.status = WhatsAppStatus::Error("Logged out".to_string());
            }
            Event::WhatsAppQrCode { .. } => {
                // QR codes are shown as text fallback; pair code is preferred
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent) {
        // Global: Ctrl+C always quits
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }

        match self.current_tab {
            Tab::Settings => self.handle_settings_key(key),
            Tab::Providers => self.handle_providers_key(key),
            Tab::WhatsApp => self.handle_whatsapp_key(key),
            Tab::Skills => self.handle_placeholder_key(key),
            Tab::Mcp => self.handle_mcp_key(key),
        }
    }

    /// Handle keys in Settings tab
    fn handle_settings_key(&mut self, key: KeyEvent) {
        match self.settings_state.input_mode {
            InputMode::Normal => match key.code {
                KeyCode::Char('q') => self.should_quit = true,
                KeyCode::Tab => self.current_tab = self.current_tab.next(),
                KeyCode::BackTab => self.current_tab = self.current_tab.prev(),
                KeyCode::Up | KeyCode::Char('k') => self.settings_state.move_up(),
                KeyCode::Down | KeyCode::Char('j') => self.settings_state.move_down(),
                KeyCode::Enter => self.settings_state.start_edit(),
                _ => {}
            },
            InputMode::Editing => match key.code {
                KeyCode::Esc => self.settings_state.cancel_edit(),
                KeyCode::Enter => self.apply_settings_edit(),
                KeyCode::Backspace => {
                    self.settings_state.edit_buffer.pop();
                }
                KeyCode::Char(c) => {
                    self.settings_state.edit_buffer.push(c);
                }
                _ => {}
            },
        }
    }

    /// Apply the current settings edit
    fn apply_settings_edit(&mut self) {
        let key = self.settings_state.edit_key.clone();
        let value = self.settings_state.edit_buffer.clone();

        if !value.is_empty() {
            if dotpath::config_set(&mut self.config, &key, &value).is_ok() {
                self.config_modified = true;
                self.settings_state.refresh(&self.config);
                self.providers_state.refresh(&self.config);
            }
        }

        self.settings_state.cancel_edit();
    }

    /// Handle keys in Providers tab
    fn handle_providers_key(&mut self, key: KeyEvent) {
        match self.providers_state.input_mode {
            InputMode::Normal => match key.code {
                KeyCode::Char('q') => self.should_quit = true,
                KeyCode::Tab => self.current_tab = self.current_tab.next(),
                KeyCode::BackTab => self.current_tab = self.current_tab.prev(),
                KeyCode::Up | KeyCode::Char('k') => self.providers_state.move_up(),
                KeyCode::Down | KeyCode::Char('j') => self.providers_state.move_down(),
                KeyCode::Enter => self.providers_state.start_edit(),
                KeyCode::Char('d') => self.remove_selected_provider(),
                _ => {}
            },
            InputMode::Editing => match key.code {
                KeyCode::Esc => self.providers_state.cancel_edit(),
                KeyCode::Enter => {
                    if self.providers_state.edit_field == EditField::ApiKey {
                        // Move to api_base field
                        self.apply_provider_api_key();
                        self.providers_state.edit_field = EditField::ApiBase;
                        self.providers_state.edit_buffer.clear();
                    } else {
                        // Apply api_base and finish
                        self.apply_provider_api_base();
                        self.providers_state.cancel_edit();
                    }
                }
                KeyCode::Backspace => {
                    self.providers_state.edit_buffer.pop();
                }
                KeyCode::Char(c) => {
                    self.providers_state.edit_buffer.push(c);
                }
                _ => {}
            },
        }
    }

    fn apply_provider_api_key(&mut self) {
        let name = self.providers_state.editing_provider.clone();
        let value = self.providers_state.edit_buffer.clone();
        if !value.is_empty() {
            if let Some(pc) = self.config.providers.get_mut(&name) {
                pc.api_key = value;
                self.config_modified = true;
            }
        }
    }

    fn apply_provider_api_base(&mut self) {
        let name = self.providers_state.editing_provider.clone();
        let value = self.providers_state.edit_buffer.clone();
        if !value.is_empty() {
            if let Some(pc) = self.config.providers.get_mut(&name) {
                pc.api_base = Some(value);
                self.config_modified = true;
            }
        }
        self.providers_state.refresh(&self.config);
    }

    fn remove_selected_provider(&mut self) {
        if let Some(i) = self.providers_state.list_state.selected() {
            if let Some(info) = self.providers_state.providers.get(i) {
                let name = info.name.clone();
                if let Some(pc) = self.config.providers.get_mut(&name) {
                    pc.api_key.clear();
                    pc.api_base = None;
                    pc.extra_headers.clear();
                    self.config_modified = true;
                    self.providers_state.refresh(&self.config);
                    self.settings_state.refresh(&self.config);
                }
            }
        }
    }

    /// Handle keys in WhatsApp tab
    fn handle_whatsapp_key(&mut self, key: KeyEvent) {
        match &self.whatsapp_state.input_mode {
            WhatsAppEditMode::Normal => self.handle_whatsapp_normal_key(key),
            WhatsAppEditMode::EditingPhone => self.handle_whatsapp_edit_phone_key(key),
            WhatsAppEditMode::AddingNumber => self.handle_whatsapp_add_number_key(key),
        }
    }

    fn handle_whatsapp_normal_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Tab => self.current_tab = self.current_tab.next(),
            KeyCode::BackTab => self.current_tab = self.current_tab.prev(),
            // Up/Down: switch focused field
            KeyCode::Up | KeyCode::Char('k') => {
                match self.whatsapp_state.focused_field {
                    WhatsAppField::AllowFrom => {
                        // Move selection up in list, or jump to Phone field
                        let sel = self.whatsapp_state.allow_from_selected.unwrap_or(0);
                        if sel > 0 {
                            self.whatsapp_state.allow_from_selected = Some(sel - 1);
                        } else {
                            self.whatsapp_state.focused_field = WhatsAppField::Phone;
                        }
                    }
                    WhatsAppField::Phone => {} // Already at top
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                match self.whatsapp_state.focused_field {
                    WhatsAppField::Phone => {
                        // Jump to AllowFrom field
                        self.whatsapp_state.focused_field = WhatsAppField::AllowFrom;
                        if !self.whatsapp_state.allow_from.is_empty()
                            && self.whatsapp_state.allow_from_selected.is_none()
                        {
                            self.whatsapp_state.allow_from_selected = Some(0);
                        }
                    }
                    WhatsAppField::AllowFrom => {
                        // Move selection down in list
                        if let Some(sel) = self.whatsapp_state.allow_from_selected {
                            if sel < self.whatsapp_state.allow_from.len().saturating_sub(1) {
                                self.whatsapp_state.allow_from_selected = Some(sel + 1);
                            }
                        }
                    }
                }
            }
            KeyCode::Char('e') | KeyCode::Enter => {
                // Edit phone number (only when Phone is focused and not pairing)
                if self.whatsapp_state.focused_field == WhatsAppField::Phone
                    && !matches!(
                        self.whatsapp_state.status,
                        WhatsAppStatus::Connecting | WhatsAppStatus::WaitingForCode { .. }
                    )
                {
                    self.whatsapp_state.input_mode = WhatsAppEditMode::EditingPhone;
                }
            }
            KeyCode::Char('a') => {
                // Add a new allow_from number
                self.whatsapp_state.add_number_buffer.clear();
                self.whatsapp_state.input_mode = WhatsAppEditMode::AddingNumber;
            }
            KeyCode::Char('d') => {
                // Delete selected allow_from number
                if self.whatsapp_state.focused_field == WhatsAppField::AllowFrom {
                    self.remove_selected_allow_from();
                }
            }
            KeyCode::Char('p') => {
                self.start_whatsapp_pairing();
            }
            KeyCode::Char('x') => {
                self.whatsapp_state.abort_pairing();
            }
            _ => {}
        }
    }

    fn handle_whatsapp_edit_phone_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                // Cancel editing, restore original number
                self.whatsapp_state.phone_input =
                    self.config.channels.whatsapp.phone_number.clone();
                self.whatsapp_state.input_mode = WhatsAppEditMode::Normal;
            }
            KeyCode::Enter => {
                self.apply_whatsapp_phone();
            }
            KeyCode::Backspace => {
                self.whatsapp_state.phone_input.pop();
            }
            KeyCode::Char(c) if c.is_ascii_digit() || c == '+' => {
                self.whatsapp_state.phone_input.push(c);
            }
            _ => {}
        }
    }

    fn handle_whatsapp_add_number_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.whatsapp_state.add_number_buffer.clear();
                self.whatsapp_state.input_mode = WhatsAppEditMode::Normal;
            }
            KeyCode::Enter => {
                self.apply_whatsapp_add_number();
            }
            KeyCode::Backspace => {
                self.whatsapp_state.add_number_buffer.pop();
            }
            KeyCode::Char(c) if c.is_ascii_digit() || c == '+' => {
                self.whatsapp_state.add_number_buffer.push(c);
            }
            _ => {}
        }
    }

    /// Apply phone number edit — save to config
    fn apply_whatsapp_phone(&mut self) {
        let phone = self.whatsapp_state.phone_input.clone();
        self.config.channels.whatsapp.phone_number = phone.clone();
        self.config.channels.whatsapp.enabled = !phone.is_empty();
        self.config_modified = true;

        self.whatsapp_state.status = if phone.is_empty() {
            WhatsAppStatus::NotConfigured
        } else {
            WhatsAppStatus::ReadyToPair
        };
        self.whatsapp_state.input_mode = WhatsAppEditMode::Normal;
        self.settings_state.refresh(&self.config);
    }

    /// Add a number to the allow_from list
    fn apply_whatsapp_add_number(&mut self) {
        let number = self.whatsapp_state.add_number_buffer.trim().to_string();
        // Strip leading '+' for consistency (config stores without +)
        let number = number.strip_prefix('+').unwrap_or(&number).to_string();
        if !number.is_empty() && !self.whatsapp_state.allow_from.contains(&number) {
            self.whatsapp_state.allow_from.push(number);
            self.whatsapp_state.allow_from_selected =
                Some(self.whatsapp_state.allow_from.len() - 1);
            // Save to config
            self.config.channels.whatsapp.allow_from = self.whatsapp_state.allow_from.clone();
            self.config_modified = true;
            self.settings_state.refresh(&self.config);
        }
        self.whatsapp_state.add_number_buffer.clear();
        self.whatsapp_state.input_mode = WhatsAppEditMode::Normal;
        self.whatsapp_state.focused_field = WhatsAppField::AllowFrom;
    }

    /// Remove the selected number from the allow_from list
    fn remove_selected_allow_from(&mut self) {
        if let Some(idx) = self.whatsapp_state.allow_from_selected {
            if idx < self.whatsapp_state.allow_from.len() {
                self.whatsapp_state.allow_from.remove(idx);
                // Update selection
                if self.whatsapp_state.allow_from.is_empty() {
                    self.whatsapp_state.allow_from_selected = None;
                } else if idx >= self.whatsapp_state.allow_from.len() {
                    self.whatsapp_state.allow_from_selected =
                        Some(self.whatsapp_state.allow_from.len() - 1);
                }
                // Save to config
                self.config.channels.whatsapp.allow_from = self.whatsapp_state.allow_from.clone();
                self.config_modified = true;
                self.settings_state.refresh(&self.config);
            }
        }
    }

    /// Start the WhatsApp pairing process in a background task
    fn start_whatsapp_pairing(&mut self) {
        // Need a phone number
        if self.whatsapp_state.phone_input.is_empty() {
            self.whatsapp_state.status = WhatsAppStatus::Error("Enter a phone number first".to_string());
            return;
        }

        // Abort any existing pairing
        self.whatsapp_state.abort_pairing();

        // Need event_tx to send pairing events back to the TUI
        let event_tx = match &self.event_tx {
            Some(tx) => tx.clone(),
            None => {
                self.whatsapp_state.status = WhatsAppStatus::Error("Internal error: no event sender".to_string());
                return;
            }
        };

        self.whatsapp_state.status = WhatsAppStatus::Connecting;

        let phone = self.whatsapp_state.phone_input.clone();
        let db_path = self.config.channels.whatsapp.resolved_db_path();

        let handle = tokio::spawn(async move {
            if let Err(e) = run_whatsapp_pairing(phone, db_path, event_tx.clone()).await {
                let _ = event_tx.send(Event::WhatsAppPairError(e.to_string()));
            }
        });

        self.whatsapp_state.pairing_task = Some(handle);
    }

    /// Handle keys in MCP tab
    fn handle_mcp_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Tab => self.current_tab = self.current_tab.next(),
            KeyCode::BackTab => self.current_tab = self.current_tab.prev(),
            KeyCode::Up | KeyCode::Char('k') => self.mcp_state.move_up(),
            KeyCode::Down | KeyCode::Char('j') => self.mcp_state.move_down(),
            KeyCode::Char(' ') => self.toggle_selected_mcp_server(),
            KeyCode::Char('d') => self.remove_selected_mcp_server(),
            _ => {}
        }
    }

    fn toggle_selected_mcp_server(&mut self) {
        if let Some(i) = self.mcp_state.list_state.selected() {
            if let Some(entry) = self.mcp_state.servers.get(i) {
                let name = entry.name.clone();
                if let Some(server) = self.config.mcp.servers.get_mut(&name) {
                    server.enabled = !server.enabled;
                    self.config_modified = true;
                    self.mcp_state.refresh(&self.config);
                    self.settings_state.refresh(&self.config);
                }
            }
        }
    }

    fn remove_selected_mcp_server(&mut self) {
        if let Some(i) = self.mcp_state.list_state.selected() {
            if let Some(entry) = self.mcp_state.servers.get(i) {
                let name = entry.name.clone();
                self.config.mcp.servers.remove(&name);
                self.config_modified = true;
                self.mcp_state.refresh(&self.config);
                self.settings_state.refresh(&self.config);
            }
        }
    }

    /// Handle keys for placeholder tabs (Skills)
    fn handle_placeholder_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Tab => self.current_tab = self.current_tab.next(),
            KeyCode::BackTab => self.current_tab = self.current_tab.prev(),
            _ => {}
        }
    }
}

/// Run the WhatsApp pairing process in the background.
///
/// This connects to WhatsApp, requests a pairing code, and sends
/// status events back to the TUI via the event channel.
async fn run_whatsapp_pairing(
    phone: String,
    db_path: std::path::PathBuf,
    event_tx: mpsc::UnboundedSender<Event>,
) -> anyhow::Result<()> {
    use whatsapp_rust::bot::Bot;
    use whatsapp_rust::store::SqliteStore;
    use whatsapp_rust_tokio_transport::TokioWebSocketTransportFactory;
    use whatsapp_rust_ureq_http_client::UreqHttpClient;
    use wacore::types::events::Event as WaEvent;
    use waproto::whatsapp as wa;

    // Ensure parent directory exists
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let backend = Arc::new(
        SqliteStore::new(&db_path.to_string_lossy())
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create WhatsApp store: {e}"))?,
    );

    let transport_factory = TokioWebSocketTransportFactory::new();
    let http_client = UreqHttpClient::new();

    let tx = event_tx.clone();
    let mut bot = Bot::builder()
        .with_backend(backend)
        .with_transport_factory(transport_factory)
        .with_http_client(http_client)
        .with_device_props(
            Some("Linux".to_string()),
            None,
            Some(wa::device_props::PlatformType::Chrome),
        )
        .with_pair_code(whatsapp_rust::pair_code::PairCodeOptions {
            phone_number: phone,
            ..Default::default()
        })
        .skip_history_sync()
        .on_event(move |event, _client| {
            let tx = tx.clone();
            async move {
                match event {
                    WaEvent::PairingCode { code, timeout } => {
                        let _ = tx.send(Event::WhatsAppPairingCode {
                            code,
                            timeout_secs: timeout.as_secs(),
                        });
                    }
                    WaEvent::PairSuccess(_) => {
                        let _ = tx.send(Event::WhatsAppPairSuccess);
                    }
                    WaEvent::PairError(err) => {
                        let _ = tx.send(Event::WhatsAppPairError(err.error.clone()));
                    }
                    WaEvent::Connected(_) => {
                        let _ = tx.send(Event::WhatsAppConnected);
                    }
                    WaEvent::LoggedOut(_) => {
                        let _ = tx.send(Event::WhatsAppLoggedOut);
                    }
                    WaEvent::PairingQrCode { code, .. } => {
                        let _ = tx.send(Event::WhatsAppQrCode { data: code });
                    }
                    _ => {}
                }
            }
        })
        .build()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to build WhatsApp bot: {e}"))?;

    // Run the bot (blocks until disconnected)
    let handle = bot
        .run()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to start WhatsApp bot: {e}"))?;

    // Wait for the bot to finish (or be aborted)
    let _ = handle.await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tab_navigation() {
        assert_eq!(Tab::Settings.next(), Tab::Providers);
        assert_eq!(Tab::Providers.next(), Tab::WhatsApp);
        assert_eq!(Tab::WhatsApp.next(), Tab::Skills);
        assert_eq!(Tab::Skills.next(), Tab::Mcp);
        assert_eq!(Tab::Mcp.next(), Tab::Settings);
        assert_eq!(Tab::Settings.prev(), Tab::Mcp);
    }

    #[test]
    fn test_app_creation() {
        let config = Config::default();
        let app = App::new(config);
        assert_eq!(app.current_tab, Tab::Settings);
        assert!(!app.should_quit);
        assert!(!app.config_modified);
        assert!(!app.settings_state.entries.is_empty());
    }

    #[test]
    fn test_settings_navigation() {
        let config = Config::default();
        let mut app = App::new(config);
        assert_eq!(app.settings_state.selected_index(), Some(0));

        app.settings_state.move_down();
        assert_eq!(app.settings_state.selected_index(), Some(1));

        app.settings_state.move_up();
        assert_eq!(app.settings_state.selected_index(), Some(0));

        // Can't go above 0
        app.settings_state.move_up();
        assert_eq!(app.settings_state.selected_index(), Some(0));
    }

    #[test]
    fn test_settings_edit_flow() {
        let config = Config::default();
        let mut app = App::new(config);

        // Start editing
        app.settings_state.start_edit();
        assert_eq!(app.settings_state.input_mode, InputMode::Editing);

        // Cancel
        app.settings_state.cancel_edit();
        assert_eq!(app.settings_state.input_mode, InputMode::Normal);
    }

    #[test]
    fn test_providers_state() {
        let mut config = Config::default();
        config.providers.anthropic.api_key = "sk-ant-test-123456789".to_string();

        let state = ProvidersState::new(&config);
        let anthropic = state.providers.iter().find(|p| p.name == "anthropic").unwrap();
        assert!(anthropic.configured);
        assert!(anthropic.api_key_masked.starts_with("sk-ant"));
    }
}
