use crossterm::event::{KeyCode, KeyModifiers};
use std::sync::Arc;
use std::time::Instant;
use std::cell::{Cell, RefCell};
use tokio::sync::{mpsc, Mutex};
use ratatui::text::Line;

use fusion_agent::agent::{Agent, AgentEvent};
use fusion_core::config::Config;
use fusion_core::models::{TokenLevel, list_models, lookup_model, CLOUDFLARE_MODELS};
use fusion_core::session::{Session, list_sessions};

use crate::event::{AppEvent, EventHandler};
use crate::ui;

/// Visible message in the TUI transcript.
#[derive(Debug, Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
}

/// TUI mode.
#[derive(Debug, Clone, PartialEq)]
pub enum AppMode {
    Normal,
    Plan,
    Yolo,
}

/// A slash command definition for autocomplete.
#[derive(Debug, Clone)]
pub struct SlashCommand {
    pub name: &'static str,
    pub description: &'static str,
    pub has_submenu: bool,
}

pub const SLASH_COMMANDS: &[SlashCommand] = &[
    SlashCommand { name: "/help", description: "Show all commands", has_submenu: false },
    SlashCommand { name: "/yolo", description: "Toggle auto-approve mode", has_submenu: false },
    SlashCommand { name: "/plan", description: "Enter plan mode", has_submenu: false },
    SlashCommand { name: "/model", description: "Switch the active model", has_submenu: true },
    SlashCommand { name: "/models", description: "List available models", has_submenu: false },
    SlashCommand { name: "/max", description: "Set output to maximum tokens", has_submenu: false },
    SlashCommand { name: "/high", description: "Set output to high tokens", has_submenu: false },
    SlashCommand { name: "/normal", description: "Set output to normal tokens", has_submenu: false },
    SlashCommand { name: "/status", description: "Show current settings", has_submenu: false },
    SlashCommand { name: "/session", description: "Show current session ID", has_submenu: false },
    SlashCommand { name: "/sessions", description: "List saved sessions", has_submenu: false },
    SlashCommand { name: "/clear", description: "Clear message history", has_submenu: false },
    SlashCommand { name: "/image", description: "Insert clipboard image (macOS)", has_submenu: false },
    SlashCommand { name: "/edit", description: "Compose/edit input in external editor", has_submenu: false },
    SlashCommand { name: "/quit", description: "Quit (session auto-saved)", has_submenu: false },
];

/// What the autocomplete popup is currently showing.
#[derive(Debug, Clone, PartialEq)]
pub enum AutocompleteMode {
    /// Normal slash command list
    Commands,
    /// Model picker sub-dialog
    Models,
    /// Effort/token level picker (shown after selecting a model that supports it)
    Effort,
}

/// An item in the autocomplete popup (works for both commands and models).
#[derive(Debug, Clone)]
pub struct AutocompleteItem {
    pub label: String,
    pub description: String,
    pub is_current: bool,
}

/// Main TUI application state.
pub struct App {
    pub messages: Vec<Message>,
    pub input: String,
    pub mode: AppMode,
    pub model: String,
    pub token_level: TokenLevel,
    pub is_thinking: bool,
    pub should_quit: bool,
    pub session_id: String,
    pub theme: String,

    // Autocomplete state
    pub autocomplete_visible: bool,
    pub autocomplete_mode: AutocompleteMode,
    pub autocomplete_items: Vec<AutocompleteItem>,
    pub autocomplete_selected: usize,

    // Pending model shorthand (used during effort selection)
    pending_model: Option<String>,

    // Timing
    turn_start: Option<Instant>,
    thought_duration: Option<f64>,
    had_thinking: bool,
    pub tick_count: u64,
    pub scroll_offset: Cell<usize>,
    pub auto_scroll: Cell<bool>,

    pub last_key_time: Option<Instant>,
    pub in_paste_burst: bool,
    pub submitted_text: Option<String>,
    pub editor_requested: Option<String>,

    // Cache for TUI message lines to prevent typing delays: (wrap_width, messages_len, lines)
    pub message_cache: RefCell<Option<(usize, usize, Vec<Line<'static>>)>>,

    // Queued user prompts to execute sequentially
    pub queued_prompts: Vec<String>,

    agent: Arc<Mutex<Agent>>,
    session: Session,
    event_tx: mpsc::UnboundedSender<AppEvent>,
    pub agent_handle: Option<tokio::task::JoinHandle<()>>,
}

impl App {
    pub fn new(config: &Config, event_tx: mpsc::UnboundedSender<AppEvent>) -> Self {
        let cwd = std::env::current_dir()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let mode = if config.yolo {
            AppMode::Yolo
        } else {
            AppMode::Normal
        };

        let session = Session::new(&cwd, &config.model);
        let session_id = session.short_id().to_string();

        let messages = Vec::new();

        let theme = std::env::var("FUSION_THEME")
            .or_else(|_| std::env::var("ZENCODE_THEME"))
            .unwrap_or_else(|_| {
                config.settings.get("theme")
                    .and_then(|v| v.as_str())
                    .unwrap_or("light")
                    .to_string()
            });

        Self {
            messages,
            input: String::new(),
            mode,
            model: config.model.clone(),
            token_level: TokenLevel::Normal,
            is_thinking: false,
            should_quit: false,
            session_id,
            theme,
            autocomplete_visible: false,
            autocomplete_mode: AutocompleteMode::Commands,
            autocomplete_items: Vec::new(),
            autocomplete_selected: 0,
            pending_model: None,
            turn_start: None,
            thought_duration: None,
            had_thinking: false,
            tick_count: 0,
            scroll_offset: Cell::new(0),
            auto_scroll: Cell::new(true),
            last_key_time: None,
            in_paste_burst: false,
            submitted_text: None,
            editor_requested: None,
            message_cache: RefCell::new(None),
            queued_prompts: Vec::new(),
            agent: Arc::new(Mutex::new(Agent::new(config, cwd))),
            session,
            event_tx,
            agent_handle: None,
        }
    }

