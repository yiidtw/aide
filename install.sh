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
URL="https://github.com/yiidtw/aide/releases/download/v${VERSION}/aide-sh-${TARGET}.tar.gz"

echo "Downloading aide-sh v${VERSION} for ${TARGET}..."

TMPDIR=$(mktemp -d)
if ! curl -fsSL "$URL" -o "${TMPDIR}/aide-sh.tar.gz"; then
  echo ""
  echo "Binary not found. Install from source instead:"
  echo "  cargo install aide-sh"
  rm -rf "${TMPDIR}"
  exit 1
fi

tar xzf "${TMPDIR}/aide-sh.tar.gz" -C "${TMPDIR}"

INSTALL_DIR="${HOME}/.local/bin"
mkdir -p "${INSTALL_DIR}"
mv "${TMPDIR}/aide-sh" "${INSTALL_DIR}/aide-sh"
chmod +x "${INSTALL_DIR}/aide-sh"
rm -rf "${TMPDIR}"

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
echo "  aide-sh pull aide/devops"
echo "  aide-sh run aide/devops --name bot"
echo "  aide-sh exec bot check-uptime"
