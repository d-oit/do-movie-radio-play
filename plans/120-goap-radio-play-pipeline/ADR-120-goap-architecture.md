# ADR-120: GOAP-Based Radio Play Generation Pipeline

**Date:** 2026-06-21
**Status:** Proposed
**Supersedes:** 110-goap-closeout (extends beyond issue resolution into full generation)

## Context

The goal is to convert a movie into a **complete radio play** (Hörspiel) that a listener can enjoy without seeing any video. The key insight:

> **The original movie audio is the radio play.** We only add an AI narrator voice to describe what a listener cannot understand without the visual.

Specifically:
- **Preserve 100%** of the original audio: all dialogue, sound effects, music, ambience
- **Identify moments** where a listener would be lost without the picture (visual-only scenes, silent actions, scene transitions, title cards, visual gags)
- **Generate German narration text** describing only what is visually critical
- **Synthesize an AI narrator voice** for those descriptions
- **Insert narration** into natural pauses or expand timing slightly — never cut original content
- **Learn** which moments need narration and which are self-explanatory from audio alone

This is **automated Audiodeskription** (audio description) optimized for radio play format.

### What the existing pipeline already provides

The current extraction pipeline identifies non-voice segments (music, ambience, silence, SFX). But not all non-voice segments need narration — a music interlude is fine without description. The new challenge is:

**Which non-voice (or low-information) segments require visual description for listener comprehension?**

This requires a higher-level analysis:
1. Does the scene make sense from audio alone? (dialogue explains everything → no narration needed)
2. Is something visually important happening with no audio cue? (→ needs narration)
3. Is there a scene change that's only apparent visually? (→ needs brief orientation)

## Decision

Adopt a GOAP planner as the orchestration layer for the radio play generation pipeline.

### World State Model

```rust
struct WorldState {
    movie_decoded: bool,
    audio_timeline_extracted: bool,       // existing pipeline
    visual_gaps_identified: bool,         // NEW: moments needing narration
    narration_scripts_generated: bool,    // German text for narrator
    narrator_voice_synthesized: bool,     // AI voice audio files
    radio_play_assembled: bool,           // final mix: original + narrator
    quality_verified: bool,
    // Resource awareness
    gpu_available: bool,
    api_keys_configured: bool,
    local_models_loaded: bool,
    // Learning
    last_quality_score: f32,
    learnings_applied: bool,
}
```

### Action Registry

| Action | Preconditions | Effects | Cost |
|--------|--------------|---------|------|
| `decode_movie` | movie_path exists | movie_decoded=true | 1.0 |
| `extract_timeline` | movie_decoded | audio_timeline_extracted=true | 2.0 |
| `identify_visual_gaps` | audio_timeline_extracted | visual_gaps_identified=true | 3.0 |
| `generate_narration` | visual_gaps_identified | narration_scripts_generated=true | 2.0 |
| `synthesize_narrator` | narration_scripts_generated | narrator_voice_synthesized=true | 5.0 |
| `assemble_radio_play` | narrator_voice_synthesized, movie_decoded | radio_play_assembled=true | 1.5 |
| `verify_quality` | radio_play_assembled | quality_verified=true | 2.0 |
| `apply_learnings` | quality_verified | learnings_applied=true | 0.5 |

### Key Stage: `identify_visual_gaps`

This is the novel algorithm — determining WHERE narration is needed:

**Input signals:**
- Non-voice segments from existing VAD pipeline (candidates)
- Segment tags (music, ambience, silence, SFX)
- Duration of silence/ambience (long silence = likely visual-only scene)
- Surrounding context (what audio came before/after)
- Spectral features (is there ANY audio information for the listener?)

**Decision heuristics (initial, then learned):**
- Pure silence > 3s in a dialogue-heavy scene → likely needs description
- Scene boundary (detected via audio fingerprint change) with no dialogue → needs orientation
- Long ambience-only section between dialogue blocks → may need "what's happening" narration
- Music-over-visual montage → may need brief description of what's shown
- Sound effects that are ambiguous without visual context → needs clarification

