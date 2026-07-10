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

/// A parsed question and selectable recommended options for Grill Mode.
#[derive(Debug, Clone, PartialEq)]
pub struct GrillQuestion {
    pub title: String,
    pub options: Vec<String>,
    pub selected: usize,
}

pub const SLASH_COMMANDS: &[SlashCommand] = &[
    SlashCommand { name: "/help", description: "Show all commands", has_submenu: false },
    SlashCommand { name: "/yolo", description: "Toggle auto-approve mode", has_submenu: false },
    SlashCommand { name: "/plan", description: "Enter plan mode", has_submenu: false },
    SlashCommand { name: "/model", description: "Switch the active model", has_submenu: true },
    SlashCommand { name: "/max", description: "Set output to maximum tokens", has_submenu: false },
    SlashCommand { name: "/high", description: "Set output to high tokens", has_submenu: false },
    SlashCommand { name: "/normal", description: "Set output to normal tokens", has_submenu: false },
    SlashCommand { name: "/key", description: "Set provider API key (saved to config)", has_submenu: false },
    SlashCommand { name: "/providers", description: "Configure provider & API key", has_submenu: true },
    SlashCommand { name: "/status", description: "Show current settings", has_submenu: false },
    SlashCommand { name: "/session", description: "Show current session ID", has_submenu: false },
    SlashCommand { name: "/sessions", description: "List saved sessions", has_submenu: false },
    SlashCommand { name: "/clear", description: "Clear message history", has_submenu: false },
    SlashCommand { name: "/grill", description: "Toggle design interview mode", has_submenu: false },
    SlashCommand { name: "/arbitrage", description: "Toggle model token arbitrage mode", has_submenu: false },
    SlashCommand { name: "/dq", description: "Clear or remove queued prompts", has_submenu: false },
    SlashCommand { name: "/theme", description: "Toggle light/dark theme", has_submenu: false },
    SlashCommand { name: "/taste", description: "Scan and learn coding style preferences", has_submenu: false },
    SlashCommand { name: "/design", description: "Scan and learn UI/design preferences", has_submenu: false },
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
    /// @ file/folder/image picker
    Files,
    /// Provider picker (cloudflare / xai / openai)
    Providers,
    /// Key input overlay — user types their API key here
    KeyInput,
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
    /// Scroll offset for the visible window in the autocomplete popup
    pub autocomplete_scroll: usize,
    /// Tracks what user has typed after `@` for file filtering
    pub at_query: String,
    /// Buffer used when in KeyInput mode — holds the API key being typed
    pub key_buffer: String,
    /// Which provider the user selected in the Providers picker
    pub pending_provider: Option<String>,

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

    /// When Some(_), a Ctrl+C was pressed and we're waiting for a second one to confirm quit
    pub quit_pending: Option<Instant>,

    /// Attached images (file paths) for the current prompt — shown as [Image #N] tags
    pub attached_images: Vec<String>,
    /// Collapsed multi-line paste blocks — shown as [Pasted: N lines] tags
    pub pasted_blocks: Vec<String>,
    pub editor_requested: Option<String>,

    // Cache for TUI message lines to prevent typing delays: (wrap_width, messages_len, lines)
    pub message_cache: RefCell<Option<(usize, usize, Vec<Line<'static>>)>>,

    // Queued user prompts to execute sequentially
    pub queued_prompts: Vec<String>,

    // True if interactive design interview mode is enabled
    pub grill_mode: bool,

    // True if model token arbitrage mode is enabled
    pub arbitrage_mode: bool,

    pub active_grill_question: Option<GrillQuestion>,

    agent: Arc<Mutex<Agent>>,
    session: Session,
    event_tx: mpsc::UnboundedSender<AppEvent>,
    pub agent_handle: Option<tokio::task::JoinHandle<()>>,
}

fn detect_theme(config: &Config) -> String {
    if let Ok(theme) = std::env::var("FUSION_THEME") {
        return theme;
    }
    if let Ok(theme) = std::env::var("ZENCODE_THEME") {
        return theme;
    }
    if let Some(theme) = config.settings.get("theme").and_then(|v| v.as_str()) {
        return theme.to_string();
    }

    // 1. Detect via COLORFGBG environment variable (supported in many terminal emulators)
    if let Ok(colorfgbg) = std::env::var("COLORFGBG") {
        if let Some(bg) = colorfgbg.split(';').last() {
            if let Ok(bg_num) = bg.parse::<i32>() {
                // Background colors 0-8 are typically dark; 9-15 are light.
                if (0..8).contains(&bg_num) || bg_num == 0 || bg_num == 16 {
                    return "dark".to_string();
                } else {
                    return "light".to_string();
                }
            }
        }
    }

    // 2. Default to dark on Termux (Android)
    if fusion_core::config::is_termux() {
        return "dark".to_string();
    }

    // 3. Detect macOS system theme
    #[cfg(target_os = "macos")]
    {
        // On macOS in Dark mode: `defaults read -g AppleInterfaceStyle` prints "Dark" (exit 0)
        // On macOS in Light mode: the key doesn't exist, so the command exits non-zero
        match std::process::Command::new("defaults")
            .args(["read", "-g", "AppleInterfaceStyle"])
            .output()
        {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if stdout.trim().eq_ignore_ascii_case("Dark") {
                    return "dark".to_string();
                } else {
                    return "light".to_string();
                }
            }
            _ => {
                // Command failed = system is in Light mode
                return "light".to_string();
            }
        }
    }

    // Default fallback — prefer light (dark text on unknown bg is always readable)
    #[allow(unreachable_code)]
    "light".to_string()
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

        let theme = detect_theme(config);

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
            autocomplete_scroll: 0,
            at_query: String::new(),
            key_buffer: String::new(),
            pending_provider: None,
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
            attached_images: Vec::new(),
            pasted_blocks: Vec::new(),
            message_cache: RefCell::new(None),
            queued_prompts: Vec::new(),
            grill_mode: false,
            arbitrage_mode: false,
            active_grill_question: None,
            quit_pending: None,
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

        let theme = detect_theme(config);

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
            autocomplete_scroll: 0,
            at_query: String::new(),
            key_buffer: String::new(),
            pending_provider: None,
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
            attached_images: Vec::new(),
            pasted_blocks: Vec::new(),
            message_cache: RefCell::new(None),
            queued_prompts: Vec::new(),
            grill_mode: false,
            arbitrage_mode: false,
            active_grill_question: None,
            quit_pending: None,
            agent: Arc::new(Mutex::new(Agent::new(config, cwd))),
            session,
            event_tx,
            agent_handle: None,
        }
    }

    /// Strip markdown bold/italic markers from text (handles inline **bold** and *italic*)
    fn strip_markdown_formatting(s: &str) -> String {
        s.replace("**", "").replace("__", "").replace("*", "").replace("_", "")
    }

    pub fn try_parse_grill_question(text: &str) -> Option<GrillQuestion> {
        let mut options = Vec::new();
        
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            
            // Detect real bullets: • always, * only if followed by space (not **bold**), - and + followed by space
            let is_bullet = trimmed.starts_with('•')
                || (trimmed.starts_with('*') && !trimmed.starts_with("**") && trimmed.chars().nth(1) == Some(' '))
                || (trimmed.starts_with('-') && trimmed.chars().nth(1) == Some(' '))
                || (trimmed.starts_with('+') && trimmed.chars().nth(1) == Some(' '));
            let content_part = trimmed.trim_start_matches(|c| c == '*' || c == '-' || c == '+' || c == '•' || c == ' ');
            let mut has_prefix = false;
            
            if let Some(punc_pos) = content_part.find(|c| c == '.' || c == ')' || c == ']') {
                let index_str = &content_part[..punc_pos].trim();
                if !index_str.is_empty() && index_str.chars().all(|c| c.is_ascii_alphanumeric()) {
                    let content = &content_part[punc_pos + 1..];
                    let stripped = Self::strip_markdown_formatting(content.trim());
                    let cleaned = stripped.trim();
                    if !cleaned.is_empty() {
                        options.push(cleaned.to_string());
                        has_prefix = true;
                    }
                }
            }
            
            if !has_prefix && is_bullet {
                let stripped = Self::strip_markdown_formatting(content_part.trim());
                let cleaned = stripped.trim();
                if !cleaned.is_empty() {
                    options.push(cleaned.to_string());
                }
            }
        }
        
        // We only treat it as a Grill Mode question if we found at least 2 options
        if options.len() < 2 {
            return None;
        }
 
        // Add custom write-in option and skip option
        options.push("Write custom response...".to_string());
        options.push("Skip (Dismiss dialog)".to_string());
 
        // Find the title (clarifying question)
        // Find the line index of the first option in the original text lines
        let first_option_idx = text.lines()
            .position(|line| {
                let trimmed = line.trim();
                if trimmed.is_empty() { return false; }
                let is_bullet = trimmed.starts_with('•')
                    || (trimmed.starts_with('*') && !trimmed.starts_with("**") && trimmed.chars().nth(1) == Some(' '))
                    || (trimmed.starts_with('-') && trimmed.chars().nth(1) == Some(' '))
                    || (trimmed.starts_with('+') && trimmed.chars().nth(1) == Some(' '));
                let content_part = trimmed.trim_start_matches(|c| c == '*' || c == '-' || c == '+' || c == '•' || c == ' ');
                if let Some(punc_pos) = content_part.find(|c| c == '.' || c == ')' || c == ']') {
                    let index_str = &content_part[..punc_pos].trim();
                    if !index_str.is_empty() && index_str.chars().all(|c| c.is_ascii_alphanumeric()) {
                        return true;
                    }
                }
                is_bullet
            });
            
        let mut question_title = "Clarifying Question".to_string();
        if let Some(opt_idx) = first_option_idx {
            let lines: Vec<&str> = text.lines().collect();
            // 1. First scan backwards for a line ending with a question mark
            for i in (0..opt_idx).rev() {
                let line = lines[i].trim();
                // Strip markdown first, then check for question mark
                let stripped = Self::strip_markdown_formatting(line);
                let stripped = stripped.trim();
                if stripped.ends_with('?') {
                    if !stripped.is_empty() {
                        question_title = stripped.to_string();
                        break;
                    }
                }
            }
            // 2. If no line ends with '?', take the first non-empty line above the first option that isn't a header label
            if question_title == "Clarifying Question" {
                for i in (0..opt_idx).rev() {
                    let line = lines[i].trim();
                    if !line.is_empty() && !line.to_lowercase().contains("choices") && !line.to_lowercase().contains("question") {
                        let cleaned = Self::strip_markdown_formatting(line);
                        let cleaned = cleaned.trim();
                        if !cleaned.is_empty() {
                            question_title = cleaned.to_string();
                            break;
                        }
                    }
                }
            }
        }



        Some(GrillQuestion {
            title: question_title,
            options,
            selected: 0,
        })
    }

    /// Clear input + all attached tags (images, pasted blocks).
    pub fn clear_input_state(&mut self) {
        self.input.clear();
        self.attached_images.clear();
        self.pasted_blocks.clear();
    }

    /// Periodic tick handler — detects when a character-by-character paste burst ends and collapses it.
    pub fn handle_tick(&mut self) {
        self.tick_count = self.tick_count.wrapping_add(1);

        // Auto-expire the quit-pending confirmation after 2s
        if let Some(pending_at) = self.quit_pending {
            if pending_at.elapsed().as_secs_f32() >= 2.0 {
                self.quit_pending = None;
            }
        }

        if self.in_paste_burst {
            if let Some(last_time) = self.last_key_time {
                if Instant::now().duration_since(last_time) >= std::time::Duration::from_millis(50) {
                    self.in_paste_burst = false;

                    // Fetch from clipboard to get the original multiline text with newlines intact
                    let text = if let Ok(clip_text) = crate::clipboard::get_clipboard_text() {
                        clip_text
                    } else {
                        self.input.clone()
                    };

                    self.input.clear();

                    let line_count = text.lines().count();
                    let char_count = text.chars().count();
                    if line_count > 3 || char_count > 80 {
                        self.pasted_blocks.push(text);
                        let token = if line_count == 1 {
                            "[Pasted: 1 line] ".to_string()
                        } else {
                            format!("[Pasted: {} lines] ", line_count)
                        };
                        self.input.push_str(&token);
                    } else {
                        self.input = text.replace('\r', "").replace('\n', " ");
                    }
                    self.update_autocomplete();
                }
            }
        }
    }

    /// Build the full prompt from tags + typed text for submission.
    pub fn build_full_prompt(&self) -> String {
        let mut parts: Vec<String> = Vec::new();

        for block in &self.pasted_blocks {
            parts.push(block.clone());
        }

        // The input already contains @image_xxx.png tokens inline; include them verbatim.
        // Additionally append file:// references so the LLM client can base64-encode images.
        if !self.input.trim().is_empty() {
            parts.push(self.input.trim().to_string());
        }

        for path in self.attached_images.iter() {
            // Skip pending placeholders
            if path.starts_with("PENDING:") {
                continue;
            }
            let abs_path = if std::path::Path::new(path).is_absolute() {
                std::path::PathBuf::from(path)
            } else {
                std::path::Path::new(&self.session.cwd).join(path)
            };
            // Append as a markdown image link so extract_local_image_paths picks it up
            parts.push(format!("[attached image](file://{})", abs_path.to_string_lossy()));
        }

        parts.join("\n\n")
    }

    /// Build the visual display prompt from tags + typed text for the TUI transcript.
    pub fn build_display_prompt(&self) -> String {
        let mut parts: Vec<String> = Vec::new();

        for block in &self.pasted_blocks {
            let line_count = block.lines().count();
            if line_count == 1 {
                parts.push("[Pasted: 1 line]".to_string());
            } else {
                parts.push(format!("[Pasted: {} lines]", line_count));
            }
        }

        // Input already contains inline @image_xxx.png tokens — use it as-is.
        if !self.input.trim().is_empty() {
            parts.push(self.input.trim().to_string());
        }

        parts.join(" ")
    }

    pub fn handle_paste(&mut self, text: String) {
        if let Some(src_path) = try_parse_image_path(&text) {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let filename = format!("image_{}.png", timestamp);
            let cwd = self.session.cwd.clone();
            let dest_path = std::path::Path::new(&cwd).join(&filename);

            let token = format!("@{} ", filename);
            self.input.push_str(&token);

            let pending_tag = format!("PENDING:{}", dest_path.to_string_lossy());
            self.attached_images.push(pending_tag);

            let tx = self.event_tx.clone();
            tokio::spawn(async move {
                let result = tokio::task::spawn_blocking(move || {
                    std::fs::copy(&src_path, &dest_path)
                        .map(|_| dest_path.clone())
                        .map_err(|e| e.to_string())
                })
                .await
                .unwrap_or_else(|_| Err("Background thread panic".to_string()));

                let _ = tx.send(AppEvent::ImageAttached(result));
            });
            return;
        }

        let line_count = text.lines().count();
        let char_count = text.chars().count();
        if line_count > 3 || char_count > 80 {
            // Collapse multi-line or long paste into a tag
            self.pasted_blocks.push(text);
            let token = if line_count == 1 {
                "[Pasted: 1 line] ".to_string()
            } else {
                format!("[Pasted: {} lines] ", line_count)
            };
            self.input.push_str(&token);
        } else {
            // Short paste — inline into input
            let cleaned = text.replace('\r', "").replace('\n', " ");
            let truncated: String = cleaned.chars().take(2000).collect();
            self.input.push_str(&truncated);
        }
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
                if self.in_paste_burst || is_paste_like {
                    self.input.push('\n');
                    self.update_autocomplete();
                    return;
                }
                let full_prompt = self.build_full_prompt();
                if !full_prompt.is_empty() {
                    self.queued_prompts.push(full_prompt);
                    self.clear_input_state();
                    self.messages.push(Message {
                        role: "system".to_string(),
                        content: format!("Prompt queued (#{}).", self.queued_prompts.len()),
                    });
                }
                return;
            }
        }

        let mut grill_action: Option<(String, Option<String>)> = None;
        if let Some(ref mut gq) = self.active_grill_question {
            match (key.modifiers, key.code) {
                (_, KeyCode::Up) => {
                    if gq.selected > 0 {
                        gq.selected -= 1;
                    } else {
                        gq.selected = gq.options.len().saturating_sub(1);
                    }
                    return;
                }
                (_, KeyCode::Down) => {
                    if !gq.options.is_empty() {
                        gq.selected = (gq.selected + 1) % gq.options.len();
                    }
                    return;
                }
                (_, KeyCode::Esc) => {
                    grill_action = Some((String::new(), None));
                }
                (_, KeyCode::Enter) => {
                    if self.input.is_empty() {
                        let opt_count = gq.options.len();
                        if opt_count > 1 && gq.selected == opt_count - 1 {
                            // Skip selected: clear question dialog with no action
                            grill_action = Some((String::new(), None));
                        } else if opt_count > 0 && gq.selected == opt_count - 2 {
                            // Custom write-in selected: require typing first
                            self.messages.push(Message {
                                role: "system".to_string(),
                                content: "Please type your custom response in the input box and press Enter.".to_string(),
                            });
                            return;
                        } else if opt_count > 0 {
                            let choice = gq.options[gq.selected].clone();
                            grill_action = Some((format!("Selected Option: {}", choice), None));
                        }
                    } else {
                        let text = self.build_full_prompt();
                        let display = self.build_display_prompt();
                        self.clear_input_state();
                        grill_action = Some((text, Some(display)));
                    }
                }
                _ => {}
            }
        }

        if let Some((prompt, display)) = grill_action {
            self.active_grill_question = None;
            if !prompt.is_empty() {
                self.handle_submit(prompt, display);
            }
            return;
        }

        if self.autocomplete_visible {
            // KeyInput mode: typing goes to key_buffer, not the main input
            if self.autocomplete_mode == AutocompleteMode::KeyInput {
                match (key.modifiers, key.code) {
                    (_, KeyCode::Esc) => {
                        self.close_autocomplete();
                        return;
                    }
                    (_, KeyCode::Enter) => {
                        let key_val = self.key_buffer.trim().to_string();
                        if !key_val.is_empty() {
                            match fusion_core::config::save_api_key(&key_val) {
                                Ok(()) => {
                                    let masked = if key_val.len() > 8 {
                                        format!("{}...{}", &key_val[..4], &key_val[key_val.len()-4..])
                                    } else { "****".to_string() };
                                    let provider = self.pending_provider.clone().unwrap_or_default();
                                    self.messages.push(Message {
                                        role: "system".to_string(),
                                        content: format!(
                                            "API key for {} saved to ~/.config/fusion/fusion.toml\nKey: {}\nRestart to apply.",
                                            provider, masked
                                        ),
                                    });
                                }
                                Err(e) => {
                                    self.messages.push(Message {
                                        role: "system".to_string(),
                                        content: format!("Failed to save key: {}", e),
                                    });
                                }
                            }
                        }
                        self.close_autocomplete();
                        return;
                    }
                    (_, KeyCode::Backspace) => {
                        self.key_buffer.pop();
                        return;
                    }
                    (_, KeyCode::Char(c)) => {
                        self.key_buffer.push(c);
                        return;
                    }
                    _ => { return; }
                }
            }

            const VISIBLE: usize = 8; // visible rows in popup
            match (key.modifiers, key.code) {
                (_, KeyCode::Up) => {
                    if self.autocomplete_selected > 0 {
                        self.autocomplete_selected -= 1;
                        // Scroll up if selection moves above viewport
                        if self.autocomplete_selected < self.autocomplete_scroll {
                            self.autocomplete_scroll = self.autocomplete_selected;
                        }
                    }
                    return;
                }
                (_, KeyCode::Down) => {
                    if self.autocomplete_selected + 1 < self.autocomplete_items.len() {
                        self.autocomplete_selected += 1;
                        // Scroll down if selection moves below viewport
                        if self.autocomplete_selected >= self.autocomplete_scroll + VISIBLE {
                            self.autocomplete_scroll = self.autocomplete_selected + 1 - VISIBLE;
                        }
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

        // Any key other than Ctrl+C clears the quit confirmation banner
        if !matches!((key.modifiers, key.code), (KeyModifiers::CONTROL, KeyCode::Char('c'))) {
            self.quit_pending = None;
        }

        match (key.modifiers, key.code) {
            (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                if let Some(pending_at) = self.quit_pending {
                    if pending_at.elapsed().as_secs_f32() < 2.0 {
                        // Second Ctrl+C within 2s — actually quit
                        self.save_session();
                        self.should_quit = true;
                        return;
                    }
                }
                // First Ctrl+C — show confirmation prompt
                self.quit_pending = Some(Instant::now());
            }
            (KeyModifiers::CONTROL, KeyCode::Char('v')) | (KeyModifiers::SUPER, KeyCode::Char('v')) => {
                // 1. Try reading clipboard text first (lightning fast!)
                if let Ok(text) = crate::clipboard::get_clipboard_text() {
                    if !text.trim().is_empty() {
                        self.handle_paste(text);
                        return;
                    }
                }

                // 2. Clipboard is empty or contains an image. Extract it in the background!
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::SystemTime::UNIX_EPOCH)
                    .map(|d| d.as_secs()).unwrap_or(0);
                let filename = format!("image_{}.png", timestamp);
                let cwd = self.session.cwd.clone();
                let dest_path = std::path::Path::new(&cwd).join(&filename);

                let token = format!("@{} ", filename);
                self.input.push_str(&token);

                let pending_tag = format!("PENDING:{}", dest_path.to_string_lossy());
                self.attached_images.push(pending_tag);

                let tx = self.event_tx.clone();
                tokio::spawn(async move {
                    let result = tokio::task::spawn_blocking(move || {
                        crate::clipboard::save_clipboard_image(&dest_path)
                    })
                    .await
                    .unwrap_or_else(|_| Err("Background thread panic".to_string()));

                    let _ = tx.send(AppEvent::ImageAttached(result));
                });
            }
            (KeyModifiers::CONTROL, KeyCode::Char('g')) => {
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::SystemTime::UNIX_EPOCH)
                    .map(|d| d.as_secs()).unwrap_or(0);
                let filename = format!("image_{}.png", timestamp);
                let cwd = self.session.cwd.clone();
                let dest_path = std::path::Path::new(&cwd).join(&filename);

                let token = format!("@{} ", filename);
                self.input.push_str(&token);

                let pending_tag = format!("PENDING:{}", dest_path.to_string_lossy());
                self.attached_images.push(pending_tag);

                let tx = self.event_tx.clone();
                tokio::spawn(async move {
                    let result = tokio::task::spawn_blocking(move || {
                        crate::clipboard::save_clipboard_image(&dest_path)
                    })
                    .await
                    .unwrap_or_else(|_| Err("Background thread panic".to_string()));

                    let _ = tx.send(AppEvent::ImageAttached(result));
                });
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
            }
            (_, KeyCode::Tab) => {
                // Tab toggles between Normal and Plan modes
                self.mode = match self.mode {
                    AppMode::Normal => AppMode::Plan,
                    AppMode::Plan => AppMode::Normal,
                    AppMode::Yolo => AppMode::Normal,
                };
            }
            (_, KeyCode::Enter) => {
                if self.in_paste_burst || is_paste_like {
                    // During paste bursts, collect newlines to collapse later
                    self.input.push('\n');
                    self.update_autocomplete();
                    return;
                }
                self.close_autocomplete();
                let text = self.input.trim().to_string();
                let has_tags = !self.pasted_blocks.is_empty() || !self.attached_images.is_empty();
                if !text.is_empty() || has_tags {
                    if text == "/image" || text.starts_with("/image ") {
                        let timestamp = std::time::SystemTime::now()
                            .duration_since(std::time::SystemTime::UNIX_EPOCH)
                            .map(|d| d.as_secs()).unwrap_or(0);
                        let filename = format!("image_{}.png", timestamp);
                        let cwd = self.session.cwd.clone();
                        let dest_path = std::path::Path::new(&cwd).join(&filename);
                        match crate::clipboard::save_clipboard_image(&dest_path) {
                            Ok(path) => {
                                self.attached_images.push(path.to_string_lossy().to_string());
                                self.input = format!("@{} ", filename);
                            }
                            Err(e) => {
                                self.messages.push(Message {
                                    role: "system".to_string(),
                                    content: format!("Error saving image: {}", e),
                                });
                                self.input.clear();
                            }
                        }
                    } else {
                        if self.attached_images.iter().any(|s| s.starts_with("PENDING:")) {
                            self.messages.push(Message {
                                role: "system".to_string(),
                                content: "⏳ Please wait for clipboard image to finish saving before submitting.".to_string(),
                            });
                            return;
                        }
                        let full_prompt = self.build_full_prompt();
                        let display_prompt = self.build_display_prompt();
                        self.clear_input_state();
                        self.auto_scroll.set(true);
                        self.handle_submit(full_prompt, Some(display_prompt));
                    }
                }
            }
            (_, KeyCode::Char(c)) => {
                self.input.push(c);
                if c == '@' && self.autocomplete_mode != AutocompleteMode::Files {
                    // Open file picker immediately on '@'
                    self.at_query.clear();
                    self.show_file_picker("");
                } else if self.autocomplete_mode == AutocompleteMode::Files {
                    // User is filtering — last word after '@' is the query
                    if let Some(at_pos) = self.input.rfind('@') {
                        self.at_query = self.input[at_pos + 1..].to_string();
                        // If user typed a space, dismiss
                        if self.at_query.contains(' ') {
                            self.close_autocomplete();
                        } else {
                            let q = self.at_query.clone();
                            self.show_file_picker(&q);
                        }
                    }
                } else {
                    self.update_autocomplete();
                }
            }
            (_, KeyCode::Backspace) => {
                if !self.input.is_empty() {
                    let trimmed = self.input.trim_end();
                    let mut matched_token_len = None;
                    let mut is_image_token = false;
                    let mut deleted_image_filename: Option<String> = None;
                    let mut is_paste_token = false;

                    // Check for @image_<timestamp>.png token at the end
                    if let Some(last_word) = trimmed.split_whitespace().last() {
                        if last_word.starts_with('@') && last_word.ends_with(".png") {
                            is_image_token = true;
                            deleted_image_filename = Some(last_word[1..].to_string());
                            let spaces_count = self.input.len() - trimmed.len();
                            matched_token_len = Some(last_word.len() + spaces_count);
                        }
                    }

                    // Fallback: check for [Pasted: ...] or legacy [Image #N] bracket tokens
                    if matched_token_len.is_none() && trimmed.ends_with(']') {
                        if let Some(start_idx) = trimmed.rfind('[') {
                            let token = &trimmed[start_idx..];
                            if token.starts_with("[Image #") {
                                is_image_token = true;
                                let spaces_count = self.input.len() - trimmed.len();
                                matched_token_len = Some(token.len() + spaces_count);
                            } else if token.starts_with("[Pasted: ") {
                                is_paste_token = true;
                                let spaces_count = self.input.len() - trimmed.len();
                                matched_token_len = Some(token.len() + spaces_count);
                            }
                        }
                    }

                    if let Some(len) = matched_token_len {
                        let new_len = self.input.len() - len;
                        self.input.truncate(new_len);
                        if is_image_token {
                            if let Some(ref fname) = deleted_image_filename {
                                // Remove the specific image whose path contains this filename
                                self.attached_images.retain(|p| !p.contains(fname.as_str()));
                            } else {
                                self.attached_images.pop();
                            }
                        } else if is_paste_token {
                            self.pasted_blocks.pop();
                        }
                    } else {
                        self.input.pop();
                    }
                }
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
                // Files mode: update filter or close if @ was deleted
                if self.autocomplete_mode == AutocompleteMode::Files {
                    if let Some(at_pos) = self.input.rfind('@') {
                        self.at_query = self.input[at_pos + 1..].to_string();
                        let q = self.at_query.clone();
                        self.show_file_picker(&q);
                    } else {
                        self.close_autocomplete();
                    }
                } else {
                    self.update_autocomplete();
                }
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
        self.at_query.clear();
        self.autocomplete_scroll = 0;
        self.key_buffer.clear();
        self.pending_provider = None;
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
                    } else if has_submenu && label == "/providers" {
                        // Transition to provider picker
                        self.input.clear();
                        self.show_provider_picker();
                    } else {
                        // Execute the command directly
                        self.input.clear();
                        self.close_autocomplete();
                        self.handle_submit(label, None);
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
                            self.handle_submit(format!("/model {}", info.shorthand), None);
                        }
                    } else {
                        self.input.clear();
                        self.close_autocomplete();
                        self.handle_submit(format!("/model {}", model_label), None);
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
                    self.handle_submit(format!("/model {}", model_name), None);

                    // Apply effort level
                    let level = match level_label.as_str() {
                        "max" => "/max",
                        "high" => "/high",
                        _ => "/normal",
                    };
                    self.handle_submit(level.to_string(), None);
                }
            }
            AutocompleteMode::Files => {
                if let Some(item) = self.autocomplete_items.get(selected_idx) {
                    let selected = item.label.clone();
                    // Replace the partial @<query> at the end of input with @<filename>
                    let query = self.at_query.clone();
                    let partial = format!("@{}", query);
                    if self.input.ends_with(&partial) {
                        let new_len = self.input.len() - partial.len();
                        self.input.truncate(new_len);
                    }
                    self.input.push('@');
                    self.input.push_str(&selected);
                    self.input.push(' ');
                    self.close_autocomplete();
                }
            }
            AutocompleteMode::Providers => {
                if let Some(item) = self.autocomplete_items.get(selected_idx) {
                    let provider = item.label.clone();
                    self.open_key_input(provider);
                }
            }
            AutocompleteMode::KeyInput => {
                // Handled entirely in the key event handler above
            }
        }
    }

    /// Show the provider selection popup.
    fn show_provider_picker(&mut self) {
        self.autocomplete_items = vec![
            AutocompleteItem {
                label: "Cloudflare".to_string(),
                description: "Workers AI · kimi, glm, qwen and more".to_string(),
                is_current: false,
            },
            AutocompleteItem {
                label: "xAI".to_string(),
                description: "Grok 3, Grok 3 Mini".to_string(),
                is_current: false,
            },
            AutocompleteItem {
                label: "OpenAI".to_string(),
                description: "GPT-4o and compatible APIs".to_string(),
                is_current: false,
            },
        ];
        self.autocomplete_mode = AutocompleteMode::Providers;
        self.autocomplete_selected = 0;
        self.autocomplete_scroll = 0;
        self.autocomplete_visible = true;
    }

    /// Open the key input overlay for a given provider.
    fn open_key_input(&mut self, provider: String) {
        self.pending_provider = Some(provider.clone());
        self.key_buffer.clear();
        // Show a single placeholder item that acts as the input prompt
        self.autocomplete_items = vec![AutocompleteItem {
            label: format!("Enter {} API key", provider),
            description: "Press Enter to save · Esc to cancel".to_string(),
            is_current: false,
        }];
        self.autocomplete_mode = AutocompleteMode::KeyInput;
        self.autocomplete_selected = 0;
        self.autocomplete_scroll = 0;
        self.autocomplete_visible = true;
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
            AutocompleteMode::Files => {
                // Re-filter file list from current at_query
                let q = self.at_query.clone();
                self.show_file_picker(&q);
            }
            AutocompleteMode::Providers | AutocompleteMode::KeyInput => {
                // Static lists — no filtering needed
            }
        }
    }

    /// Populate the file/folder autocomplete from cwd, filtered by `query`.
    fn show_file_picker(&mut self, query: &str) {
        let cwd = self.session.cwd.clone();
        let dir = std::path::Path::new(&cwd);

        let mut entries: Vec<(bool, String)> = Vec::new(); // (is_dir, name)
        if let Ok(read_dir) = std::fs::read_dir(dir) {
            for entry in read_dir.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                // Skip hidden files unless user explicitly types a dot
                if name.starts_with('.') && !query.starts_with('.') {
                    continue;
                }
                let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                let lower = name.to_lowercase();
                if query.is_empty() || lower.contains(&query.to_lowercase()) {
                    entries.push((is_dir, name));
                }
            }
        }

        // Sort: dirs first, then alphabetical
        entries.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));

        self.autocomplete_items = entries
            .into_iter()
            .take(15)
            .map(|(is_dir, name)| {
                let ext = std::path::Path::new(&name)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("");
                let description = if is_dir {
                    "directory".to_string()
                } else if matches!(ext, "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp") {
                    format!("image · .{}", ext)
                } else {
                    format!("file · .{}", ext)
                };
                let label = if is_dir {
                    format!("{}/", name)
                } else {
                    name
                };
                AutocompleteItem { label, description, is_current: false }
            })
            .collect();

        // Always switch to Files mode first so the popup shows
        self.autocomplete_mode = AutocompleteMode::Files;
        self.autocomplete_selected = 0;

        if self.autocomplete_items.is_empty() {
            // Show a placeholder so the popup still appears
            self.autocomplete_items = vec![AutocompleteItem {
                label: "(empty directory)".to_string(),
                description: "no files found".to_string(),
                is_current: false,
            }];
        }

        self.autocomplete_visible = true;
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

    fn handle_submit(&mut self, text: String, display_text: Option<String>) {
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

        let display = display_text.unwrap_or_else(|| text.clone());
        self.messages.push(Message {
            role: "user".to_string(),
            content: display,
        });

        self.session.push_message("user", &text);
        self.save_session();
        self.is_thinking = true;
        self.turn_start = Some(Instant::now());
        self.thought_duration = None;
        self.had_thinking = false;

        let tx = self.event_tx.clone();
        let agent = Arc::clone(&self.agent);
        let grill_mode_active = self.grill_mode;

        let handle = tokio::spawn(async move {
            let mut agent = agent.lock().await;
            let (agent_tx, mut agent_rx) = tokio::sync::mpsc::unbounded_channel();
            
            let tx_clone = tx.clone();
            let forwarder = tokio::spawn(async move {
                while let Some(event) = agent_rx.recv().await {
                    let _ = tx_clone.send(AppEvent::Agent(event));
                }
            });

            let mut processed_text = text.clone();
            if grill_mode_active {
                processed_text = format!(
                    "{}\n\n[SYSTEM DIRECTIVE: You are in Grill Mode. Do NOT execute any code writing or file modification tools yet. \
                     Instead, analyze my request and ask me exactly ONE clarifying design or implementation question to refine the plan. \
                     Propose recommended choices for me. Do NOT generate the full code until we align.]",
                    processed_text
                );
            }

            if let Err(e) = agent.process(&processed_text, agent_tx).await {
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

                // If Grill Mode is active, parse the response for a question & recommended choices
                if self.grill_mode {
                    if let Some(gq) = Self::try_parse_grill_question(&text) {
                        self.active_grill_question = Some(gq);
                    }
                }

                // Process the next queued prompt if any (only if a grill modal is not active!)
                if self.active_grill_question.is_none() && !self.queued_prompts.is_empty() {
                    let next_prompt = self.queued_prompts.remove(0);
                    self.handle_submit(next_prompt, None);
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
                    content: "Fusion Code commands:\n  /help          show all commands\n  /yolo          toggle auto-approve\n  /plan          enter plan mode\n  /grill         toggle grill mode (design interview)\n  /model <n>     switch model (autocomplete supported)\n  /key <value>   save API key to config file\n  /max           set maximum token output\n  /high          set high token output\n  /normal        set normal token output\n  /status        current settings\n  /theme         toggle light/dark theme\n  /image         insert clipboard image\n  /session       show session ID\n  /sessions      list saved sessions\n  /clear         clear messages\n  /dq [n]        clear queue or dequeue item n\n  @filename      mention a file/image from cwd\n  /quit          quit (session auto-saved)\n  Ctrl+C twice   quit".to_string(),
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
            "/providers" => {
                self.show_provider_picker();
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
            "/key" => {
                if let Some(api_key) = parts.get(1) {
                    let key = api_key.trim().to_string();
                    if key.is_empty() {
                        self.messages.push(Message {
                            role: "system".to_string(),
                            content: "Usage: /key <api-key-value>".to_string(),
                        });
                    } else {
                        match fusion_core::config::save_api_key(&key) {
                            Ok(()) => {
                                // Mask all but last 4 chars for display
                                let masked = if key.len() > 8 {
                                    format!("{}...{}", &key[..4], &key[key.len()-4..])
                                } else {
                                    "****".to_string()
                                };
                                self.messages.push(Message {
                                    role: "system".to_string(),
                                    content: format!(
                                        "API key saved to ~/.config/fusion/fusion.toml\nKey: {}\nRestart fusion to apply the new key.",
                                        masked
                                    ),
                                });
                            }
                            Err(e) => {
                                self.messages.push(Message {
                                    role: "system".to_string(),
                                    content: format!("Failed to save API key: {}", e),
                                });
                            }
                        }
                    }
                } else {
                    self.messages.push(Message {
                        role: "system".to_string(),
                        content: "Usage: /key <api-key-value>\nSaves your provider API key to ~/.config/fusion/fusion.toml".to_string(),
                    });
                }
            }
            "/image" => {
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::SystemTime::UNIX_EPOCH)
                    .map(|d| d.as_secs()).unwrap_or(0);
                let filename = format!("image_{}.png", timestamp);
                let cwd = self.session.cwd.clone();
                let dest_path = std::path::Path::new(&cwd).join(&filename);
                match crate::clipboard::save_clipboard_image(&dest_path) {
                    Ok(path) => {
                        self.attached_images.push(path.to_string_lossy().to_string());
                        self.input = format!("@{} ", filename);
                    }
                    Err(e) => {
                        self.messages.push(Message {
                            role: "system".to_string(),
                            content: format!("Error saving image: {}", e),
                        });
                    }
                }
            }
            "/grill" | "/grill-me" => {
                self.grill_mode = !self.grill_mode;
                let status = if self.grill_mode {
                    "ENABLED. The agent will ask clarifying questions to refine your plan before code execution."
                } else {
                    "DISABLED. Normal execution mode."
                };
                self.messages.push(Message {
                    role: "system".to_string(),
                    content: format!("Grill Mode {}", status),
                });
            }
            "/arbitrage" => {
                self.arbitrage_mode = !self.arbitrage_mode;
                let status = if self.arbitrage_mode {
                    "ENABLED. Code edits will be delegated to a fast/cheap model to save premium tokens."
                } else {
                    "DISABLED."
                };
                // Propagate to the agent in a spawned task
                let agent = Arc::clone(&self.agent);
                let mode = self.arbitrage_mode;
                tokio::spawn(async move {
                    let mut agent = agent.lock().await;
                    agent.arbitrage_mode = mode;
                });
                self.messages.push(Message {
                    role: "system".to_string(),
                    content: format!("Arbitrage Mode {}", status),
                });
            }
            "/taste" => {
                let cwd_path = std::path::Path::new(&self.session.cwd);
                self.messages.push(Message {
                    role: "system".to_string(),
                    content: "Scanning codebase for style preferences...".to_string(),
                });
                let rules = fusion_core::taste::scan_taste_preferences(cwd_path);
                if rules.is_empty() {
                    self.messages.push(Message {
                        role: "system".to_string(),
                        content: "No dominant style preferences could be automatically detected (too few files or mixed style).".to_string(),
                    });
                } else {
                    if let Err(e) = fusion_core::taste::save_taste_rules(cwd_path, &rules) {
                        self.messages.push(Message {
                            role: "error".to_string(),
                            content: format!("Failed to save taste rules: {}", e),
                        });
                    } else {
                        let mut summary = vec!["✓ Successfully learned code style!".to_string()];
                        for r in &rules {
                            summary.push(format!("  - {} (Confidence: {:.2})", r.rule, r.confidence));
                        }
                        summary.push("\nRules saved to .fusion/taste.md".to_string());
                        self.messages.push(Message {
                            role: "system".to_string(),
                            content: summary.join("\n"),
                        });
                    }
                }
            }
            "/design" => {
                let cwd_path = std::path::Path::new(&self.session.cwd);
                self.messages.push(Message {
                    role: "system".to_string(),
                    content: "Scanning codebase for design preferences...".to_string(),
                });
                let rules = fusion_core::design::scan_design_preferences(cwd_path);
                if rules.is_empty() {
                    self.messages.push(Message {
                        role: "system".to_string(),
                        content: "No design preferences detected (no frontend/UI files found or mixed patterns).".to_string(),
                    });
                } else {
                    if let Err(e) = fusion_core::design::save_design_rules(cwd_path, &rules) {
                        self.messages.push(Message {
                            role: "error".to_string(),
                            content: format!("Failed to save design rules: {}", e),
                        });
                    } else {
                        let mut summary = vec!["✓ Successfully learned design preferences!".to_string()];
                        for r in &rules {
                            summary.push(format!("  - {} (Confidence: {:.2})", r.rule, r.confidence));
                        }
                        summary.push("\nRules saved to .fusion/design.md".to_string());
                        self.messages.push(Message {
                            role: "system".to_string(),
                            content: summary.join("\n"),
                        });
                    }
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
                    if let Ok(clip_text) = crate::clipboard::get_clipboard_text() {
                        app.handle_paste(clip_text);
                    } else {
                        app.handle_paste(text);
                    }
                }
                AppEvent::ImageAttached(result) => {
                    match result {
                        Ok(path) => {
                            if let Some(pos) = app.attached_images.iter().position(|s| s.starts_with("PENDING:")) {
                                app.attached_images[pos] = path.to_string_lossy().to_string();
                            } else {
                                app.attached_images.push(path.to_string_lossy().to_string());
                            }
                        }
                        Err(e) => {
                            // Remove the pending tag since saving failed
                            if let Some(pos) = app.attached_images.iter().position(|s| s.starts_with("PENDING:")) {
                                let pending_str = app.attached_images.remove(pos);
                                // Extract the filename from PENDING:/path/to/image_xxx.png
                                if let Some(path_part) = pending_str.strip_prefix("PENDING:") {
                                    if let Some(fname) = std::path::Path::new(path_part)
                                        .file_name().and_then(|f| f.to_str())
                                    {
                                        let token = format!("@{}", fname);
                                        app.input = app.input.replace(&format!("{} ", token), "");
                                        app.input = app.input.replace(&token, "");
                                    }
                                }
                            }
                            app.messages.push(Message {
                                role: "system".to_string(),
                                content: format!("Error saving image: {}", e),
                            });
                        }
                    }
                }
                AppEvent::Tick => {
                    app.handle_tick();
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

    // Extract path from a markdown link containing `file://` or just a raw `file://` URL
    let path_str = if let Some(start_idx) = unquoted.find("file://") {
        let path_start = start_idx + 7; // skip "file://"
        let mut path_end = path_start;
        for c in unquoted[path_start..].chars() {
            if c == ')' || c == ']' || c == '\n' || c == '\r' || c == ' ' || c == '"' || c == '\'' {
                break;
            }
            path_end += c.len_utf8();
        }
        if path_end > path_start {
            &unquoted[path_start..path_end]
        } else {
            unquoted
        }
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

        // Test Markdown image format
        let markdown_format = format!("[image 1](file://{})", img_path.to_string_lossy());
        let parsed_markdown = try_parse_image_path(&markdown_format);
        assert!(parsed_markdown.is_some());
        assert_eq!(parsed_markdown.unwrap(), img_path);

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
        app.last_key_time = None;
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

    #[tokio::test]
    async fn test_smart_paste_and_image_tags() {
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

        // Test normal short paste
        app.handle_paste("short text".to_string());
        assert_eq!(app.input, "short text");
        assert!(app.pasted_blocks.is_empty());
        assert!(app.attached_images.is_empty());

        app.clear_input_state();

        // Test multi-line paste (> 3 lines)
        let multiline = "line 1\nline 2\nline 3\nline 4".to_string();
        app.handle_paste(multiline.clone());
        assert_eq!(app.input, "[Pasted: 4 lines] ");
        assert_eq!(app.pasted_blocks.len(), 1);
        assert_eq!(app.pasted_blocks[0], multiline);

        // Add some image path (drag-drop style)
        let temp_dir = std::env::temp_dir();
        let img_path = temp_dir.join("test_img.png");
        let _ = File::create(&img_path);
        app.handle_paste(img_path.to_string_lossy().to_string());
        // Should have one pending image
        assert_eq!(app.attached_images.len(), 1);
        assert!(app.attached_images[0].starts_with("PENDING:"));
        // Input should contain an @image_... token
        assert!(app.input.contains("@image_"));

        // Backspace should atomically delete the @image_xxx.png token
        let bs_key = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Backspace,
            crossterm::event::KeyModifiers::empty(),
        );
        app.handle_key(bs_key);
        // After backspace, the @image_... token should be gone and attached_images empty
        assert!(!app.input.contains("@image_"));
        assert!(app.attached_images.is_empty());

        let _ = std::fs::remove_file(img_path);
    }

    #[test]
    fn test_grill_mode() {
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

        assert!(!app.grill_mode);

        // Toggle grill mode on
        app.handle_slash("/grill");
        assert!(app.grill_mode);

        // Toggle grill mode off
        app.handle_slash("/grill-me");
        assert!(!app.grill_mode);
    }

    #[test]
    fn test_grill_question_parser() {
        let text = "Great - a todo app is a classic project.\n\n\
                    One clarifying question:\n\
                    **What kind of persistence and architecture do you want?**\n\n\
                    Recommended choices:\n\
                    1. Pure frontend, in-memory only\n\
                    2. Frontend with localStorage\n\
                    3. Full-stack with a backend + database\n\n\
                    Which direction fits your goal?";
                    
        let gq = App::try_parse_grill_question(text).unwrap();
        println!("PARSED OPTIONS: {:?}", gq.options);
        assert_eq!(gq.title, "What kind of persistence and architecture do you want?");
        assert_eq!(gq.options.len(), 5);
        assert_eq!(gq.options[0], "Pure frontend, in-memory only");
        assert_eq!(gq.options[1], "Frontend with localStorage");
        assert_eq!(gq.options[2], "Full-stack with a backend + database");
        assert_eq!(gq.options[3], "Write custom response...");
        assert_eq!(gq.options[4], "Skip (Dismiss dialog)");
        assert_eq!(gq.selected, 0);

        // Test with alphanumeric list items and bullet points
        let letter_text = "What is your primary goal for this todo app?\n\
                           * A) Learn/practice a specific stack\n\
                           * B) Build a polished, portfolio-ready UI\n\
                           * C) Solve a personal workflow need\n\
                           * D) Explore full-stack architecture\n\n\
                           Once you tell me, I'll recommend.";
                           
        let gq_letter = App::try_parse_grill_question(letter_text).unwrap();
        assert_eq!(gq_letter.title, "What is your primary goal for this todo app?");
        assert_eq!(gq_letter.options.len(), 6); // 4 options + 1 custom write-in + 1 skip
        assert_eq!(gq_letter.options[0], "Learn/practice a specific stack");
        assert_eq!(gq_letter.options[1], "Build a polished, portfolio-ready UI");
        assert_eq!(gq_letter.options[2], "Solve a personal workflow need");
        assert_eq!(gq_letter.options[3], "Explore full-stack architecture");
        assert_eq!(gq_letter.options[4], "Write custom response...");
        assert_eq!(gq_letter.options[5], "Skip (Dismiss dialog)");

        // Test with plain unicode bullets
        let unicode_bullet_text = "How do you want habit data stored and accessed across devices?\n\
                                  • Local-only (offline-first) - store habits\n\
                                  • Cloud sync with auth - use Supabase\n\
                                  • Local-first with backup - habits\n\
                                  Which of these fits your goal?";
                                  
        let gq_bullet = App::try_parse_grill_question(unicode_bullet_text).unwrap();
        assert_eq!(gq_bullet.title, "How do you want habit data stored and accessed across devices?");
        assert_eq!(gq_bullet.options.len(), 5); // 3 options + 1 custom write-in + 1 skip
        assert_eq!(gq_bullet.options[0], "Local-only (offline-first) - store habits");
        assert_eq!(gq_bullet.options[1], "Cloud sync with auth - use Supabase");
        assert_eq!(gq_bullet.options[2], "Local-first with backup - habits");
    }
}
