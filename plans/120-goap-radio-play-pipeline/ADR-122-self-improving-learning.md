# ADR-122: Self-Improving Radio Play Generation — Learning Architecture

**Date:** 2026-06-21
**Status:** Proposed
**Depends on:** ADR-120 (GOAP pipeline), ADR-121 (voice synthesis)

## Context

The system should improve after every run. Current pipeline already has a learning database (libsql) storing verified segments with spectral features. The radio play generation pipeline introduces new learnable dimensions:
- Which emotion mappings produce better listener ratings
- Which TTS provider/settings work best for specific scene types
- Optimal segment boundaries for narration insertion points
- Voice pacing and timing preferences per movie genre

### Implementation Base: `d-o-hub/rust-self-learning-memory`

Instead of building the learning system from scratch, we integrate the **`do-memory-core`** crate from `d-o-hub/rust-self-learning-memory` (v0.1.33, MIT, same org). This crate provides:

- **Episode lifecycle** (start → execute → score → learn → retrieve) — maps to GOAP execution traces
- **Pattern recognition** (ToolSequence, DecisionPoint, ErrorRecovery, ContextPattern) — learns which provider+emotion combos work
- **Reward scoring with reflection** — our quality scoring + adaptation logic
- **libSQL storage** — same DB backend already in use
- **CSM cascading retrieval** (via `chaotic_semantic_memory` crate, `d-o-hub`, v0.3.6) — find similar past scenes without external APIs, CPU-only HDC vectors
- **Episode checkpoints** — enables `--resume` capability

This saves ~2 weeks vs custom implementation and keeps learning logic in a reusable, tested crate.

### 2026 Self-Improvement Patterns (Researched)

1. **Closed-Loop Self-Improvement** (tianpan.co, 2026): generate → attempt → verify → train cycle without human in loop
2. **Self-Harness** (arXiv, June 2026): agent modifies its own operating harness based on execution trace mining; 14-21% gains
3. **MAPE Control Loops** (arXiv, 2025): Monitor → Analyze → Plan → Execute for continuous agent improvement
4. **Reflection-Based Feedback** (stackviv.ai, 2026): agents critique own output, identify errors, refine without retraining

## Decision

Implement a three-layer learning system that improves radio play quality after every run:

### Layer 1: Execution Trace Recording (Automatic)

Every GOAP execution records:
```rust
struct ExecutionTrace {
    run_id: String,
    movie_hash: String,
    timestamp: DateTime<Utc>,
    actions_executed: Vec<ActionRecord>,
    world_state_transitions: Vec<(WorldState, WorldState)>,
    replanning_events: Vec<ReplanEvent>,
    total_duration_ms: u64,
    quality_scores: QualityScores,
}

struct ActionRecord {
    action_name: String,
    input_hash: String,
    output_hash: String,
    duration_ms: u64,
    provider_used: String,
    cost_usd: f64,
    success: bool,
    error: Option<String>,
    // For TTS actions
    emotion_used: Option<String>,
    segment_tags: Vec<String>,
}

struct QualityScores {
    // Automated metrics
    audio_snr_db: f32,
    speech_naturalness_mos: Option<f32>, // if quality model available
    timing_alignment_score: f32,
    emotion_consistency: f32,
    // User feedback (optional, from review UI)
    user_rating: Option<u8>,  // 1-5
    user_corrections: Vec<Correction>,
}
```

Stored in the existing libsql learning database with new tables.

### Layer 2: Analyze & Adapt (Per-Run)

After each run completes, a MAPE-style analysis runs:

```
Monitor: collect execution trace + quality scores
Analyze: compare against historical baselines
Plan:    propose parameter adjustments
Execute: write updated config for next run
```

**Learnable Parameters:**
| Parameter | Learning Signal | Adjustment |
|-----------|----------------|------------|
| `emotion_mapping` | user corrections, quality scores | Update tag→emotion table weights |
| `segment_boundaries` | timing alignment score | Adjust `min_non_voice_ms` per genre |
| `provider_selection` | cost + quality tradeoff | Update GOAP action costs |
| `voice_speed` | user feedback | ±0.1 speed per scene type |
| `narration_density` | "too much/too little" feedback | Adjust description verbosity |
| `crossfade_duration` | audio quality metrics | Tune mixing parameters |

### Layer 3: Cross-Run Knowledge Accumulation

