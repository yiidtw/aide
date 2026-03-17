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

VERSION="${AIDE_VERSION:-0.3.1}"
BINARY="aide-sh-${TARGET}"
URL="https://github.com/yiidtw/aide/releases/download/v${VERSION}/${BINARY}"

echo "Downloading aide-sh v${VERSION} for ${TARGET}..."

INSTALL_DIR="${HOME}/.local/bin"
mkdir -p "${INSTALL_DIR}"

# TODO: Add checksum verification once sha256 files are published in releases
# CHECKSUM_URL="${URL}.sha256"
# curl -sL --fail "$CHECKSUM_URL" -o "${INSTALL_DIR}/aide-sh.sha256"
# echo "$(cat ${INSTALL_DIR}/aide-sh.sha256)  ${INSTALL_DIR}/aide-sh" | shasum -a 256 -c -

if ! curl -sL --fail-with-body "$URL" -o "${INSTALL_DIR}/aide-sh"; then
  echo ""
  echo "Error: Download failed for ${URL}"
  echo "Possible causes:"
  echo "  - Version v${VERSION} does not exist"
  echo "  - Platform ${TARGET} is not supported in this release"
  echo "  - Network issue (check your connection)"
  echo ""
  echo "Install from source instead:"
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
echo ""
echo "Tip: alias aide.sh=aide-sh for Docker-style commands"
