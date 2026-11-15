use crate::core::game_mode::{load_default_definitions, GameModeDefinition};
use crate::core::server::{Server, ServerData};
use crate::core::valve_api::{load_server_data, ReqwestValveApiClient};
use crate::services::firewall::{create_firewall_service, is_root, FirewallService};
use crate::services::ping_service::PingService;
use crate::services::settings_service::SettingsService;
use crate::ui::main_view;
use directories::ProjectDirs;
use iced::{time, Element, Subscription, Task, Theme};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Semaphore;

/// Maximum number of servers being pinged at the same time.
/// Independent pings are preserved; this only bounds in-flight probes
/// so the async runtime and socket layer do not get overwhelmed.
const MAX_CONCURRENT_PINGS: usize = 16;

/// Tracks which servers were affected by an in-flight firewall operation.
#[derive(Debug, Clone)]
enum PendingFirewallOp {
    Block(Vec<Server>),
    Unblock(Vec<Server>),
}

/// State for the preset management modal dialog.
#[derive(Debug, Clone, Default)]
pub struct PresetDialogState {
    pub new_name: String,
    pub rename_source: Option<String>,
    pub rename_value: String,
    pub delete_confirm: Option<String>,
}

/// Services and configuration passed into the Iced application.
#[derive(Clone)]
pub struct AppFlags {
    pub definitions: Vec<GameModeDefinition>,
    pub settings: SettingsService,
    pub firewall: Arc<dyn FirewallService>,
    pub ping_service: Arc<PingService>,
    pub http_client: reqwest::Client,
}

impl AppFlags {
    pub async fn new() -> Result<Self, String> {
        let project_dirs = ProjectDirs::from("", "", "server-picker-rs")
            .ok_or("Could not determine project directories")?;

        let definitions = load_default_definitions().map_err(|e| e.to_string())?;
        let settings = SettingsService::new(&project_dirs)
            .await
            .map_err(|e| e.to_string())?;
        let firewall = create_firewall_service().await.map_err(|e| e.to_string())?;

        Ok(Self {
            definitions,
            settings,
            firewall,
            ping_service: Arc::new(PingService::new()),
            http_client: reqwest::Client::new(),
        })
    }
}

/// Top-level application messages.
#[derive(Debug, Clone)]
pub enum Message {
    GameModeChanged(String),
    SearchChanged(String),
    ToggleGroupByRegion,
    LoadServers,
    ServersLoaded(Result<ServerData, String>),
    PingAll,
    ServerPinged(Server),
    BlockAll,
    BlockSelected,
    UnblockAll,
    UnblockSelected,
    FirewallOperationComplete(Result<(), String>),
    ToggleServerSelection(String),
    OpenUrl(String),
    StatusSet(String),
    DragWindow,
    MinimizeWindow,
    ToggleMaximizeWindow,
    CloseWindow,
    WindowIdReceived(Option<iced::window::Id>),
    PingPollTick,
    TogglePingPolling,
    TableScrolled(iced::widget::scrollable::Viewport),

    // Preset manager
    PresetSelected(String),
    ApplyPreset,
    SavePreset,
    PresetDialogOpened,
    PresetDialogClosed,
    PresetDialogNameChanged(String),
    PresetDialogCreate,
    PresetDialogApply(String),
    PresetDialogRenameStarted(String),
    PresetDialogRename { old_name: String, new_name: String },
    PresetDialogDuplicate(String),
    PresetDialogDeleteRequested(String),
    PresetDialogDelete(String),
}

/// Main application state.
pub struct ServerPickerApp {
    pub definitions: Vec<GameModeDefinition>,
    pub settings: SettingsService,
    firewall: Arc<dyn FirewallService>,
    ping_service: Arc<PingService>,
    http_client: reqwest::Client,

