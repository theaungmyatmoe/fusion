# Fusion — Detailed Reference

## Configuration

### User config

```bash
mkdir -p ~/.fusion
$EDITOR ~/.fusion/config.toml
```

Example (shape varies by provider; see in-product docs under `~/.fusion/docs/user-guide/` after first run):

```toml
# Model id from the built-in catalog (defaults include Cloudflare Workers AI models)
# model = "cloudflare-kimi-k2.7"

[ui]
# screen_mode = "minimal"   # prefer phone-friendly UI by default

# API keys are usually set via `fusion login` or environment variables.
```

Legacy / project-level examples may still use `fusion.toml` or `~/.config/fusion/fusion.toml` for multi-provider setups — prefer **`~/.fusion/config.toml`** for the current binary.

### Environment variables

| Variable | Description |
|----------|-------------|
| `FUSION_HOME` | Override config/data directory (default `~/.fusion`) |
| `CLOUDFLARE_ACCOUNT_ID` / `CLOUDFLARE_API_TOKEN` | Cloudflare Workers AI |
| `XAI_API_KEY` | xAI API access |
| `OPENAI_API_KEY` | OpenAI-compatible providers |
| `PROTOC` | Path to `protoc` when building from source |

Default model catalog includes Cloudflare-hosted models (e.g. Kimi K2.7 Code). Bring your own keys.


## Full CLI reference

```bash
fusion                         # TUI
fusion --minimal               # minimal / scrollback mode
fusion --fullscreen            # force full TUI
fusion -p "task"               # headless; print answer to stdout
fusion --prompt-file task.md   # headless from file
fusion -m cloudflare-kimi-k2.7 # model override
fusion --always-approve        # auto-approve all tools
fusion login                   # sign in / credentials
fusion logout
fusion --version
```

Useful interactive flows (slash commands and keybindings are documented in the bundled user guide):

| Area | Examples |
|------|----------|
| Slash commands | `/model`, `/help`, plan/mode toggles, MCP/plugins where enabled |
| Headless / CI | `fusion -p "…"`, JSON output formats via headless flags |
| Subagents / tools | Built into the agent runtime (read, edit, shell, search, MCP, …) |

```bash
# See full CLI surface
fusion --help
fusion agent --help
```


## Build from source

Requirements:

- **Rust 1.92+** (see `rust-toolchain.toml`)
- **`protoc`** (protobuf compiler) on `PATH`  
  CI and local builds need a real protoc — the repo's `bin/protoc` is a DotSlash wrapper and needs `dotslash` if you use it.

```bash
git clone https://github.com/theaungmyatmoe/fusion.git
cd fusion

# Install protoc if needed (examples)
#   macOS:  brew install protobuf
#   Debian: sudo apt install protobuf-compiler

cargo build --release -p xai-grok-pager-bin
# binary: target/release/fusion
```

Cross static Linux / Termux (needs musl toolchain):

```bash
./scripts/build-linux.sh          # both
./scripts/build-linux.sh arm64    # aarch64-unknown-linux-musl (Termux)
./scripts/build-linux.sh x86_64
```

Makefile shortcuts: `make build`, `make release`, `make termux`, `make dist`.


## Architecture (high level)

The product binary is **`fusion`** (`crates/codegen/xai-grok-pager-bin`). Under the hood this is a large Rust workspace:

```text
crates/
  codegen/     # agent, tools, TUI (pager), shell, MCP, config, update, …
  common/      # shared tool protocol, tracing, compaction, …
  build/       # protoc helpers
third_party/   # vendored layout / mermaid bits
```

Internal crate names may still say `xai-grok-*` (historical). **User-facing product identity is Fusion / Fusion AI.**


## Releases & CI

- **CI** (on `main`): build + unit tests + musl check — installs `protoc`, builds `-p xai-grok-pager-bin`
- **Release**: push a version tag `v*` → multi-target binaries + GitHub Release notes

```bash
git tag -a v0.2.0 -m "Fusion monorepo + Fusion AI identity"
git push origin v0.2.0
```
