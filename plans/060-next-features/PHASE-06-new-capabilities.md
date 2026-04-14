# Phase 06: New Capabilities

New features to extend the pipeline beyond its current scope.

## 6.0 Production Evaluation Correctness First

Before adding new runtime capabilities, prioritize testable production-eval correctness.

- Establish fixture-to-output coverage so selected real-media fixtures always produce validation reports.
- Keep Tier A evals (synthetic + one modern subtitle fixture) green on every PR.
- Fail fast when outputs are missing, malformed, or metrics are absent.
- See `plans/040-validation/PRODUCTION-EVALS.md` for the concrete matrix and gates.

## 6.1 Profile-Driven Tag Calibration

Extend `CalibrationProfile` beyond `energy_threshold_delta` so genre profiles can
tune non-voice tag rules deterministically.

- Add bounded per-tag rule deltas to `CalibrationProfile`
- Use `CorrectionRecord.original_tags` and `corrected_tags` to recommend updates
- Apply profile-aware thresholds in `src/pipeline/tags.rs`
- Improve prompt quality indirectly by improving tag quality first
- Fits the current architecture without requiring a pipeline rewrite

## 6.2 True Alternative VAD Engines

Replace the current engine stubs with real optional implementations, likely behind
feature flags.

- Implement WebRTC VAD behind a `webrtc-vad` feature, or remove the option
- Implement Silero VAD behind a `silero-vad` feature with explicit model/runtime setup
- Keep CPU-only operation and deterministic integration points where possible

## 6.3 Higher-Quality Resampling (Feature Flag)

Replace linear interpolation with `rubato` crate behind a feature flag.

- Add `rubato` as optional dependency: `rubato = { version = "...", optional = true }`
- Feature flag: `high-quality-resample`
- Default remains linear interpolation for speed
- Reduces aliasing artifacts for non-16kHz source material
- Referenced in init-prompt as intended approach

## 6.4 Streaming / Chunked Processing

Reduce memory footprint for long-form content (2+ hour movies).

- Process audio in fixed-size chunks instead of loading entire file
- Maintain state across chunk boundaries for smoothing and segmentation
- Emit segments incrementally
- Significant architecture change; scope carefully

## 6.5 WAV Format Support Extension

Support direct decoding for 24-bit and 32-bit float WAV files instead of relying
on ffmpeg fallback.

- Detect sample format in WAV header
- Convert to f32 samples internally
- Keep ffmpeg fallback for formats still not covered directly
- Small change with practical benefit for diverse media libraries

## 6.6 Validation and Reporting UX

Make validation a stronger product surface rather than only a developer check.

- Add per-profile validation presets that match extraction config
- Emit clearer report summaries for common error modes
- Optionally include per-stage metrics from extraction runs in validation output

## 6.7 Benchmark Regression Tracking

Build on the existing Criterion suite instead of reintroducing benchmark plumbing.

- Store benchmark baselines in CI artifacts or scheduled runs
- Compare critical stage timings across commits
- Alert on large regressions without blocking every PR by default

## 6.8 Multi-Feature VAD Tuning

Use existing spectral features (ZCR, spectral flux, centroid, band ratios) in VAD
classification. Currently these features are computed for tagging but not for
speech/non-speech decisions.

- Preserve the current `Frame::speech_likelihood()` approach but make thresholds
  configurable and profile-aware
- Separate heuristic tuning from engine selection
- Directly addresses TRIZ-001 (speech vs. music contradiction)
- No ML required; stays within deterministic constraints
