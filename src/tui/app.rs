use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::config::dotpath;
use crate::config::Config;

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
    pub const ALL: [Tab; 5] = [
        Tab::Settings,
        Tab::Providers,
        Tab::WhatsApp,
        Tab::Skills,
        Tab::Mcp,
    ];

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
                let api_base = pc.api_base.as_deref().unwrap_or("(default)").to_string();
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

/// View mode for Skills tab
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillsView {
    Installed,
    Search,
}

/// Focus area within the Skills tab
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillsFocus {
    /// Cursor is in the search bar (typing a query)
    SearchBar,
    /// Cursor is in the results list (navigating)
    List,
    /// Typing a value for a manual setup step (env var)
    SetupInput,
}

/// Info about a skill for display
#[derive(Debug, Clone)]
pub struct SkillEntry {
    pub name: String,
    pub description: String,
    pub source: String, // "installed", "github", "clawhub"
    pub downloads: u64,
    pub stars: u64,
}

/// Status of a single auto-setup step
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetupStepStatus {
    /// Waiting to run
    Pending,
    /// Currently running
    Running,
    /// Completed successfully
    Done,
    /// Skipped (e.g. binary already present)
    Skipped,
    /// Failed with error message
    Failed(String),
    /// Requires manual action (e.g. API key, OAuth)
    Manual,
}

/// A single step in the auto-setup process
#[derive(Debug, Clone)]
pub struct SetupStep {
    pub label: String,
    pub detail: String, // e.g. the command being run, or "already installed"
    pub status: SetupStepStatus,
}

/// Live auto-setup state shown after skill installation
#[derive(Debug, Clone)]
pub struct SkillSetupProgress {
    pub skill_name: String,
    pub steps: Vec<SetupStep>,
    /// Whether the entire setup process is finished
    pub finished: bool,
}

/// State for the Skills tab
pub struct SkillsState {
    pub list_state: ListState,
    pub installed: Vec<SkillEntry>,
    pub search_results: Vec<SkillEntry>,
    pub view: SkillsView,
    pub focus: SkillsFocus,
    pub search_buffer: String,
    pub status_message: String,
    pub loading: bool,
    /// Live auto-setup progress (shown after installing a skill)
    pub setup_progress: Option<SkillSetupProgress>,
    /// Buffer for inline setup input (env var value)
    pub setup_input_buffer: String,
    /// Index of the setup step currently being edited
    pub setup_input_step_idx: Option<usize>,
}

impl SkillsState {
    pub fn new() -> Self {
        Self {
            list_state: ListState::default(),
            installed: Vec::new(),
            search_results: Vec::new(),
            view: SkillsView::Installed,
            focus: SkillsFocus::SearchBar,
            search_buffer: String::new(),
            status_message: String::new(),
            loading: false,
            setup_progress: None,
            setup_input_buffer: String::new(),
            setup_input_step_idx: None,
        }
    }

    /// Current list being displayed
    pub fn current_list(&self) -> &[SkillEntry] {
        match self.view {
            SkillsView::Installed => &self.installed,
            SkillsView::Search => &self.search_results,
        }
    }

    pub fn move_up(&mut self) {
        if let Some(i) = self.list_state.selected() {
            if i > 0 {
                self.list_state.select(Some(i - 1));
            }
        }
    }

    pub fn move_down(&mut self) {
        let len = self.current_list().len();
        if let Some(i) = self.list_state.selected() {
            if i < len.saturating_sub(1) {
                self.list_state.select(Some(i + 1));
            }
        }
    }

