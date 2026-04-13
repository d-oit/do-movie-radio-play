# Implementation Gaps

Gaps between the current specification and the implemented runtime behavior.

## Calibration Save/Application Gap

**Affected:** Self-learning workflow closure  
**Location:** `src/main.rs`, `src/learning/calibrator.rs`

**Spec:** Learned calibration should be easy to turn into the next active profile.

**Actual:** `apply_calibration_report()` exists, but `--save-calibration` still saves
the runtime input delta rather than the learned recommendation from the latest report.

**Fix:** Replace `--save-calibration` with an explicit report-apply flow, or make it
persist the learned recommendation after calibration runs.

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

## Future Capability Gap: True Alternative VAD Engines

**Affected:** Feature completeness  
**Location:** `src/pipeline/vad/mod.rs`, `src/cli.rs`

**Spec:** Non-energy engines should either exist as real implementations or remain
clearly unavailable.

**Actual:** Unsupported engine selections now fail fast, which fixes the misleading
runtime behavior, but real WebRTC and Silero implementations still do not exist.

**Fix:** Implement those engines behind explicit feature flags, or remove them from
the CLI surface until implementation work begins.
