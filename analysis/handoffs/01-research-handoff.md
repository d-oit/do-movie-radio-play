# Research Agent Handoff - Spectral VAD Implementation

## Research Findings

### Energy-based VAD Limitations
- Current energy-only VAD struggles to distinguish speech from music/effects
- Music often has high RMS but is NOT speech
- Transient sounds (impacts, clicks) can trigger false positives
- No frequency domain context

### Better Approaches Identified (CPU-efficient, offline)

1. **Spectral Centroid**: Speech typically 250-4500Hz; music often concentrated in specific bands
2. **Spectral Flux**: Detects transients - distinguishes steady tones from speech
3. **Zero-Crossing Rate (ZCR)**: High ZCR = noise/hiss; low ZCR = tonal/music
4. **Spectral Flatness**: Music has more tonal/peaky spectrum; speech is more noisy
5. **Band Ratios**: Low vs high frequency energy distribution

### Key Research Insights
- Feature fusion outperforms single features (arXiv 2506.01365)
- Source-related features (ZCR, harmonic clarity) have higher mutual information for VAD
- Spectral flatness is a good pitch indicator substitute for CPU efficiency (rVAD-fast)
- Combining energy with spectral features provides best accuracy vs compute tradeoff

## Implementation Plan

1. **features.rs**: Already has spectral features! Enhance with:
   - spectral_centroid (exists as centroid_hz)
   - spectral_flux (exists)
   - zcr (exists)
   - Add: spectral_flatness (new - music indicator)

2. **vad/energy.rs**: Multi-feature classification
   - Keep existing energy-based logic (works well as base)
   - Add spectral features weighting
   - Add music vs speech discrimination

3. **config.rs**: Add "spectral" to VALID_VAD_ENGINES

4. **vad/mod.rs**: Create SpectralVad engine option

## Feature Weights for Spectral VAD
- RMS energy: 0.35 (base)
- Spectral centroid: 0.15 (speech frequency range)
- ZCR: 0.12 (voice vs noise)
- Spectral flux: 0.10 (transient detection)
- Low/high band ratio: 0.15 (music vs speech)
- NEW - spectral flatness: 0.13 (tonal detection)

## Files to Modify
- src/pipeline/features.rs - Add spectral_flatness
- src/pipeline/vad/energy.rs - Add multi-feature classification  
- src/config.rs - Add "spectral" engine option
- src/pipeline/vad/mod.rs - Export new engine