    pub fn selected_skill(&self) -> Option<&SkillEntry> {
        self.list_state
            .selected()
            .and_then(|i| self.current_list().get(i))
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
        Self {
            list_state,
            servers,
        }
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
            Event::SkillsLoaded(entries) => {
                self.skills_state.installed = entries;
                self.skills_state.loading = false;
                self.skills_state.status_message.clear();
                if self.skills_state.view == SkillsView::Installed
                    && !self.skills_state.installed.is_empty()
                {
                    self.skills_state.list_state.select(Some(0));
                }
            }
            Event::SkillSearchResults(entries) => {
                self.skills_state.search_results = entries;
                self.skills_state.loading = false;
                let count = self.skills_state.search_results.len();
                self.skills_state.status_message = format!("{count} results found");
                self.skills_state.view = SkillsView::Search;
                if !self.skills_state.search_results.is_empty() {
                    self.skills_state.list_state.select(Some(0));
                } else {
                    self.skills_state.list_state.select(None);
                }
            }
            Event::SkillInstalled(msg, skill_name) => {
                self.skills_state.loading = false;
                self.skills_state.status_message = msg;
                // Initialize setup progress with the skill name
                self.skills_state.setup_progress = Some(SkillSetupProgress {
                    skill_name,
                    steps: Vec::new(),
                    finished: false,
                });
                // Refresh installed skills
                self.refresh_installed_skills();
            }
            Event::SkillSetupStep(idx, step) => {
                let progress = match &mut self.skills_state.setup_progress {
                    Some(p) => p,
                    None => return, // No active setup
                };
                if idx < progress.steps.len() {
                    progress.steps[idx] = step;
                } else {
                    // Pad if needed, then push
                    while progress.steps.len() < idx {
                        progress.steps.push(SetupStep {
                            label: String::new(),
                            detail: String::new(),
                            status: SetupStepStatus::Pending,
                        });
                    }
                    progress.steps.push(step);
                }
            }
            Event::SkillSetupDone => {
                if let Some(progress) = &mut self.skills_state.setup_progress {
                    if progress.steps.is_empty() {
                        // No setup was needed — just clear the popup
                        self.skills_state.setup_progress = None;
                    } else {
                        progress.finished = true;
                    }
                }
            }
            Event::SkillRemoved(name) => {
                self.skills_state.loading = false;
                self.skills_state.status_message = format!("'{name}' removed");
                // Refresh
                self.refresh_installed_skills();
            }
            Event::SkillsError(err) => {
                self.skills_state.loading = false;
                self.skills_state.status_message = format!("Error: {err}");
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
            Tab::Skills => self.handle_skills_key(key),
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

        if !value.is_empty() && dotpath::config_set(&mut self.config, &key, &value).is_ok() {
            self.config_modified = true;
            self.settings_state.refresh(&self.config);
            self.providers_state.refresh(&self.config);
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
                #[cfg(feature = "channel-whatsapp")]
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
    #[cfg(feature = "channel-whatsapp")]
    fn start_whatsapp_pairing(&mut self) {
        // Need a phone number
        if self.whatsapp_state.phone_input.is_empty() {
            self.whatsapp_state.status =
                WhatsAppStatus::Error("Enter a phone number first".to_string());
            return;
        }

        // Abort any existing pairing
        self.whatsapp_state.abort_pairing();

        // Need event_tx to send pairing events back to the TUI
        let event_tx = match &self.event_tx {
            Some(tx) => tx.clone(),
            None => {
                self.whatsapp_state.status =
                    WhatsAppStatus::Error("Internal error: no event sender".to_string());
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

    /// Handle keys in Skills tab
    fn handle_skills_key(&mut self, key: KeyEvent) {
        // If in setup input mode (typing an env var value), handle that first
        if self.skills_state.focus == SkillsFocus::SetupInput {
            self.handle_skills_setup_input_key(key);
            return;
        }

        // If auto-setup progress is visible and finished, handle dismissal or env var input
        if let Some(progress) = &self.skills_state.setup_progress {
            if progress.finished {
                // Check if user pressed Enter on a Manual step to provide value
                let has_manual = progress
                    .steps
                    .iter()
                    .any(|s| matches!(s.status, SetupStepStatus::Manual));
                if has_manual && key.code == KeyCode::Enter {
                    // Find the first Manual step and start editing it
                    if let Some(idx) = progress
                        .steps
                        .iter()
                        .position(|s| matches!(s.status, SetupStepStatus::Manual))
                    {
                        self.skills_state.setup_input_step_idx = Some(idx);
                        self.skills_state.setup_input_buffer.clear();
                        self.skills_state.focus = SkillsFocus::SetupInput;
                        return;
                    }
                }
                // Any other key dismisses the popup
                self.skills_state.setup_progress = None;
                return;
            }
            // Setup still running — ignore keys except Esc to force-dismiss
            if key.code == KeyCode::Esc {
                self.skills_state.setup_progress = None;
            }
            return;
        }

        match &self.skills_state.focus {
            SkillsFocus::SearchBar => self.handle_skills_search_bar_key(key),
            SkillsFocus::List => self.handle_skills_list_key(key),
            SkillsFocus::SetupInput => {} // handled above
        }
    }

    /// Handle keys when the search bar is focused
    fn handle_skills_search_bar_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') if self.skills_state.search_buffer.is_empty() => {
                self.should_quit = true;
            }
            KeyCode::Tab if self.skills_state.search_buffer.is_empty() => {
                self.current_tab = self.current_tab.next();
            }
            KeyCode::BackTab if self.skills_state.search_buffer.is_empty() => {
                self.current_tab = self.current_tab.prev();
            }
            KeyCode::Esc => {
                if !self.skills_state.search_buffer.is_empty() {
                    self.skills_state.search_buffer.clear();
                } else {
                    // Move focus to list
                    self.skills_state.focus = SkillsFocus::List;
                }
            }
            KeyCode::Enter => {
                let query = self.skills_state.search_buffer.clone();
                if !query.is_empty() {
                    // Check if it looks like a repo slug (contains '/')
                    if query.contains('/') || query.starts_with("clawhub:") {
                        // Direct install
                        self.start_skill_install(query);
                    } else {
                        // Search
                        self.start_skill_search(query);
                    }
                }
            }
            KeyCode::Down => {
                // Move focus to the list if there are items
                if !self.skills_state.current_list().is_empty() {
                    self.skills_state.focus = SkillsFocus::List;
                    if self.skills_state.list_state.selected().is_none() {
                        self.skills_state.list_state.select(Some(0));
                    }
                }
            }
            KeyCode::Backspace => {
                self.skills_state.search_buffer.pop();
            }
            KeyCode::Char(c) => {
                self.skills_state.search_buffer.push(c);
            }
            _ => {}
        }
    }

    /// Handle keys when the list is focused
    fn handle_skills_list_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Tab => self.current_tab = self.current_tab.next(),
            KeyCode::BackTab => self.current_tab = self.current_tab.prev(),
            KeyCode::Up | KeyCode::Char('k') => {
                // If at the top of the list, jump back to search bar
                if self.skills_state.list_state.selected() == Some(0) {
                    self.skills_state.focus = SkillsFocus::SearchBar;
                } else {
                    self.skills_state.move_up();
                }
            }
            KeyCode::Down | KeyCode::Char('j') => self.skills_state.move_down(),
            // Enter: install selected skill from search results
            KeyCode::Enter => {
                if let Some(skill) = self.skills_state.selected_skill() {
                    if skill.source != "installed" {
                        let slug = skill.name.clone();
                        self.start_skill_install(slug);
                    }
                }
            }
            // '/' to jump to search bar
            KeyCode::Char('/') => {
                self.skills_state.search_buffer.clear();
                self.skills_state.focus = SkillsFocus::SearchBar;
            }
            // 'd' to remove selected installed skill
            KeyCode::Char('d') => {
                if self.skills_state.view == SkillsView::Installed {
                    self.remove_selected_skill();
                }
            }
            // '1'/'2' to switch views
            KeyCode::Char('1') => {
                self.skills_state.view = SkillsView::Installed;
                self.skills_state
                    .list_state
                    .select(if self.skills_state.installed.is_empty() {
                        None
                    } else {
                        Some(0)
                    });
            }
            KeyCode::Char('2') => {
                self.skills_state.view = SkillsView::Search;
                self.skills_state.list_state.select(
                    if self.skills_state.search_results.is_empty() {
                        None
                    } else {
                        Some(0)
                    },
                );
            }
            // 'r' to refresh installed skills
            KeyCode::Char('r') => {
                self.refresh_installed_skills();
            }
            // Esc to go back to search bar
            KeyCode::Esc => {
                self.skills_state.focus = SkillsFocus::SearchBar;
            }
            _ => {}
        }
    }

    /// Handle keys when typing an env var value in the setup wizard
    fn handle_skills_setup_input_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.skills_state.setup_input_buffer.clear();
                self.skills_state.setup_input_step_idx = None;
                self.skills_state.focus = SkillsFocus::SearchBar;
            }
            KeyCode::Enter => {
                // Apply the env var value
                let value = self.skills_state.setup_input_buffer.clone();
                if let Some(idx) = self.skills_state.setup_input_step_idx {
                    if let Some(progress) = &mut self.skills_state.setup_progress {
                        if let Some(step) = progress.steps.get_mut(idx) {
                            if !value.is_empty() {
                                // Extract the env var name from the detail (format: "export VAR=<value>")
                                let var_name = step
                                    .detail
                                    .strip_prefix("export ")
                                    .and_then(|s| s.split('=').next())
                                    .unwrap_or(&step.detail)
                                    .to_string();
                                // Set the env var for the current process
                                std::env::set_var(&var_name, &value);
                                // Update the step status
                                step.label = format!("${var_name}");
                                step.detail = "set ✓".to_string();
                                step.status = SetupStepStatus::Done;
                                // Save to config as well
                                self.save_env_var_to_config(&var_name, &value);
                            }
                        }
                    }
                }
                self.skills_state.setup_input_buffer.clear();
                self.skills_state.setup_input_step_idx = None;
                // Check if there are more manual steps
                let more_manual = self
                    .skills_state
                    .setup_progress
                    .as_ref()
                    .map(|p| {
                        p.steps
                            .iter()
                            .any(|s| matches!(s.status, SetupStepStatus::Manual))
                    })
                    .unwrap_or(false);
                if more_manual {
                    // Find next manual step
                    if let Some(progress) = &self.skills_state.setup_progress {
                        if let Some(next_idx) = progress
                            .steps
                            .iter()
                            .position(|s| matches!(s.status, SetupStepStatus::Manual))
                        {
                            self.skills_state.setup_input_step_idx = Some(next_idx);
                            self.skills_state.focus = SkillsFocus::SetupInput;
                            return;
                        }
                    }
                }
                self.skills_state.focus = SkillsFocus::SearchBar;
            }
            KeyCode::Backspace => {
                self.skills_state.setup_input_buffer.pop();
            }
            KeyCode::Char(c) => {
                self.skills_state.setup_input_buffer.push(c);
            }
            _ => {}
        }
    }

    /// Save an env var to config.toml (in [env] section) so gateway picks it up
    fn save_env_var_to_config(&mut self, _var_name: &str, _value: &str) {
        // For now, env vars are set in the process only.
        // TODO: optionally persist to config.toml [env] section
    }

    /// Refresh the installed skills list (async)
    pub fn refresh_installed_skills(&mut self) {
        let event_tx = match &self.event_tx {
            Some(tx) => tx.clone(),
            None => return,
        };
        self.skills_state.loading = true;
        self.skills_state.status_message = "Loading...".to_string();

        tokio::spawn(async move {
            match crate::skills::SkillInstaller::list_installed().await {
                Ok(skills) => {
                    let entries: Vec<SkillEntry> = skills
                        .into_iter()
                        .map(|s| SkillEntry {
                            name: s.name,
                            description: s.description,
                            source: "installed".to_string(),
                            downloads: 0,
                            stars: 0,
                        })
                        .collect();
                    let _ = event_tx.send(Event::SkillsLoaded(entries));
                }
                Err(e) => {
                    let _ = event_tx.send(Event::SkillsError(e.to_string()));
                }
            }
        });
    }

    /// Start searching for skills on GitHub + ClawHub in parallel (async)
    fn start_skill_search(&mut self, query: String) {
        let event_tx = match &self.event_tx {
            Some(tx) => tx.clone(),
            None => return,
        };
        self.skills_state.loading = true;
        self.skills_state.status_message = format!("Searching '{query}' on GitHub + ClawHub...");

        tokio::spawn(async move {
            let query_gh = query.clone();
            let query_ch = query.clone();

            // Search GitHub and ClawHub in parallel
            let (gh_result, ch_result) = tokio::join!(
                async {
                    let searcher = crate::skills::search::SkillSearcher::new();
                    searcher.search(&query_gh, 10).await
                },
                async {
                    let installer = crate::skills::ClawHubInstaller::new();
                    installer.search(&query_ch, 10).await
                }
            );

            let mut entries: Vec<SkillEntry> = Vec::new();

            // Add ClawHub results first (curated registry)
            match ch_result {
                Ok(results) => {
                    entries.extend(results.into_iter().map(|r| SkillEntry {
                        name: format!("clawhub:{}", r.slug),
                        description: r.description,
                        source: "clawhub".to_string(),
                        downloads: r.downloads,
                        stars: r.stars,
                    }));
                }
                Err(e) => {
                    tracing::warn!(error = %e, "ClawHub search failed, skipping");
                }
            }

            // Add GitHub results
            match gh_result {
                Ok(results) => {
                    entries.extend(results.into_iter().map(|r| SkillEntry {
                        name: r.full_name.clone(),
                        description: r.description,
                        source: "github".to_string(),
                        downloads: 0,
                        stars: r.stars as u64,
                    }));
                }
                Err(e) => {
                    tracing::warn!(error = %e, "GitHub search failed, skipping");
                }
            }

            if entries.is_empty() {
                let _ = event_tx.send(Event::SkillsError(format!("No results for '{query}'")));
            } else {
                let _ = event_tx.send(Event::SkillSearchResults(entries));
            }
        });
    }

    /// Install a skill from GitHub/ClawHub and auto-setup dependencies (async)
    fn start_skill_install(&mut self, slug: String) {
        let event_tx = match &self.event_tx {
            Some(tx) => tx.clone(),
            None => return,
        };
        self.skills_state.loading = true;
        self.skills_state.status_message = format!("Installing '{slug}'...");

        // Check if it's a clawhub: prefix
        let is_clawhub = slug.starts_with("clawhub:");
        tokio::spawn(async move {
            let result = if is_clawhub {
                let clawhub_slug = &slug["clawhub:".len()..];
                let installer = crate::skills::ClawHubInstaller::new();
                installer.install(clawhub_slug).await
            } else {
                let installer = crate::skills::SkillInstaller::new();
                installer.install(&slug).await
            };

            match result {
                Ok(info) => {
                    let msg = if info.already_existed {
                        format!("'{}' already installed", info.name)
                    } else {
                        format!("'{}' installed!", info.name)
                    };
                    let skill_name = info.name.clone();
                    let _ = event_tx.send(Event::SkillInstalled(msg, skill_name.clone()));

                    // Run auto-setup in background
                    run_auto_setup(&info.path, &skill_name, event_tx).await;
                }
                Err(e) => {
                    let _ = event_tx.send(Event::SkillsError(e.to_string()));
                }
            }
        });
    }

    /// Remove the selected installed skill
    fn remove_selected_skill(&mut self) {
        if self.skills_state.view != SkillsView::Installed {
            return;
        }
        let name = match self.skills_state.selected_skill() {
            Some(s) => s.name.clone(),
            None => return,
        };

        let event_tx = match &self.event_tx {
            Some(tx) => tx.clone(),
            None => return,
        };

        self.skills_state.status_message = format!("Removing '{name}'...");
        tokio::spawn(async move {
            match crate::skills::SkillInstaller::remove(&name).await {
                Ok(()) => {
                    let _ = event_tx.send(Event::SkillRemoved(name));
                }
                Err(e) => {
                    let _ = event_tx.send(Event::SkillsError(e.to_string()));
                }
            }
        });
    }
}

