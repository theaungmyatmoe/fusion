use crossterm::event::{KeyCode, KeyModifiers};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

use fusion_agent::agent::{Agent, AgentEvent};
use fusion_core::config::Config;
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

/// Main TUI application state.
pub struct App {
    pub messages: Vec<Message>,
    pub input: String,
    pub mode: AppMode,
    pub model: String,
    pub is_thinking: bool,
    pub should_quit: bool,
    pub session_id: String,

    agent: Arc<Mutex<Agent>>,
    session: Session,
    event_tx: mpsc::UnboundedSender<AppEvent>,
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

        let mut messages = vec![Message {
            role: "system".to_string(),
            content: format!(
                "fusion — mobile-first AI coding agent\nmodel: {}  │  session: {}  │  /help for commands",
                config.model, session_id
            ),
        }];

        if let Some(ref path) = config.config_path {
            messages.push(Message {
                role: "system".to_string(),
                content: format!("config: {}", path.display()),
            });
        }

        Self {
            messages,
            input: String::new(),
            mode,
            model: config.model.clone(),
            is_thinking: false,
            should_quit: false,
            session_id,
            agent: Arc::new(Mutex::new(Agent::new(config, cwd))),
            session,
            event_tx,
        }
    }

    /// Create from a resumed session.
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

        // Replay session messages as visible messages
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

        Self {
            messages,
            input: String::new(),
            mode,
            model: config.model.clone(),
            is_thinking: false,
            should_quit: false,
            session_id,
            agent: Arc::new(Mutex::new(Agent::new(config, cwd))),
            session,
            event_tx,
        }
    }

    /// Handle a key event.
    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) {
        match (key.modifiers, key.code) {
            (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                self.save_session();
                self.should_quit = true;
            }
            (_, KeyCode::Enter) => {
                let text = self.input.trim().to_string();
                if !text.is_empty() {
                    self.input.clear();
                    self.handle_submit(text);
                }
            }
            (_, KeyCode::Char(c)) => {
                self.input.push(c);
            }
            (_, KeyCode::Backspace) => {
                self.input.pop();
            }
            _ => {}
        }
    }

    fn handle_submit(&mut self, text: String) {
        // Slash commands
        if text.starts_with('/') {
            self.handle_slash(&text);
            return;
        }

        // Show user message
        self.messages.push(Message {
            role: "user".to_string(),
            content: text.clone(),
        });

        // Save to session
        self.session.push_message("user", &text);
        self.save_session();

        self.is_thinking = true;

        // Spawn agent processing in background
        let tx = self.event_tx.clone();
        let agent = Arc::clone(&self.agent);

        tokio::spawn(async move {
            let mut agent = agent.lock().await;
            match agent.process(&text).await {
                Ok(events) => {
                    for event in events {
                        let _ = tx.send(AppEvent::Agent(event));
                    }
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::Agent(AgentEvent::FinalResponse(format!(
                        "Error: {}",
                        e
                    ))));
                }
            }
        });
    }

    /// Handle an agent event.
    pub fn handle_agent_event(&mut self, event: AgentEvent) {
        match event {
            AgentEvent::Thinking(text) => {
                let preview = if text.len() > 80 {
                    format!("{}…", &text[..80])
                } else {
                    text
                };
                if let Some(last) = self.messages.last_mut() {
                    if last.role == "thinking" {
                        last.content = preview;
                        return;
                    }
                }
                self.messages.push(Message {
                    role: "thinking".to_string(),
                    content: preview,
                });
            }
            AgentEvent::ToolCall { name, args_preview } => {
                self.remove_thinking();
                let content = format!("⚙ {} {}", name, args_preview);
                self.messages.push(Message {
                    role: "tool".to_string(),
                    content: content.clone(),
                });
                self.session.push_message("tool", &content);
            }
            AgentEvent::ToolResult { name, output } => {
                let truncated = if output.len() > 300 {
                    format!("{}…", &output[..300])
                } else {
                    output
                };
                self.messages.push(Message {
                    role: "tool_result".to_string(),
                    content: format!("  ↳ {} → {}", name, truncated),
                });
            }
            AgentEvent::FinalResponse(text) => {
                self.is_thinking = false;
                self.remove_thinking();
                self.messages.push(Message {
                    role: "assistant".to_string(),
                    content: text.clone(),
                });
                // Save assistant response to session
                self.session.push_message("assistant", &text);
                self.save_session();
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
            // Silent fail — don't crash the TUI for session save errors
            eprintln!("session save error: {}", e);
        }
    }

    fn handle_slash(&mut self, cmd: &str) {
        let lower = cmd.to_lowercase();

        match lower.as_str() {
            "/help" | "/h" | "/?" => {
                self.messages.push(Message {
                    role: "system".to_string(),
                    content: "Fusion commands:\n  /help       this help\n  /yolo       toggle auto-approve\n  /plan       enter plan mode\n  /model <n>  switch model\n  /status     current settings\n  /sessions   list saved sessions\n  /session    show current session ID\n  /clear      clear messages\n  /exit       quit (session auto-saved)".to_string(),
                });
            }
            "/yolo" => {
                self.mode = if self.mode == AppMode::Yolo {
                    AppMode::Normal
                } else {
                    AppMode::Yolo
                };
                let msg = if self.mode == AppMode::Yolo {
                    "⚡ YOLO mode ON — all tool actions auto-approved"
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
            "/clear" => {
                self.messages.clear();
                self.messages.push(Message {
                    role: "system".to_string(),
                    content: "Cleared.".to_string(),
                });
            }
            "/session" | "/sid" => {
                self.messages.push(Message {
                    role: "system".to_string(),
                    content: format!(
                        "Session: {}\nMessages: {}\nResume with: fusion --resume {}",
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
                                    s.message_count,
                                    age,
                                    s.preview
                                ));
                            }
                            lines.push(String::new());
                            lines.push("Resume with: fusion --resume <id>".to_string());
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
                self.messages.push(Message {
                    role: "system".to_string(),
                    content: format!(
                        "model: {}  mode: {}  session: {}",
                        self.model, mode_str, self.session_id
                    ),
                });
            }
            "/exit" | "/quit" | "/q" => {
                self.save_session();
                self.should_quit = true;
            }
            _ if lower.starts_with("/model ") => {
                let new_model = cmd[7..].trim().to_string();
                if !new_model.is_empty() {
                    self.model = new_model.clone();
                    self.messages.push(Message {
                        role: "system".to_string(),
                        content: format!("Model → {}", new_model),
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

/// Run the full Ratatui TUI.
pub async fn run_tui(config: &Config) -> anyhow::Result<()> {
    run_tui_with_session(config, None).await
}

/// Run TUI with optional session resume.
pub async fn run_tui_with_session(
    config: &Config,
    resume_session: Option<Session>,
) -> anyhow::Result<()> {
    // Setup terminal
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    crossterm::execute!(
        stdout,
        crossterm::terminal::EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    // Event handler
    let (mut event_handler, event_tx) = EventHandler::new(100);
    let mut app = match resume_session {
        Some(session) => App::from_session(config, session, event_tx),
        None => App::new(config, event_tx),
    };

    // Main loop
    loop {
        terminal.draw(|frame| ui::draw(frame, &app))?;

        if let Some(event) = event_handler.next().await {
            match event {
                AppEvent::Key(key) => {
                    app.handle_key(key);
                }
                AppEvent::Agent(agent_event) => {
                    app.handle_agent_event(agent_event);
                }
                AppEvent::Resize(_, _) => {}
                AppEvent::Tick => {}
            }
        }

        if app.should_quit {
            break;
        }
    }

    // Restore terminal
    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        std::io::stdout(),
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}
