# Implementation Agent Handoff - Spectral VAD

## Changes Made

### 1. Features (src/pipeline/features.rs)
- Added `spectral_flatness` to FeatureSet struct
- Implemented spectral flatness calculation (geometric mean / arithmetic mean)
- Higher flatness = more tonal (music), lower = more noise-like (speech)
- Updated tests

### 2. Frame (src/types/frame.rs)
- Added `spectral_flatness` field
- Updated `speech_likelihood` to use flatness as bonus/penalty

### 3. Framing (src/pipeline/framing.rs)
- Added spectral_flatness calculation per frame
- Uses FFT magnitudes to compute flatness

### 4. Spectral VAD Engine (src/pipeline/vad/spectral.rs) - NEW FILE
- New VadEngine implementation using multi-feature classification
- Weights:
  - Energy: 0.25
  - Spectral centroid: 0.18 (speech frequency range bonus/penalty)
  - ZCR: 0.12 (+voice detection range, -high ZCR noise)
  - Spectral flux: 0.04 (transient detection)
  - Spectral flatness: 0.20 (tonal penalty for music)
  - Band ratios: music/hiss penalties
  - Voice indicator bonus: 0.10

### 5. Config (src/config.rs)
- Added "spectral" to VALID_VAD_ENGINES

### 6. VAD Module (src/pipeline/vad/mod.rs)
- Exports new SpectralVad engine

### 7. Tags (src/pipeline/tags.rs)
- Added "tonal" tag for high spectral_flatness + adequate RMS

## Testing Plan
1. Run quality gate (fmt + clippy + tests) - DONE ✓
2. Compare VAD output on testdata with energy vs spectral
3. Run benchmark regression check

## Notes
- Energy VAD still uses existing Frame::speech_likelihood (enhanced with flatness)
- Spectral VAD uses completely separate classify_spectral function
- Both engines share spectral features computed during framing
- spectral_flatness is computed once per frame, used by both engines
