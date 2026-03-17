# Check gh CLI and cd to project dir
check_gh() {
  if [ -n "${AIDE_PROJECT_DIR:-}" ]; then
    cd "$AIDE_PROJECT_DIR"
  fi
  if ! command -v gh &>/dev/null; then
    echo "GitHub CLI (gh) not found."
    echo ""
    echo "Install it:"
    echo "  brew install gh        # macOS"
    echo "  apt install gh         # Ubuntu"
    echo "  https://cli.github.com # other"
    echo ""
    echo "Then: gh auth login"
    exit 1
  fi
  if ! gh auth status &>/dev/null 2>&1; then
    echo "Not logged in to GitHub CLI."
    echo "Run: gh auth login"
    exit 1
  fi
}
