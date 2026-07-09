#!/data/data/com.termux/files/usr/bin/bash
# Fusion — Termux bootstrap script
# Builds the Rust binary natively on Termux (aarch64 Android)
set -euo pipefail

echo "==> Fusion for Termux (Rust native binary)"
echo

pkg update -y || true
pkg install -y git ripgrep rust

echo
echo "Clone:"
echo "  git clone https://github.com/aungmyatmoe/zencode ~/.fusion"
echo "  cd ~/.fusion"
echo
echo "Build (takes a few minutes on first compile):"
echo "  cargo build --release"
echo
echo "Run:"
echo "  export CLOUDFLARE_ACCOUNT_ID=your_id"
echo "  export CLOUDFLARE_API_TOKEN=your_token"
echo "  ./target/release/fusion"
echo
echo "Or copy to PATH:"
echo "  cp target/release/fusion \$PREFIX/bin/"
echo "  fusion"
echo
echo "Modes:"
echo "  fusion              # rich Ratatui TUI (default)"
echo "  fusion --simple     # lightweight REPL (best on small screens)"
echo "  fusion -p 'task'    # headless (non-interactive)"
echo
echo "Default model = Kimi 2.7 Code via Cloudflare Workers AI"
echo "No local GPU needed. Open models like GLM supported via routing."
echo "Use /help inside the REPL for commands."
