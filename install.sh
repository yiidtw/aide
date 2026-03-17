#!/bin/bash
set -euo pipefail

echo "aide.sh installer"
echo ""

OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)
case "$ARCH" in
  x86_64) ARCH="x86_64" ;;
  aarch64|arm64) ARCH="aarch64" ;;
  *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

case "$OS" in
  linux) TARGET="${ARCH}-unknown-linux-gnu" ;;
  darwin) TARGET="${ARCH}-apple-darwin" ;;
  *) echo "Unsupported OS: $OS"; exit 1 ;;
esac

VERSION="0.1.0"
BINARY="aide-sh-${TARGET}"
URL="https://github.com/yiidtw/aide/releases/download/v${VERSION}/${BINARY}"

echo "Downloading aide-sh v${VERSION} for ${TARGET}..."

INSTALL_DIR="${HOME}/.local/bin"
mkdir -p "${INSTALL_DIR}"

if ! curl -fsSL "$URL" -o "${INSTALL_DIR}/aide-sh"; then
  echo ""
  echo "Download failed. Install from source instead:"
  echo "  cargo install aide-sh"
  exit 1
fi

chmod +x "${INSTALL_DIR}/aide-sh"

echo ""
echo "Installed aide-sh to ${INSTALL_DIR}/aide-sh"
echo ""

if ! echo "$PATH" | grep -q "${INSTALL_DIR}"; then
  echo "Add to your PATH:"
  echo "  export PATH=\"${INSTALL_DIR}:\$PATH\""
  echo ""
fi

echo "Get started:"
echo "  aide-sh --version"
echo "  aide-sh pull aide/github-reviewer"
echo "  aide-sh run aide/github-reviewer --name reviewer"
echo "  aide-sh exec reviewer pr list"