/// Parsed skill requirements from SKILL.md metadata
struct SkillRequirements {
    /// Required binaries (e.g. ["gog", "curl"])
    bins: Vec<String>,
    /// Required environment variables (e.g. ["OPENAI_API_KEY"])
    env_vars: Vec<String>,
    /// Install commands from metadata (kind → command)
    install_commands: Vec<(String, String)>, // (label, shell command)
}

/// Parse skill requirements from a SKILL.md file
fn parse_skill_requirements(
    skill_path: &std::path::Path,
    content: &str,
) -> Option<SkillRequirements> {
    let (meta, _body) = crate::skills::loader::parse_skill_md_public(content).ok()?;

    let mut bins = Vec::new();
    let mut env_vars = Vec::new();
    let mut install_commands = Vec::new();

    if let Some(metadata) = &meta.metadata {
        if let Some(clawdbot) = metadata.get("clawdbot") {
            if let Some(requires) = clawdbot.get("requires") {
                bins = requires
                    .get("bins")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();

                env_vars = requires
                    .get("env")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
            }

            // Parse install commands
            if let Some(install_arr) = clawdbot.get("install").and_then(|v| v.as_array()) {
                for step in install_arr {
                    let label = step
                        .get("label")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Install")
                        .to_string();
                    let kind = step.get("kind").and_then(|v| v.as_str()).unwrap_or("");
                    let command = match kind {
                        "brew" => step
                            .get("formula")
                            .and_then(|v| v.as_str())
                            .map(|f| format!("brew install {f}")),
                        "npm" => step
                            .get("package")
                            .or_else(|| step.get("formula"))
                            .and_then(|v| v.as_str())
                            .map(|p| format!("npm install -g {p}")),
                        "pip" => step
                            .get("package")
                            .or_else(|| step.get("formula"))
                            .and_then(|v| v.as_str())
                            .map(|p| format!("pip install {p}")),
                        _ => step
                            .get("command")
                            .and_then(|v| v.as_str())
                            .map(String::from),
                    };
                    if let Some(cmd) = command {
                        install_commands.push((label, cmd));
                    }
                }
            }
        }
    }

    let _ = skill_path; // may be used later for script detection

    Some(SkillRequirements {
        bins,
        env_vars,
        install_commands,
    })
}

