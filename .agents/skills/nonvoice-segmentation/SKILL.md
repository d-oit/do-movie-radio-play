---
name: nonvoice-segmentation
description: Converting frame-level VAD output to non-voice segments with duration constraints
---

## When to use
Use when implementing or debugging the segmentation stage after VAD classification. Applies to tasks involving segment smoothing, merging, inversion, and duration enforcement.

## Domain model

### Pipeline stages (in order)
1. **classify_frames()**: `[Frame] → [bool]` (RMS threshold)
2. **smooth_speech()**: `[bool] → [bool]` (hangover + flicker removal)
3. **speech_segments()**: `[bool] → [Segment]` (duration filtering)
4. **merge_close_segments()**: `[Segment] → [Segment]` (gap closing)
5. **invert_to_non_voice()**: `[Segment] × total_ms → [Segment]` (complement)

### Core types
```rust
Segment {
    start_ms: u64,
    end_ms: u64,
    kind: SegmentKind,  // Speech | NonVoice
    confidence: f32,
    tags: Vec<String>,
    prompt: Option<String>,
}
```

## Implementation workflow

### Step 1: Hangover smoothing
Purpose: Prevent clipping of speech endings (stop consonants, breath bursts)

```rust
// Apply N-frame hangover after detected speech
pub fn smooth_speech(raw: &[bool], frame_ms: u32, hangover_ms: u32) -> Vec<bool> {
    let hang = (hangover_ms / frame_ms) as usize;
    // ... appends true to frames within hangover window
}
```

- **Hangover calculation**: `hangover_ms / frame_ms` (e.g., 200ms / 20ms = 10 frames)
- **Default hangover**: 200-300ms (adjustable in config)

### Step 2: Flicker removal
Purpose: Eliminate single-frame false positives/negatives

```rust
// Remove isolated speech frames surrounded by non-voice
for i in 1..out.len()-1 {
    if out[i] && !out[i-1] && !out[i+1] {
        out[i] = false;  // Isolated frame → non-voice
    }
}
```

- Only removes frames that are surrounded by non-voice
- Does not affect runs of 2+ consecutive frames

### Step 3: Segment extraction with min duration
Purpose: Filter out brief noise bursts that aren't actual speech

```rust
// Only emit segments >= min_speech_ms
pub fn speech_segments(smoothed: &[bool], frame_ms: u32, min_speech_ms: u32) -> Vec<Segment> {
    let min_frames = (min_speech_ms / frame_ms) as usize;
    // ... only include runs longer than min_frames
}
```

| Parameter | Default | Purpose |
|-----------|---------|---------|
| `min_speech_ms` | 120ms | Minimum speech segment duration |
| `min_non_voice_ms` | profile-driven (default 10000ms, radio-play 500ms) | Minimum non-voice segment duration |

### Step 4: Gap merging
Purpose: Reconnect speech separated by brief pauses (pauses, breaths)

```rust
// Merge segments within merge_gap_ms of each other
pub fn merge_close_segments(segments: &[Segment], merge_gap_ms: u32) -> Vec<Segment> {
    // Combines overlapping or adjacent segments
}
```

- Same-kind segments only (Speech + Speech, NonVoice + NonVoice)
- Gap threshold typically 100-200ms (captures normal inter-word pauses)
- Updates confidence to `min(confidence_a, confidence_b)`

### Step 5: Inversion to non-voice
Purpose: Generate non-voice segments as complement of speech

```rust
pub fn invert_to_non_voice(speech: &[Segment], total_ms: u64, min_non_voice_ms: u32) -> Vec<Segment> {
    // cursor walks through timeline, emits NonVoice gaps
    // Skips gaps < min_non_voice_ms (treats as silence, not a segment)
}
```

Key behavior:
- All gaps between speech segments become non-voice
- Gaps shorter than `min_non_voice_ms` are discarded (value is profile/config driven)
- Edge cases: Leading non-voice (before first speech), trailing non-voice (after last speech)

## Common failure modes

| Failure | Symptom | Fix |
|---------|---------|-----|
| Short breath sounds split | Multiple 50ms segments instead of one | Decrease `min_speech_ms`, increase `hangover_ms` |
| Long pauses included | 3s gaps included as non-voice | Increase `min_non_voice_ms` |
| Music/ambient bleeding | Non-speech audio detected as speech | Increase threshold in VAD stage |
| Clipped word endings | Speech segment ends 100ms early | Increase `hangover_ms` |
| Over-merged segments | Two separate scenes merged | Decrease `merge_gap_ms` |

## Edge handling

### File boundaries
- Leading segment: If audio starts with non-voice, emit if `duration >= min_non_voice_ms`
- Trailing segment: If audio ends with non-voice, emit if `duration >= min_non_voice_ms`
- Empty audio: Return empty segment list

### Boundary conditions in implementation
```rust
// From end of speech_segments():
if let Some(s) = start {
    let end = smoothed.len() as u64 * frame_ms as u64;
    if end - s >= min_speech_ms as u64 {
        segs.push(speech_seg(s, end));
    }
}
```

## Success metrics

### Quality gate checks
- All non-voice segments >= `min_non_voice_ms` for selected profile
- No overlapping segments in output
- Segment list is sorted by `start_ms`
- Total coverage equals `total_ms` (no gaps)

### Testing approach
```rust
// Merge test: 200ms + 300ms speech with 120ms gap should merge at 120ms threshold
let speech = vec![speech_seg(0, 200), speech_seg(300, 500)];
let merged = merge_close_segments(&speech, 120);
assert_eq!(merged.len(), 1);  // Merged into single segment
```

## Guardrails
- Deterministic output (same input → same segments)
- No frame-level changes after smoothing phase
- Segments must be non-overlapping and sorted
- Non-voice segments must respect `min_non_voice_ms`
