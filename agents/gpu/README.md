# gpu agent

GPU daemon for aide.sh. Wraps Ollama (LLM inference) and faster-whisper
(audio transcription) behind a single FastAPI server with VRAM-aware
scheduling and job queuing.

## Endpoints

| Method | Path              | Description                              |
|--------|-------------------|------------------------------------------|
| GET    | `/gpu/status`     | GPU info, VRAM, loaded models, queue     |
| POST   | `/gpu/generate`   | Ollama generate proxy with queuing       |
| POST   | `/gpu/transcribe` | Whisper transcription                    |
| GET    | `/gpu/models`     | List loaded models with VRAM usage       |

## Quick start

```bash
aide.sh exec gpu serve            # start daemon
aide.sh exec gpu status           # check GPU status
```

## Configuration

Environment variables (set via `aide.sh vault`):

- `OLLAMA_HOST` — Ollama API URL (default: `http://localhost:11434`)
- `GPU_SERVER_PORT` — daemon port (default: `8844`)
- `WHISPER_MODEL_SIZE` — default whisper model (default: `large-v3`)

## Requirements

- NVIDIA GPU with CUDA support
- Ollama running locally
- Python 3.10+