    pub current_game_mode: String,
    pub group_by_region: bool,
    pub search_text: String,
    pub server_data: ServerData,
    /// Indices into `server_data.servers` that match the current search/group
    /// settings, kept sorted for stable rendering. Rebuilt on input changes;
    /// ping updates mutate `server_data.servers` in place so indices stay valid.
    cached_filtered_indices: Vec<usize>,
    /// Vertical scroll offset of the server table, in logical pixels.
    pub table_scroll_offset: f32,
    /// Visible height of the server table viewport, in logical pixels.
    pub table_viewport_height: f32,
    pub selected_servers: HashSet<String>,
    pub status_message: String,
    pub pending_operation: bool,
    pub is_maximized: bool,
    pub window_id: Option<iced::window::Id>,

    /// Servers marked as blocked in the current view, keyed by `server_key`.
    pub blocked_servers: HashSet<String>,
    /// Relay IPv4 addresses currently blocked in the firewall. This is the
    /// cross-view source of truth for the blocked state.
    pub blocked_relays: HashSet<String>,
    pending_firewall_op: Option<PendingFirewallOp>,

    /// Name of the currently active preset, if any.
    pub active_preset_name: Option<String>,
    /// Modal dialog state for managing presets.
    pub preset_dialog: Option<PresetDialogState>,
    /// Whether the last-selected preset should be applied once servers load on startup.
    startup_preset_pending: bool,
}

impl ServerPickerApp {
    pub fn new(flags: AppFlags) -> (Self, Task<Message>) {
        let current_game_mode = flags.settings.game_mode().to_string();
        let group_by_region = flags.settings.group_by_region();
        let active_preset = flags
            .settings
            .settings()
            .last_selected_preset_for_game_mode(&current_game_mode)
            .map(|s| s.to_string());

        let app = Self {
            definitions: flags.definitions,
            settings: flags.settings,
            firewall: flags.firewall,
            ping_service: flags.ping_service,
            http_client: flags.http_client,
            current_game_mode,
            group_by_region,
            search_text: String::new(),
            server_data: ServerData::default(),
            cached_filtered_indices: Vec::new(),
            table_scroll_offset: 0.0,
            table_viewport_height: 600.0,
            selected_servers: HashSet::new(),
            status_message: String::from("Ready"),
            pending_operation: false,
            is_maximized: false,
            window_id: None,
            blocked_servers: HashSet::new(),
            blocked_relays: HashSet::new(),
            pending_firewall_op: None,
            active_preset_name: active_preset.clone(),
            preset_dialog: None,
            startup_preset_pending: active_preset.is_some(),
        };

        let mut tasks = vec![iced::window::oldest().map(Message::WindowIdReceived)];

        if app.current_definition().is_some() {
            tasks.push(Task::done(Message::LoadServers));
        }

        (app, Task::batch(tasks))
    }

