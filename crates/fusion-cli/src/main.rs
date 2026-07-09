use clap::Parser;
use fusion_core::config::{is_termux, load_config};
use fusion_core::session::Session;
use std::path::PathBuf;

/// Fusion — mobile-first AI coding agent for Termux (and terminals).
/// Inspired by OpenAI Codex CLI. Built in Rust with Ratatui.
#[derive(Parser, Debug)]
#[command(name = "fusion", version, about, long_about = None)]
struct Cli {
    /// Use the rich Ratatui TUI (default on most terminals)
    #[arg(long)]
    tui: bool,

    /// Force the simple mobile-friendly terminal REPL
    #[arg(long)]
    simple: bool,

    /// Override model (e.g. grok-3, @cf/zhipu-ai/glm-4)
    #[arg(long, short = 'm')]
    model: Option<String>,

    /// Auto-approve all tool actions (dangerous but fast)
    #[arg(long)]
    yolo: bool,

    /// Working directory
    #[arg(long)]
    cwd: Option<PathBuf>,

    /// Headless prompt (non-interactive)
    #[arg(short = 'p', long = "prompt")]
    prompt: Option<String>,

    /// Resume a previous session by ID (or first 8 chars)
    #[arg(long)]
    resume: Option<String>,

    /// Resume the most recent session
    #[arg(long)]
    last: bool,

    /// List recent sessions and exit
    #[arg(long)]
    sessions: bool,

    /// Remaining arguments form an initial prompt
    #[arg(trailing_var_arg = true)]
    args: Vec<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Change working directory if specified
    if let Some(ref cwd) = cli.cwd {
        std::env::set_current_dir(cwd)?;
    }

    let current_dir = std::env::current_dir()?;
    let mut config = load_config(&current_dir)
        .map_err(|e| anyhow::anyhow!("config warning: {}", e))
        .unwrap_or_else(|e| {
            eprintln!("{}", e);
            fusion_core::config::Config {
                provider: fusion_core::config::Provider::Cloudflare,
                model: "@cf/moonshotai/kimi-k2.7-code".to_string(),
                small_model: None,
                api_key: String::new(),
                base_url: String::new(),
                cloudflare_account_id: None,
                yolo: false,
                config_path: None,
                settings: Default::default(),
            }
        });

    // CLI flags override config
    if let Some(ref model) = cli.model {
        config.model = model.clone();
    }
    if cli.yolo {
        config.yolo = true;
    }

    // List sessions and exit
    if cli.sessions {
        match fusion_core::session::list_sessions() {
            Ok(sessions) => {
                if sessions.is_empty() {
                    println!("No saved sessions.");
                } else {
                    println!("Recent sessions:");
                    for (i, s) in sessions.iter().take(20).enumerate() {
                        let age = format_age(s.updated_at);
                        println!(
                            "  {}. {} ({} msgs, {}) {}",
                            i + 1,
                            &s.id[..8.min(s.id.len())],
                            s.message_count,
                            age,
                            s.preview
                        );
                    }
                    println!();
                    println!("Resume: fusion --resume <id>");
                }
            }
            Err(e) => {
                eprintln!("Error listing sessions: {}", e);
                std::process::exit(1);
            }
        }
        return Ok(());
    }

    // Resolve session for resume
    let resume_session = if cli.last {
        match Session::load_last() {
            Ok(session) => {
                eprintln!("Resuming session {}…", session.short_id());
                Some(session)
            }
            Err(e) => {
                eprintln!("Cannot resume: {}", e);
                None
            }
        }
    } else if let Some(ref id) = cli.resume {
        // Try exact match, then prefix match
        match Session::load(id) {
            Ok(session) => Some(session),
            Err(_) => {
                // Try prefix match against all sessions
                match fusion_core::session::list_sessions() {
                    Ok(sessions) => {
                        let matched = sessions.iter().find(|s| s.id.starts_with(id));
                        match matched {
                            Some(s) => match Session::load(&s.id) {
                                Ok(session) => {
                                    eprintln!("Resuming session {}…", session.short_id());
                                    Some(session)
                                }
                                Err(e) => {
                                    eprintln!("Cannot resume {}: {}", id, e);
                                    None
                                }
                            },
                            None => {
                                eprintln!("Session '{}' not found. Use --sessions to list.", id);
                                None
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        None
                    }
                }
            }
        }
    } else {
        None
    };

    // Headless mode
    let prompt = cli
        .prompt
        .or_else(|| {
            if cli.args.is_empty() {
                None
            } else {
                Some(cli.args.join(" "))
            }
        });

    if let Some(prompt_text) = prompt {
        let cwd = current_dir.to_string_lossy().to_string();
        let mut agent = fusion_agent::agent::Agent::new(&config, cwd);
        let (agent_tx, mut agent_rx) = tokio::sync::mpsc::unbounded_channel();
        let forwarder = tokio::spawn(async move {
            while let Some(event) = agent_rx.recv().await {
                match event {
                    fusion_agent::agent::AgentEvent::FinalResponse(text) => {
                        println!("{}", text);
                    }
                    fusion_agent::agent::AgentEvent::ToolCall { name, args_preview } => {
                        let preview_truncated = if args_preview.chars().count() > 200 {
                            let truncated: String = args_preview.chars().take(200).collect();
                            format!("{}...", truncated)
                        } else {
                            args_preview.clone()
                        };
                        eprintln!("[tool] {} {}", name, preview_truncated);
                    }
                    _ => {}
                }
            }
        });

        if let Err(e) = agent.process(&prompt_text, agent_tx).await {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
        let _ = forwarder.await;
        return Ok(());
    }

    // Interactive mode
    let termux = is_termux();
    let use_simple = cli.simple || (termux && !cli.tui);

    if use_simple {
        if termux {
            println!("fusion — Termux detected. Using lightweight REPL.");
        }
        fusion_tui::simple::run_simple(&config).await?;
    } else {
        if termux {
            eprintln!("Note: Running rich TUI on Termux. Use --simple if cramped.");
        }
        fusion_tui::app::run_tui_with_session(&config, resume_session).await?;
    }

    Ok(())
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
