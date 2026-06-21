# ADR-120: GOAP-Based Radio Play Generation Pipeline

**Date:** 2026-06-21
**Status:** Proposed
**Supersedes:** 110-goap-closeout (extends beyond issue resolution into full generation)

## Context

The current pipeline extracts non-voice segments from movie audio (decode → VAD → segment → tag → prompt → review) with 99.88% precision. However, converting a full movie into a complete radio play requires additional stages: scene description generation, voice synthesis with emotion, audio mixing, and quality verification. These stages have complex dependencies, variable execution times, and can fail independently.

A Goal-Oriented Action Planning (GOAP) architecture provides:
- Dynamic replanning when stages fail (TTS timeout, GPU OOM, bad audio quality)
- Cost-aware action selection (local CPU vs GPU vs paid API based on user config)
- World-state tracking enabling self-improvement across runs
- Parallelizable independent actions with dependency resolution

## Decision

Adopt a GOAP planner as the orchestration layer for the full movie-to-radio-play conversion pipeline.

### World State Model

```rust
struct WorldState {
    movie_decoded: bool,
    audio_extracted: bool,
    segments_identified: bool,
    segments_tagged: bool,
    scene_descriptions_generated: bool,
    voice_scripts_prepared: bool,
    voices_synthesized: bool,
    audio_mixed: bool,
    quality_verified: bool,
    // Resource awareness
    gpu_available: bool,
    api_keys_configured: bool,
    local_models_loaded: bool,
    // Quality metrics from previous runs
    last_quality_score: f32,
    learnings_applied: bool,
}
```

### Action Registry

| Action | Preconditions | Effects | Cost (CPU) | Cost (GPU) |
|--------|--------------|---------|------------|------------|
| `decode_movie` | movie_path exists | audio_extracted=true | 1.0 | 0.3 |
| `run_vad_pipeline` | audio_extracted | segments_identified=true | 2.0 | 0.5 |
| `tag_segments` | segments_identified | segments_tagged=true | 0.5 | 0.5 |
| `generate_descriptions` | segments_tagged | scene_descriptions_generated=true | 3.0 (LLM) | 1.0 |
| `prepare_voice_scripts` | scene_descriptions_generated | voice_scripts_prepared=true | 1.0 | 1.0 |
| `synthesize_voices` | voice_scripts_prepared | voices_synthesized=true | 10.0 (CPU TTS) | 2.0 |
| `mix_audio` | voices_synthesized, audio_extracted | audio_mixed=true | 1.5 | 0.5 |
| `verify_quality` | audio_mixed | quality_verified=true | 2.0 | 1.0 |
| `apply_learnings` | quality_verified, learning_db exists | learnings_applied=true | 0.5 | 0.5 |

### Planner Algorithm

A* search over world-state transitions:
1. Start: initial world state (movie file available)
2. Goal: `quality_verified=true` AND `audio_mixed=true`
3. Search: expand cheapest actions whose preconditions are met
4. Replan trigger: any action failure, quality score below threshold, or resource change

### Replanning Triggers

- TTS provider timeout → switch to fallback provider
- GPU OOM → fall back to CPU inference or API
- Quality verification fails → re-synthesize specific segments with adjusted emotion
- New learning data available → adjust thresholds before next segment batch

## Implementation Phases

### Phase 7.1: GOAP Core (1 week)
- `src/goap/mod.rs` — WorldState, Action trait, Planner
- `src/goap/planner.rs` — A* search with cost functions
- `src/goap/actions.rs` — Action registry wrapping existing pipeline stages

### Phase 7.2: Scene Description (1 week)
- `src/goap/describe.rs` — Generate listener descriptions for visual scenes
- Input: tagged non-voice segments + movie metadata
- Output: narrator scripts with emotion markers

### Phase 7.3: Voice Synthesis Integration (2 weeks)
- `src/goap/synthesize.rs` — TTS provider abstraction
- Provider trait with local/API implementations
- See ADR-121 for provider details

### Phase 7.4: Audio Assembly (1 week)
- `src/goap/mixer.rs` — Combine original audio + synthesized narration
- Crossfade, ducking, volume normalization
- Output: final radio play audio file (WAV/MP3/FLAC)

### Phase 7.5: Quality Loop (ongoing)
- `src/goap/verify.rs` — Automated quality checks
- Stores results in learning DB for next-run improvement
- See ADR-122 for self-improvement architecture

## CLI Integration

```bash
# Full conversion with GOAP orchestration
timeline radio-play <MOVIE> --output <DIR> --config <JSON>

# Resume interrupted conversion (GOAP replans from last checkpoint)
timeline radio-play --resume <STATE_FILE>

# Dry-run showing planned actions without execution
timeline radio-play <MOVIE> --plan-only
```

## Consequences

**Positive:**
- Resilient to partial failures — replans around broken stages
- Resource-adaptive — selects cheapest viable path (CPU/GPU/API)
- Enables incremental improvement through learning feedback loop
- Parallelizes independent actions (tag + describe can run concurrently)
- Deterministic planning given same world state (auditable)

**Negative:**
- Adds architectural complexity vs linear pipeline
- Planner overhead (~10ms per replan) — negligible vs TTS latency
- Requires careful world-state serialization for resume capability
- Testing requires mocking world-state transitions

**Risks:**
- Experimental: full GOAP for media pipelines is novel (2026)
- TTS quality may not match professional radio plays initially
- Large movies (2+ hours) need streaming/chunked processing (existing Phase 6.4 plan)