    pub fn from_session(
        config: &Config,
        session: Session,
        event_tx: mpsc::UnboundedSender<AppEvent>,
    ) -> Self {
        let cwd = session.cwd.clone();
        let session_id = session.short_id().to_string();

        let mut messages = vec![Message {
            role: "system".to_string(),
            content: format!(
                "fusion — resumed session {}  │  {} messages  │  /help for commands",
                session_id,
                session.messages.len()
            ),
        }];

        for msg in &session.messages {
            messages.push(Message {
                role: msg.role.clone(),
                content: msg.content.clone(),
            });
        }

        let mode = if config.yolo {
            AppMode::Yolo
        } else {
            AppMode::Normal
        };

        let theme = std::env::var("FUSION_THEME")
            .or_else(|_| std::env::var("ZENCODE_THEME"))
            .unwrap_or_else(|_| {
                config.settings.get("theme")
                    .and_then(|v| v.as_str())
                    .unwrap_or("light")
                    .to_string()
            });

        Self {
            messages,
            input: String::new(),
            mode,
            model: config.model.clone(),
            token_level: TokenLevel::Normal,
            is_thinking: false,
            should_quit: false,
            session_id,
            theme,
            autocomplete_visible: false,
            autocomplete_mode: AutocompleteMode::Commands,
            autocomplete_items: Vec::new(),
            autocomplete_selected: 0,
            pending_model: None,
            turn_start: None,
            thought_duration: None,
            had_thinking: false,
            tick_count: 0,
            scroll_offset: Cell::new(0),
            auto_scroll: Cell::new(true),
            last_key_time: None,
            in_paste_burst: false,
            submitted_text: None,
            editor_requested: None,
            message_cache: RefCell::new(None),
            queued_prompts: Vec::new(),
            agent: Arc::new(Mutex::new(Agent::new(config, cwd))),
            session,
            event_tx,
            agent_handle: None,
        }
    }

    pub fn handle_paste(&mut self, text: String) {
        if let Some(path) = try_parse_image_path(&text) {
            let filename = path.file_name().unwrap_or_default().to_string_lossy();
            self.messages.push(Message {
                role: "system".to_string(),
                content: format!("Detected image path: ./{} (attached).", filename),
            });
            let link = format!(" [image](file://{})", path.to_string_lossy());
            self.input.push_str(&link);
            self.update_autocomplete();
            return;
        }

        let cleaned = text.replace('\r', "").replace('\n', " ");
        let truncated: String = cleaned.chars().take(2000).collect();
        self.input.push_str(&truncated);
        self.update_autocomplete();
    }

    /// Handle a mouse event.
    pub fn handle_mouse(&mut self, mouse: crossterm::event::MouseEvent) {
        match mouse.kind {
            crossterm::event::MouseEventKind::ScrollUp => {
                self.auto_scroll.set(false);
                self.scroll_offset.set(self.scroll_offset.get().saturating_sub(3));
            }
            crossterm::event::MouseEventKind::ScrollDown => {
                self.auto_scroll.set(false);
                self.scroll_offset.set(self.scroll_offset.get().saturating_add(3));
            }
            _ => {}
        }
    }

    /// Handle a key event.
    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) {
        let now = Instant::now();
        let is_paste_like = if let Some(last_time) = self.last_key_time {
            now.duration_since(last_time) < std::time::Duration::from_millis(15)
        } else {
            false
        };

        if is_paste_like {
            self.in_paste_burst = true;
        } else {
            // Only reset if enough time has passed since last key
            self.in_paste_burst = false;
        }

        self.last_key_time = Some(now);

        if self.is_thinking {
            if key.code == KeyCode::Esc {
                if let Some(handle) = self.agent_handle.take() {
                    handle.abort();
                }
                self.is_thinking = false;
                self.remove_thinking();
                self.queued_prompts.clear();
                self.messages.push(Message {
                    role: "system".to_string(),
                    content: "Execution interrupted by user. Queue cleared.".to_string(),
                });
                if let Some(text) = self.submitted_text.take() {
                    self.input = text;
                }
                self.save_session();
                return;
            } else if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c') {
                self.save_session();
                self.should_quit = true;
                return;
            } else if key.code == KeyCode::Enter {
                let text = self.input.trim().to_string();
                if !text.is_empty() {
                    self.queued_prompts.push(text);
                    self.input.clear();
                    self.messages.push(Message {
                        role: "system".to_string(),
                        content: format!("Prompt queued (#{}).", self.queued_prompts.len()),
                    });
                }
                return;
            }
        }

        if self.autocomplete_visible {
            match (key.modifiers, key.code) {
                (_, KeyCode::Up) => {
                    if self.autocomplete_selected > 0 {
                        self.autocomplete_selected -= 1;
                    }
                    return;
                }
                (_, KeyCode::Down) => {
                    if self.autocomplete_selected + 1 < self.autocomplete_items.len() {
                        self.autocomplete_selected += 1;
                    }
                    return;
                }
                (_, KeyCode::Tab) | (_, KeyCode::Enter) => {
                    self.accept_autocomplete();
                    return;
                }
                (_, KeyCode::Esc) => {
                    self.close_autocomplete();
                    return;
                }
                _ => {
                    // Fall through to normal input, then update
                }
            }
        }

