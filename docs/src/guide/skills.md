# Skills

Skills are reusable capability sets that can be included in an Aidefile.

## Configuration

```toml
[skills]
include = ["code-review", "git-ops"]
```

Skills are loaded from the `skills/` directory in the agent's project.

## Directory structure

```
my-agent/
├── Aidefile
├── skills/
│   ├── code-review.md
│   └── git-ops.md
└── ...
```

Each skill file is a markdown document that Claude Code can reference. The content is injected into the agent's context when running tasks.

## Sharing skills

When exporting agents with `aide export`, the `skills/` directory is included. This lets teams share skill templates via git.
