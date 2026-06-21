# Radio Play Generation Pipeline — Roadmap

**Date:** 2026-06-21
**Status:** Proposed (experimental)
**Goal:** Convert any movie into a complete radio play with AI-narrated scene descriptions

## Vision

Input: Movie file (MP4/MKV/WebM)
Output: Complete radio play audio (FLAC/MP3) with:
- Original dialogue preserved
- Non-voice segments narrated with emotional AI voice describing visual scenes
- Smooth audio transitions (crossfade, ducking)
- Quality that improves with every run

## Architecture Overview

```
Movie File
    │
    ▼
┌─────────────────────────────────────────────────┐
│ GOAP Planner (ADR-120)                          │
│                                                 │
│  ┌─────────┐   ┌──────────┐   ┌─────────────┐  │
│  │ Decode  │──▶│ VAD/Tag  │──▶│ Description │  │
│  │ (exist) │   │ (exists) │   │ Generator   │  │
│  └─────────┘   └──────────┘   └──────┬──────┘  │
│                                       │         │
│  ┌─────────┐   ┌──────────┐   ┌──────▼──────┐  │
│  │ Quality │◀──│  Mixer   │◀──│    TTS      │  │
│  │ Verify  │   │          │   │ (ADR-121)   │  │
│  └────┬────┘   └──────────┘   └─────────────┘  │
│       │                                         │
│       ▼                                         │
│  ┌─────────┐                                    │
│  │ Learn   │ (ADR-122)                          │
│  └─────────┘                                    │
└─────────────────────────────────────────────────┘
    │
    ▼
Radio Play Audio + Quality Report
```

## Milestone Plan

### Milestone D: GOAP Core + Scene Description (2 weeks)

**Goal:** Plan and describe — no audio synthesis yet.

| Task | Duration | Output |
|------|----------|--------|
| GOAP planner with A* search | 3 days | `src/goap/planner.rs` |
| World state + action registry | 2 days | `src/goap/mod.rs`, `src/goap/actions.rs` |
| Scene description generator | 4 days | `src/goap/describe.rs` |
| Script preparation (text + emotion) | 2 days | `src/goap/scripts.rs` |
| CLI `radio-play --plan-only` | 1 day | Shows planned actions |

**Acceptance:**
- `timeline radio-play <MOVIE> --plan-only` outputs valid action plan
- Scene descriptions generated for all non-voice segments
- Each description includes emotion annotation

**Scene Description Strategy:**
- Use segment tags (music_dramatic, ambience_quiet, sfx_*) as context
- Generate narration text using configurable LLM (local: Qwen3 via GGUF, API: OpenAI/Anthropic)
- Fallback: template-based descriptions from tag + duration
- Output: `Vec<NarrationScript>` with text, emotion, timing

### Milestone E: Voice Synthesis Integration (2 weeks)

**Goal:** Generate spoken narration audio from scripts.

| Task | Duration | Output |
|------|----------|--------|
| VoiceSynthesizer trait | 1 day | `src/voice/mod.rs` |
| Kokoro-82M provider (ONNX) | 3 days | `src/voice/kokoro.rs` |
| Qwen3-TTS provider (GGML+ONNX) | 4 days | `src/voice/qwen3.rs` |
| Orpheus-3B provider (GGUF) | 3 days | `src/voice/orpheus.rs` |
| ElevenLabs API provider | 1 day | `src/voice/elevenlabs.rs` |
| Fallback chain + budget tracking | 1 day | `src/voice/chain.rs` |
| Model download CLI | 1 day | `timeline models` subcommand |

**Acceptance:**
- `timeline radio-play <MOVIE> --provider kokoro` produces narration audio
- Emotion tags render differently across providers
- Fallback triggers on provider failure
- Budget cap stops API calls when exceeded

**Device Priority:**
1. Auto-detect GPU (CUDA/Metal) → use if available
2. CPU fallback always works (slower but functional)
3. Config override: `"device": "cpu"` forces CPU even if GPU present

### Milestone F: Audio Mixing + Output (1 week)

**Goal:** Combine original audio + narration into final radio play.

| Task | Duration | Output |
|------|----------|--------|
| Audio mixer with crossfade | 2 days | `src/goap/mixer.rs` |
| Volume ducking during narration | 1 day | Duck movie audio under narration |
| Output encoding (FLAC/MP3/WAV) | 1 day | Via ffmpeg or rodio |
| Timing alignment (narration fits gaps) | 1 day | Speed adjustment if too long |
| End-to-end CLI integration | 1 day | `timeline radio-play` full flow |

**Acceptance:**
- Output audio plays without artifacts
- Narration doesn't overlap with movie dialogue
- Smooth transitions (50-200ms crossfade)
- Duration within 5% of original movie

### Milestone G: Learning Loop (1 week)

**Goal:** System improves after every run.

