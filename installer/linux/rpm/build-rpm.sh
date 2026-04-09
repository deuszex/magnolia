#!/bin/bash
# Build Linux .rpm package for magnolia (Fedora / RHEL / openSUSE)
#
# Usage:
#   ./build-rpm.sh                                      # native build
#   ./build-rpm.sh --target x86_64-unknown-linux-gnu   # specific target
#   ./build-rpm.sh --target aarch64-unknown-linux-gnu  # arm64
#   ./build-rpm.sh --cross                              # use cross (Docker)
#   ./build-rpm.sh --cross --target aarch64-unknown-linux-gnu
#
# Prerequisites:
#   cargo install cargo-generate-rpm
#
# When using --target without --cross, ensure the appropriate Rust target
# and C cross-linker are installed (rustup target add <triple>).
# When using --cross, only Docker is required.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
VERSION="${VERSION:-1.0.0}"
TARGET=""
USE_CROSS=false

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

echo "Building magnolia Linux .rpm package v$VERSION"
echo ""

# Check if cargo-generate-rpm is installed
if ! command -v cargo-generate-rpm &> /dev/null; then
    echo "cargo-generate-rpm not found. Installing..."
    cargo install cargo-generate-rpm
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
echo "Building .rpm package..."
cargo generate-rpm --auto-req disabled $TARGET_ARGS

# Find the generated .rpm file
RPM_FILE=$(find "$PROJECT_ROOT/target" -name "*.rpm" -type f | head -1)

if [ -n "$RPM_FILE" ]; then
    cp "$RPM_FILE" "$SCRIPT_DIR/"
    FINAL_RPM="$SCRIPT_DIR/$(basename "$RPM_FILE")"

    echo ""
    echo "Build complete: $FINAL_RPM"
    echo ""
    echo "To install:   sudo rpm -i $(basename "$RPM_FILE")"
    echo "              sudo dnf install ./$(basename "$RPM_FILE")"
    echo "To upgrade:   sudo rpm -U $(basename "$RPM_FILE")"
    echo "              sudo dnf upgrade ./$(basename "$RPM_FILE")"
    echo "To uninstall: sudo rpm -e magnolia_server"
    echo "              sudo dnf remove magnolia_server"
else
    echo "Error: .rpm file not found"
    exit 1
fi
