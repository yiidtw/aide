# aide register / unregister

Register or unregister an existing project as an agent.

## register

```bash
aide register <path> [--name <name>]
```

| Flag | Description |
|------|-------------|
| `--name <name>` | Agent name. Defaults to the directory name. |

The directory must contain an Aidefile. After registration, you can use `aide run <name>` instead of the full path.

```bash
aide register ~/projects/my-agent --name reviewer
# ✓ Registered 'reviewer' → ~/projects/my-agent
```

## unregister

```bash
aide unregister <name>
```

Removes the agent from the registry. Does **not** delete the directory or its files.

```bash
aide unregister reviewer
# ✓ Unregistered 'reviewer' (files not deleted)
```
