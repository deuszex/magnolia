#!/bin/bash
# Build Linux .deb package for magnolia
#
# Usage:
# ./build.sh # native build
# ./build.sh --target x86_64-unknown-linux-gnu # specific target
# ./build.sh --target aarch64-unknown-linux-gnu # arm64
# ./build.sh --cross # use cross (Docker)
# ./build.sh --cross --target aarch64-unknown-linux-gnu
#
# When using --target without --cross, ensure the appropriate Rust target
# and linker are installed (e.g. rustup target add <triple>, plus a C
# cross-linker). When using --cross, only Docker is required.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
VERSION="${VERSION:-1.0.0}"
TARGET=""
USE_CROSS=false

# Parse arguments
while [[ $# -gt 0 ]]; do
 case "$1" in
 --target|-t)
 TARGET="$2"
 shift 2
 ;;
 --cross|-c)
 USE_CROSS=true
 shift
 ;;
 *)
 echo "Unknown option: $1"
 echo "Usage: $0 [--target <rust-target-triple>] [--cross]"
 exit 1
 ;;
 esac
done

echo "Building magnolia Linux .deb package v$VERSION"
echo ""

# Check if cargo-deb is installed
if ! command -v cargo-deb &> /dev/null; then
 echo "cargo-deb not found. Installing..."
 cargo install cargo-deb
fi

if $USE_CROSS; then
 if ! command -v cross &> /dev/null; then
 echo "cross not found. Installing..."
 cargo install cross
 fi
 if ! docker info &> /dev/null 2>&1; then
 echo "Error: Docker is not running. Start Docker Desktop and try again."
 exit 1
 fi
 BUILD_CMD="cross"
else
 BUILD_CMD="cargo"
fi

cd "$PROJECT_ROOT/backend"

TARGET_ARGS=""
if [ -n "$TARGET" ]; then
 TARGET_ARGS="--target $TARGET"
 rustup target add "$TARGET" 2>/dev/null || true
 echo "Building release binaries for $TARGET..."
else
 echo "Building release binaries..."
fi

$BUILD_CMD build --release \
 --bin magnolia_server \
 --bin service_ctl \
 --bin create_admin \
 $TARGET_ARGS

echo ""
echo "Building .deb package..."
cargo deb --no-build $TARGET_ARGS

# Find the generated .deb file (workspace target is at project root, not backend/)
DEB_FILE=$(find "$PROJECT_ROOT/target" -name "*.deb" -type f | head -1)

if [ -n "$DEB_FILE" ]; then
 cp "$DEB_FILE" "$SCRIPT_DIR/"
 FINAL_DEB="$SCRIPT_DIR/$(basename "$DEB_FILE")"

 echo ""
 echo "Build complete: $FINAL_DEB"
 echo ""
 echo "To install: sudo dpkg -i $(basename "$DEB_FILE")"
 echo "To uninstall: sudo dpkg -r magnolia_server"
else
 echo "Error: .deb file not found"
 exit 1
fi
