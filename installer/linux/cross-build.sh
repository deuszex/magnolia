#!/bin/bash
# Cross-compile Linux .deb package using cargo-zigbuild.
# Works natively on Windows, macOS, and Linux — no Docker, no WSL.
#
# Prerequisites:
# - Zig installed (https://ziglang.org/download/ or: winget install zig.zig)
# - cargo install cargo-zigbuild
# - cargo install cargo-deb
#
# Usage:
# ./cross-build.sh # default x86_64
# ./cross-build.sh --target aarch64-unknown-linux-gnu # arm64

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
VERSION="${VERSION:-1.0.0}"
TARGET="${TARGET:-x86_64-unknown-linux-gnu}"

while [[ $# -gt 0 ]]; do
 case "$1" in
 --target|-t)
 TARGET="$2"
 shift 2
 ;;
 *)
 echo "Unknown option: $1"
 echo "Usage: $0 [--target <rust-target-triple>]"
 exit 1
 ;;
 esac
done

echo "Cross-compiling magnolia Linux .deb package v$VERSION"
echo "Target: $TARGET"
echo ""

# Check prerequisites
if ! command -v zig &> /dev/null; then
 echo "Error: zig not found."
 echo "Install from https://ziglang.org/download/ or: winget install zig.zig"
 exit 1
fi

if ! command -v cargo-zigbuild &> /dev/null; then
 echo "cargo-zigbuild not found. Installing..."
 cargo install cargo-zigbuild
fi

if ! command -v cargo-deb &> /dev/null; then
 echo "cargo-deb not found. Installing..."
 cargo install cargo-deb
fi

cd "$PROJECT_ROOT/backend"

rustup target add "$TARGET" 2>/dev/null || true

echo "Building release binaries with cargo-zigbuild..."
cargo zigbuild --release \
 --bin magnolia_server \
 --bin service_ctl \
 --bin create_admin \
 --target "$TARGET"

echo ""
echo "Normalizing maintainer script line endings (CRLF -> LF)..."
SCRIPTS_DIR="$SCRIPT_DIR/scripts"
for f in "$SCRIPTS_DIR/postinst" "$SCRIPTS_DIR/prerm" "$SCRIPTS_DIR/postrm" "$SCRIPTS_DIR/config"; do
 if [ -f "$f" ]; then
 sed -i 's/\r//' "$f"
 fi
done

echo "Building .deb package..."
cargo deb --no-build --no-strip --target "$TARGET"

DEB_FILE=$(find "$PROJECT_ROOT/target" -name "*.deb" -type f | head -1)

if [ -n "$DEB_FILE" ]; then
 cp "$DEB_FILE" "$SCRIPT_DIR/"
 echo ""
 echo "Build complete: $SCRIPT_DIR/$(basename "$DEB_FILE")"
 echo ""
 echo "To install: sudo dpkg -i $(basename "$DEB_FILE")"
 echo "To uninstall: sudo dpkg -r magnolia_server"
else
 echo "Error: .deb file not found"
 exit 1
fi
