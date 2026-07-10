#!/bin/sh
# Fusion — one-line installer
# curl -sSL https://raw.githubusercontent.com/theaungmyatmoe/fusion/main/scripts/install.sh | sh
set -eu

REPO="theaungmyatmoe/fusion"
BINARY="fusion"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
DIM='\033[0;90m'
BOLD='\033[1m'
NC='\033[0m'

info() { echo "${DIM}$1${NC}"; }
ok()   { echo "${GREEN}${BOLD}$1${NC}"; }
err()  { echo "${RED}$1${NC}" >&2; exit 1; }

# Detect platform
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

# Alpine / iSH detection
if [ -f "/etc/alpine-release" ]; then
    PLATFORM="alpine"
    INSTALL_DIR="/usr/local/bin"
# Termux detection
elif [ -n "${PREFIX:-}" ] && echo "$PREFIX" | grep -q "com.termux"; then
    PLATFORM="termux"
    INSTALL_DIR="$PREFIX/bin"
elif [ "$OS" = "darwin" ]; then
    PLATFORM="macos"
    INSTALL_DIR="$HOME/.local/bin"
elif [ "$OS" = "linux" ]; then
    PLATFORM="linux"
    INSTALL_DIR="$HOME/.local/bin"
else
    err "Unsupported OS: $OS"
fi

# Map architecture
case "$ARCH" in
    aarch64|arm64) TARGET_ARCH="aarch64" ;;
    x86_64|amd64)  TARGET_ARCH="x86_64" ;;
    i386|i486|i586|i686) TARGET_ARCH="i686" ;;
    *)             err "Unsupported architecture: $ARCH" ;;
esac

# Build target triple
case "$PLATFORM" in
    termux) TARGET="${TARGET_ARCH}-linux-android" ;;
    alpine) TARGET="${TARGET_ARCH}-unknown-linux-musl" ;;
    linux)  TARGET="${TARGET_ARCH}-unknown-linux-musl" ;;
    macos)  TARGET="${TARGET_ARCH}-apple-darwin" ;;
esac

ASSET="${BINARY}-${TARGET}"

info "Installing Fusion..."
info "  platform: $PLATFORM ($TARGET)"
info "  install:  $INSTALL_DIR/$BINARY"
echo ""

# Get latest release URL
RELEASE_URL="https://api.github.com/repos/${REPO}/releases/latest"
DOWNLOAD_URL=$(curl -sSL "$RELEASE_URL" | grep "browser_download_url.*${ASSET}" | head -1 | cut -d '"' -f 4)

if [ -z "$DOWNLOAD_URL" ]; then
    # Fallback: try .tar.gz
    DOWNLOAD_URL=$(curl -sSL "$RELEASE_URL" | grep "browser_download_url.*${ASSET}.tar.gz" | head -1 | cut -d '"' -f 4)
fi

if [ -z "$DOWNLOAD_URL" ]; then
    err "No release found for $TARGET. Check: https://github.com/${REPO}/releases"
fi

# Create install dir
mkdir -p "$INSTALL_DIR"

# Download
TMPDIR=$(mktemp -d)
trap "rm -rf $TMPDIR" EXIT

info "Downloading $DOWNLOAD_URL..."

case "$DOWNLOAD_URL" in
    *.tar.gz)
        curl -sSL "$DOWNLOAD_URL" | tar xz -C "$TMPDIR"
        mv "$TMPDIR/$BINARY" "$INSTALL_DIR/$BINARY"
        ;;
    *)
        curl -sSL -o "$INSTALL_DIR/$BINARY" "$DOWNLOAD_URL"
        ;;
esac

chmod +x "$INSTALL_DIR/$BINARY"

echo ""
ok "Fusion installed to $INSTALL_DIR/$BINARY"

# Check PATH
if ! echo "$PATH" | tr ':' '\n' | grep -q "^${INSTALL_DIR}$"; then
    echo ""
    info "Add to your PATH:"
    echo ""
    echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
    echo ""
    info "Add this to ~/.bashrc or ~/.zshrc to make it permanent."
fi

echo ""
info "Run: fusion --help"
