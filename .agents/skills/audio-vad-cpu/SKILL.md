---
name: audio-vad-cpu
description: Voice Activity Detection on CPU using energy and spectral engines
---

## When to use
Use when implementing or debugging the VAD stage of the audio pipeline. Applies to energy threshold calibration, spectral threshold tuning, frame classification, and noise floor estimation.

## Domain model

### Core concepts
- **Frame**: 20ms window of audio (320 samples at 16kHz) with computed RMS energy
- **Threshold**: RMS value that separates speech from non-voice (default: 0.015)
- **Classification**: Simple `rms >= threshold` boolean per frame
- **Hangover**: Post-speech frames appended to avoid clipping consonant endings
- **Spectral engine**: entropy/flatness/centroid-informed classification for music/noise discrimination

### Constants (from config.rs)
| Parameter | Default | Purpose |
|-----------|---------|---------|
| `sample_rate_hz` | 16000 | Standard audio processing rate |
| `frame_ms` | 20 | Frame duration in milliseconds |
| `energy_threshold` | 0.015 | RMS threshold for speech detection |
| `speech_hangover_ms` | 200-300 | Typical hangover buffer |
| `min_speech_ms` | 120 | Minimum speech segment duration |

## Implementation workflow

### Step 1: Energy threshold calibration
```
1. Compute RMS per frame: sqrt(mean(sample^2))
2. Threshold comparison: frame.rms >= config.energy_threshold
3. Threshold selection criteria:
   - Too high: Misses quiet speech (whispers, breathy voices)
   - Too low: Captures breath, mic noise, room hiss
   - Optimal: ~3-5x above noise floor
```

### Step 2: Frame size considerations
- 20ms is optimal for speech (contains ~1-2 phonemes)
- Shorter frames (10ms): Better temporal resolution, more noise sensitivity
- Longer frames (50ms): Smoother, may miss short consonants
- Must be integer samples: `(sample_rate * frame_ms) / 1000`

### Step 3: Noise floor estimation
```rust
// Collect RMS values from known non-voice regions
let noise_frames: Vec<f32> = segments
    .iter()
    .filter(|s| s.kind == SegmentKind::NonVoice)
    .flat_map(|s| frames_in_range(s.start_ms, s.end_ms))
    .map(|f| f.rms)
    .collect();

// Set threshold 4x above median noise floor
let noise_floor = median(&noise_frames);
let threshold = noise_floor * 4.0;
```

### Step 4: Real-time vs batch processing
- **Batch (default)**: Process entire file, then segment → accurate boundaries
- **Real-time**: Process with latency buffer → requires lookahead for end detection
- This codebase uses batch mode for deterministic results

## Common failure modes

| Failure | Symptom | Fix |
|---------|---------|-----|
| Breathing detected as speech | Thin high-frequency audio flagged as speech | Increase threshold by +0.002 |
| Mic self-noise | Constant low-level noise creates false positives | Lower threshold, check hardware |
| Clippy/peaky audio | RMS spikes from clipping create false speech | Pre-normalize audio, reduce threshold |
| Whispered dialogue | Low RMS speech below threshold | Reduce threshold by -0.003 |
| Electric hum (60Hz) | Periodic false detections | Apply bandpass filter 300-3400Hz |

## Calibration procedure

### Initial calibration
1. Run pipeline on diverse test set (action, documentary, drama, animation)
2. Inspect `analysis/learnings/latest-calibration.json`
3. Adjust `energy_threshold` in config based on:
   - `speech_to_non_voice` count → threshold too high
   - `non_voice_to_speech` count → threshold too low

### Per-profile adjustment (from profiles.rs)
| Profile | Delta | Rationale |
|---------|-------|-----------|
| action | +0.010 | High background noise, explosions, music |
| documentary | -0.003 | Clean narration, controlled environment |
| animation | +0.000 | Mixed: clean dialogue + sound effects |
| drama | -0.001 | Intimate whispering, quiet moments |

## Success metrics

### Quality gate checks
- `precision`: Of speech detected, % actually speech (target: >0.90)
- `recall`: Of actual speech, % detected (target: >0.85)
- Boundary accuracy: Within 40ms of manual annotation
- No segments shorter than `min_speech_ms` (120ms default)

### Testing approach
- Use synthetic audio with known speech/non-voice regions
- Validate against `tests/validation/` dataset
- Benchmark: `bash scripts/benchmark.sh testdata/generated/alternating.wav`

## Guardrails
- CPU-only implementation (no GPU dependencies)
- Deterministic outputs (seeded random if used)
- Keep `classify_frames()` pure: same input → same output
- No online learning that changes threshold during processing

## Current tuning guidance

- Run cohort-aware sweep: `python3 scripts/optimize_fp_sweep.py`
- Enforce coverage guard (`--min-coverage-ratio`) to avoid low-coverage candidates.
- Generate deployable profiles from policy: `python3 scripts/generate_optimized_profiles.py`
