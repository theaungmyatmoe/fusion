# Zencode

**Zencode** is a **mobile-first, Termux-native AI coding agent** for Android/Termux.

See the direct inspiration: https://github.com/superagent-ai/grok-cli — an open-source Grok coding agent built with **TypeScript + Bun + OpenTUI**.

Zencode follows the same spirit but is tuned for Termux phones + uses **Cloudflare Workers AI** (Kimi 2.7 Code + GLM etc.) as the default free/open route, exactly like you asked. Thanks for the Bun Android support link — that's a game changer!

We also added first-class `zencode.json` support (modeled directly on `opencode.json` schema, including a `settings` section).

Zencode brings the same Grok + OpenCode style experience, but deliberately optimized for **Termux on phones** (small screens, soft keyboards, native terminal feel, low overhead).

- Default = lightweight open REPL (the Termux hero).
- `--tui` = rich Ink TUI (pure-JS React, portable alternative to OpenTUI/Zig).
- Written in **TypeScript**. Runs on **Bun** (1.3.14+ official Android support) *or* plain **Node.js**.
- Everything stays in normal scrollback so Termux finger scroll, selection, and copy work perfectly.
- First-class **Grok / xAI** (`XAI_API_KEY`).
- Works with any OpenAI-compatible provider.
- Core agent power: `search_replace` that **requires a unique old_string** (prevents dangerous ambiguous edits), visible todos, Plan Mode, project rules, safe shell execution.
- Designed for small screens + soft keyboards: slash commands are primary, minimal modifier keys, compact output.

## Recommended approach (leveraging the Bun Android support you found)

You correctly identified the OpenTUI/Zig problem — it's not practical natively on aarch64 Termux/Android (the opencode-termux repo is the proof). 

**The great thing we're doing now, thanks to Bun 1.3.14 official Android support:**

- **Default / Termux hero**: Polished **simple REPL** (chalk + enquirer for excellent prompts/autocomplete).
  - Pure JavaScript + Node (or Bun when you have the official Android build).
  - No native compilation *ever*.
  - Stays in normal terminal scrollback — finger scroll, copy/paste, tmux inside Termux all just work.
  - This is what you'll actually use on a phone 95% of the time.

- **Optional rich mode** (`--tui`): **Ink** (the standard pure-JS React TUI library).
  - No Zig, no native deps.
  - Modern component-based UI (boxes, live updates, keyboard-driven) — the closest portable thing to the "opentui" experience in the grok-cli reference.
  - Works great on desktop. On Termux phones it runs but the simple REPL is usually more practical (we default to it).

- **Bun support**: Now realistic in Termux! Use the official 1.3.14+ Android build when you want Bun's speed, PTY features, etc. We support both Bun and Node runtimes cleanly (dev scripts, build, start commands for both). Bun also makes `bun build --compile` single binaries possible if you want that.

- LLM routing exactly as you wanted: Kimi 2.7 Code on Cloudflare as the default (easy + open), with simple routing to GLM and other open models, just like OpenCode.

This gives you the spirit of the grok-cli / OpenCode stack while being *actually* usable and low-friction on real Termux phones. We completely sidestep the Zig cross-compile hell.

We removed the direct OpenTUI deps and switched the rich path to Ink. The simple REPL is the star for mobile and is already enhanced with enquirer for pro UX.
  - No Zig, no native deps.
  - Component model similar to what you liked in the grok-cli reference.
  - Works on desktop Node. On Termux it will run but the UI may feel cramped on small screens (so we still default to simple REPL).

- Keep the flexible LLM routing you wanted (Kimi 2.7 Code on Cloudflare Workers AI as default + easy GLM / other open models, exactly like OpenCode does).

This gives you "the opentui spirit" where it makes sense, while being actually native and practical on Termux — without the cross-compile nightmare.

We removed the direct OpenTUI dependency and switched the rich path to Ink. The simple REPL (your main mobile path) stays lightweight and already works.

(Go was the initial single-binary choice. The reference made the TS/Bun/OpenTUI direction obvious.)

Trade-off: You need Node.js or Bun in Termux. Node is the path of least resistance. Bun (thanks to the official Android build you found) unlocks more features and the "full Bun + modern CLI" experience. The agent itself (tools + LLM routing) is the same either way — the JS runtime is rarely the bottleneck compared to the model calls.

## Quick Start (Termux — Node.js native)

