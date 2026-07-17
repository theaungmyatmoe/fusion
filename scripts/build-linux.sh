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

    RUSTFLAGS="-C relocation-model=pic -C link-arg=-static-pie" \
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
    echo "▶ Building Android/Termux aarch64 native binary..."
    
    # Locate Android NDK
    local ndk_dir=""
    if [ -n "$ANDROID_NDK_HOME" ] && [ -d "$ANDROID_NDK_HOME/toolchains/llvm/prebuilt" ]; then
        ndk_dir="$ANDROID_NDK_HOME"
    elif [ -n "$ANDROID_NDK_LATEST_HOME" ] && [ -d "$ANDROID_NDK_LATEST_HOME/toolchains/llvm/prebuilt" ]; then
        ndk_dir="$ANDROID_NDK_LATEST_HOME"
    fi

    if [ -z "$ndk_dir" ]; then
        for d in "$HOME/Library/Android/sdk/ndk"/* \
                 "/usr/local/lib/android/sdk/ndk"/* \
                 "/usr/local/lib/android/sdk/ndk-bundle"; do
            if [ -d "$d/toolchains/llvm/prebuilt/darwin-x86_64/bin" ] || [ -d "$d/toolchains/llvm/prebuilt/linux-x86_64/bin" ]; then
                ndk_dir="$d"
            fi
        done
    fi

    if [ -z "$ndk_dir" ]; then
        echo "❌ Error: Android NDK not found. Please set ANDROID_NDK_HOME or install it."
        exit 1
    fi
    echo "Using NDK: $ndk_dir"

    rustup target add aarch64-linux-android 2>/dev/null || true

    local host_os="linux-x86_64"
    if [ "$(uname)" = "Darwin" ]; then
        host_os="darwin-x86_64"
    fi

    local ndk_bin="$ndk_dir/toolchains/llvm/prebuilt/$host_os/bin"
    local linker="$ndk_bin/aarch64-linux-android24-clang"
    local cc="$ndk_bin/aarch64-linux-android24-clang"
    local cxx="$ndk_bin/aarch64-linux-android24-clang++"
    local ar="$ndk_bin/llvm-ar"

    local rg_override=""
    if [ -f "$(pwd)/third_party/ripgrep/rg" ]; then
        rg_override="GROK_TOOLS_BUNDLE_RG_PATH=$(pwd)/third_party/ripgrep/rg GROK_SHELL_BUNDLE_RG_PATH=$(pwd)/third_party/ripgrep/rg"
    fi

    env $rg_override \
    AR_aarch64_linux_android="$ar" \
    CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$linker" \
    CC_aarch64_linux_android="$cc" \
    CXX_aarch64_linux_android="$cxx" \
    RUSTFLAGS="-C link-arg=-static-libstdc++ -C link-arg=-lc++abi" \
    cargo build \
        --release \
        --target aarch64-linux-android \
        --no-default-features \
        --features release-dist \
        -p xai-grok-pager-bin

    cp "target/aarch64-linux-android/release/$BINARY" \
       "$RELEASE_DIR/fusion-linux-aarch64"
    # Strip debug symbols to optimize size
    "$ndk_bin/llvm-strip" "$RELEASE_DIR/fusion-linux-aarch64" 2>/dev/null || true
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
