# Quick Start

Build and run your first agent in under 5 minutes.

## 1. Scaffold a new agent

```bash
$ aide.sh init school
Created school/
  Agentfile.toml
  persona.md
  skills/
    hello.sh
```

## 2. Edit the Agentfile

Open `school/Agentfile.toml`. The template looks like this:

```toml
[agent]
name = "school"
version = "0.1.0"
description = "My first agent"
author = "you"

[persona]
file = "persona.md"

[skills.hello]
script = "skills/hello.sh"
description = "Say hello"
usage = "hello [name]"

[seed]
dir = "seed/"

[env]
required = []
optional = []
```

Add a skill, change the description, or leave the defaults -- it works out of the box.

## 3. Build the image

```bash
$ aide.sh build school/
Building school v0.1.0 ...
Image: school:0.1.0 (sha256:a3f8...)
```

This packages the Agentfile, persona, skills, and seed data into a compressed image stored in `~/.aide/images/`.

## 4. Run an instance

```bash
$ aide.sh run school --name school
Instance "school" started (id: school)
```

## 5. Execute a skill

```bash
$ aide.sh exec school hello
Hello, world!

$ aide.sh exec school hello Alice
Hello, Alice!
```

## 6. List available skills

```bash
$ aide.sh exec school
Available skills:
  hello    Say hello
```

## 7. Open the dashboard

```bash
$ aide.sh dash
Dashboard running at http://localhost:3939
```

The dashboard shows all running instances, recent logs, and skill status.

## What's next?

- [Concepts](./concepts.md) — understand images, instances, vault, and semantic injection
- [Agentfile.toml reference](../guide/agentfile.md) — full configuration guide
- [Skills](../guide/skills.md) — writing script and prompt skills
