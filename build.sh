#!/usr/bin/env bash
# build.sh — build static uc binaries for all targets
#
# Produces fully static, zero-dependency binaries:
#   dist/uc-aarch64-apple-darwin      (macOS Apple Silicon)
#   dist/uc-x86_64-unknown-linux-musl (Linux x86_64, static)
#
# Requirements:
#   - mise (manages rust toolchain)
#   - zig + cargo-zigbuild (for Linux cross-compilation)
#   - rustup target x86_64-unknown-linux-musl (one-time setup)
#
# Usage:
#   ./build.sh              build all targets
#   ./build.sh macos        build macOS only
#   ./build.sh linux        build Linux only
set -euo pipefail

eval "$(mise activate bash 2>/dev/null)" || true

DIST="dist"
mkdir -p "$DIST"

build_macos() {
	echo "==> building macOS (aarch64-apple-darwin) ..."
	cargo build --release
	cp target/release/uc "$DIST/uc-aarch64-apple-darwin"
	echo "[ok] $DIST/uc-aarch64-apple-darwin"
}

build_linux() {
	echo "==> building Linux (x86_64-unknown-linux-musl) ..."
	cargo zigbuild --release --target x86_64-unknown-linux-musl
	cp target/x86_64-unknown-linux-musl/release/uc "$DIST/uc-x86_64-unknown-linux-musl"
	echo "[ok] $DIST/uc-x86_64-unknown-linux-musl"
}

case "${1:-all}" in
	macos) build_macos ;;
	linux) build_linux ;;
	all)   build_macos; build_linux ;;
	*)     echo "usage: $0 [macos|linux|all]"; exit 1 ;;
esac

echo ""
echo "==> done"
ls -lh "$DIST"/uc-*
