use clap::Parser;
use fusion_core::config::{is_termux, load_config};
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
            // Return a minimal default config
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
        // Headless: run agent once and print result
        let cwd = current_dir.to_string_lossy().to_string();
        let mut agent = fusion_agent::agent::Agent::new(&config, cwd);
        match agent.process(&prompt_text).await {
            Ok(events) => {
                for event in events {
                    match event {
                        fusion_agent::agent::AgentEvent::FinalResponse(text) => {
                            println!("{}", text);
                        }
                        fusion_agent::agent::AgentEvent::ToolCall { name, args_preview } => {
                            eprintln!("[tool] {} {}", name, &args_preview[..args_preview.len().min(200)]);
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        return Ok(());
    }

    // Interactive mode
    let termux = is_termux();
    let use_simple = cli.simple || (termux && !cli.tui);

    if use_simple {
        // Simple REPL — stays in normal scrollback
        if termux {
            println!("fusion — Termux detected. Using lightweight REPL.");
        }
        fusion_tui::simple::run_simple(&config).await?;
    } else {
        // Rich Ratatui TUI
        if termux {
            eprintln!("Note: Running rich TUI on Termux. Use --simple if cramped.");
        }
        fusion_tui::app::run_tui(&config).await?;
    }

    Ok(())
}
