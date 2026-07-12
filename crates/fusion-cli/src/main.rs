use clap::Parser;
use fusion_core::config::load_config;
use fusion_core::session::Session;
use std::io::Write;
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

    /// Upgrade Fusion to the latest release version
    #[arg(long)]
    upgrade: bool,

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

    /// List all background tasks and sub-agent sessions
    #[arg(long)]
    tasks: bool,

    /// Resume a sub-agent task session by ID
    #[arg(long = "resume-task")]
    resume_task: Option<String>,

    /// Remaining arguments form an initial prompt
    #[arg(trailing_var_arg = true)]
    args: Vec<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // In case the terminal was left in raw mode from a previous crashed run:
    let _ = crossterm::terminal::disable_raw_mode();
    let _ = crossterm::execute!(std::io::stdout(), crossterm::event::DisableBracketedPaste);

    // Set a panic hook to clean up the terminal raw mode before exiting
    std::panic::set_hook(Box::new(move |panic_info| {
        // Restore terminal raw mode and leave alternate screen
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::event::DisableMouseCapture,
            crossterm::event::DisableBracketedPaste,
            crossterm::cursor::Show
        );

        // Format panic message
        let payload = panic_info.payload();
        let msg = if let Some(s) = payload.downcast_ref::<&str>() {
            *s
        } else if let Some(s) = payload.downcast_ref::<String>() {
            s.as_str()
        } else {
            "Box<dyn Any>"
        };

        let location = panic_info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown location".to_string());

        let backtrace = std::backtrace::Backtrace::capture();

        // Write to fusion.log
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("fusion.log")
        {
            let _ = writeln!(file, "=== PANIC ===");
            if let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
                let _ = writeln!(file, "Timestamp: {}", now.as_secs());
            }
            let _ = writeln!(file, "Location: {}", location);
            let _ = writeln!(file, "Message: {}", msg);
            let _ = writeln!(file, "Backtrace:\n{}", backtrace);
            let _ = writeln!(file, "=============\n");
        }

        // Print nice error message
        eprintln!("\n\x1b[31;1mFusion panicked:\x1b[0m {}", msg);
        eprintln!("Location: {}", location);
        eprintln!("Backtrace and details written to \x1b[33mfusion.log\x1b[0m\n");
    }));

    let cli = Cli::parse();

    if cli.upgrade {
        if let Err(e) = run_upgrade().await {
            eprintln!("\x1b[31mUpgrade failed:\x1b[0m {}", e);
            std::process::exit(1);
        }
        return Ok(());
    }

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

    // List tasks and exit
    if cli.tasks {
        match fusion_core::task_session::list_task_sessions() {
            Ok(tasks) => {
                if tasks.is_empty() {
                    println!("No task sessions.");
                } else {
                    println!("Recent tasks:");
                    for (i, t) in tasks.iter().take(20).enumerate() {
                        let age = format_age(t.updated_at);
                        println!(
                            "  {}. {} ({} msgs, {}, status: {}) - [{}] {}",
                            i + 1,
                            &t.task_id[..8.min(t.task_id.len())],
                            t.message_count,
                            age,
                            t.status,
                            t.persona,
                            t.description
                        );
                    }
                    println!();
                    println!("Resume: fusion --resume-task <id>");
                }
            }
            Err(e) => {
                eprintln!("Error listing tasks: {}", e);
                std::process::exit(1);
            }
        }
        return Ok(());
    }

    // Resume sub-agent task and exit
    if let Some(ref task_id_input) = cli.resume_task {
        let task_session = match fusion_core::task_session::TaskSession::load(task_id_input) {
            Ok(ts) => ts,
            Err(_) => {
                match fusion_core::task_session::list_task_sessions() {
                    Ok(tasks) => {
                        let matched = tasks.iter().find(|t| t.task_id.starts_with(task_id_input));
                        match matched {
                            Some(ts_summary) => {
                                match fusion_core::task_session::TaskSession::load(&ts_summary.task_id) {
                                    Ok(ts) => ts,
                                    Err(e) => {
                                        eprintln!("Cannot load task {}: {}", task_id_input, e);
                                        std::process::exit(1);
                                    }
                                }
                            }
                            None => {
                                eprintln!("Task session '{}' not found. Use --tasks to list.", task_id_input);
                                std::process::exit(1);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Error listing tasks: {}", e);
                        std::process::exit(1);
                    }
                }
            }
        };

        let persona_name = &task_session.persona;
        let persona = match fusion_agent::persona::get_persona(persona_name) {
            Some(p) => p,
            None => {
                eprintln!("Unknown persona: {}", persona_name);
                std::process::exit(1);
            }
        };

        let cwd = task_session.cwd.clone();
        let task_desc = task_session.description.clone();
        let task_id = task_session.task_id.clone();

        eprintln!("Resuming sub-agent task {} [{}]...", task_id, persona_name);

        let (agent_tx, agent_rx) = tokio::sync::mpsc::unbounded_channel();
        let forwarder = tokio::spawn(async move {
            use std::io::Write;
            let mut agent_rx = agent_rx;
            while let Some(event) = agent_rx.recv().await {
                match event {
                    fusion_agent::agent::AgentEvent::Thinking(text) => {
                        eprint!("{}", text);
                        let _ = std::io::stderr().flush();
                    }
                    fusion_agent::agent::AgentEvent::TextDelta(text) => {
                        print!("{}", text);
                        let _ = std::io::stdout().flush();
                    }
                    fusion_agent::agent::AgentEvent::ToolCall { name, args_preview } => {
                        eprintln!("\n[tool call] {} {}", name, args_preview);
                    }
                    fusion_agent::agent::AgentEvent::ToolOutputDelta { name: _, output } => {
                        eprint!("{}", output);
                        let _ = std::io::stderr().flush();
                    }
                    fusion_agent::agent::AgentEvent::ToolResult { name, output } => {
                        eprintln!("\n[tool result] {}\n{}", name, output);
                    }
                    fusion_agent::agent::AgentEvent::TaskCompleted { task_id: _, summary } => {
                        println!("\nTask completed successfully:\n{}", summary);
                    }
                    _ => {}
                }
            }
        });

        let result = fusion_agent::swarm::run_sub_agent_standalone(
            &persona,
            &task_desc,
            Some(&task_id),
            &config,
            &cwd,
            agent_tx,
        )
        .await;

        let _ = forwarder.await;

        match result.status {
            fusion_core::task_session::TaskStatus::Completed => {
                println!("Sub-agent task execution finished successfully.");
            }
            fusion_core::task_session::TaskStatus::Failed(e) => {
                eprintln!("Sub-agent task execution failed: {}", e);
                std::process::exit(1);
            }
            _ => {}
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
        let (agent_tx, agent_rx) = tokio::sync::mpsc::unbounded_channel();
        let forwarder = tokio::spawn(async move {
            use std::io::Write;
            let mut streamed_text = false;
            let mut streamed_tool_output = false;
            let mut agent_rx = agent_rx;
            while let Some(event) = agent_rx.recv().await {
                match event {
                    fusion_agent::agent::AgentEvent::Thinking(text) => {
                        eprint!("{}", text);
                        let _ = std::io::stderr().flush();
                    }
                    fusion_agent::agent::AgentEvent::TextDelta(text) => {
                        streamed_text = true;
                        print!("{}", text);
                        let _ = std::io::stdout().flush();
                    }
                    fusion_agent::agent::AgentEvent::FinalResponse(text) => {
                        if streamed_text {
                            if !text.ends_with('\n') {
                                println!();
                            }
                        } else {
                            println!("{}", text);
                        }
                    }
                    fusion_agent::agent::AgentEvent::ToolCall { name, args_preview } => {
                        let preview_truncated = if args_preview.chars().count() > 200 {
                            let truncated: String = args_preview.chars().take(200).collect();
                            format!("{}...", truncated)
                        } else {
                            args_preview.clone()
                        };
                        streamed_tool_output = false;
                        eprintln!("[tool] {} {}", name, preview_truncated);
                    }
                    fusion_agent::agent::AgentEvent::ToolOutputDelta { name: _, output } => {
                        streamed_tool_output = true;
                        eprint!("{}", output);
                        let _ = std::io::stderr().flush();
                    }
                    fusion_agent::agent::AgentEvent::ToolResult { name, output } => {
                        if !streamed_tool_output {
                            eprintln!("[tool result] {}\n{}", name, output);
                        } else if !output.ends_with('\n') {
                            eprintln!();
                        }
                    }
                    fusion_agent::agent::AgentEvent::TodoUpdate(_) => {}
                    fusion_agent::agent::AgentEvent::TaskSpawned { task_id, persona, description } => {
                        let short_id = if task_id.len() >= 8 { &task_id[..8] } else { &task_id };
                        eprintln!("[swarm task spawned: {} ({})] {}", short_id, persona, description);
                    }
                    fusion_agent::agent::AgentEvent::TaskProgress { task_id, event } => {
                        let short_id = if task_id.len() >= 8 { &task_id[..8] } else { &task_id };
                        match *event {
                            fusion_agent::agent::AgentEvent::TextDelta(text) => {
                                print!("[{}] {}", short_id, text);
                                let _ = std::io::stdout().flush();
                            }
                            _ => {}
                        }
                    }
                    fusion_agent::agent::AgentEvent::TaskCompleted { task_id, summary } => {
                        let short_id = if task_id.len() >= 8 { &task_id[..8] } else { &task_id };
                        eprintln!("[swarm task completed: {}]\n{}", short_id, summary);
                    }
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

    // Interactive mode — TUI is the default on all platforms
    let use_simple = cli.simple;

    if use_simple {
        println!("fusion — Using lightweight REPL.");
        fusion_tui::simple::run_simple(&config).await?;
    } else {
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

async fn run_upgrade() -> anyhow::Result<()> {
    let current_version = env!("CARGO_PKG_VERSION");
    println!("Current version: v{}", current_version);
    println!("Checking for latest release on GitHub...");

    let client = reqwest::Client::new();
    let response = client
        .get("https://api.github.com/repos/theaungmyatmoe/fusion/releases/latest")
        .header("User-Agent", "fusion-upgrade")
        .send()
        .await?;

    if !response.status().is_success() {
        anyhow::bail!("Failed to fetch latest release: {}", response.status());
    }

    let release: serde_json::Value = response.json().await?;
    let latest_version = release["tag_name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("tag_name not found in release payload"))?
        .trim_start_matches('v')
        .to_string();

    println!("Latest version:  v{}", latest_version);

    if latest_version == current_version {
        println!("You are already running the latest version of Fusion.");
        return Ok(());
    }

    // Determine target platform
    let os = if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "android") || fusion_core::config::is_termux() {
        "termux"
    } else if fusion_core::config::is_ish() {
        "alpine"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        anyhow::bail!("Self-upgrade is not supported on this platform.");
    };

    let arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else if cfg!(target_arch = "x86") {
        "i686"
    } else {
        anyhow::bail!("Self-upgrade is not supported on this architecture.");
    };

    let target = match os {
        "termux" => format!("{}-linux-android", arch),
        "alpine" => "i686-unknown-linux-musl".to_string(), // iSH is always 32-bit x86 musl
        "linux" => format!("{}-unknown-linux-musl", arch),
        "macos" => format!("{}-apple-darwin", arch),
        _ => anyhow::bail!("Unsupported platform"),
    };

    let asset_name = format!("fusion-{}", target);
    println!("Looking for asset: {}", asset_name);

    let assets = release["assets"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("No assets found in release payload"))?;

    let asset = assets
        .iter()
        .find(|a| a["name"].as_str().map(|n| n == asset_name).unwrap_or(false))
        .ok_or_else(|| anyhow::anyhow!("No release asset found for target {}", target))?;

    let download_url = asset["browser_download_url"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("browser_download_url not found in asset payload"))?;

    println!("Downloading new binary...");
    let bytes = client
        .get(download_url)
        .header("User-Agent", "fusion-upgrade")
        .send()
        .await?
        .bytes()
        .await?;

    let current_exe = std::env::current_exe()?;
    let exe_dir = current_exe.parent().ok_or_else(|| anyhow::anyhow!("Cannot find executable directory"))?;
    let temp_path = exe_dir.join("fusion.tmp");

    println!("Writing new binary to {}...", temp_path.display());
    std::fs::write(&temp_path, &bytes)?;

    // Swap the running binary
    let backup_path = exe_dir.join("fusion.old");
    if current_exe.exists() {
        // Rename the old running binary first (Unix allows renaming a running executable)
        std::fs::rename(&current_exe, &backup_path)?;
    }

    if let Err(e) = std::fs::rename(&temp_path, &current_exe) {
        // Rollback on failure
        if backup_path.exists() {
            let _ = std::fs::rename(&backup_path, &current_exe);
        }
        anyhow::bail!("Failed to swap binaries: {}", e);
    }

    // Set execute permissions on Unix systems
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&current_exe)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&current_exe, perms)?;
    }

    // Clean up backup file
    let _ = std::fs::remove_file(backup_path);

    println!("Upgrade successful! Fusion has been upgraded to v{}.", latest_version);
    Ok(())
}
