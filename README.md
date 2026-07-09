# Fusion

**Fusion** is a **mobile-first, Termux-native AI coding agent** built in **Rust**, inspired by [OpenAI Codex CLI](https://github.com/openai/codex).

Single static binary. Zero runtime dependencies. Runs natively on Termux (aarch64 Android) — just copy the binary and go.

## Features

- **Ratatui TUI** (default) — Codex-style rich terminal UI with status bar, scrollable messages, and input box
- **Simple REPL** (`--simple`) — lightweight fallback that stays in normal scrollback (perfect for phones)
- **Multi-provider LLM routing** — Cloudflare Workers AI (Kimi 2.7 Code), xAI/Grok, or any OpenAI-compatible API
- **Safe editing** — `search_replace` tool requires `old_string` to match exactly once (prevents ambiguous edits)
- **Agent tools** — `read_file`, `write_file`, `search_replace`, `grep` (ripgrep), `get_symbols`, `run_command`, `todo_write`
- **Permission prompts** — shell commands require approval unless YOLO mode is on
- **Slash commands** — `/help`, `/yolo`, `/plan`, `/model`, `/status`, `/exit` (easy on soft keyboards)
- **TOML config** (Codex-style) — `fusion.toml` as primary, with JSON fallback for backward compat
- **Single binary** — `cargo build --release` produces a ~5-10MB static binary

## Quick Start

### From source (any platform)

```bash
git clone https://github.com/aungmyatmoe/zencode.git
cd zencode
cargo build --release
./target/release/fusion
```

### On Termux (Android)

```bash
# Install Rust
pkg install rust ripgrep
git clone https://github.com/aungmyatmoe/zencode.git ~/.fusion
cd ~/.fusion
cargo build --release

# Set up Cloudflare creds for default Kimi 2.7 route
export CLOUDFLARE_ACCOUNT_ID=your_id
export CLOUDFLARE_API_TOKEN=your_token

# Run
./target/release/fusion
```

## Usage

```bash
fusion                    # default: rich Ratatui TUI
fusion --simple           # lightweight REPL (best on small phone screens)
fusion -p "your task"     # headless mode (non-interactive)
fusion --model grok-3     # override model
fusion --yolo             # auto-approve all tool actions
```

### Inside the REPL

Talk normally or use slash commands:

```
/help            show commands
/yolo            toggle auto-approve
/plan            enter plan mode
/model <name>    switch model
/status          current settings
/exit            quit
```

## Configuration

Fusion uses **TOML** as its primary config format (like Codex CLI). Create a `fusion.toml`:

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

### Config file locations (searched in order)

**Project-level** (highest priority):
- `./fusion.toml`
- `./fusion.json`
- `./zencode.json` (backward compat)

**Global**:
- `~/.config/fusion/fusion.toml`
- `~/.fusion/fusion.toml`

### Environment variables (always win)

```bash
CLOUDFLARE_ACCOUNT_ID    # Cloudflare Workers AI account
CLOUDFLARE_API_TOKEN     # Cloudflare API token
XAI_API_KEY              # xAI/Grok API key
FUSION_MODEL             # override model
FUSION_PROVIDER          # force provider (cloudflare/xai/openai)
FUSION_YOLO=1            # start with auto-approve
```

## Architecture

Rust Cargo workspace with 5 crates (Codex CLI-inspired):

```
crates/
├── fusion-core/     # shared types, config, search_replace safety
├── fusion-llm/      # multi-provider LLM client (Cloudflare + OpenAI-compat)
├── fusion-agent/    # agent loop, tool dispatch, event emission
├── fusion-tui/      # Ratatui TUI + simple REPL
└── fusion-cli/      # binary entry point (clap CLI)
```

### Key dependencies

| Crate | Purpose |
|-------|---------|
| `ratatui` + `crossterm` | TUI framework (Codex-style) |
| `async-openai` | OpenAI-compatible API client |
| `reqwest` | HTTP for Cloudflare Workers AI |
| `tokio` | Async runtime |
| `clap` | CLI argument parsing |
| `serde` + `toml` | Config serialization |

## Development

```bash
cargo run -p fusion-cli              # run in dev mode
cargo test --workspace               # run all tests
cargo clippy --workspace             # lint
cargo build --release                # production binary
```

## Cross-compile for Termux (aarch64)

```bash
# Using cross (recommended)
cargo install cross
cross build --release --target aarch64-unknown-linux-gnu

# Copy binary to phone
adb push target/aarch64-unknown-linux-gnu/release/fusion /sdcard/
# Then in Termux: cp /sdcard/fusion ~/.local/bin/ && chmod +x ~/.local/bin/fusion
```

## Philosophy

Fusion is an **open terminal agent**, not an IDE.
Deliberately designed to be the best AI pair programmer on a bus with a phone + Termux.

We optimize for:
- **Native terminal feel** — scrollback, copy, tmux compatibility
- **Small screens** — compact output, slash commands, one-handed use
- **Safety** — unique search_replace, permission prompts
- **Speed** — Rust binary, ~5ms startup, ~5MB memory
- **Zero deps** — single binary, no Node.js/Python/Go runtime needed

## License

MIT
