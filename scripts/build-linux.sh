#!/usr/bin/env bash
# scripts/build-linux.sh
# Cross-compile Fusion static binaries for Linux (x86_64) and Termux/Android (aarch64).
#
# Usage:
#   ./scripts/build-linux.sh           # build both targets
#   ./scripts/build-linux.sh x86_64    # Linux only
#   ./scripts/build-linux.sh arm64     # Termux/Android only
#
# Requirements (macOS cross-compile):
#   brew install FiloSottile/musl-cross/musl-cross
#   rustup target add x86_64-unknown-linux-musl aarch64-unknown-linux-musl

set -euo pipefail

TARGET="${1:-both}"
BINARY="fusion"
RELEASE_DIR="dist"

mkdir -p "$RELEASE_DIR"

build_x86_64() {
    echo "▶ Building Linux x86_64 static binary..."
    rustup target add x86_64-unknown-linux-musl 2>/dev/null || true

    CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER="x86_64-linux-musl-gcc" \
    CC_x86_64_unknown_linux_musl="x86_64-linux-musl-gcc" \
    cargo build \
        --release \
        --target x86_64-unknown-linux-musl \
        --no-default-features \
        --features release-dist \
        -p xai-grok-pager-bin

    cp "target/x86_64-unknown-linux-musl/release/$BINARY" \
       "$RELEASE_DIR/fusion-linux-x86_64"
    echo "✅ $RELEASE_DIR/fusion-linux-x86_64"
}

build_arm64() {
    echo "▶ Building Linux aarch64 static binary (Termux/Android)..."
    rustup target add aarch64-unknown-linux-musl 2>/dev/null || true

    CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER="aarch64-linux-musl-gcc" \
    CC_aarch64_unknown_linux_musl="aarch64-linux-musl-gcc" \
    cargo build \
        --release \
        --target aarch64-unknown-linux-musl \
        --no-default-features \
        --features release-dist \
        -p xai-grok-pager-bin

    cp "target/aarch64-unknown-linux-musl/release/$BINARY" \
       "$RELEASE_DIR/fusion-linux-aarch64"
    echo "✅ $RELEASE_DIR/fusion-linux-aarch64"
    echo ""
    echo "📱 To install on Termux (Android):"
    echo "   scp $RELEASE_DIR/fusion-linux-aarch64 phone:/data/data/com.termux/files/home/bin/fusion"
    echo "   chmod +x ~/bin/fusion"
}

case "$TARGET" in
    x86_64)  build_x86_64 ;;
    arm64)   build_arm64  ;;
    both)    build_x86_64; build_arm64 ;;
    *)       echo "Unknown target: $TARGET (use x86_64, arm64, or both)"; exit 1 ;;
esac

echo ""
echo "🎉 Build complete! Binaries in ./$RELEASE_DIR/"
ls -lh "$RELEASE_DIR"/
