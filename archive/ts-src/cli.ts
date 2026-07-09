#!/usr/bin/env node
/**
 * zencode — mobile-first Termux-native Grok/OpenCode-style coding agent.
 * Inspired by https://github.com/superagent-ai/grok-cli (TS + Bun + OpenTUI)
 *
 * Default: lightweight open chalk REPL (best for pure mobile Termux, low overhead, native scrollback).
 * --tui: rich OpenTUI (React + Zig core) experience when you want the full visual agent UI.
 */
import { loadConfig } from "./config.js";
import { SimpleREPL } from "./tui/simple.js";
import chalk from "chalk";

async function main() {
  const args = process.argv.slice(2);

  const useTui = args.includes("--tui") || args.includes("-t");
  const showVersion = args.includes("--version") || args.includes("-v");
  const showHelp = args.includes("--help") || args.includes("-h");

  if (showVersion) {
    console.log("zencode 0.1.0-dev (TypeScript + Bun + OpenTUI, Termux optimized)");
    process.exit(0);
  }

  if (showHelp) {
    console.log(`
zencode — mobile-first AI coding agent for Termux (Node.js native, inspired by grok-cli + OpenCode)

Usage:
  zencode                    # default: simple open terminal REPL (best on phones)
  zencode --tui              # rich OpenTUI (React terminal UI) — best on desktop
  zencode -p "your task"     # headless (future)

Environment (default route = Cloudflare Workers AI Kimi 2.7):
  CLOUDFLARE_ACCOUNT_ID + CLOUDFLARE_API_TOKEN   (highest priority; put in ~/.zshrc and restart shell)
  XAI_API_KEY                                    (optional - use real Grok models instead)

  ZENCODE_MODEL   override (e.g. @cf/zhipu-ai/glm-4)
  ZENCODE_YOLO=1  start with auto-approve

Config file (global or per-project):
  ~/.config/zencode/zencode.json     (or ./zencode.json next to your code)
  Supports the same shape as opencode.json, including a "provider.cloudflare.options" block
  with accountId + apiKey. Use this for the mobile Termux-friendly setup.

Inside the REPL: /help /yolo /plan /model <name> /status /exit

Note: Runs with plain Node.js in Termux. No Bun needed at runtime.
  After editing source: npm run build && npm link   (so your 'zencode' command uses latest)
`);
    process.exit(0);
  }

  const cfg = loadConfig();
  const runningInTermux = isTermux();

  // Early, visible guidance when the common Cloudflare/Kimi default is selected
  // but the resolved config (global or project zencode.json + env) has no accountId.
  const c: any = cfg;
  const usingCf = (cfg.model || '').startsWith('@cf/') || cfg.provider === 'cloudflare' || (cfg.model || '').includes('kimi');
  if (usingCf && !c.cloudflareAccountId) {
    console.log(chalk.yellow('⚠  No Cloudflare credentials found for the default Kimi route.'));
    console.log(chalk.gray('   config: ' + (c.configPath || '(none)')));
    console.log(chalk.gray('   Either export from your ~/.zshrc (and restart this terminal):'));
    console.log(chalk.gray('     export CLOUDFLARE_ACCOUNT_ID=6838bf50a0d8548d5945008dc7b6797c'));
    console.log(chalk.gray('     export CLOUDFLARE_API_TOKEN=cfat_...'));
    console.log(chalk.gray('   Or add a "provider" block to the config file shown above. Env vars win.'));
    console.log();
  }

  if (useTui) {
    // Rich Ink TUI (React-based, pure JS — portable alternative to OpenTUI/Zig).
    // Works on desktop Node. On Termux the simple REPL is strongly recommended
    // (small screens + no native Zig pain like the opencode-termux efforts).
    if (runningInTermux) {
      console.log("Note: Running on Termux — rich TUI may be cramped. Falling back to simple REPL is recommended.");
    }

    const { runInkTUI } = await import("./tui/ink-app.js");
    runInkTUI(cfg, /* initialMessage */ undefined);
    return;
  }

  if (runningInTermux) {
    console.log("zencode — Termux detected. Using lightweight open REPL (pure Node.js, no native deps).");
    console.log("Rich --tui (Ink) is available but the simple REPL is usually better on phones.");
  } else {
    console.log("zencode — using lightweight open REPL (mobile-friendly). Use --tui for rich Ink TUI.");
  }

  const repl = new SimpleREPL(cfg);
  await repl.run();
}

function isTermux(): boolean {
  // Common Termux environment signals
  if (process.env.TERMUX_VERSION) return true;
  const prefix = process.env.PREFIX || "";
  if (prefix.includes("com.termux")) return true;
  // Fallback: many Termux setups have this in PATH or have specific uname
  try {
    const { execSync } = require("child_process");
    const uname = execSync("uname -o 2>/dev/null || echo ''", { encoding: "utf8" }).trim();
    if (uname.toLowerCase().includes("android")) return true;
  } catch {}
  return false;
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
