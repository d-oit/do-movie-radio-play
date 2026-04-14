---
name: triz-audio-timeline
description: TRIZ inventive principles applied to audio timeline segmentation problems
---

## When to use
Use when facing fundamental tradeoffs in audio segmentation: speed vs accuracy, sensitivity vs specificity, granularity vs simplicity. TRIZ provides structured problem-solving for these contradictions.

## TRIZ core concepts

### Key principles for audio segmentation
1. **Segmentation**: Divide system into independent parts
2. **Extraction**: Remove unwanted portions (noise, silence)
3. **Local quality**: Different properties in different parts
4. **Asymmetry**: Use asymmetric solutions for asymmetric problems
5. **Prior action**: Prepare favorable conditions in advance

## Contradictions in audio VAD

### Technical contradiction: Speed vs Accuracy
| Parameter | Improving Speed | Hurting Accuracy |
|-----------|-----------------|------------------|
| Larger frame size (50ms) | Faster processing | Misses short speech |
| Shorter window | Better granularity | More noise sensitivity |

**TRIZ solution**: Segmentation principle
- Divide audio into chunks, process in parallel
- Apply coarse detection first, refine on detected speech regions only

### Physical contradiction: Sensitivity vs Specificity
| Requirement | Opposite Requirement |
|-------------|---------------------|
| Detect quiet speech (whispers) | Reject ambient noise |
| Capture short utterances | Ignore mic artifacts |

**TRIZ solution**: Separation principles
- **Separation in space**: Use frequency bandpass (300-3400Hz) to isolate speech
- **Separation in time**: Apply hysteresis (hangover) to bridge short gaps
- **Separation in condition**: Different thresholds for different profiles

## Ideal Final Result (IFR) for non-voice detection

### IFR statement
> "The non-voice detector should identify all non-speech regions without any false positives or false negatives, without consuming resources for speech regions, and without requiring manual tuning."

### Practical IFR decomposition
1. **Primary function**: Detect non-voice with >95% accuracy
2. **Ideal behavior**: No manual threshold adjustment needed
3. **Resource constraint**: CPU-only, <1x realtime
4. **Boundary**: Works across all genres without retraining

### IFR gap analysis
| Gap | Current State | IFR |
|-----|---------------|-----|
| Accuracy | ~85-90% | 95%+ |
| Tuning | Manual per-profile | Self-calibrating |
| Genres | 4 profiles | Universal |

## Resource tradeoff analysis

### Available resources
- **Time**: Processing speed, latency
- **Substance**: Memory usage, storage for corrections
- **Information**: Correction feedback data
- **Precision**: Temporal resolution of boundaries

### Resource-cost diagram
```
                    High Accuracy
                         ↑
                         │     ┌─────────────────┐
                         │     │  Ideal region   │
    Cost                 │     │  (unreachable)  │
                         │     └─────────────────┘
                         │
                         └─────────────────────────→ Low Cost
```

### Optimization strategy
1. **Accept**: ~85% accuracy is sufficient for most applications
2. **Prioritize**: Precision (avoid false non-voice) over recall for drama
3. **Leverage**: Self-learning calibration to improve over time

## Separation principles in timeline construction

### Time-based separation
```
Speech segments: [====]     [========]    [===]
                    ↑ gap        ↑ gap
                  merge if     merge if
                 gap < 200ms   gap < 200ms

Non-voice segments: [──────]  [────────]  [──]
                   gap ≥ 1s   gap ≥ 1s    skip
```

### Condition-based separation (by profile)
```
action:   threshold +0.010 → lower sensitivity, higher noise immunity
drama:    threshold -0.001 → higher sensitivity, captures whispers
```

### Hierarchical separation
1. **Coarse pass**: 50ms frames, rough boundaries
2. **Fine pass**: 20ms frames, refine boundaries
3. **Smooth pass**: Apply hangover, remove flicker
4. **Merge pass**: Close gaps, enforce min durations

## Problem-solving patterns

### Pattern 1: Detecting speech in noisy backgrounds
**Problem**: Explosions mask dialogue in action scenes
**Contradiction**: Need low threshold (sensitivity) vs high threshold (noise rejection)
**TRIZ solution**: Dynamorphism + feedback
```rust
// Adaptive threshold based on noise floor
let noise_floor = median(background_frames.rms);
let adaptive_threshold = noise_floor * 4.0 + profile_delta;
```

### Pattern 2: Handling variable speaking rates
**Problem**: Fast dialogue creates short segments; slow dialogue creates long pauses
**Contradiction**: Small min_speech_ms (capture quick exchanges) vs large (reject noise)
**TRIZ solution**: Segmentation + local quality
```rust
// Variable thresholds by region type
let threshold = match detect_region_type(segment) {
    RegionType::Dialogue => base_threshold - 0.002,
    RegionType::Narration => base_threshold,
    RegionType::Action => base_threshold + 0.005,
};
```

### Pattern 3: False non-voice at boundaries
**Problem**: Leading/trailing silence incorrectly included as non-voice
**Contradiction**: Want complete coverage vs want only meaningful segments
**TRIZ solution**: Prior action + extraction
```rust
// Trim leading/trailing segments shorter than threshold
const MIN_EDGE_SEGMENT_MS: u64 = 500;  // Discard edge gaps < 500ms
```

## Application checklist

When solving VAD problems with TRIZ:
- [ ] Identify the technical contradiction (what improves, what degrades)
- [ ] Identify the physical contradiction (two opposite requirements)
- [ ] Apply separation principles if contradiction is irreconcilable
- [ ] Check IFR: what would the ideal system do?
- [ ] Use available resources efficiently (avoid over-engineering)
- [ ] Consider dynamic solutions (adaptive threshold, multi-pass)

## Related skills
- **audio-vad-cpu**: Energy-based VAD implementation
- **nonvoice-segmentation**: Segment post-processing
- **self-learning-calibration**: Feedback-driven improvement