/// Run automatic post-install setup for a skill.
///
/// Steps:
/// 1. Check required binaries (which <bin>)
/// 2. If missing, run install commands from metadata (brew/npm/pip)
/// 3. Re-check binaries after install
/// 4. Check required env vars
/// 5. Report anything that needs manual setup
async fn run_auto_setup(
    skill_path: &std::path::Path,
    _skill_name: &str,
    event_tx: mpsc::UnboundedSender<Event>,
) {
    let skill_md_path = skill_path.join("SKILL.md");
    let content = match tokio::fs::read_to_string(&skill_md_path).await {
        Ok(c) => c,
        Err(_) => {
            let _ = event_tx.send(Event::SkillSetupDone);
            return;
        }
    };

    let reqs = match parse_skill_requirements(skill_path, &content) {
        Some(r) => r,
        None => {
            let _ = event_tx.send(Event::SkillSetupDone);
            return;
        }
    };

    // If there's nothing to check, skip auto-setup entirely
    if reqs.bins.is_empty() && reqs.env_vars.is_empty() {
        let _ = event_tx.send(Event::SkillSetupDone);
        return;
    }

    // Build the initial list of setup steps
    let mut steps: Vec<SetupStep> = Vec::new();

    // Step for each required binary
    for bin in &reqs.bins {
        steps.push(SetupStep {
            label: format!("Check {bin}"),
            detail: format!("which {bin}"),
            status: SetupStepStatus::Pending,
        });
    }

    // Step for each env var
    for var in &reqs.env_vars {
        steps.push(SetupStep {
            label: format!("Check ${var}"),
            detail: var.clone(),
            status: SetupStepStatus::Pending,
        });
    }

    // Send initial steps to build the progress popup
    for (i, step) in steps.iter().enumerate() {
        let _ = event_tx.send(Event::SkillSetupStep(i, step.clone()));
    }

    // Now process each binary check
    let bin_count = reqs.bins.len();
    for (i, bin) in reqs.bins.iter().enumerate() {
        // Mark as running
        let _ = event_tx.send(Event::SkillSetupStep(
            i,
            SetupStep {
                label: format!("Check {bin}"),
                detail: format!("which {bin}"),
                status: SetupStepStatus::Running,
            },
        ));

        // Check if binary exists
        let found = check_binary_exists(bin).await;

        if found {
            let _ = event_tx.send(Event::SkillSetupStep(
                i,
                SetupStep {
                    label: format!("Check {bin}"),
                    detail: "already installed".to_string(),
                    status: SetupStepStatus::Skipped,
                },
            ));
        } else {
            // Try to find an install command for this binary
            let install_cmd = reqs.install_commands.iter().find(|(_, cmd)| {
                // Match: the command installs something that provides this binary
                // Heuristic: check if the binary name appears in the command
                cmd.contains(bin) || reqs.install_commands.iter().any(|(_, c)| c.contains(bin))
            });

            if let Some((_label, cmd)) = install_cmd.or_else(|| reqs.install_commands.first()) {
                // Run the install command
                let _ = event_tx.send(Event::SkillSetupStep(
                    i,
                    SetupStep {
                        label: format!("Installing {bin}"),
                        detail: cmd.clone(),
                        status: SetupStepStatus::Running,
                    },
                ));

                match run_shell_command(cmd).await {
                    Ok(output) => {
                        // Re-check if binary is now available
                        if check_binary_exists(bin).await {
                            let _ = event_tx.send(Event::SkillSetupStep(
                                i,
                                SetupStep {
                                    label: format!("Installed {bin}"),
                                    detail: cmd.clone(),
                                    status: SetupStepStatus::Done,
                                },
                            ));
                        } else {
                            let _ = event_tx.send(Event::SkillSetupStep(
                                i,
                                SetupStep {
                                    label: format!("Install {bin}"),
                                    detail: format!("installed but '{bin}' not found in PATH"),
                                    status: SetupStepStatus::Failed(output),
                                },
                            ));
                        }
                    }
                    Err(e) => {
                        let _ = event_tx.send(Event::SkillSetupStep(
                            i,
                            SetupStep {
                                label: format!("Install {bin}"),
                                detail: cmd.clone(),
                                status: SetupStepStatus::Failed(e),
                            },
                        ));
                    }
                }
            } else {
                // No install command — can't auto-install
                let _ = event_tx.send(Event::SkillSetupStep(
                    i,
                    SetupStep {
                        label: format!("{bin} missing"),
                        detail: "no auto-install available".to_string(),
                        status: SetupStepStatus::Manual,
                    },
                ));
            }
        }
    }

    // Check environment variables
    for (j, var) in reqs.env_vars.iter().enumerate() {
        let idx = bin_count + j;
        let _ = event_tx.send(Event::SkillSetupStep(
            idx,
            SetupStep {
                label: format!("Check ${var}"),
                detail: var.clone(),
                status: SetupStepStatus::Running,
            },
        ));

        let is_set = std::env::var(var).is_ok_and(|v| !v.is_empty());

        if is_set {
            let _ = event_tx.send(Event::SkillSetupStep(
                idx,
                SetupStep {
                    label: format!("${var}"),
                    detail: "set".to_string(),
                    status: SetupStepStatus::Skipped,
                },
            ));
        } else {
            let _ = event_tx.send(Event::SkillSetupStep(
                idx,
                SetupStep {
                    label: format!("${var} not set"),
                    detail: format!("export {var}=<value>"),
                    status: SetupStepStatus::Manual,
                },
            ));
        }
    }

    let _ = event_tx.send(Event::SkillSetupDone);
}

