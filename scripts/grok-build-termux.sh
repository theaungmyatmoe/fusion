#!/data/data/com.termux/files/usr/bin/bash
# Grok Build — Termux compilation and wrapper installer
# This script builds xAI's open-source grok-build CLI from source on Termux,
# resolves build-time dependencies (like protoc), and installs a wrapper script
# to handle the Android /tmp directory redirection.
set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
DIM='\033[0;90m'
BOLD='\033[1m'
NC='\033[0m'

info() { echo -e "${BLUE}${BOLD}==>${NC} ${BOLD}$1${NC}"; }
sub()  { echo -e "  ${DIM}$1${NC}"; }
ok()   { echo -e "${GREEN}${BOLD}Success:${NC} $1"; }
warn() { echo -e "${YELLOW}${BOLD}Warning:${NC} $1"; }
err()  { echo -e "${RED}${BOLD}Error:${NC} $1" >&2; exit 1; }

# Verify execution inside Termux
if [ -z "${PREFIX:-}" ] || [[ ! "$PREFIX" =~ "com.termux" ]]; then
    err "This script is intended to be run inside the Termux environment on Android."
fi

# Set Termux-safe temp directory
export TMPDIR="${PREFIX}/tmp"
export TMP="$TMPDIR"
export TEMP="$TMPDIR"
mkdir -p "$TMPDIR"

info "Updating Termux repositories..."
pkg update -y || true

info "Installing build-time dependencies..."
# Grok Build requires Rust, Clang, protobuf (for protoc), pkg-config, sqlite, git, and make
pkg install -y rust clang protobuf pkg-config sqlite git make ndk-sysroot 2>/dev/null || {
    warn "Package manager installation encountered issues. Retrying packages individually..."
    pkg install -y rust || err "Failed to install Rust"
    pkg install -y clang || err "Failed to install clang"
    pkg install -y protobuf || err "Failed to install protobuf (protoc)"
    pkg install -y pkg-config || err "Failed to install pkg-config"
    pkg install -y sqlite || err "Failed to install sqlite"
    pkg install -y git || err "Failed to install git"
    pkg install -y make || err "Failed to install make"
}

# Locate/verify protoc
if ! command -v protoc >/dev/null 2>&1; then
    err "protobuf (protoc) compiler is missing. It is required for Grok Build's code generation."
fi
export PROTOC="$(command -v protoc)"
sub "Found protoc compiler at: $PROTOC"

# Define directories
WORKSPACE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GROK_SRC_DIR="${WORKSPACE_DIR}/reference/grok-build"

# Clone if not present
if [ ! -d "$GROK_SRC_DIR" ]; then
    info "Cloning grok-build source repository..."
    git clone https://github.com/xai-org/grok-build.git "$GROK_SRC_DIR"
else
    info "Found existing grok-build repository at: $GROK_SRC_DIR"
fi

info "Compiling Grok Build (this may take a few minutes on mobile)..."
cd "$GROK_SRC_DIR"

# Termux cargo / rustc environment tuning
export CARGO_TARGET_DIR="${GROK_SRC_DIR}/target"
cargo build -p xai-grok-pager-bin --release

# Locate compiled binary
COMPILED_BIN="${GROK_SRC_DIR}/target/release/xai-grok-pager"
if [ ! -f "$COMPILED_BIN" ]; then
    err "Compilation finished but release binary was not found at $COMPILED_BIN"
fi

# Create install directories
INSTALL_DIR="${HOME}/.grok/bin"
mkdir -p "$INSTALL_DIR"

# Move binary to local grok bin
cp -f "$COMPILED_BIN" "${INSTALL_DIR}/xai-grok-pager"
ok "Compiled binary placed at ${INSTALL_DIR}/xai-grok-pager"

# Create Termux launcher wrapper
# This is necessary because Android's /tmp is write-blocked, so we must override it
# to use $PREFIX/tmp whenever grok-build is launched.
WRAPPER_BIN="${PREFIX}/bin/grok-build"
info "Creating Termux wrapper launcher at: $WRAPPER_BIN"

cat << 'EOF' > "$WRAPPER_BIN"
#!/data/data/com.termux/files/usr/bin/bash
# Grok Build launcher wrapper for Termux
# Overrides TMPDIR to prevent write errors on Android /tmp

export PREFIX="/data/data/com.termux/files/usr"
export TMPDIR="${PREFIX}/tmp"
export TMP="$TMPDIR"
export TEMP="$TMPDIR"
mkdir -p "$TMPDIR"

# Forward execution to compiled grok-build binary
exec "/data/data/com.termux/files/home/.grok/bin/xai-grok-pager" "$@"
EOF

chmod +x "$WRAPPER_BIN"
ok "Wrapper script installed successfully!"

echo
echo -e "${GREEN}${BOLD}====================================================${NC}"
echo -e "${GREEN}${BOLD} Grok Build is now installed on your Termux!        ${NC}"
echo -e "${GREEN}${BOLD}====================================================${NC}"
echo
echo -e "You can launch the Grok Build agent by running:"
echo -e "  ${BOLD}grok-build${NC}"
echo
echo -e "Note: Since it is compiled natively on Android, it bypasses"
echo -e "the unsupported prebuilt architecture checks from x.ai."
echo -e "Make sure to authenticate by following the browser log directions."
echo