Bun has **official Android/Termux support** since 1.3.14 (see https://www.reddit.com/r/termux/comments/1tcqqsy/bun_1314_officially_supports_android_but_its_not/ for details and benchmarks). 

You *can* use Bun in Termux now for the full experience (faster in some workloads when using glibc-runner, plus all the Bun features like better PTY, etc.). However, the native Android build may not be as fast as desktop glibc or even Node in some benchmarks.

For maximum compatibility and simplicity on Termux, plain Node.js still works great and is the easiest path. We support both.

```bash
# Node path (easiest, maximum compatibility)
pkg update && pkg upgrade
pkg install git ripgrep nodejs
git clone https://github.com/aungmyatmoe/zencode.git ~/.zencode
cd ~/.zencode
npm install
npm run build

# Bun path (now possible natively thanks to 1.3.14 Android support)
# pkg install ... (or install the official Bun Android build)
# bun install
# bun run build:bun

# Cloudflare creds for default Kimi 2.7 route
export CLOUDFLARE_ACCOUNT_ID=...
export CLOUDFLARE_API_TOKEN=...

cd ~/storage/shared/my-project
node dist/cli.js                 # or bun run start:bun
```

**Default (recommended on phones):** The lightweight open REPL — pure, no native deps, stays in normal scrollback.

**Rich mode:** `node dist/cli.js --tui` (uses Ink — portable React TUI, no Zig).

Bun gives you the full Bun ecosystem (including if you want to experiment with OpenTUI on desktop or when the native support matures), while Node keeps things dead simple and reliable on Termux. We support both runtimes.

## How to use on a phone

- Talk normally or use slash commands (`/help`, `/yolo`, `/plan`, `/model grok-3`, `/exit`).
- Slash commands are deliberately the main way to control things because they are easy to type on a soft keyboard.
- For edits it will show a diff and ask simple `y/N/a` (or enable YOLO).
- The whole conversation stays in your normal terminal history — scroll with your finger, long-press to copy previous output, etc.

Recommended workflow:
1. Use Acode / vim / your editor on the project.
2. Switch to Termux (split screen works great).
3. Run `zencode` in the project folder.
4. Tell it what to do.
5. Review changes back in your editor.

## Current state + Model Routing + Tests

**Default model route (as requested):**
- Provider: `cloudflare` (Workers AI)
- Model: `@cf/moonshotai/kimi-k2.7-code` (Kimi 2.7 Code)

This gives you a strong code model **without needing an xAI/Grok key** by default.

You can route to other open models the same way OpenCode does:
- GLM-4 / GLM variants via Cloudflare (`@cf/zhipu-ai/glm-4` etc.)
- Any other `@cf/...` model on Workers AI
- Or fall back to xAI/Grok by setting `XAI_API_KEY` (it will auto-prefer xAI when the key is present)

**Ready-to-use defaults** (the exact Cloudflare credentials you provided — these are now in your local `zencode.json` too):
```bash
export CLOUDFLARE_ACCOUNT_ID=your_cloudflare_account_id
export CLOUDFLARE_API_TOKEN=your_cloudflare_api_token
```

You can also set them in `~/.zencode/config.json`.

### Testing
- `npm test` — runs the full Vitest suite (util safety, config, LLM routing with mocks).
- `npm run test:coverage` — with coverage report.
- `npm run test:watch` — during development.
- Manual end-to-end: build + run in a real project with Cloudflare creds.

Current coverage focuses on the critical pieces that must never regress on Termux:
- `search_replace` unique-string safety (the heart of precise edits)
- Config precedence (CF vs XAI vs file)
- LLM client routing (Cloudflare Kimi path + OpenAI-compat fallback)

- Default: working lightweight open chalk REPL (mobile Termux native) — now does **real** model calls.
- Rich mode: Ink TUI (`--tui`) — also does real calls.
- Config supports multiple providers + easy switching.
- Core safety (`search_replace` unique string) + todo list still present.

See the reference for advanced ideas (Telegram remote control is especially nice for phones, MCP/skills, etc.).

## Building a single binary (Bun)

```bash
bun build src/cli.ts --compile --outfile zencode
# Then put the resulting `zencode` binary somewhere in $PATH on Termux (aarch64)
```

We will provide prebuilt binaries for aarch64 when the project matures.

## Configuration

Environment variables always have highest priority.

### zencode.json (OpenCode-compatible)

Create a `zencode.json` (project root or `~/.config/zencode/zencode.json`) with a schema similar to `opencode.json`:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "model": "cloudflare/kimi-k2.7-code",
  "provider": {
    "cloudflare": {
      "options": {
        "accountId": "YOUR_CLOUDFLARE_ACCOUNT_ID",
        "apiKey": "YOUR_TOKEN"
      }
    }
  },
  "settings": {
    "yolo": false,
    "theme": "system"
  }
}
```

- Project `zencode.json` wins over global.
- `settings` section for extra options (yolo, future TUI prefs, etc.).
- For now, create/edit the JSON manually (interactive wizard coming later, e.g. via `/config` or on first run).

See `zencode.json.example` in the repo root for a full starting point.

Legacy `~/.zencode/config.json` is still supported as fallback.

## Headless / scripting (coming soon)

```bash
zencode -p "Review the recent changes and suggest improvements" --yolo
```

## Philosophy

Zencode is an **open terminal agent**, not an IDE.  
It is deliberately designed to be the best possible AI pair programmer you can have while sitting on a bus with a phone + Termux + Bluetooth keyboard (or just the on-screen one).

We optimize for:
- Native terminal feel (scrollback, copy, tmux compatibility)
- Small screens and one-handed use
- Safety (unique search_replace, permission prompts)
- Real agent capabilities (plan, todos, sub-tasks, precise edits)

## Development

```bash
git clone ...
cd zencode
bun install
bun run dev          # watch + run
bun run src/cli.ts   # direct
```

Contributions and real Termux-on-phone testing/feedback are very welcome.

## License

MIT
