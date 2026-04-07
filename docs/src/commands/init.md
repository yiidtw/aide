# aide init

Create an Aidefile in the current directory.

## Usage

```bash
aide init [--persona <name>]
```

## Options

| Flag | Description |
|------|-------------|
| `--persona <name>` | Set the persona name. Defaults to the directory name. |

## Example

```bash
cd ~/projects/my-agent
aide init --persona "Code Reviewer"
# ✓ Created Aidefile in ~/projects/my-agent
#   Edit it, then run `aide register .` to activate
```

## Generated Aidefile

```toml
[persona]
name = "Code Reviewer"
# style = "direct, concise"

[budget]
tokens = "200k"
max_retries = 3

[memory]
compact_after = "200k"

# [hooks]
# on_spawn = ["inject-vault"]
# on_complete = ["commit-memory"]

# [skills]
# include = ["code-review"]

[trigger]
on = "manual"

# [vault]
# keys = ["GITHUB_TOKEN"]
```

Errors if an Aidefile already exists in the current directory.
