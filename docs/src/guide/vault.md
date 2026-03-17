# Vault & Secrets

The vault is aide.sh's encrypted secret store. Secrets are injected as environment variables at skill execution time.

## Import from .env file

```bash
$ aide.sh vault import .env
Imported 5 secrets from .env
```

The `.env` file uses standard `KEY=VALUE` format:

```
NTU_COOL_TOKEN=abc123
SMTP_USER=user@example.com
SMTP_PASS=hunter2
```

## Set individual secrets

```bash
$ aide.sh vault set NTU_COOL_TOKEN=abc123
Set NTU_COOL_TOKEN

$ aide.sh vault set SMTP_USER=user@example.com SMTP_PASS=hunter2
Set SMTP_USER
Set SMTP_PASS
```

## Check vault status

```bash
$ aide.sh vault status
Vault: ~/.aide/vault.db (encrypted, AES-256-GCM)
Secrets: 5 stored
  NTU_COOL_TOKEN   set 2025-06-01
  SMTP_USER        set 2025-06-01
  SMTP_PASS        set 2025-06-01
  POP3_USER        set 2025-06-01
  POP3_PASS        set 2025-06-01
```

## Rotate encryption key

```bash
$ aide.sh vault rotate
Vault key rotated. All secrets re-encrypted.
```

## Three-tier environment scoping

When a skill runs, environment variables are resolved in this order (highest priority first):

1. **Per-skill env** — variables listed in `[skills.NAME] env`
2. **Per-agent env** — variables listed in `[env] required` and `optional`
3. **Vault** — global secrets available to all agents

If the same key exists at multiple levels, the highest-priority value wins.

```toml
# Agentfile.toml
[skills.notifications]
script = "skills/notifications.sh"
env = ["GITHUB_TOKEN"]             # skill-level: checked first

[env]
required = ["GITHUB_TOKEN"]        # agent-level: checked second
optional = ["SLACK_WEBHOOK"]       # vault: checked last
```

## Credential leak scanning

aide.sh scans skill output for potential secret leaks:

```bash
$ aide.sh exec bot notifications
[warn] Potential secret detected in output (GITHUB_TOKEN pattern). Use --allow-leak to suppress.
```

This is a best-effort check. Always review scripts that handle sensitive data.

## Security notes

- The vault database is stored at `~/.aide/vault.db`
- Encryption uses AES-256-GCM with a key derived from your system keychain
- Secrets are never written to disk in plaintext
- `aide.sh vault export` is intentionally not supported
