"""Simple priority job queue for GPU agent."""

from __future__ import annotations

import asyncio
import time
import uuid
from dataclasses import dataclass, field
from enum import Enum
from typing import Any


class Priority(str, Enum):
    HIGH = "high"
    NORMAL = "normal"
    LOW = "low"


PRIORITY_ORDER = {Priority.HIGH: 0, Priority.NORMAL: 1, Priority.LOW: 2}


@dataclass
class Job:
    id: str = field(default_factory=lambda: uuid.uuid4().hex[:12])
    priority: Priority = Priority.NORMAL
    kind: str = ""  # "generate", "transcribe"
    payload: dict[str, Any] = field(default_factory=dict)
    created_at: float = field(default_factory=time.time)
    started_at: float | None = None
    completed_at: float | None = None
    result: Any = None
    error: str | None = None
    event: asyncio.Event = field(default_factory=asyncio.Event)

    def elapsed(self) -> float:
        if self.started_at is None:
            return 0.0
        end = self.completed_at or time.time()
        return end - self.started_at


class JobQueue:
    """FIFO queue with priority levels. Processes one GPU job at a time."""

    def __init__(self, max_concurrent: int = 1) -> None:
        self._pending: list[Job] = []
        self._running: dict[str, Job] = {}
        self._completed: dict[str, Job] = {}
        self._lock = asyncio.Lock()
        self._semaphore = asyncio.Semaphore(max_concurrent)
        self._max_completed = 100  # keep last N completed jobs

    async def submit(self, job: Job) -> Job:
        """Add a job to the queue. Returns the job (caller awaits job.event)."""
        async with self._lock:
            self._pending.append(job)
            # Sort by priority, then by creation time (FIFO within same priority)
            self._pending.sort(
                key=lambda j: (PRIORITY_ORDER.get(j.priority, 1), j.created_at)
            )
        return job

    async def next(self) -> Job | None:
        """Pop the next job from the queue."""
        async with self._lock:
            if not self._pending:
                return None
            job = self._pending.pop(0)
            job.started_at = time.time()
            self._running[job.id] = job
            return job

    async def complete(self, job: Job, result: Any = None, error: str | None = None) -> None:
        """Mark a job as completed."""
        async with self._lock:
            job.completed_at = time.time()
            job.result = result
            job.error = error
            job.event.set()
            self._running.pop(job.id, None)
            self._completed[job.id] = job
            # Trim old completed jobs
            if len(self._completed) > self._max_completed:
                oldest_key = next(iter(self._completed))
                del self._completed[oldest_key]

    def status(self) -> dict:
        """Return queue status."""
        return {
            "pending": len(self._pending),
            "running": len(self._running),
            "completed": len(self._completed),
            "running_jobs": [
                {
                    "id": j.id,
                    "kind": j.kind,
                    "priority": j.priority.value,
                    "elapsed_s": round(j.elapsed(), 1),
                }
                for j in self._running.values()
            ],
            "pending_jobs": [
                {
                    "id": j.id,
                    "kind": j.kind,
                    "priority": j.priority.value,
                }
                for j in self._pending
            ],
        }

    def estimated_wait(self) -> float:
        """Rough estimate of wait time in seconds."""
        # Very rough: assume 30s per generate, 60s per transcribe
        wait = 0.0
        for job in self._running.values():
            avg = 60.0 if job.kind == "transcribe" else 30.0
            remaining = max(0, avg - job.elapsed())
            wait += remaining
        for job in self._pending:
            wait += 60.0 if job.kind == "transcribe" else 30.0
        return round(wait, 1)