        match (key.modifiers, key.code) {
            (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                self.save_session();
                self.should_quit = true;
            }
            (KeyModifiers::CONTROL, KeyCode::Char('v')) => {
                let cwd = self.session.cwd.clone();
                match crate::clipboard::save_clipboard_image(&cwd) {
                    Ok(path) => {
                        let filename = path.file_name().unwrap_or_default().to_string_lossy();
                        self.messages.push(Message {
                            role: "system".to_string(),
                            content: format!("Saved clipboard image to ./{} and appended link.", filename),
                        });
                        let link = format!(" [image](file://{})", path.to_string_lossy());
                        self.input.push_str(&link);
                    }
                    Err(_) => {
                        if let Ok(text) = crate::clipboard::get_clipboard_text() {
                            self.handle_paste(text);
                        }
                    }
                }
            }
            (KeyModifiers::CONTROL, KeyCode::Char('e')) => {
                self.editor_requested = Some(self.input.clone());
            }
            (_, KeyCode::Up) => {
                self.auto_scroll.set(false);
                self.scroll_offset.set(self.scroll_offset.get().saturating_sub(1));
            }
            (_, KeyCode::Down) => {
                self.auto_scroll.set(false);
                self.scroll_offset.set(self.scroll_offset.get().saturating_add(1));
            }
            (_, KeyCode::PageUp) => {
                self.auto_scroll.set(false);
                self.scroll_offset.set(self.scroll_offset.get().saturating_sub(10));
            }
            (_, KeyCode::PageDown) => {
                self.auto_scroll.set(false);
                self.scroll_offset.set(self.scroll_offset.get().saturating_add(10));
            }
            (_, KeyCode::BackTab) => {
                // Shift+Tab cycles modes: Normal -> Plan -> Yolo
                self.mode = match self.mode {
                    AppMode::Normal => AppMode::Plan,
                    AppMode::Plan => AppMode::Yolo,
                    AppMode::Yolo => AppMode::Normal,
                };
                let new_mode = match self.mode {
                    AppMode::Normal => "Normal",
                    AppMode::Plan => "Plan (exploring only)",
                    AppMode::Yolo => "Always-Approve (YOLO)",
                };
                self.messages.push(Message {
                    role: "system".to_string(),
                    content: format!("Mode switched to: {}", new_mode),
                });
            }
            (_, KeyCode::Enter) => {
                if self.in_paste_burst || is_paste_like {
                    // Suppress submission during paste bursts. Replace with a space.
                    self.input.push(' ');
                    self.update_autocomplete();
                    return;
                }
                self.close_autocomplete();
                let text = self.input.trim().to_string();
                if !text.is_empty() {
                    if text == "/image" || text.starts_with("/image ") {
                        let cwd = self.session.cwd.clone();
                        match crate::clipboard::save_clipboard_image(&cwd) {
                            Ok(path) => {
                                let filename = path.file_name().unwrap_or_default().to_string_lossy();
                                self.messages.push(Message {
                                    role: "system".to_string(),
                                    content: format!("Saved clipboard image to ./{} and appended link.", filename),
                                });
                                let link = format!(" [image](file://{})", path.to_string_lossy());
                                self.input.push_str(&link);
                            }
                            Err(e) => {
                                self.messages.push(Message {
                                    role: "system".to_string(),
                                    content: format!("Error saving image: {}", e),
                                });
                            }
                        }
                    } else {
                        self.input.clear();
                        self.auto_scroll.set(true);
                        self.handle_submit(text);
                    }
                }
            }
            (_, KeyCode::Char(c)) => {
                self.input.push(c);
                self.update_autocomplete();
            }
            (_, KeyCode::Backspace) => {
                self.input.pop();
                // Navigate back through stages on backspace
                if self.autocomplete_mode == AutocompleteMode::Effort {
                    // Go back to model picker
                    self.autocomplete_mode = AutocompleteMode::Models;
                    self.pending_model = None;
                    self.input = "/model ".to_string();
                } else if self.autocomplete_mode == AutocompleteMode::Models
                    && !self.input.starts_with("/model ")
                {
                    self.autocomplete_mode = AutocompleteMode::Commands;
                }
                self.update_autocomplete();
            }
            (_, KeyCode::Esc) => {
                self.close_autocomplete();
            }
            _ => {}
        }
    }

    fn close_autocomplete(&mut self) {
        self.autocomplete_visible = false;
        self.autocomplete_items.clear();
        self.autocomplete_mode = AutocompleteMode::Commands;
    }

    /// Accept the currently selected autocomplete item.
    fn accept_autocomplete(&mut self) {
        let selected_idx = self.autocomplete_selected;

        match self.autocomplete_mode {
            AutocompleteMode::Commands => {
                if let Some(item) = self.autocomplete_items.get(selected_idx) {
                    let label = item.label.clone();

                    // Check if this command has a submenu
                    let has_submenu = SLASH_COMMANDS
                        .iter()
                        .find(|c| c.name == label)
                        .map(|c| c.has_submenu)
                        .unwrap_or(false);

                    if has_submenu && label == "/model" {
                        // Transition to model picker
                        self.input = "/model ".to_string();
                        self.autocomplete_mode = AutocompleteMode::Models;
                        self.show_model_picker();
                    } else {
                        // Execute the command directly
                        self.input.clear();
                        self.close_autocomplete();
                        self.handle_submit(label);
                    }
                }
            }
            AutocompleteMode::Models => {
                if let Some(item) = self.autocomplete_items.get(selected_idx) {
                    let model_label = item.label.clone();
                    if let Some(info) = lookup_model(&model_label) {
                        // Check if model has multiple token levels
                        let has_levels = info.max_tokens_high.is_some()
                            || info.max_tokens_max.is_some();
                        if has_levels {
                            // Show effort picker as third stage
                            self.pending_model = Some(info.shorthand.to_string());
                            self.autocomplete_mode = AutocompleteMode::Effort;
                            self.input = format!("/model {} ", info.display_name);
                            self.show_effort_picker(info);
                        } else {
                            // No levels — apply immediately
                            self.input.clear();
                            self.close_autocomplete();
                            self.handle_submit(format!("/model {}", info.shorthand));
                        }
                    } else {
                        self.input.clear();
                        self.close_autocomplete();
                        self.handle_submit(format!("/model {}", model_label));
                    }
                }
            }
            AutocompleteMode::Effort => {
                if let Some(item) = self.autocomplete_items.get(selected_idx) {
                    let level_label = item.label.to_lowercase();
                    let model_name = self.pending_model.take().unwrap_or_default();
                    self.input.clear();
                    self.close_autocomplete();

                    // Apply model
                    self.handle_submit(format!("/model {}", model_name));

                    // Apply effort level
                    let level = match level_label.as_str() {
                        "max" => "/max",
                        "high" => "/high",
                        _ => "/normal",
                    };
                    self.handle_submit(level.to_string());
                }
            }
        }
    }

    /// Show the model picker popup.
    fn show_model_picker(&mut self) {
        let filter = if self.input.starts_with("/model ") {
            self.input[7..].to_lowercase()
        } else {
            String::new()
        };

        let items: Vec<AutocompleteItem> = CLOUDFLARE_MODELS
            .iter()
            .filter(|m| {
                if filter.is_empty() {
                    true
                } else {
                    m.shorthand.contains(&filter)
                        || m.display_name.to_lowercase().contains(&filter)
                }
            })
            .map(|m| {
                let is_current = self.model == m.full_id || self.model == m.shorthand;
                AutocompleteItem {
                    label: m.shorthand.to_string(),
                    description: format!(
                        "{} - {}",
                        m.display_name, m.category
                    ),
                    is_current,
                }
            })
            .collect();

        self.autocomplete_items = items;
        self.autocomplete_visible = !self.autocomplete_items.is_empty();
        // Pre-select current model if any
        self.autocomplete_selected = self
            .autocomplete_items
            .iter()
            .position(|i| i.is_current)
            .unwrap_or(0);
    }

    /// Update autocomplete based on current input.
    fn update_autocomplete(&mut self) {
        match self.autocomplete_mode {
            AutocompleteMode::Commands => {
                if self.input.starts_with("/model ") {
                    self.autocomplete_mode = AutocompleteMode::Models;
                    self.show_model_picker();
                } else if self.input.starts_with('/') && !self.input.contains(' ') {
                    let prefix = self.input.to_lowercase();
                    self.autocomplete_items = SLASH_COMMANDS
                        .iter()
                        .filter(|cmd| cmd.name.starts_with(&prefix))
                        .map(|cmd| AutocompleteItem {
                            label: cmd.name.to_string(),
                            description: cmd.description.to_string(),
                            is_current: false,
                        })
                        .collect();
                    self.autocomplete_visible = !self.autocomplete_items.is_empty();
                    self.autocomplete_selected = 0;
                } else {
                    self.autocomplete_visible = false;
                    self.autocomplete_items.clear();
                }
            }
            AutocompleteMode::Models => {
                // Filter model list as user types after "/model "
                self.show_model_picker();
            }
            AutocompleteMode::Effort => {
                // Effort picker doesn't need filtering — it's a short fixed list
            }
        }
    }

    /// Show the effort/token level picker for a specific model.
    fn show_effort_picker(&mut self, info: &fusion_core::models::ModelInfo) {
        let mut items = Vec::new();

        if let Some(tokens) = info.max_tokens_normal {
            items.push(AutocompleteItem {
                label: "Normal".to_string(),
                description: format!("{} tokens · default output", tokens),
                is_current: self.token_level == TokenLevel::Normal,
            });
        }
        if let Some(tokens) = info.max_tokens_high {
            items.push(AutocompleteItem {
                label: "High".to_string(),
                description: format!("{} tokens · extended output", tokens),
                is_current: self.token_level == TokenLevel::High,
            });
        }
        if let Some(tokens) = info.max_tokens_max {
            items.push(AutocompleteItem {
                label: "Max".to_string(),
                description: format!("{} tokens · maximum output", tokens),
                is_current: self.token_level == TokenLevel::Max,
            });
        }

        self.autocomplete_items = items;
        self.autocomplete_visible = !self.autocomplete_items.is_empty();
        // Pre-select current level
        self.autocomplete_selected = self
            .autocomplete_items
            .iter()
            .position(|i| i.is_current)
            .unwrap_or(0);
    }

    fn handle_submit(&mut self, text: String) {
        if text == "/edit" || text.starts_with("/edit ") {
            let seed = if text.starts_with("/edit ") {
                text.strip_prefix("/edit ").unwrap_or("").to_string()
            } else {
                String::new()
            };
            self.editor_requested = Some(seed);
            return;
        }

        if text.starts_with('/') {
            self.handle_slash(&text);
            return;
        }

        self.submitted_text = Some(text.clone());

        self.messages.push(Message {
            role: "user".to_string(),
            content: text.clone(),
        });

        self.session.push_message("user", &text);
        self.save_session();
        self.is_thinking = true;
        self.turn_start = Some(Instant::now());
        self.thought_duration = None;
        self.had_thinking = false;

        let tx = self.event_tx.clone();
        let agent = Arc::clone(&self.agent);

        let handle = tokio::spawn(async move {
            let mut agent = agent.lock().await;
            let (agent_tx, mut agent_rx) = tokio::sync::mpsc::unbounded_channel();
            
            let tx_clone = tx.clone();
            let forwarder = tokio::spawn(async move {
                while let Some(event) = agent_rx.recv().await {
                    let _ = tx_clone.send(AppEvent::Agent(event));
                }
            });

            if let Err(e) = agent.process(&text, agent_tx).await {
                let _ = tx.send(AppEvent::Agent(AgentEvent::FinalResponse(format!(
                    "Error: {}",
                    e
                ))));
            }
            let _ = forwarder.await;
        });
        self.agent_handle = Some(handle);
    }

    pub fn handle_agent_event(&mut self, event: AgentEvent) {
        match event {
            AgentEvent::Thinking(text) => {
                self.had_thinking = true;
                if self.thought_duration.is_none() {
                    if let Some(start) = self.turn_start {
                        self.thought_duration = Some(start.elapsed().as_secs_f64());
                    }
                }

                if let Some(last) = self.messages.last_mut() {
                    if last.role == "thinking" {
                        last.content.push_str(&text);
                        return;
                    }
                }
                self.messages.push(Message {
                    role: "thinking".to_string(),
                    content: text,
                });
            }
            AgentEvent::TextDelta(text) => {
                if let Some(last) = self.messages.last_mut() {
                    if last.role == "assistant" {
                        last.content.push_str(&text);
                        return;
                    }
                }
                self.messages.push(Message {
                    role: "assistant".to_string(),
                    content: text,
                });
            }
            AgentEvent::ToolCall { name, args_preview } => {
                if self.thought_duration.is_none() {
                    if let Some(start) = self.turn_start {
                        self.thought_duration = Some(start.elapsed().as_secs_f64());
                    }
                }

                self.remove_thinking();
                let content = format!("[tool] {} {}", name, args_preview);
                self.messages.push(Message {
                    role: "tool".to_string(),
                    content: content.clone(),
                });
                self.session.push_message("tool", &content);
            }
            AgentEvent::ToolResult { name, output } => {
                let cleaned = clean_output(&output);
                let truncated = if cleaned.chars().count() > 4000 {
                    let truncated_str: String = cleaned.chars().take(4000).collect();
                    format!("{}…\n[output truncated — 4000 chars max]", truncated_str)
                } else {
                    cleaned
                };
                self.messages.push(Message {
                    role: "tool_result".to_string(),
                    content: format!("  ↳ {} → {}", name, truncated),
                });
            }
            AgentEvent::FinalResponse(text) => {
                if self.thought_duration.is_none() {
                    if let Some(start) = self.turn_start {
                        self.thought_duration = Some(start.elapsed().as_secs_f64());
                    }
                }

                let turn_duration = self.turn_start
                    .map(|t| t.elapsed().as_secs_f64())
                    .unwrap_or(0.0);

                let thought_duration = if self.had_thinking {
                    self.thought_duration.unwrap_or(0.0)
                } else {
                    0.0
                };

                self.is_thinking = false;
                self.agent_handle = None;
                self.submitted_text = None;
                self.remove_thinking();

                // Show "Thought for Xs" (actual reasoning duration or 0.0s)
                self.messages.push(Message {
                    role: "thought_time".to_string(),
                    content: format!("{:.1}s", thought_duration),
                });

                let mut updated = false;
                if let Some(last_assistant) = self.messages.iter_mut().rev().find(|m| m.role == "assistant") {
                    last_assistant.content = text.clone();
                    updated = true;
                }
                if !updated {
                    self.messages.push(Message {
                        role: "assistant".to_string(),
                        content: text.clone(),
                    });
                }

                // Show "Turn completed in Xs"
                self.messages.push(Message {
                    role: "turn_time".to_string(),
                    content: format!("{:.1}s", turn_duration),
                });

                self.turn_start = None;
                self.session.push_message("assistant", &text);
                self.save_session();

                // Trigger desktop notification
                let summary = if text.starts_with("Error:") {
                    "Session error encountered."
                } else {
                    "Session completed successfully."
                };
                trigger_desktop_notification("Fusion Coder", summary);

                // Process the next queued prompt if any!
                if !self.queued_prompts.is_empty() {
                    let next_prompt = self.queued_prompts.remove(0);
                    self.handle_submit(next_prompt);
                }
            }
            AgentEvent::TodoUpdate(todos) => {
                let list: String = todos
                    .iter()
                    .map(|t| {
                        let icon = match t.status.as_str() {
                            "done" => "✓",
                            "in_progress" => "→",
                            _ => "○",
                        };
                        format!("  {} {}", icon, t.content)
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                self.messages.push(Message {
                    role: "system".to_string(),
                    content: format!("Todos:\n{}", list),
                });
            }
        }
    }

    fn remove_thinking(&mut self) {
        self.messages.retain(|m| m.role != "thinking");
    }

    fn save_session(&self) {
        if let Err(e) = self.session.save() {
            eprintln!("session save error: {}", e);
        }
    }

    fn handle_slash(&mut self, cmd: &str) {
        let lower = cmd.to_lowercase();
        let parts: Vec<&str> = lower.split_whitespace().collect();
        let base = parts.first().copied().unwrap_or("");

        match base {
            "/help" | "/h" | "/?" => {
                self.messages.push(Message {
                    role: "system".to_string(),
                    content: "Fusion commands:\n  /help       show all commands\n  /yolo       toggle auto-approve\n  /plan       enter plan mode\n  /model <n>  switch model (or use autocomplete)\n  /models     list available models\n  /max        set maximum token output\n  /high       set high token output\n  /normal     set normal token output\n  /status     current settings\n  /theme      toggle light/dark theme\n  /image      insert clipboard image (macOS)\n  /session    show session ID\n  /sessions   list saved sessions\n  /clear      clear messages\n  /dq [n]     clear queue or dequeue item n\n  /quit       quit (session auto-saved)".to_string(),
                });
            }
            "/yolo" => {
                self.mode = if self.mode == AppMode::Yolo {
                    AppMode::Normal
                } else {
                    AppMode::Yolo
                };
                let msg = if self.mode == AppMode::Yolo {
                    "YOLO mode ON - all tool actions auto-approved"
                } else {
                    "YOLO mode OFF"
                };
                self.messages.push(Message {
                    role: "system".to_string(),
                    content: msg.to_string(),
                });
            }
            "/plan" => {
                self.mode = AppMode::Plan;
                self.messages.push(Message {
                    role: "system".to_string(),
                    content: "Plan mode: agent will explore but not edit until you approve.".to_string(),
                });
            }
            "/max" => self.set_token_level(TokenLevel::Max),
            "/high" => self.set_token_level(TokenLevel::High),
            "/normal" => self.set_token_level(TokenLevel::Normal),
            "/models" => {
                let mut lines = vec!["Available models:".to_string()];
                let models = list_models(None);
                for m in &models {
                    let levels: Vec<&str> = [
                        m.max_tokens_normal.map(|_| "normal"),
                        m.max_tokens_high.map(|_| "high"),
                        m.max_tokens_max.map(|_| "max"),
                    ]
                    .iter()
                    .filter_map(|x| *x)
                    .collect();
                    let current = if self.model == m.full_id { " *" } else { "" };
                    lines.push(format!(
                        "  {:<14} {} [{}]{} ({})",
                        m.shorthand, m.display_name, levels.join(","),
                        current, m.category
                    ));
                }
                lines.push(String::new());
                lines.push("Switch: /model <shorthand> or use autocomplete".to_string());
                self.messages.push(Message {
                    role: "system".to_string(),
                    content: lines.join("\n"),
                });
            }
            "/clear" => {
                self.messages.clear();
                self.messages.push(Message {
                    role: "system".to_string(),
                    content: "Cleared.".to_string(),
                });
            }
            "/dq" | "/dequeue" => {
                if let Some(arg) = parts.get(1) {
                    if let Ok(idx) = arg.parse::<usize>() {
                        if idx > 0 && idx <= self.queued_prompts.len() {
                            let removed = self.queued_prompts.remove(idx - 1);
                            self.messages.push(Message {
                                role: "system".to_string(),
                                content: format!("Removed from queue: \"{}\"", removed),
                            });
                        } else {
                            self.messages.push(Message {
                                role: "error".to_string(),
                                content: format!("Invalid queue index: {}", idx),
                            });
                        }
                    } else {
                        self.messages.push(Message {
                            role: "error".to_string(),
                            content: format!("Usage: /dq <number>"),
                        });
                    }
                } else {
                    self.queued_prompts.clear();
                    self.messages.push(Message {
                        role: "system".to_string(),
                        content: "Cleared all queued prompts.".to_string(),
                    });
                }
            }
            "/session" | "/sid" => {
                self.messages.push(Message {
                    role: "system".to_string(),
                    content: format!(
                        "Session: {}\nMessages: {}\nResume: fusion --resume {}",
                        self.session.id,
                        self.session.messages.len(),
                        self.session.short_id()
                    ),
                });
            }
            "/sessions" | "/history" => {
                match list_sessions() {
                    Ok(sessions) => {
                        if sessions.is_empty() {
                            self.messages.push(Message {
                                role: "system".to_string(),
                                content: "No saved sessions.".to_string(),
                            });
                        } else {
                            let mut lines = vec!["Recent sessions:".to_string()];
                            for (i, s) in sessions.iter().take(10).enumerate() {
                                let age = format_age(s.updated_at);
                                lines.push(format!(
                                    "  {}. {} ({} msgs, {}) {}",
                                    i + 1,
                                    &s.id[..8.min(s.id.len())],
                                    s.message_count, age, s.preview
                                ));
                            }
                            lines.push(String::new());
                            lines.push("Resume: fusion --resume <id>".to_string());
                            self.messages.push(Message {
                                role: "system".to_string(),
                                content: lines.join("\n"),
                            });
                        }
                    }
                    Err(e) => {
                        self.messages.push(Message {
                            role: "error".to_string(),
                            content: format!("Failed to list sessions: {}", e),
                        });
                    }
                }
            }
            "/status" => {
                let mode_str = match self.mode {
                    AppMode::Normal => "Normal",
                    AppMode::Plan => "Plan",
                    AppMode::Yolo => "YOLO",
                };
                let level_info = match lookup_model(&self.model) {
                    Some(info) => {
                        let max_tok = info
                            .max_tokens_for(self.token_level)
                            .map(|v| format!("{}", v))
                            .unwrap_or_else(|| "default".to_string());
                        format!("  tokens: {} ({})", self.token_level, max_tok)
                    }
                    None => format!("  tokens: {}", self.token_level),
                };
                self.messages.push(Message {
                    role: "system".to_string(),
                    content: format!(
                        "model: {}  mode: {}  session: {}\n{}",
                        self.model, mode_str, self.session_id, level_info
                    ),
                });
            }
            "/exit" | "/quit" | "/q" => {
                self.save_session();
                self.should_quit = true;
            }
            "/model" => {
                if let Some(name) = parts.get(1) {
                    let mut resolved_model = name.to_string();
                    if let Some(info) = lookup_model(name) {
                        resolved_model = info.full_id.to_string();
                        self.model = info.full_id.to_string();
                        self.messages.push(Message {
                            role: "system".to_string(),
                            content: format!(
                                "model: {} ({}), tokens: {}",
                                info.display_name, info.full_id, self.token_level
                            ),
                        });
                    } else {
                        self.model = name.to_string();
                        self.messages.push(Message {
                            role: "system".to_string(),
                            content: format!("Model → {}", name),
                        });
                    }

                    self.session.model = resolved_model.clone();
                    self.save_session();

                    // Update active agent model mid-session!
                    let agent = Arc::clone(&self.agent);
                    tokio::spawn(async move {
                        let mut agent = agent.lock().await;
                        agent.update_model(&resolved_model);
                    });
                } else {
                    self.messages.push(Message {
                        role: "system".to_string(),
                        content: "Usage: /model <name>\nUse /models to see available.".to_string(),
                    });
                }
            }
            "/theme" | "/them" => {
                if let Some(theme_name) = parts.get(1) {
                    let cleaned = theme_name.trim().to_lowercase();
                    if cleaned == "light" || cleaned == "dark" {
                        self.theme = cleaned.clone();
                        self.messages.push(Message {
                            role: "system".to_string(),
                            content: format!("Theme switched to: {}", cleaned),
                        });
                    } else {
                        self.messages.push(Message {
                            role: "system".to_string(),
                            content: "Usage: /theme <light|dark>".to_string(),
                        });
                    }
                } else {
                    // Toggle theme
                    self.theme = if self.theme.eq_ignore_ascii_case("dark") {
                        "light".to_string()
                    } else {
                        "dark".to_string()
                    };
                    self.messages.push(Message {
                        role: "system".to_string(),
                        content: format!("Theme toggled to: {}", self.theme),
                    });
                }
            }
            _ => {
                self.messages.push(Message {
                    role: "system".to_string(),
                    content: "Unknown command. /help for list.".to_string(),
                });
            }
        }
    }

    fn set_token_level(&mut self, level: TokenLevel) {
        self.token_level = level;
        let info = lookup_model(&self.model);
        let msg = match info {
            Some(m) => {
                if let Some(tokens) = m.max_tokens_for(level) {
                    format!("Token output: {} → {} tokens", level, tokens)
                } else {
                    format!(
                        "Token output: {} (model '{}' doesn't support this level)",
                        level, m.display_name
                    )
                }
            }
            None => format!("Token output: {}", level),
        };
        self.messages.push(Message {
            role: "system".to_string(),
            content: msg,
        });
    }
}

fn format_age(epoch_secs: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let diff = now.saturating_sub(epoch_secs);
    if diff < 60 {
        "just now".to_string()
    } else if diff < 3600 {
        format!("{}m ago", diff / 60)
    } else if diff < 86400 {
        format!("{}h ago", diff / 3600)
    } else {
        format!("{}d ago", diff / 86400)
    }
}

pub async fn run_tui(config: &Config) -> anyhow::Result<()> {
    run_tui_with_session(config, None).await
}

pub async fn run_tui_with_session(
    config: &Config,
    resume_session: Option<Session>,
) -> anyhow::Result<()> {
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    crossterm::execute!(
        stdout,
        crossterm::terminal::EnterAlternateScreen,
        crossterm::event::EnableMouseCapture,
        crossterm::event::EnableBracketedPaste
    )?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    let (mut event_handler, event_tx) = EventHandler::new(100);
    let mut app = match resume_session {
        Some(session) => App::from_session(config, session, event_tx),
        None => App::new(config, event_tx),
    };

    loop {
        terminal.draw(|frame| ui::draw(frame, &app))?;

        if let Some(event) = event_handler.next().await {
            match event {
                AppEvent::Key(key) => app.handle_key(key),
                AppEvent::Mouse(mouse) => app.handle_mouse(mouse),
                AppEvent::Agent(agent_event) => app.handle_agent_event(agent_event),
                AppEvent::Resize(_, _) => {}
                AppEvent::Paste(text) => {
                    if !app.is_thinking {
                        app.handle_paste(text);
                    }
                }
                AppEvent::Tick => {
                    app.tick_count = app.tick_count.wrapping_add(1);
                }
            }
        }

        if let Some(seed) = app.editor_requested.take() {
            crossterm::terminal::disable_raw_mode()?;
            crossterm::execute!(
                std::io::stdout(),
                crossterm::terminal::LeaveAlternateScreen,
                crossterm::event::DisableMouseCapture,
                crossterm::event::DisableBracketedPaste
            )?;

            let result = crate::clipboard::edit_text_in_editor(&seed);

            crossterm::terminal::enable_raw_mode()?;
            crossterm::execute!(
                std::io::stdout(),
                crossterm::terminal::EnterAlternateScreen,
                crossterm::event::EnableMouseCapture,
                crossterm::event::EnableBracketedPaste
            )?;
            terminal.clear()?;

            match result {
                Ok(new_text) => {
                    app.input = new_text.trim_end().to_string();
                }
                Err(e) => {
                    app.messages.push(Message {
                        role: "system".to_string(),
                        content: format!("Error running editor: {}", e),
                    });
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        std::io::stdout(),
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture,
        crossterm::event::DisableBracketedPaste
    )?;
    terminal.show_cursor()?;

    Ok(())
}

#[cfg(target_os = "macos")]
fn trigger_desktop_notification(title: &str, message: &str) {
    let script = format!(
        "display notification \"{}\" with title \"{}\" sound name \"Glass\"",
        message.replace('\"', "\\\""),
        title.replace('\"', "\\\"")
    );
    let _ = std::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .spawn();
}

#[cfg(target_os = "linux")]
fn trigger_desktop_notification(title: &str, message: &str) {
    let _ = std::process::Command::new("notify-send")
        .arg(title)
        .arg(message)
        .spawn();
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn trigger_desktop_notification(_title: &str, _message: &str) {
    // No-op fallback
}

fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\x1b' {
            i += 1;
            if i < chars.len() && chars[i] == '[' {
                i += 1;
                while i < chars.len() {
                    let c = chars[i];
                    i += 1;
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        result.push(chars[i]);
        i += 1;
    }
    result
}

fn clean_output(s: &str) -> String {
    let stripped = strip_ansi(s);
    let mut lines = Vec::new();
    for line in stripped.lines() {
        if let Some(pos) = line.rfind('\r') {
            lines.push(line[pos + 1..].to_string());
        } else {
            lines.push(line.to_string());
        }
    }
    lines.join("\n")
}

fn try_parse_image_path(text: &str) -> Option<std::path::PathBuf> {
    let trimmed = text.trim();
    let unquoted = trimmed
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .or_else(|| trimmed.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
        .unwrap_or(trimmed);

    // Also support file:// URL
    let path_str = if unquoted.starts_with("file://") {
        unquoted.strip_prefix("file://").unwrap_or(unquoted)
    } else {
        unquoted
    };

    let path = std::path::PathBuf::from(path_str);
    if path.exists() && path.is_file() {
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let ext_lower = ext.to_lowercase();
            if ext_lower == "png"
                || ext_lower == "jpg"
                || ext_lower == "jpeg"
                || ext_lower == "webp"
                || ext_lower == "gif"
            {
                return Some(path);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    #[test]
    fn test_try_parse_image_path() {
        let temp_dir = std::env::temp_dir();
        
        // Test non-existent file
        let non_existent = temp_dir.join("non_existent_123456.png");
        assert!(try_parse_image_path(&non_existent.to_string_lossy()).is_none());

        // Test valid image file
        let img_path = temp_dir.join("test_image_123456.png");
        let _ = File::create(&img_path);
        let parsed = try_parse_image_path(&img_path.to_string_lossy());
        assert!(parsed.is_some());
        assert_eq!(parsed.unwrap(), img_path);

        // Test valid file with different extension (not image)
        let txt_path = temp_dir.join("test_text_123456.txt");
        let _ = File::create(&txt_path);
        assert!(try_parse_image_path(&txt_path.to_string_lossy()).is_none());

        // Test quoted path
        let quoted = format!("\"{}\"", img_path.to_string_lossy());
        let parsed_quoted = try_parse_image_path(&quoted);
        assert!(parsed_quoted.is_some());
        assert_eq!(parsed_quoted.unwrap(), img_path);

        // Test file:// URL format
        let url_format = format!("file://{}", img_path.to_string_lossy());
        let parsed_url = try_parse_image_path(&url_format);
        assert!(parsed_url.is_some());
        assert_eq!(parsed_url.unwrap(), img_path);

        // Cleanup
        let _ = std::fs::remove_file(img_path);
        let _ = std::fs::remove_file(txt_path);
    }

    #[test]
    fn test_task_queue() {
        let config = Config {
            model: "test-model".to_string(),
            small_model: None,
            yolo: false,
            provider: fusion_core::config::Provider::Auto,
            cloudflare_account_id: None,
            api_key: String::new(),
            base_url: String::new(),
            config_path: None,
            settings: std::collections::HashMap::new(),
        };
        let (event_tx, _event_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(&config, event_tx);

        assert!(app.queued_prompts.is_empty());

        // Queue a prompt while is_thinking is true
        app.is_thinking = true;
        app.input = "first prompt".to_string();
        let enter_key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::empty(),
        );
        app.handle_key(enter_key);

        assert!(app.input.is_empty());
        assert_eq!(app.queued_prompts.len(), 1);
        assert_eq!(app.queued_prompts[0], "first prompt");

        // Queue a second prompt
        app.input = "second prompt".to_string();
        app.handle_key(enter_key);
        assert_eq!(app.queued_prompts.len(), 2);

        // Test /dq 1 (remove first item)
        app.handle_slash("/dq 1");
        assert_eq!(app.queued_prompts.len(), 1);
        assert_eq!(app.queued_prompts[0], "second prompt");

        // Test /dq (clear all remaining)
        app.handle_slash("/dq");
        assert!(app.queued_prompts.is_empty());
    }
}
