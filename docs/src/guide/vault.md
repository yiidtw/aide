# Vault & Secrets

aide uses [age](https://age-encryption.org/) encryption to store secrets. At spawn time, secrets are decrypted and injected as environment variables into `claude -p`. They never enter the LLM context window.

## Setup

### 1. Generate a key

```bash
age-keygen -o vault.key
# Public key: age1...
```

### 2. Create secrets

```bash
cat <<'EOF' > secrets.env
export GITHUB_TOKEN='ghp_...'
export SLACK_WEBHOOK='https://hooks.slack.com/...'
EOF

age -r age1... -o vault.age secrets.env
rm secrets.env
```

### 3. Reference in Aidefile

```toml
[vault]
keys = ["GITHUB_TOKEN", "SLACK_WEBHOOK"]
```

## How it works

```
vault.age (encrypted) + vault.key (private key)
    │
    └─ age -d -i vault.key vault.age
         │
         └─ parse: export KEY='VALUE'
              │
              └─ filter by [vault].keys
                   │
                   └─ Command::env("GITHUB_TOKEN", "ghp_...")
                        │
                        └─ claude -p runs with env vars set
```

Secrets are passed via `Command::env()` — the OS process environment. They are **not** injected into the prompt, system message, or any text the LLM sees.

## File layout

```
my-agent/
├── Aidefile
├── vault.key      ← private key (never commit this)
├── vault.age      ← encrypted secrets
└── ...
```

Add to `.gitignore`:
```
vault.key
```

The `vault.age` file can be safely committed — it's encrypted.

## CLI access

```bash
# Get a single secret
aide vault get GITHUB_TOKEN

# List all key names
aide vault list
```

## MCP access

The `aide_vault_get` MCP tool lets other LLM agents retrieve secrets programmatically.
