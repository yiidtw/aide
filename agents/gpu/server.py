"""GPU daemon — FastAPI server wrapping Ollama + Whisper with VRAM-aware scheduling."""

from __future__ import annotations

import asyncio
import logging
import os
import time
from contextlib import asynccontextmanager
from typing import Any

import httpx
import uvicorn
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel

from gpu_queue import Job, JobQueue, Priority
from vram import VRAMTracker

logger = logging.getLogger("gpu-daemon")

# ---------------------------------------------------------------------------
# Config
# ---------------------------------------------------------------------------
OLLAMA_HOST = os.environ.get("OLLAMA_HOST", "http://localhost:11434")
SERVER_PORT = int(os.environ.get("GPU_SERVER_PORT", "8844"))
DEFAULT_WHISPER_MODEL = os.environ.get("WHISPER_MODEL_SIZE", "large-v3")
WHISPER_IDLE_TIMEOUT = 300  # seconds before unloading idle whisper model

# ---------------------------------------------------------------------------
# Global state
# ---------------------------------------------------------------------------
vram_tracker = VRAMTracker()
job_queue = JobQueue(max_concurrent=1)

# Whisper model (lazy-loaded)
_whisper_model: Any = None
_whisper_model_name: str | None = None
_whisper_last_used: float = 0.0


# ---------------------------------------------------------------------------
# Whisper management
# ---------------------------------------------------------------------------
def _load_whisper(model_size: str) -> Any:
    """Load a faster-whisper model, checking VRAM first."""
    global _whisper_model, _whisper_model_name, _whisper_last_used

    if _whisper_model is not None and _whisper_model_name == model_size:
        _whisper_last_used = time.time()
        return _whisper_model

    # Unload existing model if different
    if _whisper_model is not None:
        _unload_whisper()

    if not vram_tracker.can_load(model_size):
        raise RuntimeError(
            f"Not enough VRAM to load whisper {model_size}. "
            f"Free: {vram_tracker.query_gpu().vram_free_mib} MiB, "
            f"need ~{vram_tracker.estimate_vram(model_size)} MiB"
        )

    from faster_whisper import WhisperModel

    logger.info("Loading whisper model: %s", model_size)
    _whisper_model = WhisperModel(model_size, device="cuda", compute_type="float16")
    _whisper_model_name = model_size
    _whisper_last_used = time.time()
    vram_tracker.register_model(model_size)
    logger.info("Whisper model loaded: %s", model_size)
    return _whisper_model


def _unload_whisper() -> None:
    """Unload the whisper model to free VRAM."""
    global _whisper_model, _whisper_model_name
    if _whisper_model is not None:
        logger.info("Unloading whisper model: %s", _whisper_model_name)
        if _whisper_model_name:
            vram_tracker.unregister_model(_whisper_model_name)
        del _whisper_model
        _whisper_model = None
        _whisper_model_name = None
        # Help the garbage collector reclaim GPU memory
        try:
            import torch

            torch.cuda.empty_cache()
        except ImportError:
            pass


async def _whisper_idle_checker() -> None:
    """Background task to unload whisper if idle for too long."""
    while True:
        await asyncio.sleep(60)
        if (
            _whisper_model is not None
            and _whisper_last_used > 0
            and (time.time() - _whisper_last_used) > WHISPER_IDLE_TIMEOUT
        ):
            logger.info("Whisper idle for %ds, unloading", WHISPER_IDLE_TIMEOUT)
            _unload_whisper()


# ---------------------------------------------------------------------------
# Queue worker
# ---------------------------------------------------------------------------
async def _queue_worker() -> None:
    """Process jobs from the queue one at a time."""
    while True:
        job = await job_queue.next()
        if job is None:
            await asyncio.sleep(0.1)
            continue

        try:
            if job.kind == "generate":
                result = await _handle_generate(job.payload)
                await job_queue.complete(job, result=result)
            elif job.kind == "transcribe":
                result = await _handle_transcribe(job.payload)
                await job_queue.complete(job, result=result)
            else:
                await job_queue.complete(job, error=f"Unknown job kind: {job.kind}")
        except Exception as e:
            logger.exception("Job %s failed", job.id)
            await job_queue.complete(job, error=str(e))


async def _handle_generate(payload: dict) -> dict:
    """Proxy a generate request to Ollama."""
    async with httpx.AsyncClient(timeout=300.0) as client:
        resp = await client.post(
            f"{OLLAMA_HOST}/api/generate",
            json=payload,
        )
        resp.raise_for_status()
        return resp.json()


