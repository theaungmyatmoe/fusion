# Fusion

Fusion is a terminal-based AI coding assistant inspired by OpenAI Codex CLI. It runs as a **single static binary** with no external dependencies.

## Install

To install Fusion on **macOS**, **Linux**, **Alpine Linux**, **Android (Termux)**, or **iOS (iSH / UTM VMs)**, run the following one-line command:

```bash
curl -sSL https://raw.githubusercontent.com/theaungmyatmoe/fusion/main/scripts/install.sh | sh
```

> **Note** — The installer automatically detects your platform, checks for and installs missing dependencies (like `git`, `ripgrep`, and `ca-certificates` on Alpine/iSH or Termux), downloads the optimized precompiled binary, and registers it. On iOS (iSH), Fusion automatically falls back to a lightweight REPL interface.

### Android — Secure Alpine Sandbox (Optional)

To isolate Fusion's shell execution in a secure Alpine container (protecting your phone's directories from agent commands):

```bash
# 1. Install & enter Alpine sandbox
pkg install proot-distro
proot-distro install alpine
proot-distro login alpine

# 2. Install Fusion inside sandbox
curl -sSL https://raw.githubusercontent.com/theaungmyatmoe/fusion/main/scripts/install.sh | sh
```

### Build From Source

```bash
git clone https://github.com/theaungmyatmoe/fusion.git
cd fusion
cargo build --release
```

---

## Configuration

Create `fusion.toml` in your project directory or at `~/.config/fusion/fusion.toml`:

```toml
model = "@cf/moonshotai/kimi-k2.7-code"
yolo = false

[provider]
default = "cloudflare"

[provider.cloudflare]
account_id = "your_account_id"
api_key = "your_api_token"

[provider.xai]
api_key = "xai-your-key"

[settings]
# Optional agent/delegation tuning
agent_pacing_ms = 150
subagent_max_rounds = 12
subagent_timeout_secs = 900
subagent_verify_timeout_secs = 120
# Rate-limit protection when spawning sub-agents
swarm_max_concurrency = 2
swarm_spawn_stagger_ms = 750
llm_max_concurrent = 2
```

### Environment Variables

| Variable | Description |
|---|---|
| `CLOUDFLARE_ACCOUNT_ID` | Cloudflare account ID |
| `CLOUDFLARE_API_TOKEN` | Cloudflare API token |
| `XAI_API_KEY` | xAI / Grok API key |
| `FUSION_MODEL` | Override model ID |
| `FUSION_YOLO=1` | Enable auto-approve mode |

---

## Usage

```bash
fusion                    # Launch Ratatui TUI (default)
fusion --simple           # Launch lightweight scrollback REPL
fusion -p "your task"     # Execute task headless
fusion --model grok-3     # Override model
fusion --yolo             # Auto-approve all shell executions
fusion --tasks            # List background tasks and sub-agent sessions
fusion --resume-task <id> # Resume a sub-agent task session by ID
fusion --upgrade          # Self-upgrade to the latest version
```

### Chat Commands

| Command | Description |
|---|---|
| `/help` | List commands |
| `/plan` | Enter plan / thinking mode |
| `/yolo` | Toggle auto-approve mode |
| `/model <name>` | Switch LLM model |
| `/status` | View current settings |
| `/exit` | Quit |

### TUI Keymaps

| Key | Action |
|---|---|
| `Tab` | Toggle between **Normal** (`Enter:send`) and **Plan** (`Enter:plan`) modes |
| `Shift+Tab` | Cycle through all modes: **Normal → Plan → YOLO** |
| `Cmd+V` / `Ctrl+V` | Paste text from clipboard |
| `Ctrl+G` | Save and attach clipboard image (saved as `.png`, rendered as `[Image #N]`) |
| `Ctrl+E` | Open `$EDITOR` to compose input in a full-screen editor |
| `Up` / `Down` | Cycle through input history |
| `Backspace` | Atomically delete inline tag placeholders (e.g. `[Image #1]`) |

---

## License

MIT