/// Check if a binary exists in PATH
async fn check_binary_exists(bin: &str) -> bool {
    tokio::process::Command::new("which")
        .arg(bin)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Run a shell command and return stdout or error
async fn run_shell_command(cmd: &str) -> Result<String, String> {
    let output = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("failed to run: {e}"))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            format!("exit code {}", output.status.code().unwrap_or(-1))
        } else {
            // Keep just the last line of stderr for display
            stderr.lines().last().unwrap_or(&stderr).to_string()
        })
    }
}

/// Run the WhatsApp pairing process in the background.
///
/// This connects to WhatsApp, requests a pairing code, and sends
/// status events back to the TUI via the event channel.
#[cfg(feature = "channel-whatsapp")]
async fn run_whatsapp_pairing(
    phone: String,
    db_path: std::path::PathBuf,
    event_tx: mpsc::UnboundedSender<Event>,
) -> anyhow::Result<()> {
    use wa_rs::bot::Bot;
    use wa_rs::store::SqliteStore;
    use wa_rs_core::types::events::Event as WaEvent;
    use wa_rs_proto::whatsapp as wa;
    use wa_rs_tokio_transport::TokioWebSocketTransportFactory;
    use wa_rs_ureq_http::UreqHttpClient;

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
        .with_pair_code(wa_rs::pair_code::PairCodeOptions {
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
        let anthropic = state
            .providers
            .iter()
            .find(|p| p.name == "anthropic")
            .unwrap();
        assert!(anthropic.configured);
        assert!(anthropic.api_key_masked.starts_with("sk-ant"));
    }
}
