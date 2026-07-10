# Fusion

Fusion is a terminal-based AI coding assistant inspired by OpenAI Codex CLI. It runs as a single static binary with no external dependencies.

## Installation

### One-Line Installer (macOS, Linux, Android/Termux, Alpine)
Installs the latest pre-compiled release binary directly to your path:
```sh
curl -sSL https://raw.githubusercontent.com/theaungmyatmoe/fusion/main/scripts/install.sh | sh
```

### iOS (UTM Virtual Machine — Recommended for TUI)
To run the full Ratatui TUI on iOS, install a virtualized Ubuntu or Alpine Linux VM in **UTM** or **UTM SE** (App Store), and run the standard installer:
```sh
curl -sSL https://raw.githubusercontent.com/theaungmyatmoe/fusion/main/scripts/install.sh | sh
```

### iOS (iSH Alpine Linux — Simple REPL Fallback)
iSH has raw-mode terminal emulation limits that prevent the TUI from launching. Use this bootstrap command to set up dependencies and install the lightweight REPL fallback:
```sh
curl -sSL https://raw.githubusercontent.com/theaungmyatmoe/fusion/main/scripts/ish-bootstrap.sh | sh
```

### Android (Termux Alpine Sandbox)
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

## Configuration

Configure credentials and settings by creating a `fusion.toml` in your project directory or in `~/.config/fusion/fusion.toml`:

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
```

### Environment Variables
Environment variables override settings in `fusion.toml`:
```bash
CLOUDFLARE_ACCOUNT_ID    # Cloudflare account ID
CLOUDFLARE_API_TOKEN     # Cloudflare API token
XAI_API_KEY              # xAI/Grok API key
FUSION_MODEL             # Override model ID
FUSION_YOLO=1            # Enable auto-approve mode
```

## Usage

```bash
fusion                    # Launch Ratatui TUI (default)
fusion --simple           # Launch lightweight scrollback REPL
fusion -p "your task"     # Execute task headless
fusion --model grok-3     # Override model
fusion --yolo             # Auto-approve all shell executions
fusion --upgrade          # Self-upgrade Fusion to the latest version
```

### Chat Commands
* `/help` — List commands
* `/plan` — Enter plan/thinking mode
* `/yolo` — Toggle auto-approve mode
* `/model <name>` — Switch LLM model
* `/status` — View current settings
* `/exit` — Quit

### TUI Keymaps
* **`Tab`** — Toggle silently between **Normal** (`Enter:send`) and **Plan** (`Enter:plan`) modes.
* **`Shift+Tab`** — Cycle through all TUI modes (**Normal** -> **Plan** -> **YOLO**).
* **`Cmd+V`** / **`Ctrl+V`** — Paste text from the clipboard.
* **`Ctrl+V`** / **`Ctrl+G`** — Save and attach clipboard images instantly (saved as `.png` files and rendered chronologically in-line as `[Image #N]` tags).
* **`Ctrl+E`** — Open your system editor (e.g. VS Code, Vim, Nano based on `$EDITOR` env) to compose or edit your current input line in a full screen editor.
* **`Up`** / **`Down`** — Cycle through your previous input command history.
* **`Backspace`** — Atomically deletes in-line tag placeholders (like `[Image #1]`) and cleans them up from the attachment list.

## Architecture

* `crates/fusion-core` — Shared types, configs, and search-replace safety validation.
* `crates/fusion-llm` — API client for Cloudflare and OpenAI-compatible endpoints.
* `crates/fusion-agent` — Agent loops, state, and tool integrations (`read_file`, `write_file`, `search_replace`, `grep`, `get_symbols`, `run_command`).
* `crates/fusion-tui` — Ratatui TUI and REPL views.
* `crates/fusion-cli` — Argument parser and main entry point.

## License

MIT