    pub fn title(&self) -> String {
        String::from("ServerPickerX")
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::GameModeChanged(game_mode) => {
                self.current_game_mode = game_mode;
                self.settings.settings_mut().game_mode = self.current_game_mode.clone();
                self.selected_servers.clear();
                self.search_text.clear();
                let save = self.save_settings_task();
                Task::batch(vec![save, Task::done(Message::LoadServers)])
            }
            Message::SearchChanged(text) => {
                self.search_text = text;
                self.rebuild_filtered_cache();
                Task::none()
            }
            Message::ToggleGroupByRegion => {
                self.group_by_region = !self.group_by_region;
                self.settings.settings_mut().group_by_region = self.group_by_region;
                self.selected_servers.clear();
                self.rebuild_filtered_cache();
                let save = self.save_settings_task();
                Task::batch(vec![save])
            }
            Message::LoadServers => {
                let Some(definition) = self.current_definition().cloned() else {
                    self.set_status("No game mode selected");
                    return Task::none();
                };

                self.set_status("Loading servers...");
                let client = ReqwestValveApiClient::new(self.http_client.clone());
                Task::perform(
                    async move {
                        load_server_data(&client, &definition)
                            .await
                            .map_err(|e| e.to_string())
                    },
                    Message::ServersLoaded,
                )
            }
            Message::ServersLoaded(result) => match result {
                Ok(data) => {
                    self.server_data = data;
                    self.rebuild_filtered_cache();
                    if self.startup_preset_pending {
                        self.startup_preset_pending = false;
                        if let Some(name) = self.active_preset_name.clone() {
                            self.load_preset_state_without_applying(&name);
                        }
                    }
                    self.recompute_blocked_servers();
                    self.set_status(format!(
                        "Loaded {} servers (revision {})",
                        self.displayed_servers().len(),
                        self.server_data.revision
                    ));
                    Task::done(Message::PingAll)
                }
                Err(err) => {
                    self.set_status(format!("Error: {}", err));
                    Task::none()
                }
            },
            Message::PingAll => {
                let ping = self.ping_service.clone();
                let servers = self.displayed_servers().clone();
                self.set_status(format!("Pinging {} servers...", servers.len()));

                // A semaphore lets every server ping independently and concurrently,
                // but caps how many probes are active at once. Results still stream
                // back one-by-one as each ping finishes.
                let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_PINGS));
                Task::batch(servers.into_iter().map(move |server| {
                    let ping = ping.clone();
                    let semaphore = semaphore.clone();
                    Task::perform(
                        async move {
                            let _permit = semaphore.acquire().await.ok()?;
                            let mut server = server;
                            ping.ping_server(&mut server).await;
                            Some(server)
                        },
                        |result| match result {
                            Some(server) => Message::ServerPinged(server),
                            None => Message::StatusSet(String::new()),
                        },
                    )
                }))
            }
            Message::ServerPinged(server) => {
                self.update_server(server);
                Task::none()
            }
            Message::BlockAll => {
                let servers: Vec<Server> = self.filtered_servers().into_iter().cloned().collect();
                self.perform_block(servers)
            }
            Message::BlockSelected => {
                let servers = self.selected_server_models();
                if servers.is_empty() {
                    self.set_status("Select at least one server to block");
                    return Task::none();
                }
                self.perform_block(servers)
            }
            Message::UnblockAll => {
                let servers: Vec<Server> = self.displayed_servers().clone();
                self.perform_unblock(servers)
            }
            Message::UnblockSelected => {
                let servers = self.selected_server_models();
                if servers.is_empty() {
                    self.set_status("Select at least one server to unblock");
                    return Task::none();
                }
                self.perform_unblock(servers)
            }
            Message::FirewallOperationComplete(result) => {
                self.pending_operation = false;
                if let Some(op) = self.pending_firewall_op.take() {
                    if result.is_ok() {
                        match op {
                            PendingFirewallOp::Block(servers) => {
                                for server in servers {
                                    let key = self.server_key(&server);
                                    self.blocked_servers.insert(key);
                                    for relay in &server.relays {
                                        self.blocked_relays.insert(relay.ipv4.clone());
                                    }
                                }
                            }
                            PendingFirewallOp::Unblock(servers) => {
                                for server in servers {
                                    let key = self.server_key(&server);
                                    self.blocked_servers.remove(&key);
                                    for relay in &server.relays {
                                        self.blocked_relays.remove(&relay.ipv4);
                                    }
                                }
                            }
                        }
                    }
                }
                match result {
                    Ok(_) => {
                        self.set_status("Operation completed");
                        Task::done(Message::PingAll)
                    }
                    Err(err) => {
                        self.set_status(format!("Firewall error: {}", err));
                        Task::none()
                    }
                }
            }
            Message::ToggleServerSelection(key) => {
                if self.selected_servers.contains(&key) {
                    self.selected_servers.remove(&key);
                } else {
                    self.selected_servers.insert(key);
                }
                Task::none()
            }
            Message::OpenUrl(url) => {
                if let Err(err) = open::that(&url) {
                    self.set_status(format!("Failed to open {}: {}", url, err));
                }
                Task::none()
            }
            Message::StatusSet(status) => {
                if !status.is_empty() {
                    self.set_status(status);
                }
                Task::none()
            }
            Message::DragWindow => self.with_window_id(iced::window::drag),
            Message::MinimizeWindow => self.with_window_id(|id| iced::window::minimize(id, true)),
            Message::ToggleMaximizeWindow => {
                self.is_maximized = !self.is_maximized;
                let maximized = self.is_maximized;
                self.with_window_id(|id| iced::window::maximize(id, maximized))
            }
            Message::CloseWindow => self.with_window_id(iced::window::close),
            Message::WindowIdReceived(id) => {
                self.window_id = id;
                Task::none()
            }
            Message::PingPollTick => {
                if self.pending_operation {
                    return Task::none();
                }
                let ping = self.ping_service.clone();
                let servers = self.displayed_servers().clone();

                let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_PINGS));
                Task::batch(servers.into_iter().map(move |server| {
                    let ping = ping.clone();
                    let semaphore = semaphore.clone();
                    Task::perform(
                        async move {
                            let _permit = semaphore.acquire().await.ok()?;
                            let mut server = server;
                            ping.ping_server(&mut server).await;
                            Some(server)
                        },
                        |result| match result {
                            Some(server) => Message::ServerPinged(server),
                            None => Message::StatusSet(String::new()),
                        },
                    )
                }))
            }
            Message::TogglePingPolling => {
                let new_state = !self.settings.settings().ping_polling_enabled;
                self.settings.settings_mut().ping_polling_enabled = new_state;
                self.set_status(if new_state {
                    "Auto-ping enabled"
                } else {
                    "Auto-ping disabled"
                });
                let save = self.save_settings_task();
                if new_state {
                    Task::batch(vec![save, Task::done(Message::PingPollTick)])
                } else {
                    save
                }
            }
            Message::TableScrolled(viewport) => {
                let offset = viewport.absolute_offset();
                self.table_scroll_offset = offset.y;
                self.table_viewport_height = viewport.bounds().height;
                Task::none()
            }
            Message::PresetSelected(name) => {
                self.active_preset_name = Some(name);
                if let Some(name) = self.active_preset_name.clone() {
                    self.settings
                        .settings_mut()
                        .set_last_selected_preset_for_game_mode(&self.current_game_mode, &name);
                    let save = self.save_settings_task();
                    Task::batch(vec![save])
                } else {
                    Task::none()
                }
            }
            Message::ApplyPreset => {
                if let Some(name) = self.active_preset_name.clone() {
                    self.apply_preset(&name)
                } else {
                    self.set_status("Select a preset to apply");
                    Task::none()
                }
            }
            Message::SavePreset => {
                if let Some(name) = self.active_preset_name.clone() {
                    self.save_preset(&name)
                } else {
                    self.set_status("Select a preset to save, or use Save As");
                    Task::none()
                }
            }
            Message::PresetDialogOpened => {
                self.preset_dialog = Some(PresetDialogState::default());
                Task::none()
            }
            Message::PresetDialogClosed => {
                self.preset_dialog = None;
                Task::none()
            }
            Message::PresetDialogNameChanged(name) => {
                if let Some(dialog) = &mut self.preset_dialog {
                    dialog.new_name = name;
                }
                Task::none()
            }
            Message::PresetDialogCreate => {
                let name = self
                    .preset_dialog
                    .as_ref()
                    .map(|d| d.new_name.trim().to_string())
                    .unwrap_or_default();
                if name.is_empty() {
                    self.set_status("Preset name cannot be empty");
                    return Task::none();
                }
                let task = self.save_preset(&name);
                self.preset_dialog = None;
                task
            }
            Message::PresetDialogApply(name) => {
                self.active_preset_name = Some(name.clone());
                self.settings
                    .settings_mut()
                    .set_last_selected_preset_for_game_mode(&self.current_game_mode, &name);
                self.preset_dialog = None;
                self.apply_preset(&name)
            }
            Message::PresetDialogRenameStarted(name) => {
                if let Some(dialog) = &mut self.preset_dialog {
                    dialog.rename_source = Some(name.clone());
                    dialog.rename_value = name;
                }
                Task::none()
            }
            Message::PresetDialogDeleteRequested(name) => {
                if let Some(dialog) = &mut self.preset_dialog {
                    dialog.delete_confirm = Some(name);
                }
                Task::none()
            }
            Message::PresetDialogRename { old_name, new_name } => {
                let new_name = new_name.trim().to_string();
                if new_name.is_empty() {
                    self.set_status("Preset name cannot be empty");
                    return Task::none();
                }
                if new_name.eq_ignore_ascii_case(&old_name) {
                    return Task::none();
                }
                if self
                    .settings
                    .settings()
                    .find_preset(&self.current_game_mode, &new_name)
                    .is_some()
                {
                    self.set_status(format!("Preset '{}' already exists", new_name));
                    return Task::none();
                }
                if let Some(existing) = self
                    .settings
                    .settings_mut()
                    .find_preset_mut(&self.current_game_mode, &old_name)
                {
                    existing.name = new_name.clone();
                    if self.active_preset_name.as_ref() == Some(&old_name) {
                        self.active_preset_name = Some(new_name.clone());
                        self.settings
                            .settings_mut()
                            .set_last_selected_preset_for_game_mode(
                                &self.current_game_mode,
                                &new_name,
                            );
                    }
                    self.set_status(format!("Renamed preset to '{}'", new_name));
                    self.save_settings_task()
                } else {
                    self.set_status(format!("Preset '{}' not found", old_name));
                    Task::none()
                }
            }
            Message::PresetDialogDuplicate(name) => {
                let name = name.trim().to_string();
                let base = self
                    .settings
                    .settings()
                    .find_preset(&self.current_game_mode, &name)
                    .cloned();
                if let Some(mut preset) = base {
                    let mut new_name = format!("{} Copy", name);
                    let mut counter = 2;
                    while self
                        .settings
                        .settings()
                        .find_preset(&self.current_game_mode, &new_name)
                        .is_some()
                    {
                        new_name = format!("{} Copy {}", name, counter);
                        counter += 1;
                    }
                    preset.name = new_name.clone();
                    self.settings.settings_mut().add_or_update_preset(preset);
                    self.active_preset_name = Some(new_name.clone());
                    self.set_status(format!("Duplicated preset as '{}'", new_name));
                    self.save_settings_task()
                } else {
                    self.set_status(format!("Preset '{}' not found", name));
                    Task::none()
                }
            }
            Message::PresetDialogDelete(name) => {
                self.settings
                    .settings_mut()
                    .remove_preset(&self.current_game_mode, &name);
                if self.active_preset_name.as_ref() == Some(&name) {
                    self.active_preset_name = None;
                    self.settings
                        .settings_mut()
                        .clear_last_selected_preset_for_game_mode(&self.current_game_mode);
                }
                self.set_status(format!("Deleted preset '{}'", name));
                self.save_settings_task()
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        main_view::view(self)
    }

    pub fn theme(&self) -> Theme {
        crate::ui::theme::minimal()
    }

    pub fn group_by_region(&self) -> bool {
        self.group_by_region
    }

    pub fn ping_polling_enabled(&self) -> bool {
        self.settings.settings().ping_polling_enabled
    }

    pub fn subscription(&self) -> Subscription<Message> {
        if !self.settings.settings().ping_polling_enabled {
            return Subscription::none();
        }

        let interval = self.settings.settings().ping_polling_interval_seconds;
        time::every(time::Duration::from_secs(interval)).map(|_| Message::PingPollTick)
    }

    fn current_definition(&self) -> Option<&GameModeDefinition> {
        self.definitions
            .iter()
            .find(|d| d.game_mode == self.current_game_mode)
    }

    pub fn server_key(&self, server: &Server) -> String {
        server.name.clone()
    }

    fn recompute_blocked_servers(&mut self) {
        let keys: Vec<String> = self
            .displayed_servers()
            .iter()
            .filter(|server| {
                server
                    .relays
                    .iter()
                    .any(|relay| self.blocked_relays.contains(&relay.ipv4))
            })
            .map(|server| self.server_key(server))
            .collect();
        self.blocked_servers.clear();
        self.blocked_servers.extend(keys);
    }

    pub fn filtered_servers(&self) -> Vec<&Server> {
        self.cached_filtered_indices
            .iter()
            .map(|&i| &self.server_data.servers[i])
            .collect()
    }

    fn rebuild_filtered_cache(&mut self) {
        let query = self.search_text.to_lowercase();
        let mut indices: Vec<usize> = self
            .server_data
            .servers
            .iter()
            .enumerate()
            .filter(|(_, server)| {
                query.is_empty()
                    || server.name.to_lowercase().contains(&query)
                    || server.description.to_lowercase().contains(&query)
                    || server.region_name().to_lowercase().contains(&query)
                    || server.region_group().to_lowercase().contains(&query)
            })
            .map(|(i, _)| i)
            .collect();

        indices.sort_by(|&a, &b| {
            let sa = &self.server_data.servers[a];
            let sb = &self.server_data.servers[b];
            sa.region_group()
                .cmp(sb.region_group())
                .then_with(|| sa.region_name().cmp(sb.region_name()))
                .then_with(|| sa.name.cmp(&sb.name))
        });

        self.cached_filtered_indices = indices;
    }

    fn displayed_servers(&self) -> &Vec<Server> {
        &self.server_data.servers
    }

    fn update_server(&mut self, server: Server) {
        if let Some(existing) = self
            .server_data
            .servers
            .iter_mut()
            .find(|s| s.name == server.name)
        {
            *existing = server;
        }
    }

    fn selected_server_models(&self) -> Vec<Server> {
        self.filtered_servers()
            .into_iter()
            .filter(|server| self.selected_servers.contains(&self.server_key(server)))
            .cloned()
            .collect()
    }

    pub fn set_status(&mut self, message: impl Into<String>) {
        self.status_message = message.into();
    }

    fn with_window_id<F>(&mut self, f: F) -> Task<Message>
    where
        F: FnOnce(iced::window::Id) -> Task<Message>,
    {
        match self.window_id {
            Some(id) => f(id),
            None => {
                self.set_status("Window not ready");
                Task::none()
            }
        }
    }

    fn save_settings_task(&self) -> Task<Message> {
        let settings = self.settings.clone();
        Task::perform(
            async move { settings.save().await.map_err(|e| e.to_string()) },
            |result| {
                if let Err(err) = result {
                    Message::StatusSet(format!("Failed to save settings: {}", err))
                } else {
                    Message::StatusSet(String::new())
                }
            },
        )
    }

    fn save_preset(&mut self, name: &str) -> Task<Message> {
        let name = name.trim().to_string();
        if name.is_empty() {
            self.set_status("Preset name cannot be empty");
            return Task::none();
        }

        let mut keys: Vec<String> = self.blocked_servers.iter().cloned().collect();
        keys.sort();

        let preset = crate::core::preset::Preset {
            name: name.clone(),
            game_mode: self.current_game_mode.clone(),
            search_text: self.search_text.clone(),
            blocked_server_keys: keys,
        };

        self.settings.settings_mut().add_or_update_preset(preset);
        self.active_preset_name = Some(name.clone());
        self.settings
            .settings_mut()
            .set_last_selected_preset_for_game_mode(&self.current_game_mode, &name);
        self.set_status(format!("Saved preset '{}'", name));
        self.save_settings_task()
    }

    fn load_preset_state_without_applying(&mut self, name: &str) {
        let Some(preset) = self
            .settings
            .settings()
            .find_preset(&self.current_game_mode, name)
            .cloned()
        else {
            return;
        };

        self.search_text = preset.search_text.clone();

        let target_keys: std::collections::HashSet<String> = preset
            .blocked_server_keys
            .iter()
            .map(|k| k.to_lowercase())
            .collect();
        let relay_ips: Vec<String> = self
            .displayed_servers()
            .iter()
            .filter(|server| target_keys.contains(&self.server_key(server).to_lowercase()))
            .flat_map(|server| server.relays.iter().map(|relay| relay.ipv4.clone()))
            .collect();
        for ip in relay_ips {
            self.blocked_relays.insert(ip);
        }

        self.set_status(format!(
            "Preset '{}' loaded. Click Apply to enforce firewall rules.",
            preset.name
        ));
    }

    fn apply_preset(&mut self, name: &str) -> Task<Message> {
        let Some(preset) = self
            .settings
            .settings()
            .find_preset(&self.current_game_mode, name)
            .cloned()
        else {
            self.set_status(format!("Preset '{}' not found", name));
            return Task::none();
        };

        if !preset
            .game_mode
            .eq_ignore_ascii_case(&self.current_game_mode)
        {
            self.set_status("Preset does not match current game mode");
            return Task::none();
        }

        // Restore view settings from the preset.
        self.search_text = preset.search_text.clone();
        self.selected_servers.clear();

        // Find servers that should be blocked.
        let target_keys: std::collections::HashSet<String> = preset
            .blocked_server_keys
            .iter()
            .map(|k| k.to_lowercase())
            .collect();
        let target_servers: Vec<Server> = self
            .displayed_servers()
            .iter()
            .filter(|s| target_keys.contains(&self.server_key(s).to_lowercase()))
            .cloned()
            .collect();

        let matched_keys: std::collections::HashSet<String> = target_servers
            .iter()
            .map(|s| self.server_key(s).to_lowercase())
            .collect();
        let missing_count = target_keys.len() - matched_keys.len();

        self.active_preset_name = Some(preset.name.clone());
        self.settings
            .settings_mut()
            .set_last_selected_preset_for_game_mode(&self.current_game_mode, &preset.name);
        self.blocked_servers.clear();
        self.blocked_relays.clear();

        self.pending_operation = true;
        self.pending_firewall_op = Some(PendingFirewallOp::Block(target_servers.clone()));

        let status = if missing_count > 0 {
            format!(
                "Applying preset '{}' ({} servers unavailable)",
                preset.name, missing_count
            )
        } else {
            format!("Applying preset '{}'", preset.name)
        };
        self.set_status(status);

        let firewall = self.firewall.clone();
        Task::perform(
            async move {
                firewall
                    .reset_and_block_servers(&target_servers)
                    .await
                    .map_err(|e| e.to_string())
            },
            Message::FirewallOperationComplete,
        )
    }

    fn perform_block(&mut self, servers: Vec<Server>) -> Task<Message> {
        self.pending_operation = true;
        self.pending_firewall_op = Some(PendingFirewallOp::Block(servers.clone()));
        if !is_root() {
            self.set_status("Authentication required to block servers");
        } else {
            self.set_status("Blocking servers...");
        }
        let firewall = self.firewall.clone();
        Task::perform(
            async move {
                firewall
                    .block_servers(&servers)
                    .await
                    .map_err(|e| e.to_string())
            },
            Message::FirewallOperationComplete,
        )
    }

    fn perform_unblock(&mut self, servers: Vec<Server>) -> Task<Message> {
        self.pending_operation = true;
        self.pending_firewall_op = Some(PendingFirewallOp::Unblock(servers.clone()));
        if !is_root() {
            self.set_status("Authentication required to unblock servers");
        } else {
            self.set_status("Unblocking servers...");
        }
        let firewall = self.firewall.clone();
        Task::perform(
            async move {
                firewall
                    .unblock_servers(&servers)
                    .await
                    .map_err(|e| e.to_string())
            },
            Message::FirewallOperationComplete,
        )
    }
}