**Learning signal:**
- User corrections: "this segment didn't need narration" / "this moment was missing narration"
- After each run, the decision model improves its gap identification

### Planner Algorithm

A* search over world-state transitions:
1. Start: movie file available
2. Goal: `radio_play_assembled=true` AND `quality_verified=true`
3. Replan on: action failure, quality below threshold, resource change

### Output Format

```
┌─────────────────────────────────────────────────────────────┐
│ Final Radio Play Audio                                       │
├──────────┬─────────────┬──────────────┬────────────┬────────┤
│ Original │ AI Narrator │  Original    │ AI Narrator│Original│
│ Dialogue │ "Er betritt │  SFX+Music   │ "Sie sieht │Dialogue│
│ + SFX    │  den Raum"  │  (unchanged) │  ihn an"   │+ Music │
└──────────┴─────────────┴──────────────┴────────────┴────────┘
```

The narrator voice is **inserted** at identified visual gaps, never replacing original content.

## Implementation Phases

### Phase 7.1: GOAP Core (1 week)
- `src/goap/mod.rs` — WorldState, Action trait, Planner
- `src/goap/planner.rs` — A* search with cost functions
- `src/goap/actions.rs` — Action registry wrapping existing pipeline stages

### Phase 7.2: Visual Gap Identification (2 weeks)
- `src/goap/gaps.rs` — Algorithm to find moments needing narration
- Uses existing segment tags + duration + context analysis
- Outputs: `Vec<NarrationGap>` with timestamp, duration, context, priority
- Initial heuristics → refined via learning after each run

### Phase 7.3: Narration Generation (1 week)
- `src/goap/narrate.rs` — Generate German narration text for each gap
- Input: gap context (what audio is around it, segment tags, duration)
- Output: brief German description text + emotion annotation
- Constraint: narration must fit within available time slot

### Phase 7.4: Voice Synthesis (2 weeks)
- `src/voice/` — TTS provider abstraction (see ADR-121)
- Synthesize German narrator voice with appropriate emotion
- Single consistent narrator voice across entire movie

### Phase 7.5: Assembly (1 week)
- `src/goap/assemble.rs` — Insert narrator audio into original timeline
- Strategies: insert in natural pauses, or time-stretch gaps slightly
- Never cut/remove original audio content
- Crossfade narrator in/out (50-100ms)
- Volume ducking of background during narration

### Phase 7.6: Quality + Learning (ongoing)
- Automated self-evaluation: narrator doesn't overlap, timing fits, emotion consistent
- Self-learning: system adjusts gap detection thresholds autonomously after each run
- Pattern accumulation: similar scenes across movies share learned decisions
- Optional human feedback: high-weight correction signal, but system never waits for it

## CLI Integration

```bash
# Full radio play generation
timeline radio-play <MOVIE> --output <FILE> --language de

# Show what would be narrated (gap analysis only)
timeline radio-play <MOVIE> --analyze-only

# Resume interrupted generation
timeline radio-play --resume <STATE_FILE>
```

## Consequences

**Positive:**
- Preserves 100% of original movie audio — nothing is lost
- Only adds narration where truly needed for listener comprehension
- Learns to identify visual gaps better with each run
- Works with any movie without prior metadata or subtitle files
- Deterministic gap identification given same world state

**Negative:**
- Gap identification is the hardest problem — initial heuristics will be imperfect
- Without video analysis, purely audio-based gap detection has inherent limitations
- May over-narrate (annoying) or under-narrate (confusing) until learned
- Timing constraints: narration must fit in available audio gaps

**Risks:**
- Audio-only gap detection may miss important visual moments
- Future: video frame analysis (VLM) could dramatically improve gap identification
- German TTS quality for emotional narration is still evolving (2026)
