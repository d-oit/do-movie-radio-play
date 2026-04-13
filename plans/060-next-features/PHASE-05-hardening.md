# Phase 05: Hardening and Quality

Address implementation gaps and quality violations before adding new capabilities.

## Tasks

### 5.1 Remove unwrap() from production code
- Replace `fft.process().unwrap()` in `src/pipeline/features.rs:31` with proper error propagation
- Replace `profile_path.parent().unwrap()` in `src/main.rs:91` with error handling
- Aligns with project constraint: "no unwrap/expect"

### 5.2 Fix validation and benchmark config drift
- Add config loading to `validate` and `bench` subcommands
- Apply calibration profile loading consistently outside `extract`
- Remove hardcoded `16000`, `20`, and `1000` when constructing truth timelines

### 5.3 Fix advertised VAD engine behavior
- Either implement `webrtc` / `silero` or fail fast when they are selected
- Stop silently falling back to `EnergyVad::new(0.015)`
- Ensure configured threshold tuning remains effective in all engine paths

### 5.4 Fix error type semantics
- Add `IoError(String)` variant to `TimelineError`
- Map `io::Error` to `IoError` instead of `InvalidConfig`
- Remove dead `InvalidSubtitle` variant

### 5.5 Harden CSV parser
- Replace `unwrap_or(0)` with structured parse errors
- Return validation errors for malformed ground truth data
- Include row numbers in error messages or warnings

### 5.6 Fall back to ffmpeg for unsupported WAV formats
- Detect unsupported WAV sample formats in `src/io/wav.rs`
- Route 24-bit PCM and float WAV through ffmpeg instead of failing direct decode

### 5.7 Tighten config validation
- Fail clearly on invalid env override values
- Validate threshold and duration relationships, not just presence/range basics
- Keep CLI/runtime behavior deterministic and explicit

### 5.8 Close the calibration save/apply gap
- Keep `apply-calibration` as the canonical path, or update `--save-calibration`
  to persist learned recommendations instead of the runtime input delta
- Avoid multiple calibration persistence paths that disagree

### 5.9 Amortize feature-extraction allocations
- Reuse `SpectralAnalyzer` in repeated tagging and analysis paths where practical
- Keep hot-path allocations out of per-segment and per-call loops

### 5.10 Add benchmark regression visibility in CI
- Decide whether to gate on benchmark smoke or store comparison artifacts
- Keep this separate from correctness checks to avoid flaky performance gating
