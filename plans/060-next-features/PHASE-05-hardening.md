# Phase 05: Hardening and Quality

Address implementation gaps and quality violations before adding new capabilities.

## Tasks

### 5.1 Remove unwrap() from production code
- Replace `fft.process().unwrap()` in `src/pipeline/features.rs:31` with proper error propagation
- Aligns with project constraint: "no unwrap/expect"

### 5.2 Fix error type semantics
- Add `IoError(String)` variant to `TimelineError`
- Map `io::Error` to `IoError` instead of `InvalidConfig`
- Remove dead `InvalidSubtitle` variant

### 5.3 Harden CSV parser
- Replace `unwrap_or(0)` with structured parse errors
- Return validation errors for malformed ground truth data
- Include row numbers in error messages or warnings

### 5.4 Close the calibration save/apply gap
- Keep `apply-calibration` as the canonical path, or update `--save-calibration`
  to persist learned recommendations instead of the runtime input delta
- Avoid multiple calibration persistence paths that disagree

### 5.5 Amortize feature-extraction allocations
- Reuse `SpectralAnalyzer` in repeated tagging and analysis paths where practical
- Keep hot-path allocations out of per-segment and per-call loops

### 5.6 Add benchmark regression visibility in CI
- Decide whether to gate on benchmark smoke or store comparison artifacts
- Keep this separate from correctness checks to avoid flaky performance gating

## Completed In This Pass

- Validation and benchmark config drift fixed.
- Unsupported VAD engine selections now fail fast.
- Unsupported WAV direct decodes now fall back to `ffmpeg`.
- Config and env override validation tightened.
