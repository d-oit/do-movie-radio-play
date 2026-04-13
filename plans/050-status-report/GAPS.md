# Implementation Gaps

Gaps between the current specification and the implemented runtime behavior.

## VAD Engine Contract Gap

**Affected:** CLI correctness and user trust  
**Location:** `src/cli.rs`, `src/pipeline/vad/mod.rs`

**Spec:** Supported engines should behave according to the selected runtime mode.

**Actual:** `webrtc` and `silero` are still stub implementations that fall back to
energy VAD while the CLI advertises them as first-class choices.

**Fix:** Either implement the engines behind feature flags, or reject unsupported
engines at argument parsing / runtime instead of silently degrading.

## Validation Configuration Drift Gap

**Affected:** Validation fidelity, benchmark comparability  
**Location:** `src/main.rs`

**Spec:** Validation should measure the same pipeline configuration used for
real extraction and calibrated runs.

**Actual:** `validate` always uses `AnalysisConfig::default()`, ignoring runtime
config, profile-based threshold tuning, and alternate VAD engine selection.

**Fix:** Thread config loading and calibration/profile application into `validate`
and `bench`, and stop hardcoding sample rate / frame / min silence defaults when
building truth timelines.

## Calibration Save/Application Gap

**Affected:** Self-learning workflow closure  
**Location:** `src/main.rs`, `src/learning/calibrator.rs`

**Spec:** Learned calibration should be easy to turn into the next active profile.

**Actual:** `apply_calibration_report()` exists, but `--save-calibration` still saves
the runtime input delta rather than the learned recommendation from the latest report.

**Fix:** Replace `--save-calibration` with an explicit report-apply flow, or make it
persist the learned recommendation after calibration runs.

## WAV Decode Fallback Gap

**Affected:** Practical media compatibility  
**Location:** `src/pipeline/decode.rs`, `src/io/wav.rs`

**Spec:** WAV input should decode when possible, falling back to ffmpeg for formats
not handled by the direct reader.

**Actual:** Any `.wav` file is forced through the direct path, which rejects non-16-bit
PCM data.

**Fix:** Detect unsupported WAV formats and route them through ffmpeg instead of failing.

## Strict Config Validation Gap

**Affected:** Runtime predictability  
**Location:** `src/config.rs`

**Spec:** Invalid config values should fail clearly.

**Actual:** Bad environment overrides are silently ignored, and validation only checks
sample rate, frame length, and prompt confidence.

**Fix:** Validate threshold ranges, duration relationships, and engine values; surface
invalid env/config values as actionable errors.

## Validation Input Integrity Gap

**Affected:** Trustworthiness of quality reports  
**Location:** `src/validation/dataset.rs`

**Spec:** Validation artifacts should fail fast on malformed input.

**Actual:** Bad CSV rows silently become `0` or `start_ms` defaults, which can create
incorrect ground truth without any warning.

**Fix:** Return structured errors for malformed rows and optionally log line-level
warnings with file and row references.

## Benchmark Regression Coverage Gap

**Affected:** Performance regression detection  
**Location:** `.github/workflows/ci.yml`, `benches/pipeline_bench.rs`

**Spec:** Benchmarking should support regression visibility over time.

**Actual:** Criterion benches exist, but CI does not run them in a way that compares
results or flags regressions.

**Fix:** Add a benchmark job or dedicated baseline workflow that stores and compares
Criterion results.