async def _handle_transcribe(payload: dict) -> dict:
    """Run whisper transcription."""
    audio_path = payload["audio_path"]
    model_size = payload.get("model_size", DEFAULT_WHISPER_MODEL)
    language = payload.get("language")

    # Run in executor to avoid blocking the event loop
    loop = asyncio.get_event_loop()
    return await loop.run_in_executor(
        None, _transcribe_sync, audio_path, model_size, language
    )


def _transcribe_sync(audio_path: str, model_size: str, language: str | None) -> dict:
    """Synchronous whisper transcription."""
    model = _load_whisper(model_size)

    kwargs: dict[str, Any] = {}
    if language:
        kwargs["language"] = language

    segments_iter, info = model.transcribe(audio_path, **kwargs)

    segments = []
    full_text_parts = []
    for seg in segments_iter:
        segments.append(
            {
                "start": round(seg.start, 3),
                "end": round(seg.end, 3),
                "text": seg.text.strip(),
            }
        )
        full_text_parts.append(seg.text.strip())

    return {
        "language": info.language,
        "full_text": " ".join(full_text_parts),
        "segments": segments,
    }


# ---------------------------------------------------------------------------
# FastAPI app
# ---------------------------------------------------------------------------
@asynccontextmanager
async def lifespan(app: FastAPI):
    """Start background tasks on startup."""
    worker_task = asyncio.create_task(_queue_worker())
    idle_task = asyncio.create_task(_whisper_idle_checker())
    yield
    worker_task.cancel()
    idle_task.cancel()


app = FastAPI(title="GPU Daemon", version="0.1.0", lifespan=lifespan)


# --- Request / Response models ---


class GenerateRequest(BaseModel):
    model: str
    prompt: str
    stream: bool = False
    options: dict[str, Any] | None = None
    priority: Priority = Priority.NORMAL


class TranscribeRequest(BaseModel):
    audio_path: str
    model_size: str = DEFAULT_WHISPER_MODEL
    language: str | None = None
    priority: Priority = Priority.NORMAL


class TranscribeResponse(BaseModel):
    language: str
    full_text: str
    segments: list[dict[str, Any]]


# --- Endpoints ---


@app.get("/gpu/status")
async def gpu_status():
    """GPU info, loaded models, and queue depth."""
    vram = vram_tracker.status()
    queue = job_queue.status()
    return {
        **vram,
        "queue": queue,
        "estimated_wait_s": job_queue.estimated_wait(),
    }


@app.post("/gpu/generate")
async def gpu_generate(req: GenerateRequest):
    """VRAM-aware proxy to Ollama /api/generate."""
    if req.stream:
        raise HTTPException(400, "Streaming not supported in queued mode. Use Ollama directly.")

    payload = {
        "model": req.model,
        "prompt": req.prompt,
        "stream": False,
    }
    if req.options:
        payload["options"] = req.options

    job = Job(kind="generate", payload=payload, priority=req.priority)
    await job_queue.submit(job)

    # Wait for completion
    await job.event.wait()

    if job.error:
        raise HTTPException(500, detail=job.error)
    return job.result


@app.post("/gpu/transcribe", response_model=TranscribeResponse)
async def gpu_transcribe(req: TranscribeRequest):
    """Whisper transcription with VRAM management."""
    if not os.path.exists(req.audio_path):
        raise HTTPException(404, f"Audio file not found: {req.audio_path}")

    payload = {
        "audio_path": req.audio_path,
        "model_size": req.model_size,
        "language": req.language,
    }

    job = Job(kind="transcribe", payload=payload, priority=req.priority)
    await job_queue.submit(job)

    # Wait for completion
    await job.event.wait()

    if job.error:
        raise HTTPException(500, detail=job.error)
    return job.result


@app.get("/gpu/models")
async def gpu_models():
    """List loaded models with VRAM usage."""
    # Merge our tracked models with what Ollama reports
    models: dict[str, Any] = {}

    # Our tracked models (whisper, etc.)
    for name, vram_mib in vram_tracker.loaded_models.items():
        models[name] = {"source": "whisper", "vram_mib": vram_mib}

    # Ollama models
    try:
        async with httpx.AsyncClient(timeout=5.0) as client:
            resp = await client.get(f"{OLLAMA_HOST}/api/ps")
            if resp.status_code == 200:
                data = resp.json()
                for m in data.get("models", []):
                    name = m.get("name", "unknown")
                    size_bytes = m.get("size", 0)
                    models[name] = {
                        "source": "ollama",
                        "vram_mib": round(size_bytes / (1024 * 1024)),
                    }
    except httpx.HTTPError:
        pass  # Ollama might not be running

    return {"models": models}


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------
if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO, format="%(asctime)s %(name)s %(levelname)s %(message)s")
    uvicorn.run(app, host="0.0.0.0", port=SERVER_PORT)
