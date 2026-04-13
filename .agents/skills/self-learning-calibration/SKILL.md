---
name: self-learning-calibration
description: Profile-based threshold adjustment using correction feedback from manual reviews
---

## When to use
Use when improving VAD accuracy across different content genres. Applies to tasks involving threshold tuning, correction analysis, and profile management.

## Domain model

### Profile system (from profiles.rs)
Profiles encode genre-specific threshold adjustments:

| Profile | Threshold Delta | Use Case |
|---------|-----------------|----------|
| action | +0.010 | Explosions, gunfire, music drowning speech |
| documentary | -0.003 | Clean narration, controlled studio |
| animation | +0.000 | Mixed: clean dialogue + sound effects |
| drama | -0.001 | Intimate scenes, whispered dialogue |

### Calibration data flow
```
Manual correction → CorrectionRecord → corrections/*.json
                                         ↓
                              run_calibration() analyzes
                                         ↓
                              analysis/learnings/latest-calibration.json
```

## Implementation workflow

### Step 1: Correction record collection
```rust
struct CorrectionRecord {
    original_kind: String,  // "speech" or "non_voice"
    corrected_kind: String,
    segment_start_ms: u64,
    segment_end_ms: u64,
    media_path: String,
    // ... metadata
}
```

Correction types:
- `speech → non_voice`: False positive (threshold too low)
- `non_voice → speech`: False negative (threshold too high)

### Step 2: Drift calculation
```rust
// Net drift based on correction imbalance
let drift = (non_voice_to_speech as f32 - speech_to_non_voice as f32) * 0.0005;
let recommended_delta = base.energy_threshold_delta + drift;
```

Interpretation:
- More `speech → non_voice`: Threshold too low → increase delta
- More `non_voice → speech`: Threshold too high → decrease delta
- Multiplier (0.0005): Conservative adjustment per correction

### Step 3: Precision/recall tradeoff per genre

| Genre | Priority | Strategy |
|-------|----------|----------|
| action | Recall | Lower threshold to catch speech in noise |
| documentary | Precision | Higher threshold for clean narration |
| animation | Balanced | Medium threshold |
| drama | Precision | Lower threshold for whispers |

### Step 4: A/B testing approach
```rust
// Compare two thresholds on same media
let baseline = pipeline::run(&config, threshold: 0.015);
let candidate = pipeline::run(&config, threshold: 0.017);

let delta = compare_segments(&baseline.segments, &candidate.segments);
// If delta < 5% change, thresholds are functionally equivalent
```

## Calibration procedure

### Running calibration
```bash
cargo run --bin calibrate -- corrections/ documentary
```

Output: `analysis/learnings/latest-calibration.json`
```json
{
  "version": 1,
  "profile": "documentary",
  "records_seen": 47,
  "speech_to_non_voice": 12,
  "non_voice_to_speech": 3,
  "recommended_energy_threshold_delta": -0.005
}
```

### Interpretation guide
| Correction Pattern | Diagnosis | Adjustment |
|--------------------|-----------|------------|
| Mostly `speech → non_voice` | Threshold too low | +0.002 to +0.005 |
| Mostly `non_voice → speech` | Threshold too high | -0.002 to -0.005 |
| Mixed, balanced | Good threshold | Minor tweak ±0.001 |
| <10 corrections | Insufficient data | Gather more corrections |

### Manual threshold override
```bash
cargo run -- analyze media.mp4 --threshold 0.017
# Overrides profile delta for single run
```

## Data collection guidelines

### Minimum sample size
- At least 30 corrections per profile before trusting calibration
- Corrections should cover diverse content within genre
- Avoid corrections from single media file dominating dataset

### Correction quality
- Mark as `non_voice`: breathing, mic noise, ambient sound, music
- Mark as `speech`: any audible human voice, including whispers
- Ignore segments with unclear audio quality

### File naming convention
```
corrections/
  movie1_action_scene1.json
  documentary_episode2.json
  drama_s01e03_interview.json
```

## Success metrics

### Calibration quality checks
- `records_seen >= 30`: Sufficient sample size
- Correction ratio balanced: neither type > 80% of total
- Recommended delta within ±0.020 of base (catch extreme drift)

### Validation approach
1. Run calibrated config on held-out test set
2. Compare against manual annotations
3. Target: precision > 0.90, recall > 0.85

## Guardrails
- CPU-only processing
- No automated threshold changes without human review
- Profile deltas are additive to base threshold (never replace)
- Calibration report is read-only artifact (does not auto-apply)
