# aide vault

Access vault secrets from the CLI.

## aide vault get

```bash
aide vault get <key>
```

Decrypts the vault and prints the value of the specified key. Output has no trailing newline (suitable for piping).

```bash
aide vault get GITHUB_TOKEN
# ghp_abc123...

# Use in scripts
export TOKEN=$(aide vault get GITHUB_TOKEN)
```

## aide vault list

```bash
aide vault list
```

Lists all key names in the vault (values are not shown).

```bash
aide vault list
# GITHUB_TOKEN
# SLACK_WEBHOOK
```

## Vault file location

The vault files (`vault.age`, `vault.key`) are looked up in the current working directory. Run these commands from the agent's directory, or the directory containing your vault files.
