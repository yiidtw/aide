# Teaching Monster Competition

## Scoring Rubric (4 dimensions, 1-10 each)

### Accuracy
- Zero hallucination: all facts, formulas, definitions correct
- Penalize: invented facts, wrong formulas, misleading simplifications

### Logic & Flow
- Scaffolding: simple → complex, prerequisites introduced first
- Coherence: smooth transitions, logical connections
- Penalize: random jumps, missing prerequisites, circular explanations

### Adaptability
- Persona match: difficulty calibrated to student background
- Content depth: appropriate for topic complexity
- Penalize: mismatch between persona and content, ignored constraints

### Engagement
- Multimodal: slides complement (not repeat) narration
- Storytelling: narrative arc, examples, analogies
- Penalize: dry facts, slides = narration text, no motivation

## Pipeline Architecture

storylens teach pipeline (runs on formace-00 GPU server):
1. **Scaffold** — LLM generates course outline (3-7 sections)
2. **Script** — LLM generates narration + slide content per section
3. **Self-eval** — Cross-model evaluation before expensive rendering
4. **TTS** — Edge-TTS generates audio with word timestamps
5. **Visuals** — Manim animations (primary) or Pillow statics (fallback)
6. **Composite** — FFmpeg muxes audio + visuals → MP4 + WebVTT

## API Contract

```
POST /api/competition/generate
{
  "request_id": "topic-id",
  "course_requirement": "topic text",
  "student_persona": "persona description"
}

Response (streaming, heartbeat newlines every 30s):
{
  "video_url": "https://api.storylens.ai/static/teach/{id}/teaching_video.mp4",
  "subtitle_url": "https://api.storylens.ai/static/teach/{id}/teaching_video.vtt",
  "supplementary_url": []
}
```

## Typical Generation Time
~5 minutes per topic (Manim rendering is the bottleneck)
