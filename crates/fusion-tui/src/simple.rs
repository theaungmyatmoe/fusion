use fusion_agent::agent::{Agent, AgentEvent};
use fusion_core::config::Config;

/// Lightweight simple REPL — stays in normal terminal scrollback.
/// Perfect for Termux phones where finger scroll + copy need to work.
pub async fn run_simple(config: &Config) -> anyhow::Result<()> {
    let cwd = std::env::current_dir()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let is_termux = fusion_core::config::is_termux();
    let platform = if is_termux {
        "Termux (mobile)"
    } else {
        "terminal"
    };

    println!(
        "\x1b[38;2;124;58;237;1mfusion\x1b[0m  \x1b[90mmobile-first coding agent for {}\x1b[0m",
        platform
    );

    let mode = if config.yolo { "YOLO" } else { "Normal" };
    println!("model: {}   mode: {}", config.model, mode);

    if let Some(ref path) = config.config_path {
        println!("\x1b[90mconfig: {}\x1b[0m", path.display());
    }

    // Check for missing Cloudflare creds
    let needs_cf = config.model.starts_with("@cf/")
        || config.model.contains("kimi")
        || matches!(config.provider, fusion_core::config::Provider::Cloudflare);
    if needs_cf && config.cloudflare_account_id.is_none() {
        println!("\x1b[33m⚠  No Cloudflare credentials found for the default Kimi route.\x1b[0m");
        println!("\x1b[90m   Set them in fusion.toml or export CLOUDFLARE_ACCOUNT_ID + CLOUDFLARE_API_TOKEN\x1b[0m");
        println!();
    }

    println!("\x1b[90mEverything stays in normal scrollback (finger scroll + copy work great). Use /help.\x1b[0m");
    println!();

    let mut agent = Agent::new(config, cwd);
    let mut rl = rustyline::DefaultEditor::new()?;
    let mut yolo = config.yolo;
    let mut current_mode = mode.to_string();
    let mut pending_image: Option<String> = None;

    loop {
        let readline = rl.readline("\x1b[38;2;124;58;237;1mz>\x1b[0m ");
        let line = match readline {
            Ok(line) => line,
            Err(rustyline::error::ReadlineError::Interrupted) => {
                println!("\x1b[90mbye (Ctrl+C)\x1b[0m");
                break;
            }
            Err(rustyline::error::ReadlineError::Eof) => {
                println!("\x1b[90mbye (Ctrl+D)\x1b[0m");
                break;
            }
            Err(err) => {
                println!("\x1b[31mReadline error:\x1b[0m {:?}", err);
                break;
            }
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let _ = rl.add_history_entry(trimmed);

        // Slash commands
        if trimmed.starts_with('/') {
            let lower = trimmed.to_lowercase();
            match lower.as_str() {
                "/exit" | "/quit" | "/q" => {
                    println!("\x1b[90mbye\x1b[0m");
                    break;
                }
                "/help" | "/h" | "/?" => {
                    println!("\x1b[38;2;124;58;237mFusion commands (mobile-friendly):\x1b[0m");
                    println!("  /help            show this");
                    println!("  /yolo            toggle auto-approve");
                    println!("  /plan            enter plan mode");
                    println!("  /model <name>    switch model");
                    println!("  /status          current settings");
                    println!("  /image           insert clipboard image (macOS)");
                    println!("  /exit, /quit     leave");
                    println!();
                    println!("\x1b[90mJust type to chat with the agent.\x1b[0m");
                }
                "/yolo" => {
                    yolo = !yolo;
                    current_mode = if yolo { "YOLO".to_string() } else { "Normal".to_string() };
                    if yolo {
                        println!("\x1b[33mYOLO mode ON ⚡\x1b[0m");
                    } else {
                        println!("YOLO mode OFF");
                    }
                }
                "/plan" => {
                    current_mode = "Plan".to_string();
                    println!("\x1b[38;2;124;58;237mPlan mode:\x1b[0m agent will explore but not edit until you approve.");
                }
                "/status" => {
                    println!("model={}  mode={}", config.model, current_mode);
                }
                "/image" => {
                    let cwd = std::env::current_dir()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    match crate::clipboard::save_clipboard_image(&cwd) {
                        Ok(path) => {
                            let filename = path.file_name().unwrap_or_default().to_string_lossy();
                            println!("\x1b[32mSaved clipboard image to ./{}\x1b[0m", filename);
                            println!("\x1b[90mWill be automatically attached to your next prompt.\x1b[0m");
                            let link = format!(" [image](file://{})", path.to_string_lossy());
                            pending_image = Some(link);
                        }
                        Err(e) => {
                            println!("\x1b[31mError saving image:\x1b[0m {}", e);
                        }
                    }
                }
                _ if lower.starts_with("/model ") => {
                    let new_model = trimmed[7..].trim();
                    println!("Model → {}", new_model);
                }
                _ => {
                    println!("\x1b[90mUnknown command. /help for list.\x1b[0m");
                }
            }
            continue;
        }

        // User message
        let prompt_text = if let Some(ref link) = pending_image {
            let full = format!("{} {}", trimmed, link);
            println!("\x1b[38;2;59;130;246;1mYou:\x1b[0m {} \x1b[90m(attached clipboard image)\x1b[0m", trimmed);
            pending_image = None;
            full
        } else {
            println!("\x1b[38;2;59;130;246;1mYou:\x1b[0m {}", trimmed);
            trimmed.to_string()
        };

        let (agent_tx, mut agent_rx) = tokio::sync::mpsc::unbounded_channel();
        let printer = tokio::spawn(async move {
            use std::io::Write;
            let mut is_first_thinking = true;
            let mut is_first_text = true;

            while let Some(event) = agent_rx.recv().await {
                match event {
                    AgentEvent::Thinking(text) => {
                        if is_first_thinking {
                            print!("\x1b[38;2;139;92;246m🤔 Thinking:\x1b[0m \x1b[90m");
                            is_first_thinking = false;
                        }
                        print!("{}", text);
                        let _ = std::io::stdout().flush();
                    }
                    AgentEvent::TextDelta(text) => {
                        if !is_first_thinking {
                            // Close thinking tag
                            print!("\x1b[0m\n");
                            is_first_thinking = true;
                        }
                        if is_first_text {
                            print!("\x1b[32mAgent:\x1b[0m ");
                            is_first_text = false;
                        }
                        print!("{}", text);
                        let _ = std::io::stdout().flush();
                    }
                    AgentEvent::ToolCall { name, args_preview } => {
                        if !is_first_thinking {
                            print!("\x1b[0m\n");
                            is_first_thinking = true;
                        }
                        is_first_text = true;
                        println!("\x1b[36m[tool] {}\x1b[0m {}", name, &args_preview[..args_preview.len().min(200)]);
                    }
                    AgentEvent::ToolResult { name: _, output } => {
                        let cleaned = output.replace("\r\n", "\n");
                        let truncated = if cleaned.len() > 1000 {
                            let truncated_str: String = cleaned.chars().take(1000).collect();
                            format!("{}...\n[output truncated]", truncated_str)
                        } else {
                            cleaned
                        };
                        for line in truncated.lines() {
                            println!("\x1b[90m  → {}\x1b[0m", line);
                        }
                    }
                    AgentEvent::FinalResponse(text) => {
                        if !is_first_thinking {
                            print!("\x1b[0m\n");
                        }
                        if is_first_text {
                            println!("\x1b[32mAgent:\x1b[0m {}", text);
                        } else {
                            println!();
                        }
                    }
                    AgentEvent::TodoUpdate(todos) => {
                        println!("\x1b[33mTodos:\x1b[0m");
                        for t in &todos {
                            let icon = match t.status.as_str() {
                                "done" => "✓",
                                "in_progress" => "→",
                                _ => "○",
                            };
                            println!("  {} {}", icon, t.content);
                        }
                    }
                }
            }
        });

        if let Err(e) = agent.process(&prompt_text, agent_tx).await {
            let msg = format!("{}", e);
            println!("\n\x1b[31mAgent error:\x1b[0m {}", msg.lines().next().unwrap_or(&msg));
        }

        let _ = printer.await;
    }

    Ok(())
}