```sql
-- New tables in learning.db
CREATE TABLE run_traces (
    id TEXT PRIMARY KEY,
    movie_hash TEXT NOT NULL,
    created_at TEXT NOT NULL,
    quality_score REAL,
    total_cost_usd REAL,
    duration_ms INTEGER
);

CREATE TABLE emotion_outcomes (
    id INTEGER PRIMARY KEY,
    segment_tag TEXT NOT NULL,
    emotion_used TEXT NOT NULL,
    provider TEXT NOT NULL,
    quality_score REAL,
    user_approved BOOLEAN DEFAULT NULL,
    run_id TEXT REFERENCES run_traces(id)
);

CREATE TABLE provider_performance (
    id INTEGER PRIMARY KEY,
    provider TEXT NOT NULL,
    scene_type TEXT,
    avg_quality REAL,
    avg_latency_ms INTEGER,
    failure_rate REAL,
    cost_per_char REAL,
    last_updated TEXT
);

CREATE TABLE adaptation_log (
    id INTEGER PRIMARY KEY,
    parameter TEXT NOT NULL,
    old_value TEXT,
    new_value TEXT,
    reason TEXT,
    improvement_delta REAL,
    applied_at TEXT
);
```

### Improvement Cycle

```
Run N:
  1. Load learnings from DB (emotion mappings, provider scores, timing prefs)
  2. GOAP plans with learned costs/preferences
  3. Execute pipeline
  4. Record trace
  5. Auto-evaluate quality
  6. Propose adaptations (bounded: max 10% change per parameter per run)
  7. Store adaptations for Run N+1

Run N+1:
  1. Load learnings (now includes Run N adaptations)
  ... (cycle continues)
```

### Safety Bounds

Prevent runaway self-modification:
- **Max adaptation rate**: ±10% per parameter per run
- **Rollback trigger**: if quality drops >15% vs 3-run rolling average, revert last adaptation
- **Human override**: `timeline radio-play --no-learn` disables adaptation
- **Adaptation log**: every change is recorded with reason (auditable)
- **Baseline preservation**: initial config always available via `--reset-learnings`

### Quality Verification (Automated)

Without human rating, automated quality signals:
1. **Audio SNR**: synthesized speech signal-to-noise ratio
2. **Timing alignment**: narration fits within non-voice gaps (no overlap with movie speech)
3. **Emotion consistency**: emotion doesn't change abruptly within a single scene
4. **Duration ratio**: narration doesn't exceed available gap duration
5. **Silence gaps**: no awkward silences between narration and movie audio

Optional (if quality model available):
6. **MOS estimation**: neural MOS predictor (e.g., UTMOS) rates naturalness

## Implementation

### Dependencies

```toml
[dependencies]
do-memory-core = { git = "https://github.com/d-o-hub/rust-self-learning-memory", features = ["csm"] }
chaotic_semantic_memory = "0.3"  # transitive via do-memory-core csm feature
```

### Phase 7.5.1: Trace Recording (2 days)
- Wrap GOAP actions as `do-memory-core` episodes with execution steps
- Map `ActionRecord` to episode step with tool usage tracking
- Store via existing libsql backend (shared with current learning DB)

### Phase 7.5.2: Quality Metrics (3 days)
- Implement automated quality scoring (SNR, timing, consistency)
- Store per-segment and per-run scores
- Baseline establishment from first 3 runs

### Phase 7.5.3: MAPE Analyzer (3 days)
- Cross-run comparison logic
- Adaptation proposal generation
- Bounded parameter adjustment
- Rollback detection

### Phase 7.5.4: Feedback Integration (2 days)
- Review UI extension: rate narration quality per segment
- User correction import (mark segments as "wrong emotion", "too fast", etc.)
- Corrections feed into Layer 2 analysis

### Phase 7.5.5: Provider Performance Tracking (1 day)
- Track latency, failure rate, cost per provider
- Feed into GOAP cost functions for smarter provider selection
- Auto-deprioritize providers with high failure rates

## CLI Commands

```bash
# View learning stats
timeline learning-stats --radio-play

# Show adaptation history
timeline learning-log --last 10

# Reset learnings (keep execution history, reset adaptations)
timeline reset-learnings --confirm

# Export learnings for sharing/backup
timeline export-learnings --output learnings.json

# Run without learning (frozen config)
timeline radio-play <MOVIE> --no-learn
```

## Consequences

**Positive:**
- Quality improves automatically with each run
- No manual tuning required after initial setup
- Learns user preferences (voice speed, narration style, emotion)
- Provider selection optimizes cost/quality over time
- Fully auditable — every adaptation is logged with reason

**Negative:**
- Cold start: first 3 runs operate without historical data
- Adaptation may overfit to specific movie genres (mitigated by genre-aware storage)
- Additional DB writes add ~5ms per segment (negligible vs TTS)
- Requires careful bounded adaptation to prevent quality oscillation

**Risks:**
- Automated quality metrics may not correlate with human preference
- Without user feedback, learning plateau is reached quickly (mitigation: review UI)
- Long-term DB growth needs periodic compaction (archive runs older than 90 days)
