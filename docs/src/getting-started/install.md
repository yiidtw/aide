# Installation

aide.sh ships as a single static binary. No runtime dependencies.

## One-line install (recommended)

```bash
curl -fsSL https://aide.sh/install | bash
```

This detects your OS and architecture, downloads the latest release, and places the binary at `~/.local/bin/aide`.

## Cargo install

If you have a Rust toolchain:

```bash
cargo install aide
```

## Build from source

```bash
git clone https://github.com/AIDEdotsh/aide.git
cd aide
cargo build --release
cp target/release/aide ~/.local/bin/
```

## Verify

```bash
$ aide --version
aide 0.1.0
```

Make sure `~/.local/bin` is in your `PATH`:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

## Shell alias (optional)

For convenience, alias the binary to `aide.sh`:

```bash
echo 'alias aide.sh="aide"' >> ~/.bashrc
source ~/.bashrc
```

## Next

Once installed, proceed to the [Quick Start](./quickstart.md).
