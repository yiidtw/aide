#!/bin/bash
# aide.sh installer — Docker for AI agents
# Usage: curl -fsSL https://hub.aide.sh/install | bash
set -euo pipefail

DOWNLOAD_BASE="https://hub.aide.sh/dl"
INSTALL_DIR="${AIDE_INSTALL_DIR:-/usr/local/bin}"
BINARY_NAME="aide.sh"

echo ""
echo "  aide.sh — Docker for AI agents"
echo ""

# Detect platform
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)
case "$ARCH" in
  x86_64|amd64) ARCH="x86_64" ;;
  arm64|aarch64) ARCH="aarch64" ;;
  *) echo "error: unsupported architecture: $ARCH"; exit 1 ;;
esac

case "$OS" in
  darwin) PLATFORM="apple-darwin" ;;
  linux)  PLATFORM="unknown-linux-gnu" ;;
  *)      echo "error: unsupported OS: $OS"; exit 1 ;;
esac

TARGET="aide-sh-${ARCH}-${PLATFORM}"
echo "  platform: ${OS}/${ARCH}"
echo "  downloading: ${TARGET}"
echo ""

# Download
TMP=$(mktemp)
HTTP_CODE=$(curl -fsSL -w "%{http_code}" -o "$TMP" "${DOWNLOAD_BASE}/${TARGET}" 2>/dev/null || true)

if [ ! -s "$TMP" ] || [ "$HTTP_CODE" = "404" ]; then
  echo "error: binary not available for ${OS}/${ARCH}"
  echo ""
  echo "  Build from source instead:"
  echo "    cargo install --git https://github.com/yiidtw/aide"
  echo ""
  rm -f "$TMP"
  exit 1
fi

chmod +x "$TMP"

# Install
if [ -w "$INSTALL_DIR" ]; then
  mv "$TMP" "${INSTALL_DIR}/aide-sh"
  ln -sf "${INSTALL_DIR}/aide-sh" "${INSTALL_DIR}/${BINARY_NAME}"
else
  echo "  installing to ${INSTALL_DIR} (requires sudo)"
  sudo mv "$TMP" "${INSTALL_DIR}/aide-sh"
  sudo chmod +x "${INSTALL_DIR}/aide-sh"
  sudo ln -sf "${INSTALL_DIR}/aide-sh" "${INSTALL_DIR}/${BINARY_NAME}"
fi

echo "  installed: ${INSTALL_DIR}/${BINARY_NAME}"
echo ""
echo "  Get started:"
echo "    aide.sh pull ydwu/jenny"
echo "    aide.sh run ydwu/jenny --name jenny.me"
echo "    aide.sh exec -it jenny.me cool courses"
echo "    aide.sh ps"
echo ""
echo "  Docs: https://aide.sh"
echo ""
