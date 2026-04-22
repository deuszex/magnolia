#!/bin/bash
# Cross-compile Linux .rpm package using cargo-zigbuild.
# Works natively on Windows, macOS, and Linux, no Docker, no WSL.
# This workaround was done because one of the ways to compile is
# cargo cross, which uses docker. Also not everyone wants to fight WSL,
# if they are even doing this on windows to begin with.
#
# Prerequisites:
#   Zig installed (https://ziglang.org/download/ or: winget install zig.zig)
#   cargo install cargo-zigbuild
#   cargo install cargo-generate-rpm
#
# Usage:
#   ./cross-build-rpm.sh                                      # default x86_64
#   ./cross-build-rpm.sh --target aarch64-unknown-linux-gnu  # arm64

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
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

echo "Cross-compiling magnolia Linux .rpm package v$VERSION"
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

if ! command -v cargo-generate-rpm &> /dev/null; then
    echo "cargo-generate-rpm not found. Installing..."
    cargo install cargo-generate-rpm
fi

cd "$PROJECT_ROOT/backend"

rustup target add "$TARGET" 2>/dev/null || true

echo "Building release binaries with cargo-zigbuild..."
cargo zigbuild --release \
    --bin magnolia_server \
    --bin service_ctl \
    --bin create_admin \
    --target "$TARGET"

# cargo-generate-rpm resolves asset source paths relative to backend/,
# so Cargo.toml uses ../target/release/* (workspace root).
# Copy the cross-compiled binaries there so generate-rpm can find them.
echo "Staging binaries for packaging..."
mkdir -p "../target/release"
cp "../target/$TARGET/release/magnolia_server" "../target/release/magnolia_server"
cp "../target/$TARGET/release/service_ctl"     "../target/release/service_ctl"
cp "../target/$TARGET/release/create_admin"    "../target/release/create_admin"

echo ""
echo "Building .rpm package..."
cargo generate-rpm --auto-req disabled

RPM_FILE=$(find "$PROJECT_ROOT/backend/target" -name "*.rpm" -type f | head -1)

if [ -n "$RPM_FILE" ]; then
    cp "$RPM_FILE" "$SCRIPT_DIR/"
    echo ""
    echo "Build complete: $SCRIPT_DIR/$(basename "$RPM_FILE")"
    echo ""
    echo "To install:   sudo rpm -i $(basename "$RPM_FILE")"
    echo "              sudo dnf install ./$(basename "$RPM_FILE")"
    echo "To upgrade:   sudo rpm -U $(basename "$RPM_FILE")"
    echo "To uninstall: sudo rpm -e magnolia_server"
else
    echo "Error: .rpm file not found"
    exit 1
fi
