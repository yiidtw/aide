#!/usr/bin/env bash
# release — build release binary
# usage: release [target]
set -euo pipefail

cd "${AIDE_PROJECT_DIR:-$(pwd)}"

TARGET="${1:-}"

if [ -n "$TARGET" ]; then
    echo "=== cargo build --release --target $TARGET ==="
    cargo build --release --target "$TARGET" 2>&1
else
    echo "=== cargo build --release ==="
    cargo build --release 2>&1
fi

BIN="target/release/aide-sh"
if [ -n "$TARGET" ]; then
    BIN="target/$TARGET/release/aide-sh"
fi

if [ -f "$BIN" ]; then
    SIZE=$(ls -lh "$BIN" | awk '{print $5}')
    SHA=$(shasum -a 256 "$BIN" | cut -c1-12)
    echo ""
    echo "binary: $BIN"
    echo "size:   $SIZE"
    echo "sha256: $SHA..."
fi

echo ""
echo "release: done"
