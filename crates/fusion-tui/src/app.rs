use crossterm::event::{KeyCode, KeyModifiers};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

use fusion_agent::agent::{Agent, AgentEvent};
use fusion_core::config::Config;

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

    agent: Arc<Mutex<Agent>>,
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

        let mut messages = vec![Message {
            role: "system".to_string(),
            content: format!(
                "fusion — mobile-first AI coding agent\nmodel: {}  │  /help for commands",
                config.model
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
            agent: Arc::new(Mutex::new(Agent::new(config, cwd))),
            event_tx,
        }
    }

    /// Handle a key event.
    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) {
        match (key.modifiers, key.code) {
            (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
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

        self.is_thinking = true;

        // Spawn agent processing in background using Arc<Mutex<>>
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
                // Show a brief, truncated thinking preview — not the full raw reasoning
                let preview = if text.len() > 80 {
                    format!("{}…", &text[..80])
                } else {
                    text
                };
                // Replace any existing thinking message instead of appending
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
                // Remove thinking message when tools start
                self.remove_thinking();
                self.messages.push(Message {
                    role: "tool".to_string(),
                    content: format!("⚙ {} {}", name, args_preview),
                });
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
                // Remove thinking message when final response arrives
                self.remove_thinking();
                self.messages.push(Message {
                    role: "assistant".to_string(),
                    content: text,
                });
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

    /// Remove any "thinking" messages from the transcript.
    fn remove_thinking(&mut self) {
        self.messages.retain(|m| m.role != "thinking");
    }

    fn handle_slash(&mut self, cmd: &str) {
        let lower = cmd.to_lowercase();

        match lower.as_str() {
            "/help" | "/h" | "/?" => {
                self.messages.push(Message {
                    role: "system".to_string(),
                    content: "Fusion commands:\n  /help     this help\n  /yolo     toggle auto-approve\n  /plan     enter plan mode\n  /model <name>  switch model\n  /status   current settings\n  /clear    clear messages\n  /exit     quit".to_string(),
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
            "/status" => {
                let mode_str = match self.mode {
                    AppMode::Normal => "Normal",
                    AppMode::Plan => "Plan",
                    AppMode::Yolo => "YOLO",
                };
                self.messages.push(Message {
                    role: "system".to_string(),
                    content: format!("model: {}  mode: {}", self.model, mode_str),
                });
            }
            "/exit" | "/quit" | "/q" => {
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

/// Run the full Ratatui TUI.
pub async fn run_tui(config: &Config) -> anyhow::Result<()> {
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
    let mut app = App::new(config, event_tx);

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
                AppEvent::Resize(_, _) => {
                    // Ratatui handles resize automatically on next draw
                }
                AppEvent::Tick => {
                    // Could update spinner animation here
                }
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
