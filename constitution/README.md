# aide.sh Constitution

> All your data, one trusted agent.

This directory is the single source of truth for aide.sh architecture.
Implementation MUST align to these documents. If code diverges, update
the constitution first — then the code.

## Rules

1. Each file ≤ 200 lines. When a file exceeds 200 lines, refactor into
   multiple files or extract a sub-document.
2. Documents are living — update as decisions evolve.
3. The constitution has formal force: future formal methods will verify
   that system behavior conforms to these specifications.

## Index

| File | What it governs |
|------|----------------|
| [OVERVIEW.md](OVERVIEW.md) | What aide.sh is, tagline, thesis |
| [TRUST-MODEL.md](TRUST-MODEL.md) | Four trust layers: galv, arun, amem, apay |
| [AGENT-OS.md](AGENT-OS.md) | OS analogy, kernel/user memory, container runtime |
| [DISPATCH-SYNC.md](DISPATCH-SYNC.md) | Cross-machine dispatch, vault/skill/memory sync |
| [SKILLS.md](SKILLS.md) | Public vs private skills, taxonomy, repos |
| [AMEM.md](AMEM.md) | Distributed agent memory — consistency model, sync protocol, research path |
| [DEPLOYMENT.md](DEPLOYMENT.md) | GitOps pipeline, CI/CD, your-server deployment |
| [ECOSYSTEM.md](ECOSYSTEM.md) | 5 agent PMs, 8 domains, product map, revenue model |
