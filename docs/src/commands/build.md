# aide.sh build / push / pull

Build, publish, and fetch agent images.

## aide.sh build

Build an agent image from an Agentfile.

```
aide.sh build [PATH] [-t TAG]
```

| Flag | Description |
|------|-------------|
| `PATH` | Directory containing `Agentfile.toml` (default: `.`) |
| `-t, --tag TAG` | Tag the image (`name:version`) |

### Build process

1. **Parse** -- Loads and parses `Agentfile.toml`.
2. **Validate** -- Checks that all referenced files exist (persona, skill scripts, prompt files, seed directory).
3. **Lint** -- Runs the full lint suite including credential leak scanning.
4. **Collect** -- Gathers all files: `Agentfile.toml`, persona, skill scripts/prompts, and seed directory contents.
5. **Archive** -- Creates `<name>-<version>.tar.gz`.
6. **Checksum** -- Computes SHA-256 of the archive.

### Example

```bash
aide.sh build agents/jenny/
aide.sh build . -t jenny:0.2.0
```

## aide.sh push

Push a built agent image to the registry.

```
aide.sh push [IMAGE]
```

| Flag | Description |
|------|-------------|
| `IMAGE` | Directory or image name to push (default: `.`) |

### Push process

1. Builds the image (same as `aide.sh build`).
2. Reads registry credentials from the vault (`AIDE_REGISTRY_TOKEN`).
3. Uploads the `.tar.gz` archive to `hub.aide.sh`.

Requires prior authentication via `aide.sh login`.

## aide.sh pull

Download an agent image from the registry.

```
aide.sh pull <USER>/<TYPE>[:VERSION]
```

### Pull process

1. Resolves the image reference. Version defaults to `latest`.
2. Downloads the archive from `hub.aide.sh`.
3. Extracts to `~/.aide/types/<user>/<type>/`.

### Example

```bash
aide.sh pull ydwu/school-assistant
aide.sh pull ydwu/school-assistant:0.1.0
```

## Related commands

- `aide.sh images` -- List locally available agent images.
- `aide.sh search <query>` -- Search the registry.
- `aide.sh login` -- Authenticate with the registry.
