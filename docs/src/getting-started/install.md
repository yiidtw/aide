# Installation

## From crates.io (recommended)

```bash
cargo install aide-sh
```

This installs the `aide` binary.

## Prerequisites

- **Rust toolchain** — [rustup.rs](https://rustup.rs) if you don't have `cargo`
- **Claude Code CLI** — aide calls `claude -p` under the hood. Install from [claude.ai/code](https://claude.ai/code)
- **age** (optional) — for vault encryption. `brew install age` or your package manager

## Verify

```bash
aide --version
# aide-sh 2.0.0-alpha.2
```

## From source

```bash
git clone https://github.com/yiidtw/aide.git
cd aide
cargo install --path .
```

## Next

Once installed, proceed to the [Quick Start](./quickstart.md).