| Task | Duration | Output |
|------|----------|--------|
| Execution trace recording | 1 day | All actions logged to DB |
| Automated quality metrics | 2 days | SNR, timing, consistency scores |
| MAPE analyzer | 2 days | Proposes config adaptations |
| Review UI for narration rating | 1 day | Extend existing review player |
| Adaptation application + rollback | 1 day | Safe bounded learning |

**Acceptance:**
- Run 2 measurably different from Run 1 (adapted parameters)
- Quality score trend is non-decreasing over 5 runs
- Rollback triggers if quality drops >15%
- `timeline learning-stats --radio-play` shows improvement curve

### Milestone H: Hardening + Polish (ongoing)

- Streaming/chunked processing for 2+ hour movies
- Multi-language narration (DE, ES, FR from existing test corpus)
- Voice consistency across entire movie (same narrator voice)
- Parallel segment synthesis (batch TTS calls)
- Resume interrupted conversions from checkpoint
- CI integration with quality gates

## Resource Requirements

### Local-Only Setup (Free)
- Disk: ~4GB for all models (Kokoro + Qwen3 + Orpheus)
- RAM: 4GB minimum (Kokoro), 8GB recommended (Qwen3), 16GB (Orpheus)
- CPU: Any x86-64 or ARM64 (AVX2 recommended)
- GPU: Optional, significantly speeds up Orpheus/Qwen3

### API Setup (Paid)
- ElevenLabs: $5-22/month depending on usage
- OpenAI TTS: ~$15 per hour of generated audio
- No local model storage needed

### Hybrid (Recommended)
- Kokoro for draft/preview (free, fast, CPU)
- ElevenLabs for final render (best quality)
- Budget cap prevents surprise costs

## Success Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| Conversion time | <2x movie duration (GPU), <10x (CPU) | Benchmark suite |
| Narration quality MOS | >3.5/5.0 (Kokoro), >4.0 (ElevenLabs) | Automated + user rating |
| Timing accuracy | 100% narration within gaps | Overlap detection |
| Learning improvement | >5% quality gain over 5 runs | Per-movie tracking |
| Provider reliability | <5% failure rate per provider | Execution traces |
| Cost efficiency | <$1 per movie hour (API mode) | Budget tracking |

## Dependencies on Existing Pipeline

| Existing Component | Used By | Modification Needed |
|-------------------|---------|-------------------|
| `src/pipeline/` (VAD, tags) | Milestone D (scene context) | None — read-only |
| `src/learning/database.rs` | Milestone G (trace storage) | Add tables |
| `src/config.rs` | All milestones (voice config) | Extend struct |
| `src/io/` (JSON, EDL, VTT) | Export narration scripts | Minor additions |
| `src/review.rs` | Milestone G (rating UI) | Extend template |
| `config/profiles/` | Provider selection | New profile type |

## External Crate Dependencies (d-o-hub org)

| Crate | Version | Use | Impact |
|-------|---------|-----|--------|
| `do-memory-core` | 0.1.33 | Learning system (ADR-122) — episodes, patterns, reward scoring | Saves ~2 weeks vs custom impl |
| `chaotic_semantic_memory` | 0.3.6 | Scene similarity search via HDC vectors (CPU-only, no API) | Find similar past scenes for emotion reuse |

These are from the same GitHub org (`d-o-hub`) and share the libSQL backend already in use.

```toml
# Cargo.toml additions
[dependencies]
do-memory-core = { git = "https://github.com/d-o-hub/rust-self-learning-memory", features = ["csm"] }
```

## Language: German (de-DE) Primary

All narration output defaults to German. The scene description generator and TTS providers must:
- Generate description text in German
- Synthesize speech with German pronunciation and prosody
- Use German emotion descriptions for Qwen3-TTS voice control
- Support the German Orpheus fine-tune (`Orpheus-3b-German-FT-Q8_0.gguf`)
- Config override via `"language": "en"` for other locales

## Open Questions

1. **Scene description quality**: Template-based (deterministic) vs LLM-generated (creative but variable)?
   - Proposal: Template as fallback, LLM as primary with quality gate
2. **Voice consistency**: Single narrator or multiple voices for different characters?
   - Proposal: Single narrator default, multi-voice as advanced config
3. **Copyright considerations**: Narration added to copyrighted movie audio?
   - Proposal: User's responsibility; tool is accessibility-focused (audio description)
4. **Subtitle integration**: Use existing SRT for dialogue timing?
   - Proposal: Yes — existing `src/validation/srt.rs` already parses subtitles

## Experimental Status

This entire pipeline is **experimental** (2026). Expected evolution:
- TTS models improve rapidly — provider trait allows hot-swapping
- GOAP planning may be overkill if failure rates are low — can simplify to linear
- Learning system needs real-world data to validate improvement claims
- First viable end-to-end demo expected within 6 weeks of starting Milestone D
