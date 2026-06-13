#!/data/data/com.termux/files/usr/bin/bash
# Zencode Termux bootstrap
# Supports BOTH Node.js (always works) and Bun (official Android support since 1.3.14)
# See the post you linked: https://www.reddit.com/r/termux/comments/1tcqqsy/bun_1314_officially_supports_android_but_its_not/
# Rich TUI uses Ink (pure JS, no Zig) — OpenTUI/Zig path is avoided for mobile compatibility.
set -euo pipefail

echo "==> Zencode for Termux (mobile-first, Bun + Node support)"

pkg update -y || true
pkg install -y git ripgrep nodejs

echo
echo "Bun 1.3.14+ has official Android/Termux support (native, no proot needed for basic use)."
echo "It may not be faster than Node in all benchmarks (per the post), but unlocks Bun features."
echo "We support BOTH — Node for max compatibility, Bun if you want it."
echo
echo "Clone:"
echo "  git clone https://github.com/aungmyatmoe/zencode ~/.zencode"
echo "  cd ~/.zencode"
echo
echo "=== Node.js path (recommended default for Termux phones) ==="
echo "  npm install"
echo "  npm run build"
echo "  # Run: node dist/cli.js"
echo
echo "=== Bun path (for full features when you have the official build) ==="
echo "  # Install Bun (official Android build or other method)"
echo "  bun install"
echo "  bun run build:bun"
echo "  # Run: bun run start:bun"
echo
echo "Run (default = lightweight open REPL — best on phones):"
echo "  # Using provided Cloudflare credentials for immediate Kimi 2.7 use (edit/remove for your own keys)"
echo "  export CLOUDFLARE_ACCOUNT_ID=your_cloudflare_account_id"
echo "  export CLOUDFLARE_API_TOKEN=your_cloudflare_api_token"
echo "  node dist/cli.js                 # or bun run start:bun"
echo
echo "Rich TUI: node dist/cli.js --tui   # uses Ink (portable)"
echo
echo "Tip: After build, run 'npm link' (from the zencode dir) to make the 'zencode' command globally available in your PATH."
echo
echo "Default model = Kimi 2.7 Code via Cloudflare (remote, perfect for Termux)."
echo "No local GPU needed. Open models like GLM supported via routing."
