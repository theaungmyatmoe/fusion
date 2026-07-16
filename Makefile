# Fusion — Build Targets
# ─────────────────────────────────────────────────────────────────────────────

BINARY := fusion
RELEASE_DIR := dist

.PHONY: all build release linux arm64 termux clean install help

## Default: dev build for current platform
all: build

## Dev build (current platform, unoptimised)
build:
	cargo build

## Release build (current platform, optimised)
release:
	cargo build --release
	@echo "✅ Binary: target/release/$(BINARY)"

## Static Linux x86_64 binary (requires musl-cross)
linux:
	./scripts/build-linux.sh x86_64

## Static Linux aarch64 binary — Termux / Android / ARM servers
arm64:
	./scripts/build-linux.sh arm64

## Alias for arm64 (Termux)
termux: arm64

## Build all release targets (macOS + Linux x86_64 + Linux aarch64)
dist: release linux arm64

## Clean build artifacts
clean:
	cargo clean
	rm -rf $(RELEASE_DIR)

## Install to /usr/local/bin (current platform release build)
install: release
	install -m 755 target/release/$(BINARY) /usr/local/bin/$(BINARY)
	@echo "✅ Installed: /usr/local/bin/$(BINARY)"

## Install musl cross-compilers (macOS only, requires brew)
setup-cross:
	brew install FiloSottile/musl-cross/musl-cross
	rustup target add x86_64-unknown-linux-musl aarch64-unknown-linux-musl
	@echo "✅ Cross-compilation toolchain ready"

help:
	@echo ""
	@echo "  Fusion Build Targets"
	@echo "  ─────────────────────────────────────────────"
	@echo "  make build         Dev build (current platform)"
	@echo "  make release       Release build (current platform)"
	@echo "  make linux         Static Linux x86_64 binary"
	@echo "  make arm64         Static Linux aarch64 (Termux/Android)"
	@echo "  make termux        Alias for arm64"
	@echo "  make dist          All release targets"
	@echo "  make install       Install to /usr/local/bin"
	@echo "  make setup-cross   Install musl cross-compilers (macOS)"
	@echo "  make clean         Remove build artifacts"
	@echo ""
