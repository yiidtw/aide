"""VRAM tracking and management for GPU agent."""

from __future__ import annotations

import subprocess
import xml.etree.ElementTree as ET
from dataclasses import dataclass, field


# Rough VRAM estimates for common models (in MiB)
VRAM_ESTIMATES: dict[str, int] = {
    # Whisper models
    "tiny": 400,
    "base": 500,
    "small": 1000,
    "medium": 2000,
    "large-v2": 3200,
    "large-v3": 3200,
    # Ollama models (rough q4 estimates)
    "llama3.2:1b": 1200,
    "llama3.2:3b": 2800,
    "llama3.1:8b": 5500,
    "llama3.1:70b": 42000,
    "mistral:7b": 5000,
    "phi3:3.8b": 3000,
    "gemma2:2b": 2000,
    "gemma2:9b": 6500,
}


@dataclass
class GPUInfo:
    name: str = "unknown"
    vram_total_mib: int = 0
    vram_used_mib: int = 0
    vram_free_mib: int = 0
    temperature_c: int | None = None
    utilization_pct: int | None = None


@dataclass
class VRAMTracker:
    """Track VRAM usage and loaded models."""

    loaded_models: dict[str, int] = field(default_factory=dict)  # model_name -> vram_mib

    def query_gpu(self) -> GPUInfo:
        """Query nvidia-smi for current GPU info."""
        try:
            result = subprocess.run(
                ["nvidia-smi", "-q", "-x"],
                capture_output=True,
                text=True,
                timeout=5,
            )
            if result.returncode != 0:
                return GPUInfo()

            root = ET.fromstring(result.stdout)
            gpu = root.find("gpu")
            if gpu is None:
                return GPUInfo()

            info = GPUInfo()

            product_name = gpu.find("product_name")
            if product_name is not None and product_name.text:
                info.name = product_name.text

            fb_memory = gpu.find("fb_memory_usage")
            if fb_memory is not None:
                total = fb_memory.find("total")
                used = fb_memory.find("used")
                free = fb_memory.find("free")
                if total is not None and total.text:
                    info.vram_total_mib = int(total.text.split()[0])
                if used is not None and used.text:
                    info.vram_used_mib = int(used.text.split()[0])
                if free is not None and free.text:
                    info.vram_free_mib = int(free.text.split()[0])

            temp = gpu.find("temperature")
            if temp is not None:
                gpu_temp = temp.find("gpu_temp")
                if gpu_temp is not None and gpu_temp.text:
                    info.temperature_c = int(gpu_temp.text.split()[0])

            util = gpu.find("utilization")
            if util is not None:
                gpu_util = util.find("gpu_util")
                if gpu_util is not None and gpu_util.text:
                    info.utilization_pct = int(gpu_util.text.split()[0])

            return info

        except (FileNotFoundError, subprocess.TimeoutExpired, ET.ParseError):
            return GPUInfo()

    def estimate_vram(self, model_name: str) -> int:
        """Estimate VRAM needed for a model in MiB."""
        if model_name in VRAM_ESTIMATES:
            return VRAM_ESTIMATES[model_name]
        # Default: assume ~4GB for unknown models
        return 4000

    def can_load(self, model_name: str) -> bool:
        """Check if there's enough free VRAM to load a model."""
        if model_name in self.loaded_models:
            return True  # already loaded
        gpu = self.query_gpu()
        needed = self.estimate_vram(model_name)
        return gpu.vram_free_mib >= needed

    def register_model(self, model_name: str, vram_mib: int | None = None) -> None:
        """Register a model as loaded."""
        if vram_mib is None:
            vram_mib = self.estimate_vram(model_name)
        self.loaded_models[model_name] = vram_mib

    def unregister_model(self, model_name: str) -> None:
        """Unregister a model."""
        self.loaded_models.pop(model_name, None)

    def status(self) -> dict:
        """Return full status dict."""
        gpu = self.query_gpu()
        return {
            "gpu_name": gpu.name,
            "vram_total_mib": gpu.vram_total_mib,
            "vram_used_mib": gpu.vram_used_mib,
            "vram_free_mib": gpu.vram_free_mib,
            "temperature_c": gpu.temperature_c,
            "utilization_pct": gpu.utilization_pct,
            "loaded_models": dict(self.loaded_models),
        }
